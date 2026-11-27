//! Installed language modules: `<root>\<from>-<to>\<version>\` (the root is
//! `%LOCALAPPDATA%\Pdfrp\models` in the desktop app), and how a module gets there.
//!
//! Installing a module, file by file:
//! 1. the `.zst` file downloads into `<root>\<from>-<to>\<version>.part\<asset>.part`;
//! 2. its size and SHA-256 must be the manifest's (Mozilla's), or it is deleted;
//! 3. it is unpacked (zstd) into `<name>.part`, at most its unpacked size;
//! 4. the unpacked size and SHA-256 must be the manifest's, or it is deleted;
//! 5. it takes its own name and the `.zst` file goes.
//!
//! When every file is there, a marker (`pdfrp-modul.json`) is written and the `.part` folder is
//! renamed to `<version>`; other versions of the direction are removed. Nothing half-done is ever
//! in a `<version>` folder, so the helper never sees it. The helper maps model files into memory,
//! and Windows keeps a mapped file from being deleted or replaced: the caller stops the helper
//! before [`ModelStore::install`] and [`ModelStore::remove`].

use std::io::{BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use migez_update::{FetchError, Transport};
use sha2::{Digest, Sha256};

use crate::manifest::{Direction, Manifest};

/// The marker of a finished module: what was installed, written last.
pub const MARKER: &str = "pdfrp-modul.json";

/// Testing: install from a local folder holding the release's files instead of GitHub.
pub const OVERRIDE_ENV: &str = "MIGEZ_MODELS_DIR";

/// Unpacks a module file: reads the `.zst` from the first argument, writes at most the third
/// argument's bytes to the second, returns how many it wrote.
pub type Unpack = fn(&mut dyn Read, &mut dyn Write, u64) -> Result<u64, String>;

/// zstd, for the release's `.zst` files (ruzstd, pure Rust). It only ever reads a file whose
/// SHA-256 was just checked against the manifest; past `limit` bytes it stops with an error rather
/// than write on, and a file that isn't zstd, or is cut short, is an error too.
pub fn unpack_zstd(input: &mut dyn Read, output: &mut dyn Write, limit: u64) -> Result<u64, String> {
    let mut decoder = ruzstd::StreamingDecoder::new(input).map_err(|e| format!("not a zstd file: {e}"))?;
    let written = std::io::copy(&mut (&mut decoder).take(limit.saturating_add(1)), output).map_err(|e| format!("zstd: {e}"))?;
    if written > limit {
        return Err(format!("it unpacks to more than {limit} bytes"));
    }
    Ok(written)
}

/// Where modules download from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Source {
    /// The manifest's release on GitHub, through `migez_update`'s HTTPS client.
    GitHub,
    /// Testing ([`OVERRIDE_ENV`]): a local folder holding the release's files. Never the network.
    Local(PathBuf),
}

impl Source {
    /// GitHub, unless [`OVERRIDE_ENV`] names a local folder (anything else is refused).
    pub fn from_env() -> Result<Source, FetchError> {
        match std::env::var_os(OVERRIDE_ENV) {
            Some(v) if !v.is_empty() => {
                let dir = PathBuf::from(v);
                if dir.is_absolute() && dir.is_dir() {
                    Ok(Source::Local(dir))
                } else {
                    Err(FetchError::Refused(format!("{OVERRIDE_ENV} must name a local folder")))
                }
            }
            _ => Ok(Source::GitHub),
        }
    }

    /// The transport and the address the files are under. Over HTTPS this is
    /// [`migez_update::github_transport`], the client and User-Agent of the update check.
    pub fn open(&self, manifest: &Manifest, user_agent: &str) -> Result<(Box<dyn Transport>, String), FetchError> {
        match self {
            Source::GitHub => Ok((migez_update::github_transport(user_agent)?, manifest.base_url.clone())),
            Source::Local(dir) => {
                let mut base = dir.display().to_string();
                if !base.ends_with(std::path::MAIN_SEPARATOR) {
                    base.push(std::path::MAIN_SEPARATOR);
                }
                Ok((Box::new(migez_update::FileTransport), base))
            }
        }
    }
}

