//! The bundled faces (`assets/fonts`, OFL-1.1: Inter Regular and SemiBold, JetBrains Mono) as
//! real embedded PDF fonts, for generated text the standard fonts can't draw.
//!
//! The standard fonts are written in WinAnsiEncoding, which has `ç ö ü` but not the Turkish
//! `ğ Ğ ı İ ş Ş`, nor Central European, Greek or Cyrillic letters. Text with such letters is
//! drawn in a bundled face instead ([`TextFace::for_text`]): a composite font (`/Type0`,
//! `/Identity-H`) whose `CIDFontType2` descendant embeds the face's full TrueType program, with
//! a `/CIDToGIDMap` and a `/ToUnicode` map covering every character of the face. Copy, search
//! and text extraction get the letters back; other viewers draw the embedded outlines and, for
//! a form field whose `/DA` names the font, can encode newly typed text through the same map.
//!
//! One such font serves a whole document. Appearances are built without a document, so they
//! carry a complete direct copy of the font (the [`BundledFace::placeholder`], whose large streams
//! share the bundled bytes rather than copying them); [`crate::shaped_text::store_streams`], which
//! every caller already runs before storing an appearance, swaps it for the document's shared font
//! ([`BundledFace::object`]), written once and found again by its [`MARK`].

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::{Arc, OnceLock};

use migez_cos::{Bytes, Dict, Document, ObjRef, Object, PdfString, Stream};
use skrifa::instance::{LocationRef, Size};
use skrifa::{FontRef, MetadataProvider};

use crate::shaped_text::is_thai;
use crate::{CraftFont, Std14, win_ansi, win_ansi_encodable};

/// The private key on a font Pdfrp wrote from a bundled face, naming the face (its
/// PostScript name). The document's shared font is found again by it ([`BundledFace::object`]).
pub const MARK: &[u8] = b"PCBundled";

/// Most objects searched, newest first, for the document's shared font and program.
const SEARCH: usize = 8192;

/// Most candidate programs decoded while looking for the bundled one.
const MAX_DECODES: usize = 8;

/// One of the bundled faces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BundledFace {
    /// Inter Regular: the face for Helvetica and Times text, and for anything italic.
    Sans,
    /// Inter SemiBold: for bold Helvetica and Times text.
    SansBold,
    /// JetBrains Mono Regular: for Courier text, so it stays fixed-pitch.
    Mono,
}

impl BundledFace {
    pub const ALL: [BundledFace; 3] = [BundledFace::Sans, BundledFace::SansBold, BundledFace::Mono];

