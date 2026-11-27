//! The ZIP container of a UDF file: its central directory and its stored or Deflate entries. Read
//! only, bounded: encrypted entries, ZIP64 archives and other compression methods are refused, and
//! no entry or archive may unpack past [`MAX_ENTRY`] or [`MAX_TOTAL`].

use std::io::Read;

/// The most entries a UDF's directory may list.
const MAX_ENTRIES: usize = 512;
/// The most one entry may unpack to.
pub(super) const MAX_ENTRY: usize = 64 << 20;
/// The most all the entries read from one archive may unpack to.
const MAX_TOTAL: usize = 256 << 20;

/// One entry of the central directory.
#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub name: String,
    method: u16,
    flags: u16,
    compressed: usize,
    size: usize,
    local: usize,
}

/// A ZIP archive's directory over its bytes.
pub(super) struct Archive<'a> {
    bytes: &'a [u8],
    pub entries: Vec<Entry>,
}

fn slice(b: &[u8], start: usize, len: usize) -> Option<&[u8]> {
    b.get(start..start.checked_add(len)?)
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = slice(b, at, 2)?;
    Some(u16::from_le_bytes([*s.first()?, *s.get(1)?]))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = slice(b, at, 4)?;
    Some(u32::from_le_bytes([*s.first()?, *s.get(1)?, *s.get(2)?, *s.get(3)?]))
}

fn damaged() -> String {
    "the UDF file's ZIP container is damaged".to_string()
}

impl<'a> Archive<'a> {
    /// Read the directory of `bytes`. Errors are sentences for the user.
    pub fn open(bytes: &'a [u8]) -> Result<Self, String> {
        if !bytes.starts_with(b"PK\x03\x04") {
            return Err("this is not a UDF file: it is not a ZIP container".into());
        }
        // The end of central directory record: 22 bytes, then a comment of up to 65 535 bytes.
        let last = bytes.len().checked_sub(22).ok_or_else(damaged)?;
        let first = last.saturating_sub(65_535);
        let eocd = (first..=last).rev().find(|&i| u32_at(bytes, i) == Some(0x0605_4b50)).ok_or_else(damaged)?;
        let count = usize::from(u16_at(bytes, eocd + 10).ok_or_else(damaged)?);
        let dir = u32_at(bytes, eocd + 16).ok_or_else(damaged)?;
        if count == 0xFFFF || dir == 0xFFFF_FFFF {
            return Err("this UDF file uses a ZIP64 container, which is not supported".into());
        }
        if count > MAX_ENTRIES {
            return Err(format!("this UDF file lists {count} entries; at most {MAX_ENTRIES} are read"));
        }
        let mut at = usize::try_from(dir).map_err(|_| damaged())?;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(bytes, at) != Some(0x0201_4b50) {
                return Err(damaged());
            }
            let field = |off: usize| u16_at(bytes, at + off).map(usize::from).ok_or_else(damaged);
            let long = |off: usize| u32_at(bytes, at + off).and_then(|v| usize::try_from(v).ok()).ok_or_else(damaged);
            let flags = u16_at(bytes, at + 8).ok_or_else(damaged)?;
            let method = u16_at(bytes, at + 10).ok_or_else(damaged)?;
            let compressed = long(20)?;
            let size = long(24)?;
            let (name_len, extra_len, comment_len) = (field(28)?, field(30)?, field(32)?);
            let local = long(42)?;
            let name = slice(bytes, at + 46, name_len).ok_or_else(damaged)?;
            entries.push(Entry { name: String::from_utf8_lossy(name).replace('\\', "/"), method, flags, compressed, size, local });
            at = at.checked_add(46 + name_len + extra_len + comment_len).ok_or_else(damaged)?;
        }
        Ok(Self { bytes, entries })
    }

    /// The entry named `name` (letter case aside), if the directory lists it.
    pub fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// The unpacked bytes of `entry`.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>, String> {
        if entry.flags & 1 != 0 {
            return Err(format!("{} is encrypted in the UDF file", entry.name));
        }
        if entry.size > MAX_ENTRY || entry.compressed > MAX_TOTAL {
            return Err(format!("{} is too large to read", entry.name));
        }
        let local = entry.local;
        if u32_at(self.bytes, local) != Some(0x0403_4b50) {
            return Err(damaged());
        }
        let name_len = usize::from(u16_at(self.bytes, local + 26).ok_or_else(damaged)?);
        let extra_len = usize::from(u16_at(self.bytes, local + 28).ok_or_else(damaged)?);
        let start = local.checked_add(30 + name_len + extra_len).ok_or_else(damaged)?;
        let raw = slice(self.bytes, start, entry.compressed).ok_or_else(damaged)?;
        let data = match entry.method {
            0 => raw.to_vec(),
            8 => {
                let mut out = Vec::with_capacity(entry.size.min(MAX_ENTRY));
                flate2::read::DeflateDecoder::new(raw)
                    .take(MAX_ENTRY as u64 + 1)
                    .read_to_end(&mut out)
                    .map_err(|e| format!("{}: {e}", entry.name))?;
                out
            }
            m => return Err(format!("{} uses ZIP compression method {m}, which is not supported", entry.name)),
        };
        if data.len() > MAX_ENTRY {
            return Err(format!("{} is too large to read", entry.name));
        }
        Ok(data)
    }
}
