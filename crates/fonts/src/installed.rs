//! Pdfrp: the fonts installed on the user's computer as Edit text's fallback for letters a
//! paragraph's embedded subset doesn't have (a Word PDF's Calibri subset without `Ğ`, say).
//!
//! Only the same font is used: the installed face whose PostScript name is the paragraph's font's
//! (subset tag aside), or else the one of its family with its weight and slant; never a nearby
//! family (then Pdfrp's own OFL faces stand in, as before). The face's own embedding
//! permission (OS/2 `fsType`) is obeyed: a restricted or bitmap-only font is never embedded; an
//! installable, editable or preview & print one goes into the user's own document as a subset of
//! the glyphs the text draws ([`crate::subset`]), or whole when the font forbids subsetting. No
//! installed font is ever bundled with or distributed by Pdfrp: it is read from the user's font
//! folders on the user's computer, into that user's document. What was embedded, from where and
//! under which permission is written to a journal ([`take_journal`]) the app writes to its log.
//!
//! Off unless the app turns it on ([`enable`], with the user's font folders); a test turns it on
//! for its own thread ([`set_for_this_thread`]), so other tests keep the earlier behaviour.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// What a font's OS/2 `fsType` allows a document to do with it (OpenType, OS/2 table).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Permission {
    /// 0: may be embedded and installed.
    Installable,
    /// Bit 1: must not be embedded.
    Restricted,
    /// Bit 2: may be embedded for preview and print.
    PreviewPrint,
    /// Bit 3: may be embedded in documents that are edited.
    Editable,
}

/// A font's embedding permission and its two limits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Embedding {
    pub permission: Permission,
    /// Bit 8: the whole font must be embedded, not a subset.
    pub no_subsetting: bool,
    /// Bit 9: only bitmaps may be embedded, no outlines.
    pub bitmap_only: bool,
    pub fs_type: u16,
}

impl Embedding {
    /// Bits 1–3 are exclusive in OS/2 version 3 and later; an older font may set several, and
    /// then the least restrictive applies.
    pub fn of(fs_type: u16) -> Self {
        let permission = if fs_type & 0x0008 != 0 {
            Permission::Editable
        } else if fs_type & 0x0004 != 0 {
            Permission::PreviewPrint
        } else if fs_type & 0x0002 != 0 {
            Permission::Restricted
        } else {
            Permission::Installable
        };
        Self { permission, no_subsetting: fs_type & 0x0100 != 0, bitmap_only: fs_type & 0x0200 != 0, fs_type }
    }

    /// Whether the outlines may go into the user's document.
    pub fn allows_outlines(self) -> bool {
        self.permission != Permission::Restricted && !self.bitmap_only
    }

    /// The permission in words, for the journal.
    pub fn describe(self) -> String {
        let permission = match self.permission {
            Permission::Installable => "installable",
            Permission::Restricted => "restricted",
            Permission::PreviewPrint => "preview & print",
            Permission::Editable => "editable",
        };
        let mut out = format!("{permission} (fsType 0x{:04X})", self.fs_type);
        if self.no_subsetting {
            out.push_str(", no subsetting");
        }
        if self.bitmap_only {
            out.push_str(", bitmap only");
        }
        out
    }
}

/// One installed face, as its file's tables name it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledFace {
    pub path: PathBuf,
    /// The family (name ID 1).
    pub family: String,
    /// The PostScript name (name ID 6).
    pub postscript: String,
    pub bold: bool,
    pub italic: bool,
    pub fs_type: u16,
    /// TrueType outlines (`glyf`), which can be subset.
    pub truetype: bool,
}

