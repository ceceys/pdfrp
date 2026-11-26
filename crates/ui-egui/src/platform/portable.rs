//! Portable mode (#157): a marker file beside the executable ([`MARKERS`]) keeps the settings,
//! log files, crash-recovery autosaves and new digital IDs in `<exe dir>/Pdfrp Data`, so a
//! portable copy (on a USB stick, say) writes nothing to `%APPDATA%`. In `%LOCALAPPDATA%` it keeps
//! only its single-instance socket and lock, two empty files in `%LOCALAPPDATA%\Pdfrp` that
//! must stay private to the user (#889, `apps/migez-pdf/src/single_instance.rs`). The Windows
//! portable zip ships with `portable.txt`; the MSI doesn't. PhotoCraft uses the same scheme
//! (`photocraft/apps/photocraft/src/app_dirs.rs`).
//!
//! If that folder can't be written (read-only medium, Program Files), portable mode stays off, the
//! per-user folders are used and the app says why.
//!
//! 1.1.0: the program was called Mig-EZ Pdf up to 1.0.1. A portable copy of it ([`FORMER_MARKER`],
//! [`FORMER_DATA_DIR`]) stays portable, and its data folder is copied to [`DATA_DIR`] once, the
//! first time the new copy starts there; the former folder is left as it was ([`copy_folder`]).

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Files beside the executable that switch on portable mode (any one; contents are ignored). The
/// last is the marker of the program's former name ([`FORMER_MARKER`]).
pub const MARKERS: [&str; 3] = ["portable.txt", "Pdfrp.portable", FORMER_MARKER];
/// The data folder created beside the executable in portable mode.
pub const DATA_DIR: &str = "Pdfrp Data";
/// The portable marker of the program's former name (Mig-EZ Pdf, up to 1.0.1).
pub const FORMER_MARKER: &str = "Mig-EZ Pdf.portable";
/// The portable data folder of the program's former name (Mig-EZ Pdf, up to 1.0.1): copied to
/// [`DATA_DIR`] once, never changed or removed.
pub const FORMER_DATA_DIR: &str = "Mig-EZ Pdf Data";

/// The outcome of looking for a portable marker.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Portable {
    /// `<exe dir>/Pdfrp Data` when portable mode is on, `None` otherwise.
    pub dir: Option<PathBuf>,
    /// Set when a marker was found but its data folder can't be written.
    pub unwritable: Option<Unwritable>,
}

/// A marker was found, but the data folder beside it can't be created or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Unwritable {
    pub marker: PathBuf,
    pub folder: PathBuf,
    /// The operating system's error text.
    pub error: String,
}

impl std::fmt::Display for Unwritable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "portable mode is off: {} was found, but {} can't be written ({}); settings are kept in the user folder instead",
            self.marker.display(),
            self.folder.display(),
            self.error
        )
    }
}

/// Portable mode for this process, resolved once (the check probes the disk).
pub fn current() -> &'static Portable {
    static CURRENT: OnceLock<Portable> = OnceLock::new();
    CURRENT.get_or_init(|| {
        let exe = std::env::current_exe().ok();
        let portable = resolve(exe.as_deref().and_then(Path::parent));
        if let Some(dir) = &portable.dir {
            log::info!("portable mode: settings are kept in {}", dir.display());
        }
        if let Some(unwritable) = &portable.unwritable {
            log::warn!("{unwritable}");
        }
        portable
    })
}

/// The portable data folder, when portable mode is on.
pub fn data_dir() -> Option<&'static Path> {
    current().dir.as_deref()
}

/// Look for a marker in `exe_dir` and, when one is there, create its data folder beside it.
pub fn resolve(exe_dir: Option<&Path>) -> Portable {
    let Some((exe_dir, marker)) = exe_dir.and_then(|d| find_marker(d).map(|m| (d, m))) else {
        return Portable::default();
    };
    let folder = exe_dir.join(DATA_DIR);
    let former = exe_dir.join(FORMER_DATA_DIR);
    if !folder.exists() && former.is_dir() {
        match copy_folder(&former, &folder, &[]) {
            Ok(files) => log::info!("portable mode: copied {} ({files} files) to {}", former.display(), folder.display()),
            Err(e) => log::warn!("portable mode: copying {} to {}: {e}", former.display(), folder.display()),
        }
    }
    match ensure_writable(&folder) {
        Ok(()) => Portable { dir: Some(folder), unwritable: None },
        Err(e) => Portable { dir: None, unwritable: Some(Unwritable { marker, folder, error: e.to_string() }) },
    }
}

/// The first marker file in `exe_dir`, if any. A folder with a marker's name is not a marker.
pub fn find_marker(exe_dir: &Path) -> Option<PathBuf> {
    MARKERS.iter().map(|m| exe_dir.join(m)).find(|p| p.is_file())
}

