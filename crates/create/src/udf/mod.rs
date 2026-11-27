//! UDF files (the documents the UYAP Document Editor uses) to PDF. A UDF file is a ZIP container
//! (`zip`): `content.xml` holds the document (`model`), `sign.sgn` its e-signature, which is never
//! checked and never carried into the PDF: nothing on the pages says it was there, only the PDF's
//! Subject ([`SIGNED_SUBJECT`]). The PDF is laid out and drawn here (`layout`); the fonts are
//! the installed faces the document names (subsets of the glyphs it draws), else similar
//! installed faces, else the bundled one. Nothing is sent anywhere: the file is read and the PDF
//! made on this computer.
//!
//! The format is not documented in public: what the attributes mean is what UDF files show
//! (Java Swing `StyleConstants` names, lengths in points, colours as signed 32-bit ARGB), and
//! the notes [`UdfInfo::notes`] carries say what was assumed or left out.

mod layout;
mod model;
mod zip;

use std::path::PathBuf;

use migez_cos::{Dict, Document, Object};

use crate::{CreateError, Pages, set_info};

/// File extensions of UDF files.
pub const UDF_EXTENSIONS: [&str; 1] = ["udf"];

/// The Subject of the PDF made from a UDF file with an e-signature, in the wording of the legal
/// review (26.11.2026). Turkish, as UDF files are: it is part of the PDF, not of the interface.
pub(crate) const SIGNED_SUBJECT: &str = "UDF dosyasından dönüştürüldü; elektronik imza taşınmadı";

/// What is said of a UDF file with an e-signature before it is converted, in English: the app
/// says it in the interface language, from its catalog, where the Turkish is the legal review's
/// wording, word for word (26.11.2026). Change neither without that review.
pub const UDF_SIGNED_NOTE: &str = "This UDF file contains an electronic signature. Pdfrp does not verify the signature and does not carry it over to the PDF; the PDF to be created does not replace the signed document. Where the signed document is required, use and keep the UDF file itself. The appearance may differ from how it looks in the UYAP Document Editor.";

/// What is said of a UDF file without one (the same review's wording).
pub const UDF_NOTE: &str = "The appearance may differ from how it looks in the UYAP Document Editor.";

/// What converting a UDF found besides the pages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UdfInfo {
    /// The file carries an e-signature (`sign.sgn`): it is not checked, and the PDF doesn't have it.
    pub signed: bool,
    pub pages: usize,
    /// What was assumed or left out, in English, one sentence each.
    pub notes: Vec<String>,
}

/// Whether `bytes` named `name` is a UDF file: a `.udf` name and a ZIP container.
pub fn is_udf(name: &str, bytes: &[u8]) -> bool {
    name.to_ascii_lowercase().ends_with(".udf") && bytes.starts_with(b"PK\x03\x04")
}

/// Whether the UDF file `bytes` carries an e-signature (`sign.sgn`). Only the ZIP directory is read.
pub fn udf_signed(bytes: &[u8]) -> bool {
    zip::Archive::open(bytes).is_ok_and(|a| a.find("sign.sgn").is_some())
}

/// The PDF of the UDF file `bytes`, titled `title`. `font_dirs` are the folders installed fonts
/// are in, used when the app hasn't turned the installed fonts on for this thread itself.
pub fn from_udf(title: &str, bytes: &[u8], font_dirs: &[PathBuf]) -> Result<(Document, UdfInfo), CreateError> {
    let archive = zip::Archive::open(bytes).map_err(CreateError::Invalid)?;
    let entry = archive.find("content.xml").ok_or_else(|| CreateError::Invalid("this is not a UDF file: it has no content.xml".into()))?;
    let xml = archive.read(entry).map_err(CreateError::Invalid)?;
    let mut udf = model::read(&xml).map_err(CreateError::Invalid)?;
    let signed = archive.find("sign.sgn").is_some();
    let scoped = !migez_fonts::installed::active() && !font_dirs.is_empty();
    if scoped {
        migez_fonts::installed::set_for_this_thread(Some(font_dirs.to_vec()));
    }
    let made = convert(title, &mut udf, signed);
    if scoped {
        migez_fonts::installed::set_for_this_thread(None);
    }
    let (doc, pages, notes) = made?;
    Ok((doc, UdfInfo { signed, pages, notes }))
}

fn convert(title: &str, udf: &mut model::Udf, signed: bool) -> Result<(Document, usize, Vec<String>), CreateError> {
    let mut doc = Document::new_empty();
    let mut notes = std::mem::take(&mut udf.notes);
    // Pictures first: each becomes an XObject `/Im<n>`, and its run says where it is drawn.
    let mut pictures = Dict::new();
    let mut bad = 0;
    for run in udf.runs_mut() {
        let model::RunKind::Image { png, width, height } = &run.kind else { continue };
        match crate::image_xobject(&mut doc, "picture.png", png) {
            Ok((obj, (w, h))) => {
                let (w, h) = match (*width, *height) {
                    (Some(fw), Some(fh)) => (fw, fh),
                    (Some(fw), None) if w > 0.0 => (fw, h * fw / w),
                    (None, Some(fh)) if h > 0.0 => (w * fh / h, fh),
                    _ => (w, h),
                };
                let index = pictures.len();
                pictures.set(format!("Im{index}").into_bytes(), Object::Ref(obj));
                run.kind = model::RunKind::Placed { index, width: w.clamp(1.0, 14_400.0), height: h.clamp(1.0, 14_400.0) };
            }
            Err(_) => bad += 1,
        }
    }
    if bad > 0 {
        notes.push(format!("{bad} picture(s) that could not be read were left out"));
    }
    let pages = layout::write(&mut doc, udf, &pictures, &mut notes)?;
    let count = pages.len();
    let mut tree = Pages::of(&doc)?;
    for (w, h, resources, content) in pages {
        tree.add(&mut doc, w, h, resources, Some(content));
    }
    tree.finish(&mut doc)?;
    set_info(&mut doc, title, signed.then_some(SIGNED_SUBJECT));
    Ok((doc, count, notes))
}

#[cfg(test)]
mod tests;