/// A font's family for matching (also Edit text's paragraph grouping): its PostScript or family
/// name without the subset tag, the style after `-` or `,` and the foundry endings `PSMT`, `MT`
/// and `PS`, lowercased letters and digits only. `TimesNewRomanPS-BoldMT` → `timesnewroman`,
/// `ArialMT` → `arial`, `ABCDEF+Calibri` → `calibri`, `Times New Roman` → `timesnewroman`.
pub fn family_key(name: &str) -> String {
    let name = name.rsplit('+').next().unwrap_or(name);
    let name = name.split([',', '-']).next().unwrap_or(name);
    let mut key: String = name.chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect();
    for ending in ["psmt", "mt", "ps"] {
        if key.len() > ending.len() + 2 && key.ends_with(ending) {
            key.truncate(key.len() - ending.len());
            break;
        }
    }
    key
}

/// A whole PostScript name compared letter by letter (subset tag aside, lowercased).
fn postscript_key(name: &str) -> String {
    name.rsplit('+').next().unwrap_or(name).chars().filter(char::is_ascii_alphanumeric).flat_map(|c| c.to_lowercase()).collect()
}

static GLOBAL: OnceLock<Vec<PathBuf>> = OnceLock::new();

thread_local! {
    static THREAD: RefCell<Option<Vec<PathBuf>>> = const { RefCell::new(None) };
}

/// Turn the fallback on for the whole app, with the folders installed fonts are in. Once.
pub fn enable(dirs: Vec<PathBuf>) {
    let _ = GLOBAL.set(dirs);
}

/// Turn it on (`Some`) or off (`None`) for this thread only, whatever the app set: for tests.
pub fn set_for_this_thread(dirs: Option<Vec<PathBuf>>) {
    THREAD.with(|t| *t.borrow_mut() = dirs);
}

fn active_dirs() -> Option<Vec<PathBuf>> {
    THREAD.with(|t| t.borrow().clone()).or_else(|| GLOBAL.get().cloned())
}

/// Whether the fallback is on for this thread.
pub fn active() -> bool {
    active_dirs().is_some()
}

/// Font files looked at in one set of folders, at most.
const MAX_FILES: usize = 4096;
/// A font file larger than this is passed over.
const MAX_FILE: u64 = 32 << 20;

type Catalog = Arc<Vec<InstalledFace>>;

fn catalog(dirs: &[PathBuf]) -> Catalog {
    static CATALOGS: Mutex<Vec<(Vec<PathBuf>, Catalog)>> = Mutex::new(Vec::new());
    let Ok(mut all) = CATALOGS.lock() else { return Arc::new(Vec::new()) };
    if let Some((_, c)) = all.iter().find(|(d, _)| d == dirs) {
        return c.clone();
    }
    let c: Catalog = Arc::new(scan(dirs));
    all.push((dirs.to_vec(), c.clone()));
    c
}

/// The faces in `dirs` (and one level of subfolders): `.ttf` and `.otf` files, read only as far
/// as their table directory and the `name`, `OS/2` and `head` tables.
fn scan(dirs: &[PathBuf]) -> Vec<InstalledFace> {
    let mut out = Vec::new();
    let mut seen = 0usize;
    let mut pending: Vec<(PathBuf, usize)> = dirs.iter().map(|d| (d.clone(), 0)).collect();
    while let Some((dir, depth)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            if seen >= MAX_FILES {
                return out;
            }
            let path = entry.path();
            let Ok(kind) = entry.file_type() else { continue };
            if kind.is_dir() && depth < 1 {
                pending.push((path, depth + 1));
                continue;
            }
            let ext = path.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
            if !kind.is_file() || !matches!(ext.as_deref(), Some("ttf" | "otf")) {
                continue;
            }
            seen += 1;
            if let Some(face) = read_face(&path) {
                out.push(face);
            }
        }
    }
    out
}

fn be16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at.checked_add(1)?)?]))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(at)?, *d.get(at.checked_add(1)?)?, *d.get(at.checked_add(2)?)?, *d.get(at.checked_add(3)?)?]))
}

