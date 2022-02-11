use serde::{Deserialize, Serialize, Serializer};
use std::{
    cmp::Ordering,
    convert::TryFrom,
    fmt::{Debug, Display, Formatter},
    hash::{Hash, Hasher},
    str::FromStr,
};

#[derive(Debug)]
pub struct PackageNameParseError(String);

impl std::error::Error for PackageNameParseError {}

impl Display for PackageNameParseError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "\"{}\" is not a valid npm package name", self.0)
    }
}

#[derive(Clone, Deserialize)]
#[serde(try_from = "String")]
pub struct PackageName {
    name: String,
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

fn is_valid_package_name_byte(byte: u8) -> bool {
    match byte {
        b'a'..=b'z' | b'0'..=b'9' | b'_' | b'-' | b'.' => true,
        _ => false,
    }
}

fn is_bytes_valid_pkg_name(bytes: &[u8]) -> bool {
    bytes.iter().all(|&byte| is_valid_package_name_byte(byte))
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
                                true => Ok(PackageName { name }),
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

        Ok(PackageName { name })
    }
}
