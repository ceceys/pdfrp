//! Downloading a newer release's installer to %TEMP%, checking its SHA-256 against the release's
//! SHA256SUMS.txt, and starting it with its wizard showing (never a silent install).

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::fetch::{FetchError, MAX_INSTALLER, MAX_SUMS, Transport};
use crate::release::{Release, expected_sha256, installer_name};

/// The installer's switches for an update: no restart of Windows, and `/MIGEZYENIDENAC=1`, which
/// tells the installer the update was started from Pdfrp: it skips the pages an update doesn't
/// need, and its last page starts Pdfrp again (`kurulum/pdfrp.iss`). Never `/SILENT`,
/// `/VERYSILENT` or `/SUPPRESSMSGBOXES`: the wizard shows, the user starts the installation on its
/// "Ready to Install" page, and it asks before it closes a program that is still running.
pub const INSTALLER_ARGS: [&str; 2] = ["/NORESTART", "/MIGEZYENIDENAC=1"];

/// Why an update couldn't be made ready.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InstallError {
    /// SHA256SUMS.txt or the installer couldn't be fetched.
    Fetch(FetchError),
    /// SHA256SUMS.txt lists no SHA-256 for the installer (or two different ones).
    NotListed(String),
    /// The downloaded installer isn't the file SHA256SUMS.txt (or GitHub's digest) describes.
    /// It is deleted and never started.
    Mismatch { expected: String, actual: String },
    /// The download is shorter or longer than the release says.
    Size { expected: u64, actual: u64 },
    /// The file couldn't be written or read.
    Io(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::Fetch(e) => write!(f, "{e}"),
            InstallError::NotListed(name) => write!(f, "SHA256SUMS.txt lists no SHA-256 for {name}"),
            InstallError::Mismatch { expected, actual } => write!(f, "SHA-256 mismatch: expected {expected}, got {actual}"),
            InstallError::Size { expected, actual } => write!(f, "size mismatch: expected {expected} bytes, got {actual}"),
            InstallError::Io(e) => write!(f, "{e}"),
        }
    }
}

impl From<FetchError> for InstallError {
    fn from(e: FetchError) -> InstallError {
        InstallError::Fetch(e)
    }
}

/// A downloaded installer whose SHA-256 matched. On Windows the file stays open, shared for
/// reading only, from the check until it is started: nothing can change it in between.
#[derive(Debug)]
pub struct Verified {
    pub path: PathBuf,
    pub sha256: String,
    /// The open, read-only handle that keeps the file as it was checked.
    _file: File,
}

impl Verified {
    /// Starts the installer with `args` and lets go of the file.
    pub fn launch(self, args: &[String]) -> std::io::Result<()> {
        std::process::Command::new(&self.path).args(args).spawn()?;
        Ok(())
    }
}

/// The SHA-256 of what `reader` holds, in lowercase hex.
pub fn sha256_hex(mut reader: impl Read) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(buf.get(..n).unwrap_or_default());
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// How a download folder's name starts: `pdfrp-guncelleme-<pid>-<8 hex>-<attempt>`.
pub const DOWNLOAD_PREFIX: &str = "pdfrp-guncelleme-";

/// Whether `name` is exactly how [`fresh_folder`] names a download folder.
pub fn is_download_folder(name: &str) -> bool {
    let Some(rest) = name.strip_prefix(DOWNLOAD_PREFIX) else { return false };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    match rest.split('-').collect::<Vec<_>>()[..] {
        [pid, stamp, attempt] => digits(pid) && stamp.len() == 8 && stamp.bytes().all(|b| b.is_ascii_hexdigit()) && digits(attempt),
        _ => false,
    }
}

/// Whether `name` is an installer as [`installer_name`] names one: `Pdfrp Kurulum 1.2.0.exe`.
fn is_installer_file(name: &str) -> bool {
    name.strip_prefix("Pdfrp Kurulum ").and_then(|rest| rest.strip_suffix(".exe")).and_then(crate::Version::parse).is_some()
}

