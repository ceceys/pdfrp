//! Where the update check reads from: GitHub over HTTPS (the only network the app reaches of its
//! own accord), or, for testing, local files named by `MIGEZ_UPDATE_URL`.

use std::io::{Read, Write};
use std::path::PathBuf;

/// The latest release of the public repository. GitHub's `releases/latest` never answers with a
/// draft or a pre-release.
pub const LATEST_URL: &str = "https://api.github.com/repos/ceceys/pdfrp/releases/latest";

/// Testing only: answer the update check from local files instead of GitHub, never from the
/// network. A path or `file:///` URL names the release JSON (its assets' `browser_download_url`
/// are local paths or `file:///` URLs too); `status:404`, `status:403`, `status:429` (any number)
/// makes every request fail with that HTTP status.
pub const OVERRIDE_ENV: &str = "MIGEZ_UPDATE_URL";

/// The hosts the check and the download may reach, over HTTPS on port 443 only. `github.com` serves
/// a release's download links and redirects them to GitHub's file hosts: `objects.githubusercontent.com`
/// and, for releases uploaded since 2025, `release-assets.githubusercontent.com`.
pub const ALLOWED_HOSTS: [&str; 4] = ["api.github.com", "github.com", "objects.githubusercontent.com", "release-assets.githubusercontent.com"];

/// On `github.com` only this repository's release downloads may be reached (an asset's
/// `browser_download_url`): `https://github.com/ceceys/pdfrp/releases/download/<tag>/<file>`.
/// The file hosts it redirects to keep any path (their addresses are GitHub's own signed links).
pub const DOWNLOAD_PATH: &str = "/ceceys/pdfrp/releases/download/";

/// The most bytes read for the release JSON.
pub const MAX_RELEASE_JSON: u64 = 1 << 20;
/// The most bytes read for SHA256SUMS.txt.
pub const MAX_SUMS: u64 = 64 << 10;
/// The most bytes downloaded for the installer (1.1.0's is about 32 MB).
pub const MAX_INSTALLER: u64 = 512 << 20;

/// Why a request gave no body.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FetchError {
    /// The server answered with this HTTP status (a missing local file reads as 404).
    Status(u16),
    /// No answer: no connection, DNS, TLS, a timeout, or the transfer broke off.
    Network(String),
    /// The body was larger than the limit for it.
    TooLarge,
    /// The address (or a redirect) leaves HTTPS or the allowed hosts, or isn't local in test mode.
    Refused(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Status(code) => write!(f, "HTTP {code}"),
            FetchError::Network(e) => write!(f, "network: {e}"),
            FetchError::TooLarge => f.write_str("the answer is too large"),
            FetchError::Refused(e) => write!(f, "refused: {e}"),
        }
    }
}

/// Reads one address. Implementations: [`FileTransport`] (tests) and, on Windows, `HttpsTransport`.
pub trait Transport: Send + Sync {
    /// Writes the body at `url` to `sink` as it arrives and returns its length. Fails with
    /// [`FetchError::TooLarge`] past `limit` bytes.
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError>;
}

/// Whether `url` is HTTPS to an [allowed host](ALLOWED_HOSTS) on the default port, with no user
/// name, password or port in it, and on `github.com` under [`DOWNLOAD_PATH`]. Checked before every
/// request and every redirect.
pub fn allowed(url: &str) -> Result<(), String> {
    let refuse = || Err(format!("{url} is not an allowed address (HTTPS to GitHub only)"));
    let Some(rest) = url.get(..8).filter(|s| s.eq_ignore_ascii_case("https://")).and_then(|_| url.get(8..)) else {
        return refuse();
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let host = rest.get(..end).unwrap_or_default();
    // No user info, port, percent-escape or backslash (which a URL parser reads as a slash).
    if host.is_empty() || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-') {
        return refuse();
    }
    if !ALLOWED_HOSTS.iter().any(|h| h.eq_ignore_ascii_case(host)) {
        return refuse();
    }
    if host.eq_ignore_ascii_case("github.com") {
        let after = rest.get(end..).unwrap_or_default();
        let path = after.get(..after.find(['?', '#']).unwrap_or(after.len())).unwrap_or_default();
        if !is_download_path(path) {
            return refuse();
        }
    }
    Ok(())
}

/// Whether `path` (an address's path, without its query) names a file under [`DOWNLOAD_PATH`]:
/// plain segments of letters, digits and `-._~` only, so no `.` or `..` segment, percent-escape,
/// backslash, space or control character can lead a URL parser out of it. The owner and the
/// repository are read without regard to case, as GitHub reads them.
fn is_download_path(path: &str) -> bool {
    let Some(rest) = path.get(..DOWNLOAD_PATH.len()).filter(|p| p.eq_ignore_ascii_case(DOWNLOAD_PATH)).and_then(|_| path.get(DOWNLOAD_PATH.len()..))
    else {
        return false;
    };
    !rest.is_empty()
        && rest.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'.' | b'_' | b'~'))
        })
}

