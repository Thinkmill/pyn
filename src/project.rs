use crate::PackageManager;
use linked_hash_map::LinkedHashMap as InsertionOrderMap;
pub use package_name::PackageName;
use serde::{ser::SerializeMap, Deserialize, Serialize, Serializer};
use serde_json::Value;
use std::{
    collections::BTreeMap as KeyOrderedMap,
    convert::TryFrom,
    io::ErrorKind,
    path::{Path, PathBuf},
};
mod package_name;

type Dependencies = KeyOrderedMap<PackageName, String>;

#[derive(Debug, Clone)]
enum PkgJsonValue {
    StoredElsewhere,
    Value(Value),
}

#[derive(Debug, Clone, Deserialize)]
#[serde(try_from = "InsertionOrderMap<String, Value>")]
pub struct PackageJson {
    pub name: PackageName,
    pub dependencies: Dependencies,
    pub dev_dependencies: Dependencies,
    pub optional_dependencies: Dependencies,
    pub peer_dependencies: Dependencies,
    storage: InsertionOrderMap<String, PkgJsonValue>,
}

impl PackageJson {
    pub fn remove_dep(&mut self, pkg: &PackageName) {
        self.dependencies.remove(pkg);
        self.dev_dependencies.remove(pkg);
        self.optional_dependencies.remove(pkg);
        self.peer_dependencies.remove(pkg);
    }
    pub fn read(path: &Path) -> std::io::Result<PackageJson> {
        let contents = std::fs::read_to_string(path)?;
        Ok(serde_json::from_str(&contents)?)
    }
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let stringified = serde_json::to_string_pretty(self)?;
        std::fs::write(path, stringified)
    }
    pub fn find(path: &Path) -> std::io::Result<(PackageJson, PathBuf)> {
        let package_json_path = path.join("package.json");
        match std::fs::read_to_string(&package_json_path) {
            Ok(contents) => {
                let package_json: PackageJson = serde_json::from_str(&contents)?;
                Ok((package_json, package_json_path))
            }
            Err(err) if err.kind() == ErrorKind::NotFound => {
                if let Some(parent_path) = path.parent() {
                    PackageJson::find(parent_path)
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
}

impl Serialize for PackageJson {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_map(None)?;
        for (key, val) in &self.storage {
            if let PkgJsonValue::Value(value) = val {
                state.serialize_entry(key, value)?;
            } else {
                if key.as_str() == "name" {
                    state.serialize_entry(key, &self.name)?;
                } else {
                    let deps = match key.as_str() {
                        "dependencies" => &self.dependencies,
                        "devDependencies" => &self.dev_dependencies,
                        "peerDependencies" => &self.peer_dependencies,
                        "optionalDependencies" => &self.optional_dependencies,
                        _ => unreachable!("key cannot be {}", key),
                    };
                    if !deps.is_empty() {
                        state.serialize_entry(key, deps)?;
                    }
                }
            }
        }
        state.end()
    }
}

const DEPENDENCY_TYPES: [&'static str; 4] = [
    "dependencies",
    "devDependencies",
    "peerDependencies",
    "optionalDependencies",
];

impl TryFrom<InsertionOrderMap<String, Value>> for PackageJson {
    type Error = std::io::Error;
    fn try_from(value: InsertionOrderMap<String, Value>) -> Result<Self, Self::Error> {
        let mut name = None;
        let mut dependencies = KeyOrderedMap::new();
        let mut dev_dependencies = KeyOrderedMap::new();
        let mut peer_dependencies = KeyOrderedMap::new();
        let mut optional_dependencies = KeyOrderedMap::new();
        let mut storage = InsertionOrderMap::with_capacity(value.len());
        for (key, value) in value {
            let map = match key.as_str() {
                "name" => {
                    name = Some(serde_json::from_value(value)?);
                    storage.insert(key, PkgJsonValue::StoredElsewhere);
                    continue;
                }
                "dependencies" => &mut dependencies,
                "devDependencies" => &mut dev_dependencies,
                "peerDependencies" => &mut peer_dependencies,
                "optionalDependencies" => &mut optional_dependencies,
                _ => {
                    storage.insert(key, PkgJsonValue::Value(value));
                    continue;
                }
            };
            *map = serde_json::from_value(value)?;
            storage.insert(key, PkgJsonValue::StoredElsewhere);
        }
        for &dep_type in &DEPENDENCY_TYPES {
            if !storage.contains_key(dep_type) {
                storage.insert(dep_type.to_owned(), PkgJsonValue::StoredElsewhere);
            }
        }
        Ok(PackageJson {
            name: name
                .ok_or_else(|| std::io::Error::new(ErrorKind::InvalidData, "missing name field"))?,
            dependencies,
            dev_dependencies,
            optional_dependencies,
            peer_dependencies,
            storage,
        })
    }
}

struct JsProject {
    manager: PackageManager,
    root_path: Box<Path>,
}

impl JsProject {
    pub fn manager(&self) -> PackageManager {
        self.manager
    }
    pub fn root_path(&self) -> &Path {
        &self.root_path
    }
}