/// Removes what earlier updates left under `parent` (%TEMP%) and returns how many folders went.
/// Only folders named as [`fresh_folder`] names them ([`is_download_folder`]), real folders (not
/// links), holding nothing but installers ([`installer_name`]), with neither the folder nor a file
/// in it changed for `older_than`: a download under way in another copy of the app, or an
/// installer still running, is left alone. A file that can't be removed (in use) keeps its folder.
pub fn clean_old_downloads(parent: &Path, older_than: std::time::Duration) -> usize {
    let old = |meta: &std::fs::Metadata| meta.modified().ok().and_then(|t| t.elapsed().ok()).is_some_and(|age| age >= older_than);
    let Ok(entries) = std::fs::read_dir(parent) else { return 0 };
    let mut removed = 0;
    'folders: for entry in entries.flatten() {
        if !entry.file_name().to_str().is_some_and(is_download_folder) {
            continue;
        }
        let dir = entry.path();
        // Not followed through a link or junction: only a real folder of ours.
        let Ok(meta) = std::fs::symlink_metadata(&dir) else { continue };
        if !meta.is_dir() || !old(&meta) {
            continue;
        }
        let Ok(files) = std::fs::read_dir(&dir) else { continue };
        let mut installers = Vec::new();
        for file in files {
            let Ok(file) = file else { continue 'folders };
            let ours = file.file_name().to_str().is_some_and(is_installer_file);
            let Ok(meta) = std::fs::symlink_metadata(file.path()) else { continue 'folders };
            if !ours || !meta.is_file() || !old(&meta) {
                continue 'folders;
            }
            installers.push(file.path());
        }
        for file in &installers {
            if std::fs::remove_file(file).is_err() {
                continue 'folders;
            }
        }
        if std::fs::remove_dir(&dir).is_ok() {
            removed += 1;
        }
    }
    removed
}

/// A new folder of our own under `parent` (%TEMP%): never one that already exists, so nothing
/// placed there beforehand is used.
fn fresh_folder(parent: &Path) -> Result<PathBuf, InstallError> {
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let mut last = String::new();
    for attempt in 0..16u32 {
        let dir = parent.join(format!("{DOWNLOAD_PREFIX}{}-{nanos:08x}-{attempt}", std::process::id()));
        match std::fs::create_dir(&dir) {
            Ok(()) => return Ok(dir),
            Err(e) => last = e.to_string(),
        }
    }
    Err(InstallError::Io(format!("{}: {last}", parent.display())))
}

/// Opens `path` for reading, shared for reading only on Windows: while it is open no one can
/// write, rename or delete the file.
pub(crate) fn open_checked(path: &Path) -> std::io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt as _;
        /// `FILE_SHARE_READ`.
        const SHARE_READ: u32 = 0x1;
        OpenOptions::new().read(true).share_mode(SHARE_READ).open(path)
    }
    #[cfg(not(windows))]
    {
        File::open(path)
    }
}

/// Counts what passes through, for the download's progress.
struct Counting<'a, W: Write> {
    inner: W,
    done: &'a AtomicU64,
}

impl<W: Write> Write for Counting<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.done.fetch_add(n as u64, Ordering::Relaxed);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Downloads `release`'s installer into a new folder under `parent` (the app passes %TEMP%) and
/// checks it: its SHA-256 must be the one SHA256SUMS.txt gives for it, and GitHub's own digest of
/// the asset when GitHub gives one, and its size the release's. `done` counts the bytes received.
/// A file that fails is deleted.
pub fn download(transport: &dyn Transport, release: &Release, parent: &Path, done: &AtomicU64) -> Result<Verified, InstallError> {
    let name = installer_name(&release.version);
    let mut sums = Vec::new();
    transport.get(&release.sums.url, MAX_SUMS, &mut sums)?;
    let expected = expected_sha256(&String::from_utf8_lossy(&sums), &name).ok_or_else(|| InstallError::NotListed(name.clone()))?;
    if let Some(github) = &release.installer.sha256
        && *github != expected
    {
        return Err(InstallError::Mismatch { expected, actual: github.clone() });
    }
    let dir = fresh_folder(parent)?;
    let path = dir.join(&name);
    let result = fetch_and_check(transport, release, &path, &expected, done);
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }
    result
}

fn fetch_and_check(transport: &dyn Transport, release: &Release, path: &Path, expected: &str, done: &AtomicU64) -> Result<Verified, InstallError> {
    let io = |e: std::io::Error| InstallError::Io(format!("{}: {e}", path.display()));
    {
        let file = OpenOptions::new().write(true).create_new(true).open(path).map_err(io)?;
        let mut sink = Counting { inner: BufWriter::new(file), done };
        let written = transport.get(&release.installer.url, MAX_INSTALLER, &mut sink)?;
        sink.flush().map_err(io)?;
        let file = sink.inner.into_inner().map_err(|e| io(e.into_error()))?;
        file.sync_all().map_err(io)?;
        if release.installer.size > 0 && written != release.installer.size {
            return Err(InstallError::Size { expected: release.installer.size, actual: written });
        }
    }
    // Read back through the handle that stays open until the installer starts.
    let file = open_checked(path).map_err(io)?;
    let actual = sha256_hex(&file).map_err(io)?;
    if actual != expected {
        drop(file);
        return Err(InstallError::Mismatch { expected: expected.to_owned(), actual });
    }
    Ok(Verified { path: path.to_owned(), sha256: actual, _file: file })
}

/// The installer's switches for the copy at `exe`: [`INSTALLER_ARGS`], and `/CURRENTUSER` for a
/// copy installed for the current user only (under `local_app_data`, %LOCALAPPDATA%) or
/// `/ALLUSERS` for one under `program_files` (%ProgramFiles%), so the update replaces that copy
/// instead of installing a second one. Anywhere else the installer asks as it would.
pub fn installer_args(exe: &Path, local_app_data: Option<&Path>, program_files: Option<&Path>) -> Vec<String> {
    let mut args: Vec<String> = INSTALLER_ARGS.iter().map(|s| (*s).to_owned()).collect();
    let under = |root: Option<&Path>| {
        root.is_some_and(|root| {
            let (exe, root) = (exe.to_string_lossy().to_lowercase(), root.to_string_lossy().to_lowercase());
            let root = root.trim_end_matches(['\\', '/']);
            !root.is_empty() && exe.strip_prefix(root).is_some_and(|rest| rest.starts_with(['\\', '/']))
        })
    };
    if under(local_app_data) {
        args.push("/CURRENTUSER".into());
    } else if under(program_files) {
        args.push("/ALLUSERS".into());
    }
    args
}
