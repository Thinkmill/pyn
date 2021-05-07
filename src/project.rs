use crate::PackageManager;
use linked_hash_map::LinkedHashMap;
pub use package_name::PackageName;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
    path::Path,
};
mod package_name;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PackageJson {
    name: PackageName,
    version: String,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    dependencies: HashMap<PackageName, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    dev_dependencies: HashMap<PackageName, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    optional_dependencies: HashMap<PackageName, String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    peer_dependencies: HashMap<PackageName, String>,
    #[serde(flatten)]
    other: HashMap<String, Value>,
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
