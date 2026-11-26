//! The release GitHub answers with, the two assets an update needs, and SHA256SUMS.txt.

use crate::fetch::{FetchError, MAX_RELEASE_JSON, Transport};
use crate::version::Version;

/// The checksum list every release carries beside the installer (`sha256sum` format).
pub const SUMS_NAME: &str = "SHA256SUMS.txt";

/// The installer's name in a release: `Pdfrp Kurulum 1.2.0.exe`. GitHub stores an uploaded
/// asset's name with its spaces as dots (`Pdfrp.Kurulum.1.2.0.exe`); [`same_name`] takes both.
pub fn installer_name(version: &Version) -> String {
    format!("Pdfrp Kurulum {version}.exe")
}

/// Whether two asset names are the same file: equal once spaces are read as dots (GitHub renames
/// them so on upload), letter case aside.
pub fn same_name(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim().replace(' ', ".").to_ascii_lowercase();
    norm(a) == norm(b)
}

/// One downloadable file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// `browser_download_url`.
    pub url: String,
    /// The size GitHub gives, 0 when it gives none.
    pub size: u64,
    /// GitHub's own `sha256:<hex>` digest of the asset, when it gives one (lowercase hex).
    pub sha256: Option<String>,
}

/// A newer release, with the installer and the checksum list the update needs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    /// `tag_name`, `v1.2.0`.
    pub tag: String,
    /// The release's web page (`html_url`).
    pub page: String,
    pub installer: Asset,
    pub sums: Asset,
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// The running version is the latest (or newer): the latest release's version.
    UpToDate(Version),
    /// A newer release.
    Newer(Release),
}

/// Why a check found nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckError {
    /// No connection to GitHub (offline, DNS, TLS, a timeout).
    Offline(String),
    /// GitHub turned the request down for now: HTTP 403 or 429 (its rate limit).
    RateLimited,
    /// HTTP 404: the repository has no published release.
    NotFound,
    /// Another HTTP status.
    Status(u16),
    /// The answer isn't a release the app can use (bad JSON, no installer, no SHA256SUMS.txt).
    Invalid(String),
    /// An address outside HTTPS to GitHub.
    Refused(String),
}

impl From<FetchError> for CheckError {
    fn from(e: FetchError) -> CheckError {
        match e {
            FetchError::Status(403 | 429) => CheckError::RateLimited,
            FetchError::Status(404) => CheckError::NotFound,
            FetchError::Status(code) => CheckError::Status(code),
            FetchError::Network(e) => CheckError::Offline(e),
            FetchError::TooLarge => CheckError::Invalid("the answer is too large".into()),
            FetchError::Refused(e) => CheckError::Refused(e),
        }
    }
}

impl std::fmt::Display for CheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CheckError::Offline(e) => write!(f, "no connection to GitHub: {e}"),
            CheckError::RateLimited => f.write_str("GitHub refused the request for now (HTTP 403/429)"),
            CheckError::NotFound => f.write_str("no published release (HTTP 404)"),
            CheckError::Status(code) => write!(f, "HTTP {code}"),
            CheckError::Invalid(e) => write!(f, "unusable release: {e}"),
            CheckError::Refused(e) => write!(f, "{e}"),
        }
    }
}

/// What the release JSON says, before deciding whether it is newer.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Latest {
    pub version: Version,
    pub tag: String,
    pub page: String,
    pub draft: bool,
    pub pre_release: bool,
    pub assets: Vec<Asset>,
}

/// The longest string field kept from the JSON (names, URLs).
const MAX_FIELD: usize = 2048;
/// The most assets read.
const MAX_ASSETS: usize = 64;

