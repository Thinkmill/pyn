use serde::Deserialize;
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;
use std::{collections::HashMap, ffi::OsStr};

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

fn get_package_root(path: &Path) -> io::Result<PackageJson> {
    let package_json_path = path.join(Path::new("package.json"));
    if package_json_path.exists() {
        let contents = fs::read_to_string(package_json_path)?;
        let package_json: PackageJson = serde_json::from_str(&contents)?;
        return Ok(package_json);
    } else if let Some(parent_path) = path.parent() {
        return get_package_root(parent_path);
    } else {
        println!("Could not find a package root");
        std::process::exit(1)
    }
}

fn get_project_root(path: &Path) -> io::Result<ProjectRoot> {
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
        return get_project_root(parent_path);
    }

    eprintln!("No lockfile could be found. If you haven't used a package manager in this project yet, please do that first to generate a lockfile");
    std::process::exit(1)
}

fn run_package_manager<S: AsRef<OsStr>>(path: &Path, args: &[S]) -> io::Result<()> {
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

fn find_binary_location(current_dir: &Path, binary: &str) -> std::path::PathBuf {
    let mut path_string = String::from("node_modules/.bin/");
    path_string.push_str(binary);
    let path = current_dir.join(Path::new(&path_string));
    if path.exists() {
        return path;
    } else if let Some(parent) = current_dir.parent() {
        return find_binary_location(parent, binary);
    } else {
        eprintln!("Could not find a script or binary named {}", binary);
        std::process::exit(1)
    }
}

// TODO: do script running in rust rather than offloading to the package manager
// (i'm not totally sure about that)
fn run_script_or_binary(current_dir: &Path, args: &[String]) -> io::Result<()> {
    let pkg = get_package_root(current_dir)?;
    let bin = args[0].as_ref();
    if pkg.scripts.contains_key(bin) {
        let project = get_project_root(current_dir)?;
        let mut child = project
            .package_manager
            .cmd()
            .arg("run")
            .args(args)
            .spawn()?;
        let status = child.wait()?;
        if let Some(code) = status.code() {
            std::process::exit(code)
        }
    } else {
        let binary = find_binary_location(current_dir, &bin);
        let mut child = Command::new(binary).args(&args[1..]).spawn()?;
        let status = child.wait()?;
        if let Some(code) = status.code() {
            std::process::exit(code)
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let args: Vec<String> = env::args().skip(1).collect();
    let current_dir = env::current_dir()?;
    match args.get(0) {
        Some(first_arg) => match first_arg.as_str() {
            "add" | "install" | "remove" | "why" => run_package_manager(&current_dir, &args),
            _ => run_script_or_binary(&current_dir, &args),
        },
        _ => run_package_manager(&current_dir, &["install"]),
    }
}