    /// The face's font program and names.
    pub fn craft(self) -> &'static CraftFont {
        match self {
            Self::Sans => &crate::craft::INTER_REGULAR_FACE,
            Self::SansBold => &crate::craft::INTER_SEMIBOLD_FACE,
            Self::Mono => &crate::craft::JETBRAINS_MONO_FACE,
        }
    }

    /// The resource name generated content gives the font.
    pub fn resource_name(self) -> &'static str {
        match self {
            Self::Sans => "PCInter",
            Self::SansBold => "PCInterSB",
            Self::Mono => "PCMono",
        }
    }

    /// The font's `/BaseFont` and [`MARK`]: the face's PostScript name.
    pub fn postscript_name(self) -> &'static str {
        match self {
            Self::Sans => "Inter-Regular",
            Self::SansBold => "Inter-SemiBold",
            Self::Mono => "JetBrainsMono-Regular",
        }
    }

    /// The face a [`MARK`] names.
    pub fn from_mark(name: &[u8]) -> Option<Self> {
        Self::ALL.into_iter().find(|f| f.postscript_name().as_bytes() == name)
    }

    /// The face that stands in for the standard `face`: the monospaced one for Courier, the bold
    /// one for bold Helvetica and Times, Inter Regular otherwise (there is no bundled italic or
    /// serif face; nothing is slanted or emboldened synthetically).
    pub fn for_standard(face: Std14) -> Self {
        match face {
            Std14::Courier | Std14::CourierBold | Std14::CourierOblique | Std14::CourierBoldOblique => Self::Mono,
            Std14::HelveticaBold | Std14::HelveticaBoldOblique | Std14::TimesBold | Std14::TimesBoldItalic => Self::SansBold,
            _ => Self::Sans,
        }
    }

    fn table(self) -> Option<&'static Table> {
        static TABLES: [OnceLock<Option<Table>>; 3] = [OnceLock::new(), OnceLock::new(), OnceLock::new()];
        let i = match self {
            Self::Sans => 0,
            Self::SansBold => 1,
            Self::Mono => 2,
        };
        TABLES.get(i)?.get_or_init(|| Table::build(self)).as_ref()
    }

    /// Whether the face has a glyph for `c` (a tab is drawn as a space, a soft hyphen as a hyphen).
    pub fn has(self, c: char) -> bool {
        self.table().is_some_and(|t| t.cids.contains_key(&drawn_as(c)))
    }

    /// `s` as the font's two-byte codes (CIDs); a character the face lacks becomes its `?`.
    pub fn encode(self, s: &str) -> Vec<u8> {
        let Some(t) = self.table() else { return win_ansi(s) };
        let mut out = Vec::with_capacity(s.len().saturating_mul(2));
        for c in s.chars() {
            out.extend_from_slice(&t.cid(c).to_be_bytes());
        }
        out
    }

    /// The advance of `s` at `size` points.
    pub fn width(self, s: &str, size: f64) -> f64 {
        let Some(t) = self.table() else { return Std14::Helvetica.text_width(s, size) };
        s.chars().map(|c| t.widths.get(usize::from(t.cid(c)).saturating_sub(1)).copied().unwrap_or(0.0)).sum::<f64>() * size / 1000.0
    }

    /// A complete font dictionary with its streams as direct objects, for an appearance built
    /// without a document. [`crate::shaped_text::store_streams`] replaces it with the document's
    /// shared font. The streams share the bundled bytes; nothing large is copied.
    pub fn placeholder(self) -> Object {
        match self.table() {
            Some(t) => Object::Dict(t.font_dict(self, &mut |o| o)),
            None => Object::Null,
        }
    }

    /// The document's font for this face: one already there (written by an earlier edit, found
    /// by its [`MARK`] and checked to be exactly this font), or a new one. Its program is shared
    /// with a full embedding of the same face the text editor wrote (`PCFullFont`), if any.
    pub fn object(self, doc: &mut Document) -> Result<ObjRef, String> {
        let t = self.table().ok_or("the bundled face could not be read")?;
        let mut numbers = doc.object_numbers();
        numbers.reverse();
        numbers.truncate(SEARCH);
        let mut decodes = 0usize;
        let mut program: Option<ObjRef> = None;
        for (r, o) in doc.scan_objects_subset(numbers) {
            let Ok(o) = o else { continue };
            match o.as_ref() {
                Object::Dict(d) if d.name(MARK) == Some(self.postscript_name().as_bytes()) && t.is_this(doc, d, &mut decodes) => return Ok(r),
                Object::Stream(s) if program.is_none() && t.is_program(s, &mut decodes) => program = Some(r),
                _ => {}
            }
        }
        let font = t.font_dict(self, &mut |o| match o {
            // The program: the one already in the document, or a compressed copy.
            Object::Stream(s) if s.dict.contains(b"PCFullFont") => {
                Object::Ref(program.unwrap_or_else(|| doc.add(Object::Stream(Stream::flate(s.dict.clone(), &s.raw)))))
            }
            Object::Stream(s) => Object::Ref(doc.add(Object::Stream(Stream::flate(s.dict.clone(), &s.raw)))),
            o @ Object::Dict(_) => Object::Ref(doc.add(o)),
            o => o,
        });
        Ok(doc.add(Object::Dict(font)))
    }
}

/// What the bundled face draws for `c`: a tab as a space and a soft hyphen as a hyphen, as the
/// standard fonts draw them.
fn drawn_as(c: char) -> char {
    match c {
        '\t' => ' ',
        '\u{ad}' => '-',
        c => c,
    }
}

/// Whether `c` makes text leave the standard fonts: it isn't a line break, WinAnsiEncoding can't
/// hold it, and it isn't Thai (drawn from the Thai face beside the text's own font).
fn beyond_standard(c: char) -> bool {
    !matches!(c, '\n' | '\r') && !win_ansi_encodable(c) && !is_thai(c)
}

/// The face generated text is drawn in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFace {
    /// A standard font in WinAnsiEncoding (not embedded).
    Standard(Std14),
    /// A bundled face, embedded.
    Bundled(BundledFace),
}

