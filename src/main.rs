use project::{PackageJson, PackageName};
use serde::Deserialize;
use std::{
    collections::HashMap,
    convert::TryInto,
    env,
    ffi::OsStr,
    fmt, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::{exit, Command},
};
use structopt::StructOpt;
use thiserror::Error;

mod project;

#[derive(Debug, Error)]
enum Error {
    #[error("Could not find a package root")]
    CouldNotFindPackageRoot,
    #[error("No lockfile could be found. If you haven't used a package manager in this project yet, please do that first to generate a lockfile")]
    CouldNotFindLockfile,
    #[error("Could not find a script or binary named {0}")]
    CouldNotFindScriptOrBinary(String),
    #[error("Child process exited with {0}")]
    ChildProcessExit(i32),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    Serde(#[from] serde_json::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Copy)]
enum PackageManager {
    PNPM,
    NPM,
    Yarn,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PackageManager::PNPM => write!(f, "pnpm"),
            PackageManager::NPM => write!(f, "npm"),
            PackageManager::Yarn => write!(f, "Yarn"),
        }
    }
}

impl PackageManager {
    fn cmd(&self) -> Command {
        Command::new(match self {
            PackageManager::PNPM => "pnpm",
            PackageManager::NPM => "npm",
            PackageManager::Yarn => "yarn",
        })
    }
}

struct ProjectRoot {
    package_manager: PackageManager,
    dir: Box<Path>,
}

impl ProjectRoot {
    fn find(path: &Path) -> Result<ProjectRoot> {
        let dir = fs::read_dir(path)?;

        for entry in dir {
            let entry = entry?;
            let package_manager = match entry.file_name().to_str() {
                Some("yarn.lock") => PackageManager::Yarn,
                Some("pnpm-lock.yaml") => PackageManager::PNPM,
                Some("package-lock.json") => PackageManager::NPM,
                _ => continue,
            };
            let project = ProjectRoot {
                dir: Box::from(path),
                package_manager,
            };
            return Ok(project);
        }
        if let Some(parent_path) = path.parent() {
            ProjectRoot::find(parent_path)
        } else {
            Err(Error::CouldNotFindLockfile)
        }
    }
}

#[derive(Deserialize, fmt::Debug)]
struct OldPackageJson {
    #[serde(default)]
    scripts: HashMap<String, String>,
}

impl OldPackageJson {
    fn find(path: &Path) -> Result<OldPackageJson> {
        let package_json_path = path.join(Path::new("package.json"));
        match fs::read_to_string(package_json_path) {
            Ok(contents) => {
                let package_json: OldPackageJson = serde_json::from_str(&contents)?;
                Ok(package_json)
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if let Some(parent_path) = path.parent() {
                    OldPackageJson::find(parent_path)
                } else {
                    Err(Error::CouldNotFindPackageRoot)
                }
            }
            Err(err) => Err(err.into()),
        }
    }
}

fn run_package_manager<S: AsRef<OsStr>>(path: &Path, args: &[S]) -> Result<()> {
    let project = ProjectRoot::find(path)?;
    eprintln!(
        "🧞 Found {} project at {}",
        project.package_manager,
        project.dir.to_string_lossy()
    );
    let status = project
        .package_manager
        .cmd()
        .args(args)
        .current_dir(path)
        .status()?;

    let code = status.code().unwrap_or(1);
    if code == 0 {
        Ok(())
    } else {
        Err(Error::ChildProcessExit(code))
    }
}

fn find_binary_location(current_dir: &Path, binary: &str) -> Result<PathBuf> {
    let mut path = current_dir.to_owned();
    path.push("node_modules/.bin/");
    path.push(binary);
    if path.exists() {
        Ok(path)
    } else if let Some(parent) = current_dir.parent() {
        find_binary_location(parent, binary)
    } else {
        Err(Error::CouldNotFindScriptOrBinary(binary.to_owned()))
    }
}

// TODO: do script running in rust rather than offloading to the package manager
// (i'm not totally sure about that)
fn run_script_or_binary(current_dir: &Path, args: &[String]) -> Result<()> {
    let pkg = OldPackageJson::find(current_dir)?;
    let bin = args[0].as_ref();
    let status = if pkg.scripts.contains_key(bin) {
        let project = ProjectRoot::find(current_dir)?;
        project
            .package_manager
            .cmd()
            .arg("run")
            .args(args)
            .status()?
    } else {
        let binary = find_binary_location(current_dir, &bin)?;
        Command::new(binary).args(&args[1..]).status()?
    };
    let code = status.code().unwrap_or(1);
    if code == 0 {
        Ok(())
    } else {
        Err(Error::ChildProcessExit(code))
    }
}

#[derive(StructOpt)]
enum Pyn {
    /// Lists the avialable scripts in your package
    Scripts,
    /// Removes dependencies from the current package and runs install
    Remove {
        dependencies: Vec<String>,
        /// Removes the package from everywhere in the project
        #[structopt(long, short)]
        everywhere: bool,
        /// Skips the install step
        #[structopt(long, short)]
        skip_install: bool,
    },
    #[structopt(external_subcommand)]
    Other(Vec<String>),
}

#[derive(StructOpt)]
#[structopt(about = "your nifty package manager runner")]
struct Opts {
    #[structopt(subcommand)]
    pyn: Option<Pyn>,
}

fn main() {
    let opt = Opts::from_args()
        .pyn
        .unwrap_or_else(|| Pyn::Other(vec!["install".to_owned()]));
    let current_dir = env::current_dir().unwrap();

    match opt {
        Pyn::Scripts => {
            let (pkg_json, _) = PackageJson::find(&current_dir).unwrap();
            println!("Available scripts:\n{:#?}", pkg_json.scripts);
        }
        Pyn::Remove {
            everywhere,
            dependencies,
            skip_install,
        } => {
            println!("Removing {:?}", dependencies);
            let deps_as_pkg_names: Vec<PackageName> = dependencies
                .into_iter()
                .map(|dep| dep.try_into())
                .collect::<std::result::Result<_, _>>()
                .unwrap();
            let do_remove = |pkg_jsons: &mut [(PackageJson, PathBuf)]| {
                for (pkg_json, pkg_json_path) in pkg_jsons {
                    // remove the dependencies
                    for pkg in &deps_as_pkg_names {
                        pkg_json.remove_dep(pkg);
                    }
                    // write the updated package.json back to disk
                    pkg_json.write(&pkg_json_path).unwrap();
                    // run install
                    if !skip_install {
                        run_package_manager(&current_dir, &["install"]).unwrap();
                    }
                }
            };
            if everywhere {
                // TODO: Find workspaces, and all the package.jsons, and remove the dependencies from them
                println!("Remove from everywhere is not implemented yet");
                exit(1);
            } else {
                // find the closest package json
                let pkg_json = PackageJson::find(&current_dir).unwrap();
                do_remove(&mut [pkg_json])
            }
        }
        Pyn::Other(args) => {
            let result = match args.get(0).unwrap().as_str() {
                "add" | "install" | "remove" | "why" => run_package_manager(&current_dir, &args),
                _ => run_script_or_binary(&current_dir, &args),
            };
            if let Err(err) = result {
                match err {
                    Error::ChildProcessExit(code) => std::process::exit(code),
                    _ => {
                        eprintln!("{}", err);
                        std::process::exit(1);
                    }
                }
            }
        }
    }
}
