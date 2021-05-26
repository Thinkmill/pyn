use serde::{Deserialize, Serialize, Serializer};
use std::{
    cmp::Ordering,
    convert::TryFrom,
    fmt::{Debug, Display, Formatter},
    hash::{Hash, Hasher},
    num::NonZeroUsize,
    str::FromStr,
};

#[derive(Debug)]
pub struct PackageNameParseError(String);

impl PackageNameParseError {
    pub fn name(self) -> String {
        self.0
    }
}

impl std::error::Error for PackageNameParseError {}

impl Display for PackageNameParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} is not a valid npm package name", self.0)
    }
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct PackageName {
    name: String,
    scoped_name_start: Option<NonZeroUsize>,
}

impl Serialize for PackageName {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.name.serialize(serializer)
    }
}

impl Debug for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PackageName").field(&self.name).finish()
    }
}

impl Display for PackageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl AsRef<str> for PackageName {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl Hash for PackageName {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name.hash(state);
    }
}

impl PartialEq<PackageName> for PackageName {
    fn eq(&self, other: &PackageName) -> bool {
        self.name == other.name
    }
}

impl Eq for PackageName {}

impl PartialOrd<PackageName> for PackageName {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.name.partial_cmp(&other.name)
    }
}

impl Ord for PackageName {
    fn cmp(&self, other: &Self) -> Ordering {
        self.name.cmp(&other.name)
    }
}

pub struct ScopedPackageName<'a> {
    scope: &'a str,
    name: &'a str,
}

impl ScopedPackageName<'_> {
    pub fn scope(&self) -> &str {
        self.scope
    }
    pub fn name(&self) -> &str {
        self.name
    }
}

fn is_valid_package_name_byte(byte: u8) -> bool {
    match byte {
        b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' => true,
        _ => false,
    }
}

fn is_bytes_valid_pkg_name(bytes: &[u8]) -> bool {
    for &byte in bytes {
        if !is_valid_package_name_byte(byte) {
            return false;
        }
    }
    true
}

impl TryFrom<String> for PackageName {
    type Error = PackageNameParseError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        PackageName::new(value.into())
    }
}

impl TryFrom<&'_ str> for PackageName {
    type Error = PackageNameParseError;
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        PackageName::new(value.into())
    }
}

impl FromStr for PackageName {
    type Err = PackageNameParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        PackageName::new(s.into())
    }
}

impl Into<String> for PackageName {
    fn into(self) -> String {
        self.name
    }
}

#[test]
fn keystone_monorepo_name() {
    PackageName::new("@keystone-next/mono-repo".to_owned()).unwrap();
}

impl PackageName {
    pub fn as_str(&self) -> &str {
        self.name.as_ref()
    }
    pub fn is_scoped(&self) -> bool {
        self.scoped_name_start.is_some()
    }
    pub fn scoped(&self) -> Option<ScopedPackageName<'_>> {
        match self.scoped_name_start {
            Some(pos) => Some(ScopedPackageName {
                name: &self.as_str()[1..pos.get() - 1],
                scope: &self.as_str()[pos.get()..],
            }),
            None => None,
        }
    }
    pub fn new(name: String) -> Result<PackageName, PackageNameParseError> {
        if name.len() > 214 || name.len() == 0 {
            return Err(PackageNameParseError(name));
        }
        let bytes = name.as_bytes();
        match bytes[0] {
            b'.' | b'_' => return Err(PackageNameParseError(name)),
            b'@' => {
                for (pos, &byte) in bytes[1..].iter().enumerate() {
                    match byte {
                        b'/' => {
                            let rest = &bytes[pos + 2..];
                            return match is_bytes_valid_pkg_name(rest) {
                                true => Ok(PackageName {
                                    name,
                                    scoped_name_start: Some(NonZeroUsize::new(pos + 1).unwrap()),
                                }),
                                false => Err(PackageNameParseError(name)),
                            };
                        }
                        byte if is_valid_package_name_byte(byte) => {}
                        _ => return Err(PackageNameParseError(name)),
                    }
                }
                return Err(PackageNameParseError(name));
            }
            _ => {
                if !is_bytes_valid_pkg_name(bytes) {
                    return Err(PackageNameParseError(name));
                }
            }
        }

        if INVALID_NAMES.contains(&name.as_str()) {
            return Err(PackageNameParseError(name));
        }
        Ok(PackageName {
            name,
            scoped_name_start: None,
        })
    }
}

const INVALID_NAMES: &[&str] = &[
    "node_modules",
    "favicon.ico",
    "assert",
    "async_hooks",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "inspector",
    "module",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "timers",
    "tls",
    "trace_events",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "wasi",
    "worker_threads",
    "zlib",
];