impl TextFace {
    /// The face `text` is drawn in when its own (chosen) face is the standard `standard`: that
    /// face while it can draw every character, as before; the bundled face that stands in for it
    /// ([`BundledFace::for_standard`]) when `text` has letters WinAnsiEncoding lacks and that face
    /// has (Turkish `ğ ı ş İ`, Central European, Greek, Cyrillic). Courier text falls back to
    /// Inter when the monospaced face lacks one of them. Thai is left to the Thai face, line
    /// breaks to the layout. Decide once for a whole text and measure every piece of it with the
    /// answer: a word measured in another face than its line is drawn in would wrap wrongly.
    pub fn for_text(standard: Std14, text: &str) -> Self {
        if matches!(standard, Std14::Symbol | Std14::ZapfDingbats) || text.is_ascii() {
            return Self::Standard(standard);
        }
        let mut beyond = text.chars().filter(|c| beyond_standard(*c)).peekable();
        if beyond.peek().is_none() {
            return Self::Standard(standard);
        }
        let preferred = BundledFace::for_standard(standard);
        let drawn = || text.chars().filter(|c| !matches!(c, '\n' | '\r') && !is_thai(*c));
        let face = if preferred == BundledFace::Mono && !drawn().all(|c| BundledFace::Mono.has(c)) { BundledFace::Sans } else { preferred };
        if beyond.any(|c| face.has(c)) { Self::Bundled(face) } else { Self::Standard(standard) }
    }

    /// The advance of `s` at `size` points.
    pub fn width(self, s: &str, size: f64) -> f64 {
        match self {
            Self::Standard(f) => f.text_width(s, size),
            Self::Bundled(b) => b.width(s, size),
        }
    }

    /// `s` as the font's codes, for a string operand (`literal`).
    pub fn encode(self, s: &str) -> Vec<u8> {
        match self {
            Self::Standard(_) => win_ansi(s),
            Self::Bundled(b) => b.encode(s),
        }
    }

    /// Whether `c` is drawn as itself rather than as `?`.
    pub fn draws(self, c: char) -> bool {
        match self {
            Self::Standard(_) => win_ansi_encodable(c),
            Self::Bundled(b) => b.has(c),
        }
    }

    /// The bundled face, if this is one.
    pub fn bundled(self) -> Option<BundledFace> {
        match self {
            Self::Bundled(b) => Some(b),
            Self::Standard(_) => None,
        }
    }
}

/// The first character of `text` a generated appearance drawing it in [`TextFace::for_text`]
/// (with standard face `standard`) would show as `?`: one neither that face nor (for Thai) the
/// Thai face can draw. Line breaks don't count. `None` when all of it is drawn.
pub fn first_undrawable(standard: Std14, text: &str) -> Option<char> {
    if text.is_ascii() {
        return text.chars().find(|c| !matches!(c, '\n' | '\r') && !win_ansi_encodable(*c));
    }
    let face = TextFace::for_text(standard, text);
    text.split(['\n', '\r']).find_map(|line| first_undrawable_in_line(face, line))
}

/// The first character of `line`, drawn as it is in `drawn` (with its Thai from the Thai face),
/// that comes out as `?`: a line break in it counts.
pub fn first_undrawable_in_line(drawn: TextFace, line: &str) -> Option<char> {
    let thai_fails = line.chars().any(is_thai) && crate::shaped_text::thai_pieces(line).is_err();
    line.chars().find(|&c| if is_thai(c) { thai_fails } else { !drawn.draws(c) })
}

/// The refusal for text with `c`, which neither the standard fonts nor the bundled faces can
/// draw, in `what` ("the comment", "the watermark").
pub fn undrawable_message(what: &str, c: char) -> String {
    format!(
        "the fonts can't draw \"{}\" (U+{:04X}) in {what}; Latin (Turkish included), Greek and Cyrillic letters can be written there",
        c.escape_debug(),
        u32::from(c)
    )
}

/// Whether `d` is a [`BundledFace::placeholder`] (a bundled font not yet in a document).
pub fn is_placeholder(d: &Dict) -> bool {
    d.contains(MARK) && d.get(b"DescendantFonts").and_then(Object::as_array).is_some_and(|a| matches!(a.first(), Some(Object::Dict(_))))
}

/// The document's shared font for the placeholder `d` (see [`crate::shaped_text::store_streams`]).
pub fn shared_for_placeholder(doc: &mut Document, d: &Dict) -> Option<ObjRef> {
    BundledFace::from_mark(d.name(MARK)?)?.object(doc).ok()
}

