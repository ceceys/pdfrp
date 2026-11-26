//! Pdfrp: Edit text's installed-font fallback ([`migez_fonts::installed`]). When a
//! paragraph's own font (a subset a Word PDF embeds, say) can't draw the new text, the installed
//! face that is that font — the same PostScript name, or the same family in the same weight and
//! slant, never a nearby family — is embedded into the user's document as a subset of the glyphs
//! the text needs, within the face's own embedding permission, and draws it. Only when the app has
//! turned the fallback on; otherwise, and when no such face may or can draw the text, the earlier
//! fallbacks (a standard font, Pdfrp's own OFL faces) stand in as before.

use migez_cos::{Dict, Document};
use migez_fonts::installed::InstalledFace;
use migez_fonts::pdf::Metrics;

/// An installed face embedded for a rewrite.
pub(crate) struct Installed {
    /// Its font resource, among the page's fonts.
    pub resource: String,
    pub metrics: Metrics,
    /// How the rewrite's result names it ([`crate::LineEdit::substituted`]).
    pub label: String,
}

/// How a rewrite's result names an installed face it drew with.
pub(crate) fn label(face: &InstalledFace) -> String {
    format!("{} (installed)", face.family)
}

/// The installed face that is `base_font` (with `bold` and `italic`), embedded into `doc` as a font
/// resource in `fonts` for drawing `text`. `None`, and nothing changed, when the fallback is off or
/// no installed face may and can draw `text`.
pub(crate) fn embed(doc: &mut Document, fonts: &mut Dict, base_font: &str, bold: bool, italic: bool, text: &str) -> Option<Installed> {
    if !migez_fonts::installed::active() {
        return None;
    }
    let embedded = migez_fonts::installed::embed_for(doc, fonts, base_font, bold, italic, text)?;
    let font = fonts.get(embedded.resource.as_bytes()).and_then(|r| doc.dict(r))?;
    Some(Installed { metrics: Metrics::from_dict(doc, &font), label: label(&embedded.face), resource: embedded.resource })
}