fn read_face(path: &Path) -> Option<InstalledFace> {
    let mut file = std::fs::File::open(path).ok()?;
    if file.metadata().ok()?.len() > MAX_FILE {
        return None;
    }
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    let version = be32(&header, 0)?;
    // TrueType (0x00010000 or 'true') or CFF ('OTTO'); collections ('ttcf') are passed over.
    if !matches!(version, 0x0001_0000 | 0x7472_7565 | 0x4F54_544F) {
        return None;
    }
    let count = usize::from(be16(&header, 4)?).min(64);
    let mut records = vec![0u8; count * 16];
    file.read_exact(&mut records).ok()?;
    let mut read = |tag: &[u8; 4], limit: usize| -> Option<Vec<u8>> {
        let at = (0..count).map(|i| i * 16).find(|r| records.get(*r..r + 4) == Some(tag.as_slice()))?;
        let (offset, length) = (be32(&records, at + 8)?, be32(&records, at + 12)? as usize);
        file.seek(SeekFrom::Start(u64::from(offset))).ok()?;
        let mut data = vec![0u8; length.min(limit)];
        file.read_exact(&mut data).ok()?;
        Some(data)
    };
    let name = read(b"name", 512 << 10)?;
    let os2 = read(b"OS/2", 128);
    let head = read(b"head", 64);
    let truetype = (0..count).any(|i| records.get(i * 16..i * 16 + 4) == Some(b"glyf".as_slice()));
    let family = name_string(&name, 1)?;
    let postscript = name_string(&name, 6).unwrap_or_else(|| family.replace(' ', ""));
    let (bold, italic) = match (&os2, &head) {
        (Some(os2), _) if os2.len() >= 64 => {
            let selection = be16(os2, 62)?;
            (selection & 0x0020 != 0, selection & 0x0001 != 0)
        }
        (_, Some(head)) => {
            let style = be16(head, 44)?;
            (style & 0x0001 != 0, style & 0x0002 != 0)
        }
        _ => (false, false),
    };
    let fs_type = os2.as_deref().and_then(|t| be16(t, 8)).unwrap_or(0);
    Some(InstalledFace { path: path.to_path_buf(), family, postscript, bold, italic, fs_type, truetype })
}

/// Name `id` of a `name` table: the Windows Unicode English record, else any Windows Unicode
/// one, else a Macintosh Roman one.
fn name_string(name: &[u8], id: u16) -> Option<String> {
    let count = usize::from(be16(name, 2)?);
    let strings = usize::from(be16(name, 4)?);
    let mut best: Option<(u8, String)> = None;
    for i in 0..count {
        let r = 6 + i * 12;
        let (platform, encoding, language, nid) = (be16(name, r)?, be16(name, r + 2)?, be16(name, r + 4)?, be16(name, r + 6)?);
        if nid != id {
            continue;
        }
        let (length, offset) = (usize::from(be16(name, r + 8)?), usize::from(be16(name, r + 10)?));
        let Some(bytes) = name.get(strings + offset..strings + offset + length) else { continue };
        let (rank, text) = match (platform, encoding) {
            (3, 1 | 10) => {
                let (pairs, _) = bytes.as_chunks::<2>();
                let units: Vec<u16> = pairs.iter().map(|c| u16::from_be_bytes(*c)).collect();
                (if language == 0x0409 { 0 } else { 1 }, String::from_utf16_lossy(&units))
            }
            (1, 0) => (2, bytes.iter().map(|&b| char::from(b)).collect()),
            _ => continue,
        };
        if !text.is_empty() && best.as_ref().is_none_or(|(r, _)| rank < *r) {
            best = Some((rank, text));
        }
    }
    best.map(|(_, t)| t)
}

/// The installed face that is `base_font` (a PDF font's name; a subset tag is ignored) with the
/// given weight and slant, when the fallback is on: the face with that PostScript name, else the
/// face of that family (name ID 1) with that weight and slant. Never another family.
pub fn find(base_font: &str, bold: bool, italic: bool) -> Option<InstalledFace> {
    let dirs = active_dirs()?;
    let faces = catalog(&dirs);
    let wanted = postscript_key(base_font);
    if wanted.is_empty() {
        return None;
    }
    if let Some(face) = faces.iter().find(|f| postscript_key(&f.postscript) == wanted) {
        return Some(face.clone());
    }
    let family = family_key(base_font);
    faces.iter().find(|f| family_key(&f.family) == family && postscript_key(&f.family) == family && f.bold == bold && f.italic == italic).cloned()
}