/// The bundled face of a font Pdfrp wrote from one (it carries the [`MARK`]), for a form whose
/// `/DA` names it.
pub fn face_of(d: &Dict) -> Option<BundledFace> {
    (d.name(b"Subtype") == Some(b"Type0")).then(|| d.name(MARK).and_then(BundledFace::from_mark)).flatten()
}

/// A face's font as PDF data: one CID per character the face maps (in character order, from 1),
/// each drawing that character's glyph.
struct Table {
    cids: HashMap<char, u16>,
    /// Advance per CID (CID − 1), in thousandths of an em.
    widths: Vec<f64>,
    /// The CID of `?`, for characters the face lacks.
    question: u16,
    program: Arc<Vec<u8>>,
    gid_map: Arc<Vec<u8>>,
    to_unicode: Arc<Vec<u8>>,
    family: &'static str,
    ascent: f64,
    descent: f64,
    cap_height: f64,
    italic_angle: f64,
    bbox: [f64; 4],
}

impl Table {
    fn build(face: BundledFace) -> Option<Table> {
        let craft = face.craft();
        let font = FontRef::new(craft.bytes).ok()?;
        let loc = LocationRef::default();
        let m = font.metrics(Size::unscaled(), loc);
        let scale = 1000.0 / f64::from(m.units_per_em.max(1));
        let advances = font.glyph_metrics(Size::unscaled(), loc);
        let mut cids = HashMap::new();
        let mut widths = Vec::new();
        let mut gid_map = vec![0u8, 0u8];
        let mut entries = Vec::new();
        for (cp, gid) in font.charmap().mappings().filter(|(_, g)| g.to_u32() > 0) {
            let Some(c) = char::from_u32(cp) else { continue };
            let (Ok(cid), Ok(glyph)) = (u16::try_from(widths.len() + 1), u16::try_from(gid.to_u32())) else { break };
            let width = f64::from(advances.advance_width(gid).unwrap_or(0.0)) * scale;
            gid_map.extend_from_slice(&glyph.to_be_bytes());
            // As `/W` writes it, so layout measures exactly what viewers advance by.
            widths.push(if width.is_finite() { round3(width) } else { 0.0 });
            cids.insert(c, cid);
            let hex: String = c.encode_utf16(&mut [0; 2]).iter().map(|u| format!("{u:04X}")).collect();
            entries.push(format!("<{cid:04X}> <{hex}>\n"));
        }
        let question = *cids.get(&'?')?;
        let mut cmap = String::from(
            "/CIDInit /ProcSet findresource begin\n12 dict begin\nbegincmap\n/CIDSystemInfo << /Registry (Adobe) /Ordering (UCS) /Supplement 0 >> def\n/CMapName /Adobe-Identity-UCS def\n/CMapType 2 def\n1 begincodespacerange\n<0000> <FFFF>\nendcodespacerange\n",
        );
        // A CMap block holds at most 100 entries.
        for block in entries.chunks(100) {
            let _ = write!(cmap, "{} beginbfchar\n{}endbfchar\n", block.len(), block.concat());
        }
        cmap.push_str("endcmap\nCMapName currentdict /CMap defineresource pop\nend\nend\n");
        let bounds = m.bounds?;
        Some(Table {
            cids,
            widths,
            question,
            program: Arc::new(craft.bytes.to_vec()),
            gid_map: Arc::new(gid_map),
            to_unicode: Arc::new(cmap.into_bytes()),
            family: craft.family,
            ascent: f64::from(m.ascent) * scale,
            descent: f64::from(m.descent) * scale,
            cap_height: f64::from(m.cap_height.unwrap_or(m.ascent)) * scale,
            italic_angle: f64::from(m.italic_angle),
            bbox: [bounds.x_min, bounds.y_min, bounds.x_max, bounds.y_max].map(|v| f64::from(v) * scale),
        })
    }

    fn cid(&self, c: char) -> u16 {
        self.cids.get(&drawn_as(c)).copied().unwrap_or(self.question)
    }

