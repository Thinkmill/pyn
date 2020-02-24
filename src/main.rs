use serde::Deserialize;
use serde_json::Value;
use std::collections::{HashMap, VecDeque};
use std::env;
use std::fmt;
use std::fs;
use std::io;
use std::path::Path;
use std::process::Command;

enum PackageManager {
    PNPM,
    NPM,
    Yarn,
    Bolt,
}

impl fmt::Display for PackageManager {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match *self {
            PackageManager::PNPM => write!(f, "pnpm"),
            PackageManager::NPM => write!(f, "npm"),
            PackageManager::Yarn => write!(f, "Yarn"),
            PackageManager::Bolt => write!(f, "Bolt"),
        }
    }
}

impl PackageManager {
    fn cmd(&self) -> Command {
        Command::new(match self {
            PackageManager::PNPM => "pnpm",
            PackageManager::NPM => "npm",
            PackageManager::Yarn => "yarn",
            PackageManager::Bolt => "bolt",
        })
    }
}

struct ProjectRoot {
    package_manager: PackageManager,
    dir: Box<Path>,
}

struct PackageRoot {
    // dir: Box<Path>,
    package_json: PackageJson,
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

fn get_package_root(path: &Path) -> io::Result<PackageRoot> {
    let package_json_path = path.join(Path::new("package.json"));
    if package_json_path.exists() {
        let contents = fs::read_to_string(package_json_path)?;
        let package_json: PackageJson = serde_json::from_str(&contents)?;
        let project = PackageRoot {
            // dir: Box::from(path),
            package_json: package_json,
        };
        return Ok(project);
    } else if let Some(parent_path) = path.parent() {
        return get_package_root(parent_path);
    } else {
        println!("Could not find a package root");
        std::process::exit(1)
    }
}

fn get_project_root(path: &Path) -> io::Result<ProjectRoot> {
    let mut dir = fs::read_dir(path)?;

    while let Some(res) = dir.next() {
        let entry = res?;
        let file_name = String::from(entry.file_name().to_string_lossy());
        if file_name == "yarn.lock" {
            let package_json_content_string =
                fs::read_to_string(path.join(Path::new("package.json")))?;
            let package_json_content: Value = serde_json::from_str(&package_json_content_string)?;
            let package_manager = if package_json_content
                .as_object()
                .unwrap()
                .contains_key("bolt")
            {
                PackageManager::Bolt
            } else {
                PackageManager::Yarn
            };
            let project = ProjectRoot {
                dir: Box::from(path),
                package_manager,
            };
            return Ok(project);
        }
        if file_name == "pnpm-lock.yaml" {
            let project = ProjectRoot {
                dir: Box::from(path),
                package_manager: PackageManager::PNPM,
            };
            return Ok(project);
        }
        if file_name == "package-lock.json" {
            let project = ProjectRoot {
                dir: Box::from(path),
                package_manager: PackageManager::NPM,
            };
            return Ok(project);
        }
    }
    if let Some(parent_path) = path.parent() {
        return get_project_root(parent_path);
    }

    println!("No lockfile could be found. If you haven't used a package manager in this p yet, please do that first to generate a lockfile");
    std::process::exit(1)
}

fn run_package_manager(path: &Path, args: VecDeque<String>) -> io::Result<()> {
    let project = get_project_root(path)?;
    println!(
        "Found {} project at {}",
        project.package_manager,
        project.dir.to_string_lossy()
    );
    let mut child = project
        .package_manager
        .cmd()
        .args(args)
        .current_dir(path)
        .spawn()?;
    child.wait()?;

    Ok(())
}

fn find_binary_location(current_dir: &Path, binary: &String) -> std::path::PathBuf {
    let mut path_string = String::from("node_modules/.bin/");
    path_string.push_str(binary.as_str());
    let path = current_dir.join(Path::new(&path_string));
    if path.exists() {
        return path;
    } else if let Some(parent) = current_dir.parent() {
        return find_binary_location(parent, binary);
    } else {
        println!("Could not find a script or binary named {}", binary);
        std::process::exit(1)
    }
}

// TODO: do script running in rust rather than offloading to the package manager
fn run_script_or_binary(current_dir: &Path, mut args: VecDeque<String>) -> io::Result<()> {
    let pkg = get_package_root(current_dir)?;
    if let Some(bin) = args.front() {
        if pkg.package_json.scripts.contains_key(bin) {
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
            let binary = find_binary_location(current_dir, bin);
            args.pop_front();
            let mut child = Command::new(binary).args(args).spawn()?;
            let status = child.wait()?;
            if let Some(code) = status.code() {
                std::process::exit(code)
            }
        }
    }

    Ok(())
}

fn main() -> io::Result<()> {
    let mut i = 0;
    let env_args = env::args();

    let mut args = VecDeque::with_capacity(if env_args.len() == 0 {
        1
    } else {
        env_args.len()
    });
    for argument in env_args {
        if i != 0 {
            args.push_back(argument);
        }
        i = i + 1;
    }
    let current_dir = env::current_dir()?;
    if args.len() == 0 {
        args.push_back(String::from("install"));
        return run_package_manager(&current_dir, args);
    }
    let add_string = String::from("add");
    if args.get(0) == Option::Some(&add_string) {
        return run_package_manager(&current_dir, args);
    }
    return run_script_or_binary(&current_dir, args);
}
