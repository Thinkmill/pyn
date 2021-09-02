use crate::{package_json::PackageJson, package_name::PackageName, Error, PackageManager};
use ignore::WalkBuilder;
use serde::Deserialize;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
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

#[derive(Debug, Clone)]
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

#[derive(Debug, Clone)]
pub struct Project {
    pub root: Package,
    pub packages: Option<HashMap<PackageName, Package>>,
    pub manager: PackageManager,
}

/*
You are adding 'next' to this package. Did you mean:
o "^11.1.0" (latest on npm)
o "^11.0.1" (used in "@examples/next-demo")
o "^10.3.1" (used in "@keystone-next/keystone" and 3 other packages)
*/

type VersionSpecifier = String;

impl Project {
    pub fn dir(&self) -> &Path {
        self.root.path()
    }
    /**
    Finds all the usages of a dependency, returning the versions used, and the
    names of the packages where each version is specified.
    */
    pub fn find_dependents(
        &self,
        name: &PackageName,
    ) -> BTreeMap<VersionSpecifier, Vec<PackageName>> {
        let mut matches: BTreeMap<VersionSpecifier, Vec<PackageName>> = Default::default();
        for pkg in self.iter() {
            for deps in pkg.pkg_json.iter_normal_deps() {
                if let Some(specifier) = deps.get(name) {
                    matches
                        .entry(specifier.clone())
                        .or_default()
                        .push(pkg.pkg_json.name.clone())
                }
            }
        }

        matches
    }
    pub fn get(&self, name: &PackageName) -> Option<&Package> {
        if &self.root.pkg_json.name == name {
            Some(&self.root)
        } else {
            self.packages.as_ref().and_then(|map| map.get(name))
        }
    }
    pub fn get_mut(&mut self, name: &PackageName) -> Option<&mut Package> {
        if &self.root.pkg_json.name == name {
            Some(&mut self.root)
        } else {
            self.packages.as_mut().and_then(|map| map.get_mut(name))
        }
    }
    pub fn closest_pkg(&self, mut path: &Path) -> Option<&Package> {
        let mut pkg_map = HashMap::new();
        pkg_map.insert(self.root.path(), &self.root);
        if let Some(pkgs) = &self.packages {
            for (_, pkg) in pkgs {
                pkg_map.insert(pkg.path(), pkg);
            }
        }

        loop {
            match pkg_map.get(path) {
                Some(&pkg) => break Some(pkg),
                None => match path.parent() {
                    Some(parent_path) => path = parent_path,
                    None => break None,
                },
            }
        }
    }
    pub fn closest_pkg_mut(&mut self, path: &Path) -> Option<&mut Package> {
        match self.closest_pkg(path) {
            Some(pkg) => {
                let name = pkg.pkg_json.name.clone();
                self.get_mut(&name)
            }
            None => None,
        }
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Package> {
        self.packages
            .iter_mut()
            .map(|map| map.iter_mut().map(|(_, pkg)| pkg))
            .flatten()
            .chain(std::iter::once(&mut self.root))
    }

    pub fn iter(&self) -> impl Iterator<Item = &Package> {
        self.packages
            .iter()
            .map(|map| map.iter().map(|(_, pkg)| pkg))
            .flatten()
            .chain(std::iter::once(&self.root))
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    #[test]
    fn project_find() {
        let mut current_dir = env::current_dir().unwrap();
        current_dir.push("fixtures/basic");
        dbg!(Project::find(&current_dir).unwrap());
    }

    #[test]
    fn find_packages_test() {
        let mut current_dir = env::current_dir().unwrap();
        current_dir.push("fixtures/basic");
        dbg!(find_packages(&current_dir, vec!["packages/*".to_owned()]));
    }
}