    /// The `/Type0` font, with `store` deciding what each stream and the descendant and
    /// descriptor dictionaries become: themselves (a placeholder) or references.
    fn font_dict(&self, face: BundledFace, store: &mut dyn FnMut(Object) -> Object) -> Dict {
        let name = face.postscript_name();
        let stream = |data: &Arc<Vec<u8>>, dict: Dict| Object::Stream(Stream { dict, raw: Bytes::from(data.clone()) });
        let mut program = Dict::new();
        program.set(b"Length1".to_vec(), Object::Int(self.program.len() as i64));
        // The text editor's full embeddings (`embedding::embed`) recognise and reuse it.
        program.set(b"PCFullFont".to_vec(), Object::Bool(true));
        let mut descriptor = Dict::new();
        descriptor.set(b"Type".to_vec(), Object::name("FontDescriptor"));
        descriptor.set(b"FontName".to_vec(), Object::name(name));
        descriptor.set(b"FontFamily".to_vec(), Object::String(PdfString::literal(self.family.as_bytes().to_vec())));
        descriptor.set(b"Flags".to_vec(), Object::Int(if face == BundledFace::Mono { 1 | 32 } else { 32 }));
        descriptor.set(b"ItalicAngle".to_vec(), Object::Real(self.italic_angle));
        descriptor.set(b"Ascent".to_vec(), Object::Real(self.ascent.round()));
        descriptor.set(b"Descent".to_vec(), Object::Real(self.descent.round()));
        descriptor.set(b"CapHeight".to_vec(), Object::Real(self.cap_height.round()));
        descriptor.set(b"StemV".to_vec(), Object::Int(if face == BundledFace::SansBold { 120 } else { 80 }));
        descriptor.set(b"FontBBox".to_vec(), Object::Array(self.bbox.iter().map(|v| Object::Real(v.round())).collect()));
        descriptor.set(b"FontFile2".to_vec(), store(stream(&self.program, program)));
        let mut system = Dict::new();
        system.set(b"Registry".to_vec(), Object::String(PdfString::literal(b"Adobe".to_vec())));
        system.set(b"Ordering".to_vec(), Object::String(PdfString::literal(b"Identity".to_vec())));
        system.set(b"Supplement".to_vec(), Object::Int(0));
        let mut cid = Dict::new();
        cid.set(b"Type".to_vec(), Object::name("Font"));
        cid.set(b"Subtype".to_vec(), Object::name("CIDFontType2"));
        cid.set(b"BaseFont".to_vec(), Object::name(name));
        cid.set(b"CIDSystemInfo".to_vec(), Object::Dict(system));
        cid.set(b"FontDescriptor".to_vec(), store(Object::Dict(descriptor)));
        cid.set(b"CIDToGIDMap".to_vec(), store(stream(&self.gid_map, Dict::new())));
        cid.set(b"DW".to_vec(), Object::Int(0));
        cid.set(b"W".to_vec(), Object::Array(vec![Object::Int(1), Object::Array(self.widths.iter().map(|w| Object::Real(*w)).collect())]));
        let mut top = Dict::new();
        top.set(b"Type".to_vec(), Object::name("Font"));
        top.set(b"Subtype".to_vec(), Object::name("Type0"));
        top.set(b"BaseFont".to_vec(), Object::name(name));
        top.set(b"Encoding".to_vec(), Object::name("Identity-H"));
        top.set(b"DescendantFonts".to_vec(), Object::Array(vec![store(Object::Dict(cid))]));
        top.set(b"ToUnicode".to_vec(), store(stream(&self.to_unicode, Dict::new())));
        top.set(MARK.to_vec(), Object::name(name));
        top
    }

    /// Whether the font `d` (carrying this face's mark) is exactly the one [`Self::font_dict`]
    /// writes: its maps and program are this face's. A document can't pass off another font as it.
    fn is_this(&self, doc: &Document, d: &Dict, decodes: &mut usize) -> bool {
        let same = |o: Option<&Object>, expected: &[u8]| {
            o.is_some_and(|o| matches!(doc.resolve(o).as_ref(), Object::Stream(s) if s.decoded_within(expected.len()).is_ok_and(|v| v == expected)))
        };
        if d.name(b"Subtype") != Some(b"Type0") || d.name(b"Encoding") != Some(b"Identity-H") || !same(d.get(b"ToUnicode"), &self.to_unicode) {
            return false;
        }
        let Some(cid) = d.get(b"DescendantFonts").map(|a| doc.resolve(a)).and_then(|a| a.as_array().and_then(|a| a.first().cloned())) else {
            return false;
        };
        let Some(cid) = doc.dict(&cid) else { return false };
        if !same(cid.get(b"CIDToGIDMap"), &self.gid_map) {
            return false;
        }
        let Some(descriptor) = cid.get(b"FontDescriptor").and_then(|o| doc.dict(o)) else { return false };
        descriptor.get(b"FontFile2").is_some_and(|p| matches!(doc.resolve(p).as_ref(), Object::Stream(s) if self.is_program(s, decodes)))
    }