/// Why a module wasn't installed (or removed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// The download failed.
    Fetch(FetchError),
    /// A file's SHA-256 isn't the manifest's (downloaded or unpacked); it was deleted.
    Mismatch { file: String },
    /// A file's size isn't the manifest's; it was deleted.
    Size { file: String },
    /// The file couldn't be unpacked.
    Unpack { file: String, reason: String },
    /// Reading or writing the modules' folder failed.
    Io(String),
    /// The user cancelled.
    Cancelled,
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Fetch(e) => write!(f, "download: {e}"),
            InstallError::Mismatch { file } => write!(f, "{file}: the SHA-256 doesn't match"),
            InstallError::Size { file } => write!(f, "{file}: the size doesn't match"),
            InstallError::Unpack { file, reason } => write!(f, "{file}: can't unpack: {reason}"),
            InstallError::Io(e) => write!(f, "{e}"),
            InstallError::Cancelled => f.write_str("cancelled"),
        }
    }
}

fn io(e: std::io::Error) -> InstallError {
    InstallError::Io(e.to_string())
}

/// The installed modules under one folder.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModelStore {
    root: PathBuf,
}

/// `%LOCALAPPDATA%\Pdfrp\models` on Windows; the user's data folder elsewhere.
pub fn default_root() -> Option<PathBuf> {
    #[cfg(windows)]
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
    #[cfg(not(windows))]
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("share")));
    base.filter(|b| b.is_absolute()).map(|b| b.join("Pdfrp").join("models"))
}

/// The SHA-256 as lower-case hex.
pub fn hex(digest: &[u8]) -> String {
    digest.iter().fold(String::with_capacity(64), |mut s, b| {
        use std::fmt::Write as _;
        let _ = write!(s, "{b:02x}");
        s
    })
}

/// Writes through to a file while hashing, counting and watching for a cancel.
struct Hashing<'a, W: Write> {
    inner: W,
    hasher: Sha256,
    written: u64,
    progress: Option<&'a AtomicU64>,
    cancel: &'a AtomicBool,
}

impl<W: Write> Write for Hashing<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.cancel.load(Ordering::Relaxed) {
            return Err(std::io::Error::other("cancelled"));
        }
        let n = self.inner.write(buf)?;
        let done = buf.get(..n).unwrap_or_default();
        self.hasher.update(done);
        self.written = self.written.saturating_add(n as u64);
        if let Some(p) = self.progress {
            p.fetch_add(n as u64, Ordering::Relaxed);
        }
        Ok(n)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// A version folder's name: digits and dots.
