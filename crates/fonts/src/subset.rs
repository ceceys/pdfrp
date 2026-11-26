//! A glyph-keeping TrueType subset (Pdfrp), for embedding an installed font into the user's
//! own document ([`crate::installed`]): only the outlines of the glyphs the text draws go in.
//!
//! Glyph ids stay as they are, so a `/CIDToGIDMap` written for the whole font fits the subset: every
//! glyph the text doesn't need keeps its place with no outline (an empty `glyf` entry, as a space
//! has), and the glyphs a kept composite is built from are kept with it. Only what a
//! `CIDFontType2` program needs (ISO 32000-2 §9.9) and the font's own records of who made it and
//! how it may be embedded (`name`, `OS/2`) go in; `post` loses its glyph names (version 3.0).
//! Layout and kerning tables don't: the PDF places every glyph itself. Fonts with CFF outlines are
//! not subset here (callers fall back to Pdfrp's own faces).

use std::collections::BTreeSet;

/// The tables a subset keeps, in the order of their tags.
const KEPT: [&[u8; 4]; 13] = [b"OS/2", b"cvt ", b"fpgm", b"gasp", b"glyf", b"head", b"hhea", b"hmtx", b"loca", b"maxp", b"name", b"post", b"prep"];

fn be16(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes([*d.get(at)?, *d.get(at.checked_add(1)?)?]))
}

fn be32(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes([*d.get(at)?, *d.get(at.checked_add(1)?)?, *d.get(at.checked_add(2)?)?, *d.get(at.checked_add(3)?)?]))
}

/// A font's tables: tag and data.
type Tables<'a> = Vec<([u8; 4], &'a [u8])>;

/// The font's tables: tag and data.
pub(crate) fn tables(font: &[u8]) -> Result<Tables<'_>, String> {
    let count = usize::from(be16(font, 4).ok_or("the font has no table directory")?);
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let record = 12usize.saturating_add(i.saturating_mul(16));
        let tag: [u8; 4] = font.get(record..record.saturating_add(4)).and_then(|t| t.try_into().ok()).ok_or("the table directory is cut short")?;
        let offset = be32(font, record.saturating_add(8)).ok_or("the table directory is cut short")? as usize;
        let length = be32(font, record.saturating_add(12)).ok_or("the table directory is cut short")? as usize;
        let data = font.get(offset..offset.saturating_add(length)).ok_or("a table lies outside the font")?;
        out.push((tag, data));
    }
    Ok(out)
}

fn table<'a>(tables: &[([u8; 4], &'a [u8])], tag: &[u8; 4]) -> Option<&'a [u8]> {
    tables.iter().find(|(t, _)| t == tag).map(|(_, d)| *d)
}

/// Where each glyph's outline starts in `glyf`, and where the last one ends (`n + 1` entries).
fn glyph_offsets(loca: &[u8], long: bool, glyphs: usize) -> Result<Vec<usize>, String> {
    (0..=glyphs)
        .map(|g| {
            if long {
                be32(loca, g.saturating_mul(4)).map(|v| v as usize)
            } else {
                be16(loca, g.saturating_mul(2)).map(|v| usize::from(v).saturating_mul(2))
            }
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| "the font's glyph locations are cut short".into())
}

/// Glyph `g`'s outline data in `glyf`; empty when it has none.
fn outline_of<'a>(glyf: &'a [u8], offsets: &[usize], g: usize) -> &'a [u8] {
    match (offsets.get(g), offsets.get(g.saturating_add(1))) {
        (Some(&a), Some(&b)) if a < b => glyf.get(a..b).unwrap_or_default(),
        _ => &[],
    }
}

/// The glyphs composite glyph `data` is built from.
fn components(data: &[u8]) -> Vec<u16> {
    let mut out = Vec::new();
    // A simple glyph (no contours count below zero) is built from nothing.
    if data.len() < 10 || be16(data, 0).is_none_or(|c| (c as i16) >= 0) {
        return out;
    }
    let mut at = 10usize;
    // At most one component per four bytes: a damaged glyph can't loop.
    for _ in 0..data.len() / 4 {
        let (Some(flags), Some(glyph)) = (be16(data, at), be16(data, at.saturating_add(2))) else { break };
        out.push(glyph);
        at = at.saturating_add(4).saturating_add(if flags & 0x0001 != 0 { 4 } else { 2 });
        at = at.saturating_add(if flags & 0x0008 != 0 {
            2
        } else if flags & 0x0040 != 0 {
            4
        } else if flags & 0x0080 != 0 {
            8
        } else {
            0
        });
        if flags & 0x0020 == 0 {
            break;
        }
    }
    out
}