/// The face's whole file, read once per path.
pub fn program(face: &InstalledFace) -> Option<Arc<Vec<u8>>> {
    static PROGRAMS: Mutex<Vec<(PathBuf, Arc<Vec<u8>>)>> = Mutex::new(Vec::new());
    let mut all = PROGRAMS.lock().ok()?;
    if let Some((_, bytes)) = all.iter().find(|(p, _)| *p == face.path) {
        return Some(bytes.clone());
    }
    let bytes = Arc::new(std::fs::read(&face.path).ok().filter(|b| b.len() as u64 <= MAX_FILE)?);
    // A handful of faces at a time: the oldest goes first.
    if all.len() >= 8 {
        all.remove(0);
    }
    all.push((face.path.clone(), bytes.clone()));
    Some(bytes)
}

static JOURNAL: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// Note what was embedded (or refused) for the app's log.
pub fn record(entry: String) {
    if let Ok(mut j) = JOURNAL.lock()
        && j.len() < 1024
    {
        j.push(entry);
    }
}

/// What was embedded since the last call, for the app to log.
pub fn take_journal() -> Vec<String> {
    JOURNAL.lock().map(|mut j| std::mem::take(&mut *j)).unwrap_or_default()
}

/// What [`embed_for`] embedded: the font resource, and the face, for telling the user.
#[derive(Clone, Debug)]
pub struct Embedded {
    pub resource: String,
    pub face: InstalledFace,
}

/// Embed the installed face that is `base_font` (see [`find`]) into `doc`, as font resource in
/// `fonts`, for drawing `text`: a subset of the glyphs `text` needs, or the whole font when its
/// permission forbids subsetting. `None` (and nothing changed) when the fallback is off, there is
/// no such face, its permission forbids embedding its outlines, it lacks one of `text`'s
/// characters, or it has CFF outlines. What happens is journalled ([`take_journal`]).
pub fn embed_for(
    doc: &mut migez_cos::Document,
    fonts: &mut migez_cos::Dict,
    base_font: &str,
    bold: bool,
    italic: bool,
    text: &str,
) -> Option<Embedded> {
    let (face, bytes) = Lookups::default().usable(base_font, bold, italic)?;
    if !crate::embedding::covers(&bytes, text) {
        return None;
    }
    embed_found(doc, fonts, &face, &bytes, text)
}

/// Embed `face` (whose program is `bytes`, and which [`Lookups::usable`] allowed) for drawing
/// `text`, journalling what happened.
pub fn embed_found(
    doc: &mut migez_cos::Document,
    fonts: &mut migez_cos::Dict,
    face: &InstalledFace,
    bytes: &[u8],
    text: &str,
) -> Option<Embedded> {
    let embedding = Embedding::of(face.fs_type);
    let place = face.path.display();
    match crate::embedding::embed_installed(doc, fonts, bytes, text, embedding.no_subsetting) {
        Ok((resource, glyphs)) => {
            let how =
                if embedding.no_subsetting { "whole (the font forbids subsetting)".to_string() } else { format!("as a subset of {glyphs} glyphs") };
            record(format!("installed font {} ({place}) embedded {how}; embedding permission: {}", face.postscript, embedding.describe()));
            Some(Embedded { resource, face: face.clone() })
        }
        Err(e) => {
            record(format!("installed font {} ({place}) not embedded: {e}", face.postscript));
            None
        }
    }
}

/// A face that may be embedded, with its program.
type Usable = Option<(InstalledFace, Arc<Vec<u8>>)>;

/// Families already looked up (by font name, bold, italic), for callers that ask per character.
#[derive(Default)]
pub struct Lookups(HashMap<(String, bool, bool), Usable>);