/// Copies at most `limit` bytes from `body` to `sink`; [`FetchError::TooLarge`] past it.
pub(crate) fn copy_limited(body: impl Read, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
    let mut body = body.take(limit.saturating_add(1));
    let copied = std::io::copy(&mut body, sink).map_err(|e| FetchError::Network(e.to_string()))?;
    if copied > limit {
        return Err(FetchError::TooLarge);
    }
    Ok(copied)
}

/// Where the check reads, from [`OVERRIDE_ENV`]: GitHub when it is unset or empty.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// [`LATEST_URL`] over HTTPS.
    GitHub,
    /// Testing: the release JSON at this local path or `file:///` URL.
    Local(String),
    /// Testing: every request fails with this HTTP status.
    Status(u16),
}

impl Source {
    /// The source `MIGEZ_UPDATE_URL` names (see [`OVERRIDE_ENV`]). A value that is neither local
    /// nor a status, an `https://` address included, is refused: the test hook never reaches
    /// the network.
    pub fn from_env() -> Result<Source, FetchError> {
        match std::env::var(OVERRIDE_ENV) {
            Ok(v) if !v.trim().is_empty() => Source::parse(v.trim()),
            _ => Ok(Source::GitHub),
        }
    }

    /// [`Source::from_env`] for a given value.
    pub fn parse(value: &str) -> Result<Source, FetchError> {
        if let Some(code) = value.strip_prefix("status:") {
            return code.trim().parse::<u16>().map(Source::Status).map_err(|_| FetchError::Refused(format!("{OVERRIDE_ENV}: {value}")));
        }
        match local_path(value) {
            Some(_) => Ok(Source::Local(value.to_owned())),
            None => Err(FetchError::Refused(format!("{OVERRIDE_ENV} must name a local file: {value}"))),
        }
    }

    /// Whether this is the test hook rather than GitHub.
    pub fn is_override(&self) -> bool {
        *self != Source::GitHub
    }

    /// The release address and the transport that reads it. `user_agent` is the only thing the
    /// HTTPS requests carry besides the address (`Pdfrp/<version>`).
    pub fn open(&self, user_agent: &str) -> Result<(Box<dyn Transport>, String), FetchError> {
        match self {
            Source::GitHub => https(user_agent).map(|t| (t, LATEST_URL.to_owned())),
            Source::Local(url) => Ok((Box::new(FileTransport), url.clone())),
            Source::Status(code) => Ok((Box::new(StatusTransport(*code)), "status:".to_owned())),
        }
    }
}

#[cfg(windows)]
fn https(user_agent: &str) -> Result<Box<dyn Transport>, FetchError> {
    Ok(Box::new(crate::https::HttpsTransport::new(user_agent)?))
}

#[cfg(not(windows))]
fn https(_user_agent: &str) -> Result<Box<dyn Transport>, FetchError> {
    Err(FetchError::Refused("the update check is only built for Windows".into()))
}

/// The local file a test URL names: an absolute path, or a `file:` URL (`file:///C:/a%20b.json`).
pub fn local_path(url: &str) -> Option<PathBuf> {
    let path = match url.get(..5).filter(|s| s.eq_ignore_ascii_case("file:")) {
        Some(_) => {
            let rest = url.get(5..)?;
            let rest = rest.strip_prefix("//").unwrap_or(rest);
            // `file:///C:/x` keeps a slash before the drive letter; `file:///home/x` is absolute as it is.
            let rest = match rest.strip_prefix('/') {
                Some(r) if r.get(1..2) == Some(":") => r,
                _ => rest,
            };
            PathBuf::from(percent_decode(rest)?)
        }
        None => PathBuf::from(url),
    };
    path.is_absolute().then_some(path)
}

/// `%XX` escapes decoded; `None` for a broken escape or text that isn't UTF-8 afterwards.
fn percent_decode(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while let Some(&b) = bytes.get(i) {
        if b == b'%' {
            let hex = text.get(i + 1..i + 3)?;
            out.push(u8::from_str_radix(hex, 16).ok()?);
            i += 3;
        } else {
            out.push(b);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Testing: reads local files ([`local_path`]); a missing file is HTTP 404, and an `https:` or any
/// other address is refused, so nothing reaches the network.
pub struct FileTransport;

impl Transport for FileTransport {
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
        let path = local_path(url).ok_or_else(|| FetchError::Refused(format!("{url} is not a local file ({OVERRIDE_ENV})")))?;
        let file = match std::fs::File::open(&path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Err(FetchError::Status(404)),
            Err(e) => return Err(FetchError::Network(e.to_string())),
        };
        copy_limited(file, limit, sink)
    }
}

/// Testing: every request fails with one HTTP status.
struct StatusTransport(u16);

impl Transport for StatusTransport {
    fn get(&self, _url: &str, _limit: u64, _sink: &mut dyn Write) -> Result<u64, FetchError> {
        Err(FetchError::Status(self.0))
    }
}