/// `font` (TrueType outlines) with only the outlines of `keep`, glyph 0 and the glyphs they are
/// built from; every other glyph is left empty in its place. See the module's notes.
pub fn glyf_subset(font: &[u8], keep: &BTreeSet<u16>) -> Result<Vec<u8>, String> {
    let all = tables(font)?;
    let head = table(&all, b"head").ok_or("the font has no head table")?;
    let maxp = table(&all, b"maxp").ok_or("the font has no maxp table")?;
    let loca = table(&all, b"loca").ok_or("only fonts with TrueType outlines (glyf) are subset")?;
    let glyf = table(&all, b"glyf").ok_or("only fonts with TrueType outlines (glyf) are subset")?;
    let glyphs = usize::from(be16(maxp, 4).ok_or("the font's maxp table is cut short")?);
    let long = be16(head, 50).ok_or("the font's head table is cut short")? != 0;
    let offsets = glyph_offsets(loca, long, glyphs)?;
    let outline = |g: usize| outline_of(glyf, &offsets, g);
    // The glyphs kept, closed over the composites' parts.
    let mut wanted: BTreeSet<u16> = keep.iter().copied().filter(|g| usize::from(*g) < glyphs).collect();
    wanted.insert(0);
    let mut stack: Vec<u16> = wanted.iter().copied().collect();
    while let Some(g) = stack.pop() {
        for part in components(outline(usize::from(g))) {
            if usize::from(part) < glyphs && wanted.insert(part) {
                stack.push(part);
            }
        }
    }
    // The new outlines, each on a four-byte boundary, and their locations (long format).
    let mut new_glyf = Vec::new();
    let mut new_loca = Vec::with_capacity(glyphs.saturating_add(1).saturating_mul(4));
    for g in 0..glyphs {
        new_loca.extend_from_slice(&u32::try_from(new_glyf.len()).map_err(|_| "the subset is too large")?.to_be_bytes());
        if u16::try_from(g).is_ok_and(|g| wanted.contains(&g)) {
            new_glyf.extend_from_slice(outline(g));
            while new_glyf.len() % 4 != 0 {
                new_glyf.push(0);
            }
        }
    }
    new_loca.extend_from_slice(&u32::try_from(new_glyf.len()).map_err(|_| "the subset is too large")?.to_be_bytes());
    let mut new_head = head.to_vec();
    // Long glyph locations; the whole-font checksum is set once the font is written.
    new_head.get_mut(50..52).ok_or("the font's head table is cut short")?.copy_from_slice(&1u16.to_be_bytes());
    new_head.get_mut(8..12).ok_or("the font's head table is cut short")?.copy_from_slice(&[0; 4]);
    // `post` version 3.0: no glyph names; the header (angle, underline, pitch) is kept.
    let post = table(&all, b"post").map(|p| {
        let mut p = p.get(..32).map(<[u8]>::to_vec).unwrap_or_else(|| vec![0; 32]);
        p.resize(32, 0);
        p[0..4].copy_from_slice(&0x0003_0000u32.to_be_bytes());
        p
    });
    let mut out_tables: Vec<([u8; 4], Vec<u8>)> = Vec::new();
    for tag in KEPT {
        let data = match tag {
            b"glyf" => Some(new_glyf.clone()),
            b"loca" => Some(new_loca.clone()),
            b"head" => Some(new_head.clone()),
            b"post" => post.clone(),
            other => table(&all, other).map(<[u8]>::to_vec),
        };
        if let Some(data) = data {
            out_tables.push((*tag, data));
        }
    }
    let mut out = write_sfnt(&out_tables)?;
    // head.checkSumAdjustment: 0xB1B0AFBA less the checksum of the whole font.
    let adjustment = 0xB1B0_AFBAu32.wrapping_sub(checksum(&out));
    let head_at = head_offset(&out).ok_or("the subset has no head table")?;
    out.get_mut(head_at.saturating_add(8)..head_at.saturating_add(12))
        .ok_or("the subset's head table is cut short")?
        .copy_from_slice(&adjustment.to_be_bytes());
    Ok(out)
}

fn checksum(data: &[u8]) -> u32 {
    data.chunks(4).fold(0u32, |sum, chunk| {
        let mut word = [0u8; 4];
        word[..chunk.len()].copy_from_slice(chunk);
        sum.wrapping_add(u32::from_be_bytes(word))
    })
}

