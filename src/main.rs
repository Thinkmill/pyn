use serde::Deserialize;
use std::{
    collections::HashMap,
    env,
    ffi::OsStr,
    fmt, fs, io,
    path::{Path, PathBuf},
    process::Command,
};
use thiserror::Error;

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

#[derive(Deserialize, fmt::Debug)]
struct PackageJson {
    #[serde(default)]
    scripts: HashMap<String, String>,
}

impl Default for PackageJson {
    fn default() -> Self {
        PackageJson {
            scripts: HashMap::new(),
        }
    }
}

fn get_package_root(path: &Path) -> Result<PackageJson> {
    let package_json_path = path.join(Path::new("package.json"));
    if package_json_path.exists() {
        let contents = fs::read_to_string(package_json_path)?;
        let package_json: PackageJson = serde_json::from_str(&contents)?;
        Ok(package_json)
    } else if let Some(parent_path) = path.parent() {
        get_package_root(parent_path)
    } else {
        Err(Error::CouldNotFindPackageRoot)
    }
}

fn get_project_root(path: &Path) -> Result<ProjectRoot> {
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
        get_project_root(parent_path)
    } else {
        Err(Error::CouldNotFindLockfile)
    }
}

fn run_package_manager<S: AsRef<OsStr>>(path: &Path, args: &[S]) -> Result<()> {
    let project = get_project_root(path)?;
    eprintln!(
        "🧞 Found {} project at {}",
        project.package_manager,
        project.dir.to_string_lossy()
    );
    project
        .package_manager
        .cmd()
        .args(args)
        .current_dir(path)
        .spawn()?
        .wait()?;

    Ok(())
}

fn find_binary_location(current_dir: &Path, binary: &str) -> Result<PathBuf> {
    let mut path_string = String::from("node_modules/.bin/");
    path_string.push_str(binary);
    let path = current_dir.join(Path::new(&path_string));
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
    let pkg = get_package_root(current_dir)?;
    let bin = args[0].as_ref();
    let mut child = if pkg.scripts.contains_key(bin) {
        let project = get_project_root(current_dir)?;
        project
            .package_manager
            .cmd()
            .arg("run")
            .args(args)
            .spawn()?
    } else {
        let binary = find_binary_location(current_dir, &bin)?;
        Command::new(binary).args(&args[1..]).spawn()?
    };
    let status = child.wait()?;
    let code = status.code().unwrap_or(1);
    if code == 0 {
        Ok(())
    } else {
        Err(Error::ChildProcessExit(code))
    }
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let current_dir = env::current_dir().unwrap();
    let result = match args.get(0) {
        Some(first_arg) => match first_arg.as_str() {
            "add" | "install" | "remove" | "why" => run_package_manager(&current_dir, &args),
            _ => run_script_or_binary(&current_dir, &args),
        },
        _ => run_package_manager(&current_dir, &["install"]),
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
