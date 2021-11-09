use crate::{Error, PackageName};
use linked_hash_map::LinkedHashMap as InsertionOrderMap;
use serde::{
    de::{self, Visitor},
    ser::SerializeMap,
    Deserialize, Serialize, Serializer,
};
use serde_json::Value;
use std::{collections::BTreeMap as KeyOrderedMap, path::Path, str::FromStr};

type Dependencies = KeyOrderedMap<PackageName, String>;

#[derive(Debug, Clone)]
enum PkgJsonValue {
    StoredElsewhere,
    Value(Value),
}

#[derive(Debug, Clone)]
pub struct PackageJson {
    pub name: PackageName,
    pub dependencies: Dependencies,
    pub dev_dependencies: Dependencies,
    pub optional_dependencies: Dependencies,
    pub peer_dependencies: Dependencies,
    pub scripts: InsertionOrderMap<String, String>,
    storage: InsertionOrderMap<String, PkgJsonValue>,
}

impl PackageJson {
    pub fn iter_normal_deps(&self) -> impl Iterator<Item = &Dependencies> {
        [
            &self.dependencies,
            &self.dev_dependencies,
            &self.optional_dependencies,
        ]
        .into_iter()
    }
    pub fn remove_dep(&mut self, pkg: &PackageName) {
        self.dependencies.remove(pkg);
        self.dev_dependencies.remove(pkg);
        self.optional_dependencies.remove(pkg);
        self.peer_dependencies.remove(pkg);
    }
    pub fn write(&self, path: &Path) -> std::io::Result<()> {
        let mut stringified = serde_json::to_string_pretty(self)?;
        stringified.push('\n');
        std::fs::write(path, stringified)
    }
}

impl FromStr for PackageJson {
    type Err = Error;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(serde_json::from_str(s)?)
    }
}

impl Serialize for PackageJson {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut state = serializer.serialize_map(None)?;
        for (key, val) in &self.storage {
            if let PkgJsonValue::Value(value) = val {
                state.serialize_entry(key, value)?;
            } else {
                match key.as_str() {
                    "name" => state.serialize_entry(key, &self.name)?,
                    "scripts" => {
                        if !self.scripts.is_empty() {
                            state.serialize_entry(key, &self.scripts)?
                        }
                    }
                    _ => {
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

impl<'de> Deserialize<'de> for PackageJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PackageJsonVisitor;
        impl<'de> Visitor<'de> for PackageJsonVisitor {
            type Value = PackageJson;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                write!(formatter, "a package.json")
            }
            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut storage = InsertionOrderMap::with_capacity(map.size_hint().unwrap_or(0));
                let mut name = None;
                let mut scripts = InsertionOrderMap::new();
                let mut dependencies = KeyOrderedMap::new();
                let mut dev_dependencies = KeyOrderedMap::new();
                let mut peer_dependencies = KeyOrderedMap::new();
                let mut optional_dependencies = KeyOrderedMap::new();
                while let Some(key) = map.next_key::<String>()? {
                    let dep_map = match key.as_str() {
                        "name" => {
                            if name.is_some() {
                                return Err(de::Error::duplicate_field("name"));
                            }
                            name = Some(map.next_value()?);
                            storage.insert(key, PkgJsonValue::StoredElsewhere);
                            continue;
                        }
                        "scripts" => {
                            scripts = map.next_value()?;
                            storage.insert(key, PkgJsonValue::StoredElsewhere);
                            continue;
                        }
                        "dependencies" => &mut dependencies,
                        "devDependencies" => &mut dev_dependencies,
                        "peerDependencies" => &mut peer_dependencies,
                        "optionalDependencies" => &mut optional_dependencies,
                        _ => {
                            storage.insert(key, PkgJsonValue::Value(map.next_value()?));
                            continue;
                        }
                    };
                    *dep_map = map.next_value()?;
                    storage.insert(key, PkgJsonValue::StoredElsewhere);
                }

                for &dep_type in &DEPENDENCY_TYPES {
                    if !storage.contains_key(dep_type) {
                        storage.insert(dep_type.to_owned(), PkgJsonValue::StoredElsewhere);
                    }
                }
                Ok(PackageJson {
                    name: name.ok_or_else(|| de::Error::missing_field("name"))?,
                    scripts,
                    dependencies,
                    dev_dependencies,
                    optional_dependencies,
                    peer_dependencies,
                    storage,
                })
            }
        }
        deserializer.deserialize_map(PackageJsonVisitor)
    }
}
