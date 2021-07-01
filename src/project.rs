use crate::{package_json::PackageJson, package_name::PackageName, Error, PackageManager};
use ignore::WalkBuilder;
use serde::Deserialize;
use std::{
    collections::HashMap,
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
};

#[derive(Debug, Deserialize)]
struct PnpmWorkspaceConfig {
    packages: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum NpmOrYarnWorkspaceConfig {
    Packages(Vec<String>),
    Nested { packages: Vec<String> },
}

#[derive(Debug, Deserialize)]
struct PackageJsonForNpmOrYarnWorkspaceConfig {
    workspaces: Option<NpmOrYarnWorkspaceConfig>,
}

#[derive(Debug)]
pub struct Package {
    pub pkg_json_path: PathBuf,
    pub pkg_json: PackageJson,
}

impl Package {
    pub fn find<P: Into<PathBuf>>(path: P) -> std::io::Result<Package> {
        let mut pkg_json_path = path.into();
        pkg_json_path.push("package.json");
        match std::fs::read_to_string(&pkg_json_path) {
            Ok(contents) => Ok(Package {
                pkg_json_path,
                pkg_json: serde_json::from_str(&contents)?,
            }),
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if pkg_json_path.pop() {
                    Package::find(pkg_json_path)
                } else {
                    Err(std::io::Error::new(
                        ErrorKind::NotFound,
                        "could not find a package.json",
                    ))
                }
            }
            Err(err) => Err(err.into()),
        }
    }
    pub fn path(&self) -> &Path {
        self.pkg_json_path.parent().unwrap()
    }
    pub fn write(&self) -> std::io::Result<()> {
        self.pkg_json.write(&self.pkg_json_path)
    }
}

#[derive(Debug)]
pub struct Project {
    pub root: Package,
    pub packages: Option<HashMap<PackageName, Package>>,
    pub manager: PackageManager,
}

impl Project {
    pub fn dir(&self) -> &Path {
        self.root.path()
    }
    pub fn find(path: &Path) -> Result<Project, Error> {
        let dir = fs::read_dir(path)?;

        for entry in dir {
            let entry = entry?;
            let package_manager = match entry.file_name().to_str() {
                Some("yarn.lock") => PackageManager::Yarn,
                Some("pnpm-lock.yaml") => PackageManager::PNPM,
                Some("package-lock.json") => PackageManager::NPM,
                _ => continue,
            };
            let pkg_json_path = path.join("package.json");

            let pkg_json_string = fs::read_to_string(&pkg_json_path)?;

            let package_globs: Option<Vec<String>> = match package_manager {
                PackageManager::NPM | PackageManager::Yarn => {
                    let pkg_json: PackageJsonForNpmOrYarnWorkspaceConfig =
                        serde_json::from_str(&pkg_json_string)?;
                    pkg_json.workspaces.map(|config| match config {
                        NpmOrYarnWorkspaceConfig::Nested { packages }
                        | NpmOrYarnWorkspaceConfig::Packages(packages) => packages,
                    })
                }
                PackageManager::PNPM => {
                    match fs::read_to_string(path.join("pnpm-workspace.yaml")) {
                        Ok(contents) => {
                            let config: PnpmWorkspaceConfig = serde_yaml::from_str(&contents)?;
                            Some(config.packages)
                        }
                        Err(err) if err.kind() == ErrorKind::NotFound => None,
                        Err(err) => return Err(err.into()),
                    }
                }
            };

            eprintln!(
                "🧞 Found {} project at {}",
                package_manager,
                pkg_json_path.parent().unwrap().display()
            );

            return Ok(Project {
                manager: package_manager,
                packages: package_globs.map(|globs| {
                    find_packages(path, globs)
                        .into_iter()
                        .map(|path| {
                            let pkg = Package::find(path.parent().unwrap()).unwrap();
                            let name = pkg.pkg_json.name.clone();
                            (name, pkg)
                        })
                        .collect()
                }),
                root: Package {
                    pkg_json: pkg_json_string.parse()?,
                    pkg_json_path,
                },
            });
        }
        if let Some(parent_path) = path.parent() {
            Project::find(parent_path)
        } else {
            Err(Error::CouldNotFindLockfile)
        }
    }
}

#[test]
fn project_find() {
    let mut current_dir = env::current_dir().unwrap();
    current_dir.push("fixtures/basic");
    dbg!(Project::find(&current_dir));
}

#[test]
fn find_packages_test() {
    let mut current_dir = env::current_dir().unwrap();
    current_dir.push("fixtures/basic");
    dbg!(find_packages(&current_dir, vec!["packages/*".to_owned()]));
}
fn find_packages(root: &Path, globs: Vec<String>) -> Vec<PathBuf> {
    let mut builder = ignore::overrides::OverrideBuilder::new(root);
    for mut glob in globs {
        glob.push_str("/package.json");
        builder.add(&glob).unwrap();
    }

    let overrides_thing = builder.build().unwrap();
    let mut package_json_paths = vec![];

    for result in WalkBuilder::new(root)
        .overrides(overrides_thing.clone())
        .build()
    {
        let dir_entry = result.unwrap();
        let path = dir_entry.path();
        match overrides_thing.matched(path, false) {
            ignore::Match::Whitelist(_) => package_json_paths.push(path.to_owned()),
            _ => {}
        }
    }
    package_json_paths
}