    /// Whether `s` is this face's full program (as [`Self::font_dict`] or the text editor writes
    /// it), decoding at most [`MAX_DECODES`] candidates in all.
    fn is_program(&self, s: &Stream, decodes: &mut usize) -> bool {
        if !s.dict.contains(b"PCFullFont") || s.dict.int(b"Length1") != i64::try_from(self.program.len()).ok() || *decodes >= MAX_DECODES {
            return false;
        }
        *decodes += 1;
        s.decoded_within(self.program.len()).is_ok_and(|v| v == *self.program)
    }
}

/// `v` to three decimals.
fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    const PANGRAM: &str = "Pijamalı hasta yağız şoföre çabucak güvendi. PİJAMALI HASTA YAĞIZ ŞOFÖRE ÇABUCAK GÜVENDİ. ğĞıIİişŞçÇöÖüÜ";

    #[test]
    fn turkish_text_leaves_the_standard_fonts_for_the_bundled_face() {
        assert_eq!(TextFace::for_text(Std14::Helvetica, "Café — 5€"), TextFace::Standard(Std14::Helvetica));
        assert_eq!(TextFace::for_text(Std14::Helvetica, "çöü ÇÖÜ"), TextFace::Standard(Std14::Helvetica), "WinAnsi has these");
        assert_eq!(TextFace::for_text(Std14::Helvetica, PANGRAM), TextFace::Bundled(BundledFace::Sans));
        assert_eq!(TextFace::for_text(Std14::HelveticaBold, "yağız"), TextFace::Bundled(BundledFace::SansBold));
        assert_eq!(TextFace::for_text(Std14::TimesItalic, "şoför"), TextFace::Bundled(BundledFace::Sans));
        assert_eq!(TextFace::for_text(Std14::Courier, "şoför"), TextFace::Bundled(BundledFace::Mono));
        // Nothing the bundled faces have: the standard font draws what it can, as before.
        assert_eq!(TextFace::for_text(Std14::Helvetica, "签名"), TextFace::Standard(Std14::Helvetica));
        assert_eq!(TextFace::for_text(Std14::Helvetica, "ğ 签"), TextFace::Bundled(BundledFace::Sans));
        // Thai is the Thai face's.
        assert_eq!(TextFace::for_text(Std14::Helvetica, "ชื่อ"), TextFace::Standard(Std14::Helvetica));
        assert_eq!(TextFace::for_text(Std14::ZapfDingbats, "ğ"), TextFace::Standard(Std14::ZapfDingbats));
    }

    #[test]
    fn every_bundled_face_draws_turkish_and_western_european_text() {
        for face in BundledFace::ALL {
            for c in PANGRAM.chars().chain("€™—–„“”‘’•…‰Šš ŽžŒœŸƒ†‡ˆ˜‹›×÷°±²³µ¶·¹º»«¿¡ßÆæØøÅåÐðÞþ".chars())
            {
                assert!(face.has(c), "{face:?} has no {c:?}");
            }
            for c in "ĞğİıŞşÂâÎîÛû".chars() {
                assert!(face.has(c), "{face:?} has no {c:?}");
            }
            // Every WinAnsi character, so a bundled text draws them all.
            for code in 0x20u8..=0xFF {
                let Some(c) = crate::encodings::WIN_ANSI.get(usize::from(code)).and_then(|&u| char::from_u32(u)).filter(|_| code != 0x7F) else {
                    continue;
                };
                if c == '\u{0}' || c.is_control() {
                    continue;
                }
                assert!(face.has(c), "{face:?} has no {c:?} (WinAnsi 0x{code:02X})");
            }
        }
        assert_eq!(first_undrawable(Std14::Helvetica, PANGRAM), None);
        assert_eq!(first_undrawable(Std14::Helvetica, "ğ\n签"), Some('签'));
        assert_eq!(first_undrawable(Std14::Helvetica, "plain\r\ntext"), None);
        assert_eq!(first_undrawable(Std14::Helvetica, "a\u{7}"), Some('\u{7}'));
        assert!(undrawable_message("the comment", '签').contains("(U+7B7E) in the comment"));
    }

    #[test]
    fn codes_and_widths_follow_the_face() {
        let face = BundledFace::Sans;
        let codes = face.encode("ğa");
        assert_eq!(codes.len(), 4, "two bytes a character");
        assert_ne!(&codes[..2], &codes[2..]);
        // A missing character is the face's `?`; a tab a space.
        assert_eq!(face.encode("签"), face.encode("?"));
        assert_eq!(face.encode("\t"), face.encode(" "));
        assert_eq!(face.width("\u{ad}", 10.0), face.width("-", 10.0));
        let w = face.width("ğğ", 10.0);
        assert!(w > 5.0 && w < 15.0, "{w}");
        assert!((face.width("ğğ", 20.0) - 2.0 * w).abs() < 1e-9);
        // Monospaced: every letter as wide.
        let mono = BundledFace::Mono;
        assert!((mono.width("ı", 10.0) - mono.width("W", 10.0)).abs() < 1e-9);
        assert!((TextFace::Bundled(face).width("ş", 10.0) - face.width("ş", 10.0)).abs() < 1e-12);
    }

    /// The font is written once per document: the placeholder an appearance carries becomes a
    /// reference to it, and its ToUnicode map reads the text back.
    #[test]
    fn one_font_per_document_and_its_maps_read_the_text_back() {
        let mut doc = Document::new_empty();
        let first = BundledFace::Sans.object(&mut doc).unwrap();
        let count = doc.object_numbers().len();
        assert_eq!(BundledFace::Sans.object(&mut doc).unwrap(), first, "found again");
        assert_eq!(doc.object_numbers().len(), count, "nothing new written");
        let bold = BundledFace::SansBold.object(&mut doc).unwrap();
        assert_ne!(bold, first);
        let font = doc.dict(&Object::Ref(first)).unwrap();
        assert_eq!(face_of(&font), Some(BundledFace::Sans));
        let m = crate::pdf::Metrics::from_dict(&doc, &font);
        let codes = BundledFace::Sans.encode(PANGRAM);
        assert_eq!(m.decode(&codes), PANGRAM, "ToUnicode gives the text back");
        for (c, (code, _)) in PANGRAM.chars().zip(m.codes(&codes)) {
            assert!((m.width(code) * 10.0 - BundledFace::Sans.width(&c.to_string(), 10.0)).abs() < 1e-6, "{c}");
        }
        assert_eq!(crate::pdf::embedded_sfnt(&doc, &font).unwrap(), BundledFace::Sans.craft().bytes);
        // A placeholder is complete, and storing it swaps it for the shared font.
        let Object::Dict(placeholder) = BundledFace::Sans.placeholder() else { panic!("a dictionary") };
        assert!(is_placeholder(&placeholder));
        let mut res = Dict::new();
        let mut fonts = Dict::new();
        fonts.set(b"PCInter".to_vec(), Object::Dict(placeholder));
        res.set(b"Font".to_vec(), Object::Dict(fonts));
        crate::shaped_text::store_streams(&mut doc, &mut res);
        let stored = res.get(b"Font").and_then(Object::as_dict).and_then(|f| f.get(b"PCInter")).cloned();
        assert_eq!(stored, Some(Object::Ref(first)));
        assert_eq!(doc.object_numbers().len(), count + 6, "only the bold font was added (program, descriptor, two maps, two fonts)");
    }

    /// Another font under the mark (a hostile file, or a damaged copy) is not taken for the
    /// shared one: a new one is written.
    #[test]
    fn a_different_font_under_the_mark_is_not_reused() {
        let mut doc = Document::new_empty();
        let mut fake = Dict::new();
        fake.set(b"Type".to_vec(), Object::name("Font"));
        fake.set(b"Subtype".to_vec(), Object::name("Type0"));
        fake.set(b"Encoding".to_vec(), Object::name("Identity-H"));
        fake.set(MARK.to_vec(), Object::name("Inter-Regular"));
        fake.set(b"ToUnicode".to_vec(), Object::Ref(doc.add(Object::Stream(Stream::from_raw(Dict::new(), b"wrong".to_vec())))));
        let fake = doc.add(Object::Dict(fake));
        let made = BundledFace::Sans.object(&mut doc).unwrap();
        assert_ne!(made, fake);
        assert_eq!(BundledFace::Sans.object(&mut doc).unwrap(), made);
    }
}
