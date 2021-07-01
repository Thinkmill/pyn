use package_json::PackageJson;
pub(crate) use package_name::PackageName;
use project::Project;
use serde::Deserialize;
use std::{
    env,
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
    process::{exit, Command},
};
use structopt::StructOpt;
use thiserror::Error;

mod package_json;
mod package_name;
mod project;

#[derive(Debug, Error)]
pub enum Error {
    #[error("No lockfile could be found. If you haven't used a package manager in this project yet, please do that first to generate a lockfile")]
    CouldNotFindLockfile,
    #[error("Could not find a script or binary named {0}")]
    CouldNotFindScriptOrBinary(String),
    #[error("Child process exited with {0}")]
    ChildProcessExit(i32),
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("{0}")]
    SerdeJson(#[from] serde_json::Error),
    #[error("{0}")]
    SerdeYaml(#[from] serde_yaml::Error),
}

type Result<T> = std::result::Result<T, Error>;

#[derive(Deserialize)]
struct PackageJsonFromUnpkg {
    version: String,
}

fn get_npm_package_version(package: &str) -> String {
    let client = reqwest::blocking::Client::new();
    let request_url = String::from("https://unpkg.com/") + package + "/package.json";

    let pkg_json = client
        .get(request_url)
        .send()
        .unwrap()
        .json::<PackageJsonFromUnpkg>()
        .unwrap();
    pkg_json.version
}

// This will keep breaking as the version of react changes, use for debug at will
#[test]
fn get_latest_version_of_react() {
    let result = get_npm_package_version("react");
    assert_eq!(result, "17.0.2")
}

#[derive(Debug, Clone, Copy)]
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

fn run_package_manager_at_project_root<S: AsRef<OsStr>>(
    project: &Project,
    args: &[S],
) -> Result<()> {
    let status = project
        .manager
        .cmd()
        .args(args)
        .current_dir(project.dir())
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
fn run_script_or_binary(current_dir: &Path, project: &Project, args: &[String]) -> Result<()> {
    let (pkg, _) = PackageJson::find(current_dir)?;
    let bin = args[0].as_ref();
    let status = if pkg.scripts.contains_key(bin) {
        let project = Project::find(current_dir)?;
        project.manager.cmd().arg("run").args(args).status()?
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
enum Subcommand {
    /// Lists the avialable scripts in your package
    Scripts,
    /// Adds dependencies to the current package and runs install
    Add {
        dependencies: Vec<PackageName>,
        /// Skips the install step
        #[structopt(long, short)]
        skip_install: bool,
        /// Add to dev dependencies
        #[structopt(long, short)]
        dev: bool,
    },
    /// Removes dependencies from the current package and runs install
    Remove {
        dependencies: Vec<PackageName>,
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
    subcommand: Option<Subcommand>,
}

fn main() {
    let opt = Opts::from_args()
        .subcommand
        .unwrap_or_else(|| Subcommand::Other(vec!["install".to_owned()]));
    let current_dir = env::current_dir().unwrap();
    let project = Project::find(&current_dir).unwrap();
    match opt {
        Subcommand::Scripts => {
            let (pkg_json, _) = PackageJson::find(&current_dir).unwrap();
            println!("Available scripts:\n{:#?}", pkg_json.scripts);
        }
        Subcommand::Add {
            dependencies,
            skip_install,
            dev,
        } => {
            let (mut pkg_json, pkg_json_path) = PackageJson::find(&current_dir).unwrap();
            // add the dependencies
            for dep in dependencies {
                let version = get_npm_package_version(dep.as_str());
                if dev {
                    pkg_json.dev_dependencies.insert(dep, version);
                } else {
                    pkg_json.dependencies.insert(dep, version);
                }
            }
            // write the updated package.json back to disk
            pkg_json.write(&pkg_json_path).unwrap();
            // run install
            if !skip_install {
                run_package_manager_at_project_root(&project, &["install"]).unwrap();
            }
        }
        Subcommand::Remove {
            everywhere,
            dependencies,
            skip_install,
        } => {
            println!("Removing {:?}", dependencies);
            let do_remove = |mut pkg_json: PackageJson, pkg_json_path: &Path| {
                // remove the dependencies
                for pkg in &dependencies {
                    pkg_json.remove_dep(pkg);
                }
                // write the updated package.json back to disk
                pkg_json.write(&pkg_json_path).unwrap();
                // run install
                if !skip_install {
                    run_package_manager_at_project_root(&project, &["install"]).unwrap();
                }
            };
            if everywhere {
                // TODO: Find workspaces, and all the package.jsons, and remove the dependencies from them
                println!("Remove from everywhere is not implemented yet");
                exit(1);
            } else {
                // find the closest package json
                let (pkg_json, pkg_json_path) = PackageJson::find(&current_dir).unwrap();
                do_remove(pkg_json, &pkg_json_path)
            }
        }
        Subcommand::Other(args) => {
            let result = match args.get(0).unwrap().as_str() {
                "add" | "install" | "remove" | "why" => {
                    run_package_manager_at_project_root(&project, &args)
                }
                _ => run_script_or_binary(&current_dir, &project, &args),
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