/// Copies the folder `from` to `to`, which must not exist yet, and returns how many files went.
/// Used once, when the program's former name (Mig-EZ Pdf) left settings, recent files and unsaved
/// work behind: `from` is only read, never changed or removed. Folders and ordinary files are
/// copied; links, junctions and other reparse points are skipped, and so are the running copy's
/// single-instance files (`*.sock`, `*.lock`: a running former copy holds its lock, and neither is
/// data) and the entries named in `skip` at the top level. The copy is made beside `to` under a
/// temporary name and renamed into place at the end, so an interrupted copy leaves no half-made
/// `to` behind; when another copy of the app got there first, theirs stays and this one goes.
pub fn copy_folder(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<usize> {
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, format!("{} already exists", to.display())));
    }
    let name = to.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let staging = to.with_file_name(format!(".{name}.copying-{}", std::process::id()));
    if let Some(parent) = staging.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let _ = std::fs::remove_dir_all(&staging);
    let copied = copy_tree(from, &staging, skip).and_then(|files| std::fs::rename(&staging, to).map(|()| files));
    if copied.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    copied
}

/// Whether `meta` is a link, junction or other reparse point (on Windows, an AF_UNIX socket is one).
fn is_reparse_point(meta: &std::fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt as _;
        /// `FILE_ATTRIBUTE_REPARSE_POINT`.
        const REPARSE_POINT: u32 = 0x400;
        meta.file_attributes() & REPARSE_POINT != 0 || meta.file_type().is_symlink()
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}

/// [`copy_folder`]'s walk: `to` is created, `skip` applies to `from`'s own entries only.
fn copy_tree(from: &Path, to: &Path, skip: &[&str]) -> std::io::Result<usize> {
    std::fs::create_dir(to)?;
    let mut files = 0;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        let name = entry.file_name();
        if name.to_str().is_some_and(|n| skip.iter().any(|s| s.eq_ignore_ascii_case(n))) {
            continue;
        }
        let meta = std::fs::symlink_metadata(entry.path())?;
        if is_reparse_point(&meta) {
            continue;
        }
        let runtime = Path::new(&name).extension().and_then(|e| e.to_str()).is_some_and(|e| e.eq_ignore_ascii_case("sock") || e.eq_ignore_ascii_case("lock"));
        if meta.is_dir() {
            files += copy_tree(&entry.path(), &to.join(&name), &[])?;
        } else if meta.is_file() && !runtime {
            std::fs::copy(entry.path(), to.join(&name))?;
            files += 1;
        }
    }
    Ok(files)
}