/// Reads GitHub's release JSON (`GET /repos/{owner}/{repo}/releases/latest`).
pub fn parse_latest(json: &[u8]) -> Result<Latest, CheckError> {
    let value: serde_json::Value = serde_json::from_slice(json).map_err(|e| CheckError::Invalid(format!("not JSON: {e}")))?;
    let text = |v: &serde_json::Value, key: &str| v.get(key).and_then(serde_json::Value::as_str).filter(|s| s.len() <= MAX_FIELD).map(str::to_owned);
    let tag = text(&value, "tag_name").ok_or_else(|| CheckError::Invalid("no tag_name".into()))?;
    let version = Version::parse(&tag).ok_or_else(|| CheckError::Invalid(format!("tag {tag:?} is not a version like v1.2.3")))?;
    let assets = value
        .get("assets")
        .and_then(serde_json::Value::as_array)
        .map(|list| {
            list.iter()
                .take(MAX_ASSETS)
                .filter_map(|a| {
                    let digest = text(a, "digest").and_then(|d| d.strip_prefix("sha256:").map(str::to_ascii_lowercase)).filter(|h| is_sha256(h));
                    Some(Asset {
                        name: text(a, "name")?,
                        url: text(a, "browser_download_url")?,
                        size: a.get("size").and_then(serde_json::Value::as_u64).unwrap_or(0),
                        sha256: digest,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Latest {
        version,
        tag,
        page: text(&value, "html_url").unwrap_or_default(),
        draft: value.get("draft").and_then(serde_json::Value::as_bool).unwrap_or(false),
        pre_release: value.get("prerelease").and_then(serde_json::Value::as_bool).unwrap_or(false),
        assets,
    })
}

/// Asks `url` (normally [`crate::LATEST_URL`]) for the latest release and compares it with
/// `current`. A newer release must carry its installer ([`installer_name`]) and SHA256SUMS.txt;
/// a draft or pre-release is never offered.
pub fn check(transport: &dyn Transport, url: &str, current: &Version) -> Result<Outcome, CheckError> {
    let mut body = Vec::new();
    transport.get(url, MAX_RELEASE_JSON, &mut body)?;
    let latest = parse_latest(&body)?;
    if latest.draft || latest.pre_release || latest.version.is_pre_release() || latest.version <= *current {
        return Ok(Outcome::UpToDate(latest.version));
    }
    let wanted = installer_name(&latest.version);
    let find = |name: &str| latest.assets.iter().find(|a| same_name(&a.name, name)).cloned();
    let installer = find(&wanted).ok_or_else(|| CheckError::Invalid(format!("release {} has no {wanted}", latest.tag)))?;
    let sums = find(SUMS_NAME).ok_or_else(|| CheckError::Invalid(format!("release {} has no {SUMS_NAME}", latest.tag)))?;
    Ok(Outcome::Newer(Release { version: latest.version, tag: latest.tag, page: latest.page, installer, sums }))
}

/// Whether `hex` is a SHA-256 in lowercase hex.
pub(crate) fn is_sha256(hex: &str) -> bool {
    hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// The entries of a `sha256sum` list: `<hex>  <name>` per line (`*<name>` in binary mode), the
/// hex in lowercase. Blank lines, `#` comments and malformed lines are skipped.
pub fn parse_sums(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (hash, name) = line.split_once(char::is_whitespace)?;
            let hash = hash.to_ascii_lowercase();
            let name = name.trim_start();
            let name = name.strip_prefix('*').unwrap_or(name).trim_end();
            (is_sha256(&hash) && !name.is_empty()).then(|| (hash, name.to_owned()))
        })
        .collect()
}

/// The SHA-256 SHA256SUMS.txt gives `name` ([`same_name`]); `None` when it lists none, or lists
/// two different ones.
pub fn expected_sha256(sums: &str, name: &str) -> Option<String> {
    let mut found: Option<String> = None;
    for (hash, listed) in parse_sums(sums) {
        if !same_name(&listed, name) {
            continue;
        }
        match &found {
            Some(other) if *other != hash => return None,
            _ => found = Some(hash),
        }
    }
    found
}
