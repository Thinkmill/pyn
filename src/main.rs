pub(crate) use package_name::PackageName;
use project::{Package, Project};
use serde::Deserialize;
use std::{
    env,
    ffi::OsStr,
    fmt, io,
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
struct RegistryMetadata {
    #[serde(rename = "dist-tags")]
    dist_tags: DistTags,
}

#[derive(Deserialize)]
struct DistTags {
    latest: String,
}

async fn get_npm_package_version(
    client: reqwest::Client,
    package: &str,
) -> std::result::Result<String, reqwest::Error> {
    let pkg_json = client
        .get(format!("https://registry.npmjs.org/{}", package))
        .header(
            reqwest::header::ACCEPT,
            "application/vnd.npm.install-v1+json",
        )
        .send()
        .await?
        .json::<RegistryMetadata>()
        .await?;
    Ok(pkg_json.dist_tags.latest)
}

#[tokio::main]
async fn get_latest_versions(
    packages: Vec<PackageName>,
) -> std::result::Result<Vec<(PackageName, String)>, reqwest::Error> {
    let client = reqwest::Client::new();
    let mut futures_unordered = futures::stream::FuturesOrdered::new();
    for pkg in packages {
        let client = client.clone();
        futures_unordered.push(async move {
            get_npm_package_version(client, pkg.as_str())
                .await
                .map(|version| (pkg, version))
        })
    }
    use futures::TryStreamExt;
    futures_unordered.try_collect().await
}

#[test]
fn blah() {
    let react_name = PackageName::try_from("react").unwrap();
    let react_dom_name = PackageName::try_from("react-dom").unwrap();
    let mut result = get_latest_versions(vec![react_name.clone(), react_dom_name.clone()]).unwrap();
    result.sort();
    assert_eq!(
        result,
        vec![
            (react_name, "17.0.2".into()),
            (react_dom_name, "17.0.2".into())
        ]
    )
}

// This will keep breaking as the version of react changes, use for debug at will
#[tokio::test]
async fn get_latest_version_of_react() {
    let result = get_npm_package_version(Default::default(), "react")
        .await
        .unwrap();
    assert_eq!(result, "17.0.2")
}

#[tokio::main]
async fn get_npm_package_version_sync(pkg: &str) -> std::result::Result<String, reqwest::Error> {
    get_npm_package_version(Default::default(), pkg).await
}

#[derive(Debug, Clone, Copy)]
pub enum PackageManager {
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
    let pkg = &project.closest_pkg(current_dir).unwrap().pkg_json;
    let bin = args[0].as_ref();
    let status = if pkg.scripts.contains_key(bin) {
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
    /// Upgrades a dependency everywhere in the project and runs install
    Upgrade {
        dependencies: Vec<PackageName>,
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

fn add_dep(pkg: &mut Package, dep: PackageName, version: String, dev: bool) {
    if dev {
        pkg.pkg_json.dev_dependencies.insert(dep, version);
    } else {
        pkg.pkg_json.dependencies.insert(dep, version);
    }
}

fn add(
    project: &mut Project,
    current_dir: &Path,
    dependencies: Vec<PackageName>,
    dev: bool,
) -> Result<()> {
    let mut pkg = project.closest_pkg(&current_dir).unwrap().clone();

    let deps_with_latests = get_latest_versions(dependencies).unwrap();

    for (dep, latest_version) in deps_with_latests {
        let existing_versions = project.find_dependents(&dep);
        let latest_version_range = format!("^{}", latest_version);
        if existing_versions.len() == 0 || existing_versions.get(&latest_version_range).is_some() {
            add_dep(&mut pkg, dep, latest_version_range, dev);
        } else {
            use dialoguer::{theme::ColorfulTheme, Select};
            if existing_versions.len() > 1 {
                println!(
                    "There are multiple versions of {} in the repo, which one do you want to add?",
                    &dep
                );
            } else {
                println!(
                    "An old version of {} is already in the repo, which one do you want to add?",
                    &dep
                );
            }

            let latest_dep_string = format!("{} (latest version)", &latest_version_range);

            let items: Vec<_> = std::iter::once(&latest_dep_string)
                .chain(existing_versions.keys())
                .collect();
            let selection = Select::with_theme(&ColorfulTheme::default())
                .items(&items)
                .default(0)
                .interact()?;
            let version = match selection {
                0 => latest_version_range,
                _ => items[selection].clone(),
            };
            add_dep(&mut pkg, dep, version, dev);
        }
    }
    // write the updated package.json back to disk
    pkg.write()?;
    Ok(())
}

fn upgrade(project: &mut Project, dependencies: Vec<PackageName>) -> Result<()> {
    for dep in dependencies {
        let existing_versions = project.find_dependents(&dep);
        let latest_version = format!("^{}", get_npm_package_version_sync(dep.as_str()).unwrap());

        if existing_versions.len() == 0 {
            println!(
                "{} is not present in the repo. Did you mean to add it?",
                &dep
            );
        } else {
            println!("{} has been upgraded to {}", &dep, latest_version);
        }
    }
    Ok(())
}

fn main() {
    let opt = Opts::from_args()
        .subcommand
        .unwrap_or_else(|| Subcommand::Other(vec!["install".to_owned()]));
    let current_dir = env::current_dir().unwrap();
    let mut project = Project::find(&current_dir).unwrap();
    match opt {
        Subcommand::Scripts => {
            let pkg_json = &project.closest_pkg(&current_dir).unwrap().pkg_json;
            println!("Available scripts:\n{:#?}", pkg_json.scripts);
        }
        Subcommand::Add {
            dependencies,
            skip_install,
            dev,
        } => {
            // add the dependency
            add(&mut project, &current_dir, dependencies, dev).unwrap();
            // run install
            if !skip_install {
                run_package_manager_at_project_root(&project, &["install"]).unwrap();
            }
        }
        Subcommand::Upgrade {
            dependencies,
            skip_install,
        } => {
            // add the dependency
            upgrade(&mut project, dependencies).unwrap();
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
            let do_remove = |pkg: &mut Package| {
                // remove the dependencies
                for dep in &dependencies {
                    pkg.pkg_json.remove_dep(dep);
                }
                // write the updated package.json back to disk
                pkg.write().unwrap();
            };
            if everywhere {
                match &mut project.packages {
                    Some(_) => (),
                    None => {
                        eprintln!(
                            "This project is not a monorepo, you can't use the --everywhere flag."
                        );
                        exit(1);
                    }
                };
                for pkg in project.iter_mut() {
                    do_remove(pkg);
                }
                // Loop and call do_remove
            } else {
                // find the closest package json
                let pkg = project.closest_pkg_mut(&current_dir).unwrap();
                do_remove(pkg)
            }
            // run install
            if !skip_install {
                run_package_manager_at_project_root(&project, &["install"]).unwrap();
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
