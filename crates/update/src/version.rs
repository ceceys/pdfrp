//! Semantic versions (semver.org 2.0.0): `1.2.3`, `v1.2.3`, `1.2.3-rc.1`; build metadata (`+…`)
//! is read and ignored, as semver orders it.

use std::cmp::Ordering;
use std::fmt;

/// The longest version text read. Release tags come from the network.
const MAX_LEN: usize = 64;

/// A version: three numbers and the pre-release identifiers, if any.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    /// Empty for a release; `rc.1` is `[Text("rc"), Number(1)]`.
    pub pre: Vec<Identifier>,
}

/// One dot-separated pre-release identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Identifier {
    Number(u64),
    Text(String),
}

impl Version {
    /// A release version (no pre-release part).
    pub fn new(major: u64, minor: u64, patch: u64) -> Version {
        Version { major, minor, patch, pre: Vec::new() }
    }

    /// Reads `1.2.3`, with an optional leading `v` (release tags are `v1.2.3`), an optional
    /// `-pre.release` part and optional `+build` metadata. `None` for anything else: two or four
    /// numbers, leading zeros, empty parts, other characters, or more than 64 characters.
    pub fn parse(text: &str) -> Option<Version> {
        let text = text.trim();
        if text.is_empty() || text.len() > MAX_LEN {
            return None;
        }
        let text = text.strip_prefix(['v', 'V']).unwrap_or(text);
        let text = match text.split_once('+') {
            Some((version, build)) => {
                if build.is_empty() || !build.split('.').all(|part| !part.is_empty() && part.bytes().all(identifier_byte)) {
                    return None;
                }
                version
            }
            None => text,
        };
        let (core, pre) = match text.split_once('-') {
            Some((core, pre)) => (core, Some(pre)),
            None => (text, None),
        };
        let mut numbers = core.split('.');
        let major = number(numbers.next()?)?;
        let minor = number(numbers.next()?)?;
        let patch = number(numbers.next()?)?;
        if numbers.next().is_some() {
            return None;
        }
        let pre = match pre {
            None => Vec::new(),
            Some(pre) => pre.split('.').map(identifier).collect::<Option<Vec<_>>>()?,
        };
        Some(Version { major, minor, patch, pre })
    }

    /// Whether this is a pre-release (`1.2.0-rc.1`), which the update check never offers.
    pub fn is_pre_release(&self) -> bool {
        !self.pre.is_empty()
    }
}

fn identifier_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

/// A numeric part: digits, no leading zero.
fn number(text: &str) -> Option<u64> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) || (text.len() > 1 && text.starts_with('0')) {
        return None;
    }
    text.parse().ok()
}

fn identifier(text: &str) -> Option<Identifier> {
    if text.is_empty() || !text.bytes().all(identifier_byte) {
        return None;
    }
    if text.bytes().all(|b| b.is_ascii_digit()) {
        return number(text).map(Identifier::Number);
    }
    Some(Identifier::Text(text.to_owned()))
}

impl Ord for Version {
    /// Semver precedence (§11): the three numbers, then a pre-release below its release, then the
    /// identifiers one by one (numbers below text, numbers by value, text in ASCII order), then
    /// the shorter list first.
    fn cmp(&self, other: &Self) -> Ordering {
        (self.major, self.minor, self.patch).cmp(&(other.major, other.minor, other.patch)).then_with(|| {
            match (self.pre.is_empty(), other.pre.is_empty()) {
                (true, true) => Ordering::Equal,
                (true, false) => Ordering::Greater,
                (false, true) => Ordering::Less,
                (false, false) => {
                    for (a, b) in self.pre.iter().zip(&other.pre) {
                        let order = match (a, b) {
                            (Identifier::Number(a), Identifier::Number(b)) => a.cmp(b),
                            (Identifier::Number(_), Identifier::Text(_)) => Ordering::Less,
                            (Identifier::Text(_), Identifier::Number(_)) => Ordering::Greater,
                            (Identifier::Text(a), Identifier::Text(b)) => a.as_bytes().cmp(b.as_bytes()),
                        };
                        if order != Ordering::Equal {
                            return order;
                        }
                    }
                    self.pre.len().cmp(&other.pre.len())
                }
            }
        })
    }
}

impl PartialOrd for Version {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)?;
        for (i, part) in self.pre.iter().enumerate() {
            f.write_str(if i == 0 { "-" } else { "." })?;
            match part {
                Identifier::Number(n) => write!(f, "{n}")?,
                Identifier::Text(t) => f.write_str(t)?,
            }
        }
        Ok(())
    }
}