fn is_version(name: &str) -> bool {
    !name.is_empty() && name.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

impl ModelStore {
    pub fn new(root: impl Into<PathBuf>) -> ModelStore {
        ModelStore { root: root.into() }
    }

    /// The folder the helper is started with.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Where the module is installed.
    pub fn folder(&self, d: &Direction) -> PathBuf {
        self.root.join(d.id()).join(&d.version)
    }

    /// The marker the finished module carries: its direction, version and files.
    fn marker(d: &Direction) -> serde_json::Value {
        serde_json::json!({
            "direction": d.id(),
            "version": d.version,
            "files": d.files.iter().map(|f| serde_json::json!([f.name, f.unpacked_size, f.unpacked_sha256])).collect::<Vec<_>>(),
        })
    }

    /// Whether this version of the module is installed: its marker is the manifest's, and every
    /// file is there with its size. (The SHA-256 was checked when it was installed.)
    pub fn is_installed(&self, d: &Direction) -> bool {
        let folder = self.folder(d);
        let marker = match std::fs::File::open(folder.join(MARKER)) {
            Ok(file) => {
                let mut text = String::new();
                if file.take(64 << 10).read_to_string(&mut text).is_err() {
                    return false;
                }
                text
            }
            Err(_) => return false,
        };
        serde_json::from_str::<serde_json::Value>(&marker).is_ok_and(|m| m == Self::marker(d))
            && d.files.iter().all(|f| std::fs::metadata(folder.join(&f.name)).is_ok_and(|m| m.is_file() && m.len() == f.unpacked_size))
    }

    /// Installs the module (see the module docs). `progress` counts the downloaded bytes; setting
    /// `cancel` stops it, leaving nothing behind. The helper must not be running.
    pub fn install(
        &self,
        d: &Direction,
        source: (&dyn Transport, &str),
        unpack: Unpack,
        progress: &AtomicU64,
        cancel: &AtomicBool,
    ) -> Result<(), InstallError> {
        let parent = self.root.join(d.id());
        let staging = parent.join(format!("{}.part", d.version));
        if staging.exists() {
            std::fs::remove_dir_all(&staging).map_err(io)?;
        }
        std::fs::create_dir_all(&staging).map_err(io)?;
        let result = self.install_into(d, &staging, source, unpack, progress, cancel);
        if result.is_err() {
            let _ = std::fs::remove_dir_all(&staging);
        }
        result
    }

    fn install_into(
        &self,
        d: &Direction,
        staging: &Path,
        (transport, base): (&dyn Transport, &str),
        unpack: Unpack,
        progress: &AtomicU64,
        cancel: &AtomicBool,
    ) -> Result<(), InstallError> {
        for f in &d.files {
            if cancel.load(Ordering::Relaxed) {
                return Err(InstallError::Cancelled);
            }
            // 1-2. Download into a .part file, checking its size and SHA-256.
            let packed = staging.join(format!("{}.part", f.asset));
            let url = format!("{base}{}", f.asset);
            let file = std::fs::File::create(&packed).map_err(io)?;
            let mut sink = Hashing { inner: std::io::BufWriter::new(file), hasher: Sha256::new(), written: 0, progress: Some(progress), cancel };
            let fetched = transport.get(&url, f.size, &mut sink);
            if cancel.load(Ordering::Relaxed) {
                return Err(InstallError::Cancelled);
            }
            fetched.map_err(InstallError::Fetch)?;
            sink.flush().map_err(io)?;
            let (written, digest) = (sink.written, hex(&sink.hasher.finalize()));
            drop(sink.inner);
            if written != f.size {
                let _ = std::fs::remove_file(&packed);
                return Err(InstallError::Size { file: f.asset.clone() });
            }
            if digest != f.sha256 {
                let _ = std::fs::remove_file(&packed);
                return Err(InstallError::Mismatch { file: f.asset.clone() });
            }
            // 3-4. Unpack into a .part file, checking its size and SHA-256.
            let unpacked = staging.join(format!("{}.part", f.name));
            let mut input = BufReader::new(std::fs::File::open(&packed).map_err(io)?);
            let out = std::fs::File::create(&unpacked).map_err(io)?;
            let mut sink = Hashing { inner: std::io::BufWriter::new(out), hasher: Sha256::new(), written: 0, progress: None, cancel };
            let result = unpack(&mut input, &mut sink, f.unpacked_size);
            if cancel.load(Ordering::Relaxed) {
                return Err(InstallError::Cancelled);
            }
            result.map_err(|reason| InstallError::Unpack { file: f.asset.clone(), reason })?;
            sink.flush().map_err(io)?;
            let (written, digest) = (sink.written, hex(&sink.hasher.finalize()));
            drop(sink.inner);
            drop(input);
            if written != f.unpacked_size {
                let _ = std::fs::remove_file(&unpacked);
                return Err(InstallError::Size { file: f.name.clone() });
            }
            if digest != f.unpacked_sha256 {
                let _ = std::fs::remove_file(&unpacked);
                return Err(InstallError::Mismatch { file: f.name.clone() });
            }
            // 5. Its own name; the .zst goes.
            std::fs::rename(&unpacked, staging.join(&f.name)).map_err(io)?;
            std::fs::remove_file(&packed).map_err(io)?;
        }
        let marker = serde_json::to_vec_pretty(&Self::marker(d)).map_err(|e| InstallError::Io(e.to_string()))?;
        std::fs::write(staging.join(MARKER), marker).map_err(io)?;
        // Into place: an older copy of this version goes first (the helper is stopped), then
        // the other versions of this direction.
        let parent = self.root.join(d.id());
        let target = self.folder(d);
        if target.exists() {
            std::fs::remove_dir_all(&target).map_err(io)?;
        }
        std::fs::rename(staging, &target).map_err(io)?;
        for entry in std::fs::read_dir(&parent).map_err(io)?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let other = name != d.version && (is_version(&name) || name.strip_suffix(".part").is_some_and(is_version));
            if other && entry.file_type().is_ok_and(|t| t.is_dir()) {
                let _ = std::fs::remove_dir_all(entry.path());
            }
        }
        Ok(())
    }

    /// Removes every installed version of the module. The helper must not be running.
    pub fn remove(&self, d: &Direction) -> Result<(), InstallError> {
        let folder = self.root.join(d.id());
        match std::fs::remove_dir_all(&folder) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }
}