/// An sfnt (TrueType outlines) of `tables`, which are in tag order.
fn write_sfnt(tables: &[([u8; 4], Vec<u8>)]) -> Result<Vec<u8>, String> {
    let count = u16::try_from(tables.len()).map_err(|_| "too many tables")?;
    let mut power = 1u16;
    let mut selector = 0u16;
    while power.saturating_mul(2) <= count {
        power = power.saturating_mul(2);
        selector = selector.saturating_add(1);
    }
    let search_range = power.saturating_mul(16);
    let mut out = Vec::new();
    out.extend_from_slice(&0x0001_0000u32.to_be_bytes());
    for v in [count, search_range, selector, count.saturating_mul(16).saturating_sub(search_range)] {
        out.extend_from_slice(&v.to_be_bytes());
    }
    let mut offset = 12usize.saturating_add(tables.len().saturating_mul(16));
    for (tag, data) in tables {
        out.extend_from_slice(tag);
        out.extend_from_slice(&checksum(data).to_be_bytes());
        out.extend_from_slice(&u32::try_from(offset).map_err(|_| "the subset is too large")?.to_be_bytes());
        out.extend_from_slice(&u32::try_from(data.len()).map_err(|_| "a table is too large")?.to_be_bytes());
        offset = offset.saturating_add(data.len().div_ceil(4).saturating_mul(4));
    }
    for (_, data) in tables {
        out.extend_from_slice(data);
        while out.len() % 4 != 0 {
            out.push(0);
        }
    }
    Ok(out)
}

fn head_offset(font: &[u8]) -> Option<usize> {
    let count = usize::from(be16(font, 4)?);
    (0..count).find_map(|i| {
        let record = 12 + i * 16;
        (font.get(record..record + 4)? == b"head").then(|| be32(font, record + 8).map(|v| v as usize)).flatten()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use skrifa::{FontRef, MetadataProvider, raw::TableProvider};

    const INTER: &[u8] = include_bytes!("../../../assets/fonts/Inter-Regular.ttf");

    fn gid(font: &FontRef<'_>, c: char) -> u16 {
        font.charmap().map(c).map(|g| g.to_u32() as u16).unwrap_or_default()
    }

    /// The subset keeps the glyphs asked for (and what they are built from) with their outlines,
    /// leaves the others empty in their places, and is a valid font much smaller than the face.
    #[test]
    fn a_subset_keeps_only_the_outlines_asked_for_in_their_places() {
        let full = FontRef::new(INTER).unwrap();
        let keep: BTreeSet<u16> = "Iğdır".chars().map(|c| gid(&full, c)).collect();
        let subset = glyf_subset(INTER, &keep).unwrap();
        assert!(subset.len() * 4 < INTER.len(), "{} of {}", subset.len(), INTER.len());
        let font = FontRef::new(&subset).unwrap();
        assert_eq!(font.maxp().unwrap().num_glyphs(), full.maxp().unwrap().num_glyphs(), "glyph ids keep their places");
        assert!(font.cmap().is_err(), "no Unicode map: the PDF's /CIDToGIDMap and /ToUnicode do that");
        let glyf = font.glyf().unwrap();
        let loca = font.loca(None).unwrap();
        let has_outline = |g: u16| loca.get_glyf(skrifa::GlyphId::new(u32::from(g)), &glyf).ok().flatten().is_some();
        for g in &keep {
            assert!(has_outline(*g), "glyph {g} kept");
        }
        // A glyph not asked for, nor built into one asked for, is empty.
        let z = gid(&full, 'Z');
        assert!(!keep.contains(&z) && !has_outline(z));
        // The head checksum adjustment makes the whole font sum to 0xB1B0AFBA.
        assert_eq!(checksum(&subset), 0xB1B0_AFBA);
    }

    /// A composite glyph brings the glyphs it is built from.
    #[test]
    fn composite_glyphs_bring_their_parts() {
        let full = FontRef::new(INTER).unwrap();
        let glyf = full.glyf().unwrap();
        let loca = full.loca(None).unwrap();
        // Some accented capital in the face is a composite; find one and check its parts survive.
        let composite = "ĞŞİÇÖÜÂÊÎÔÛÁÉÍÓÚ"
            .chars()
            .map(|c| gid(&full, c))
            .find(|g| matches!(loca.get_glyf(skrifa::GlyphId::new(u32::from(*g)), &glyf), Ok(Some(skrifa::raw::tables::glyf::Glyph::Composite(_)))));
        let Some(composite) = composite else {
            eprintln!("skipped: the face draws every accented capital as a simple glyph");
            return;
        };
        let subset = glyf_subset(INTER, &BTreeSet::from([composite])).unwrap();
        let font = FontRef::new(&subset).unwrap();
        let (sglyf, sloca) = (font.glyf().unwrap(), font.loca(None).unwrap());
        let Ok(Some(skrifa::raw::tables::glyf::Glyph::Composite(c))) = sloca.get_glyf(skrifa::GlyphId::new(u32::from(composite)), &sglyf) else {
            panic!("the composite is kept")
        };
        for part in c.components() {
            assert!(sloca.get_glyf(part.glyph.into(), &sglyf).ok().flatten().is_some(), "part {:?} kept", part.glyph);
        }
    }
}