impl Lookups {
    /// The face that is `base_font` with its program, when it may be embedded and can be subset
    /// (or embedded whole), looked up once per font and style.
    pub fn usable(&mut self, base_font: &str, bold: bool, italic: bool) -> Option<(InstalledFace, Arc<Vec<u8>>)> {
        self.0
            .entry((base_font.to_owned(), bold, italic))
            .or_insert_with(|| {
                let face = find(base_font, bold, italic)?;
                let e = Embedding::of(face.fs_type);
                if !e.allows_outlines() {
                    record(format!(
                        "installed font {} ({}) not embedded: its embedding permission is {}",
                        face.postscript,
                        face.path.display(),
                        e.describe()
                    ));
                    return None;
                }
                if !face.truetype && !e.no_subsetting {
                    record(format!("installed font {} ({}) not embedded: only TrueType outlines are subset", face.postscript, face.path.display()));
                    return None;
                }
                let bytes = program(&face)?;
                Some((face, bytes))
            })
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedding_permission_follows_fs_type() {
        assert_eq!(Embedding::of(0).permission, Permission::Installable);
        assert_eq!(Embedding::of(0x0002).permission, Permission::Restricted);
        assert_eq!(Embedding::of(0x0004).permission, Permission::PreviewPrint);
        assert_eq!(Embedding::of(0x0008).permission, Permission::Editable);
        // An old font with several bits: the least restrictive.
        assert_eq!(Embedding::of(0x0006).permission, Permission::PreviewPrint);
        assert_eq!(Embedding::of(0x000A).permission, Permission::Editable);
        assert!(!Embedding::of(0x0002).allows_outlines());
        assert!(!Embedding::of(0x0208).allows_outlines(), "bitmap only");
        assert!(Embedding::of(0x0004).allows_outlines() && Embedding::of(0x0108).allows_outlines());
        assert!(Embedding::of(0x0108).no_subsetting);
        assert_eq!(Embedding::of(0x0108).describe(), "editable (fsType 0x0108), no subsetting");
    }

    #[test]
    fn family_keys_match_the_names_pdfs_and_fonts_give_one_face() {
        for (name, key) in [
            ("ABCDEF+Calibri", "calibri"),
            ("Calibri,Bold", "calibri"),
            ("TimesNewRomanPSMT", "timesnewroman"),
            ("TimesNewRomanPS-BoldItalicMT", "timesnewroman"),
            ("Times New Roman", "timesnewroman"),
            ("ArialMT", "arial"),
            ("Arial-BoldMT", "arial"),
            ("CourierNewPSMT", "couriernew"),
            ("Helvetica-Bold", "helvetica"),
        ] {
            assert_eq!(family_key(name), key, "{name}");
        }
    }

    /// A copy of the bundled Inter in a folder is found by its PostScript name and by its family
    /// with the right style, never by another family; the fallback is off unless turned on.
    #[test]
    fn an_installed_face_is_found_by_name_only_when_the_fallback_is_on() {
        let dir = std::env::temp_dir().join(format!("migez-installed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Inter-Regular.ttf"), include_bytes!("../../../assets/fonts/Inter-Regular.ttf")).unwrap();
        set_for_this_thread(None);
        assert!(find("ABCDEF+Inter-Regular", false, false).is_none(), "off unless turned on");
        set_for_this_thread(Some(vec![dir.clone()]));
        let face = find("ABCDEF+Inter-Regular", false, false).expect("by PostScript name");
        assert_eq!((face.family.as_str(), face.bold, face.truetype), ("Inter", false, true));
        assert!(find("Inter", false, false).is_some(), "by family and style");
        assert!(find("Inter", true, false).is_none(), "no bold face installed: not the regular one");
        assert!(find("Arial", false, false).is_none(), "another family is never used");
        set_for_this_thread(None);
        let _ = std::fs::remove_file(dir.join("Inter-Regular.ttf"));
        let _ = std::fs::remove_dir(&dir);
    }
}