/// Create `dir` and prove a file can be written in it (a folder can exist yet be read-only).
fn ensure_writable(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let probe = dir.join(".write-test");
    std::fs::write(&probe, b"")?;
    // A leftover probe is harmless; don't fail portable mode over it.
    let _ = std::fs::remove_file(&probe);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn temp(tag: &str) -> PathBuf {
        static N: AtomicUsize = AtomicUsize::new(0);
        let d = std::env::temp_dir().join(format!("migez-portable-{tag}-{}-{}", std::process::id(), N.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn no_marker_leaves_portable_mode_off() {
        let exe = temp("off");
        assert_eq!(resolve(Some(&exe)), Portable::default());
        assert!(!exe.join(DATA_DIR).exists(), "no data folder without a marker");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn either_marker_keeps_the_data_beside_the_exe() {
        for marker in MARKERS {
            let exe = temp("on");
            std::fs::write(exe.join(marker), b"").unwrap();
            let p = resolve(Some(&exe));
            assert_eq!(p.unwritable, None, "{marker}");
            let dir = p.dir.unwrap();
            assert_eq!(dir, exe.join(DATA_DIR));
            assert!(dir.is_dir(), "the data folder is created");
            assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "the write probe is removed");
            let _ = std::fs::remove_dir_all(&exe);
        }
    }

    #[test]
    fn an_existing_data_folder_is_reused() {
        let exe = temp("reuse");
        std::fs::write(exe.join("portable.txt"), b"Pdfrp portable mode").unwrap();
        std::fs::create_dir_all(exe.join(DATA_DIR)).unwrap();
        std::fs::write(exe.join(DATA_DIR).join("app.ron"), b"()").unwrap();
        assert_eq!(resolve(Some(&exe)).dir, Some(exe.join(DATA_DIR)));
        assert_eq!(std::fs::read(exe.join(DATA_DIR).join("app.ron")).unwrap(), b"()", "existing settings are kept");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn a_marker_folder_is_not_a_marker() {
        let exe = temp("dir-marker");
        std::fs::create_dir_all(exe.join("portable.txt")).unwrap();
        assert_eq!(resolve(Some(&exe)), Portable::default());
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn unwritable_data_folder_turns_portable_mode_off_and_says_why() {
        let exe = temp("unwritable");
        std::fs::write(exe.join("portable.txt"), b"").unwrap();
        // A file where the data folder should go: it can't be created, on every platform.
        std::fs::write(exe.join(DATA_DIR), b"in the way").unwrap();
        let p = resolve(Some(&exe));
        assert_eq!(p.dir, None);
        let unwritable = p.unwritable.unwrap();
        assert_eq!(unwritable.marker, exe.join("portable.txt"));
        assert_eq!(unwritable.folder, exe.join(DATA_DIR));
        let message = unwritable.to_string();
        assert!(message.contains("portable mode is off") && message.contains("can't be written"), "{message}");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn no_exe_dir_leaves_portable_mode_off() {
        assert_eq!(resolve(None), Portable::default());
    }

    /// 1.1.0: a portable copy of the former name (Mig-EZ Pdf) stays portable, and its data folder
    /// is copied once: the former folder keeps everything, a later start copies nothing again.
    #[test]
    fn the_former_names_portable_data_is_copied_once_and_kept() {
        let exe = temp("former");
        std::fs::write(exe.join(FORMER_MARKER), b"").unwrap();
        let former = exe.join(FORMER_DATA_DIR);
        std::fs::create_dir_all(former.join("Digital IDs")).unwrap();
        std::fs::write(former.join("app.ron"), b"(recent:[\"a.pdf\"])").unwrap();
        std::fs::write(former.join("Digital IDs").join("id.p12"), b"KEY").unwrap();
        let p = resolve(Some(&exe));
        let dir = p.dir.expect("the former marker still switches portable mode on");
        assert_eq!(dir, exe.join(DATA_DIR));
        assert_eq!(std::fs::read(dir.join("app.ron")).unwrap(), b"(recent:[\"a.pdf\"])");
        assert_eq!(std::fs::read(dir.join("Digital IDs").join("id.p12")).unwrap(), b"KEY");
        assert_eq!(std::fs::read(former.join("app.ron")).unwrap(), b"(recent:[\"a.pdf\"])", "the former folder is kept");
        std::fs::write(dir.join("app.ron"), b"NEW").unwrap();
        assert_eq!(resolve(Some(&exe)).dir, Some(dir.clone()));
        assert_eq!(std::fs::read(dir.join("app.ron")).unwrap(), b"NEW", "copied once, never over the new settings");
        let _ = std::fs::remove_dir_all(&exe);
    }

    #[test]
    fn copy_folder_copies_data_skips_runtime_files_and_never_replaces() {
        let root = temp("copy");
        let from = root.join("old");
        std::fs::create_dir_all(from.join("Recovery").join("deep")).unwrap();
        std::fs::create_dir_all(from.join("logs")).unwrap();
        std::fs::write(from.join("app.ron"), b"SETTINGS").unwrap();
        std::fs::write(from.join("Recovery").join("deep").join("unsaved.pdf"), b"UNSAVED").unwrap();
        std::fs::write(from.join("logs").join("old.log"), b"LOG").unwrap();
        std::fs::write(from.join("instance-s1.sock"), b"").unwrap();
        std::fs::write(from.join("instance-s1.lock"), b"").unwrap();
        let to = root.join("new");
        assert_eq!(copy_folder(&from, &to, &["logs"]).unwrap(), 2);
        assert_eq!(std::fs::read(to.join("app.ron")).unwrap(), b"SETTINGS");
        assert_eq!(std::fs::read(to.join("Recovery").join("deep").join("unsaved.pdf")).unwrap(), b"UNSAVED");
        assert!(!to.join("logs").exists(), "a skipped top-level entry stays behind");
        assert!(!to.join("instance-s1.sock").exists() && !to.join("instance-s1.lock").exists(), "no single-instance files");
        assert!(from.join("app.ron").is_file() && from.join("logs").join("old.log").is_file(), "the source is only read");
        // The temporary folder is gone, and an existing target is never replaced.
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 2, "only old and new");
        std::fs::write(to.join("app.ron"), b"NEW").unwrap();
        assert_eq!(copy_folder(&from, &to, &[]).unwrap_err().kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(to.join("app.ron")).unwrap(), b"NEW");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_failed_copy_leaves_no_target_behind() {
        let root = temp("copy-fails");
        let to = root.join("new");
        assert!(copy_folder(&root.join("missing"), &to, &[]).is_err());
        assert!(!to.exists());
        assert_eq!(std::fs::read_dir(&root).unwrap().count(), 0, "no temporary folder left");
        let _ = std::fs::remove_dir_all(&root);
    }
}
