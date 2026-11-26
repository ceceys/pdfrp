//! Pdfrp: Word, Excel and PowerPoint to PDF, and PDF to Word, through the Microsoft Office
//! installed on this computer (migez-office: COM automation, Windows only). Where Office (or
//! the app a file needs) isn't installed, Office documents keep going through the LibreOffice
//! sidecar ([`crate::office_to_pdf`]) and PDFs export to Word with Pdfrp's own writer.
//!
//! The interface and the automation tools share what is decided here: which files go to Office,
//! whether it is installed (read once from the registry; Office isn't started for that), and the
//! conversions themselves. A conversion waits for Office for up to [`PDF_SURESI`] or
//! [`WORD_SURESI`], so callers run it off the frame thread, one at a time (migez-office also
//! queues them: a second one waits, and says [`OfficeHatasi::Mesgul`] if its time runs out).
//!
//! Office opens the file it converts without Protected View, also when it came from the
//! internet; macros are off. The callers warn about such a file and give the result its mark.

use std::path::Path;
use std::sync::OnceLock;

pub use migez_office::{OfficeDurumu, OfficeHatasi, PDF_SURESI, Uygulama, WORD_SURESI, uygulama_sec};

/// The installed Office apps, read once per process.
static STATUS: OnceLock<OfficeDurumu> = OnceLock::new();

/// The installed Office apps: read from the registry the first time (a few registry keys, no
/// Office started), then remembered for the session. Off Windows, none.
pub fn status() -> OfficeDurumu {
    *STATUS.get_or_init(migez_office::mevcut)
}

/// [`status`] if it has been read already, without reading it.
pub fn status_now() -> Option<OfficeDurumu> {
    STATUS.get().copied()
}

/// Read [`status`] on a thread of its own, so the first Open of an Office file finds it known.
/// The app calls this once at startup; later calls do nothing.
pub fn read_status_in_background() {
    if STATUS.get().is_some() {
        return;
    }
    // A thread that can't start leaves the status to be read on first use, which is quick too.
    let _ = std::thread::Builder::new().name("migez-office-status".into()).spawn(|| {
        let _ = status();
    });
}

/// The Office app that converts `path` to PDF on this computer: the file's extension is one an
/// Office app converts (Word: doc, docx, rtf, odt; Excel: xls, xlsx, ods, csv; PowerPoint: ppt,
/// pptx, odp, in any case) and that app is installed. Plain text (`.txt`) is left to Pdfrp's
/// own conversion. `None` sends the file the usual way (LibreOffice, or not an Office file).
pub fn converts_to_pdf(path: &Path) -> Option<Uygulama> {
    let app = uygulama_sec(path)?;
    let text = path.extension().is_some_and(|e| e.eq_ignore_ascii_case("txt"));
    (!text && status().var_mi(app)).then_some(app)
}

/// Whether PDFs can be turned into Word documents here: Microsoft Word is installed.
pub fn converts_pdf_to_word() -> bool {
    status().word
}

/// Convert the Word, Excel or PowerPoint file `input` to the PDF `output` with Office (the app
/// is chosen by the extension). `output` is replaced if it exists, in one step, never left half
/// written; its folder must exist. Waits up to [`PDF_SURESI`].
pub fn to_pdf(input: &Path, output: &Path) -> Result<(), OfficeHatasi> {
    migez_office::pdfe_donustur(input, output)
}

/// Convert the PDF `input` to the Word document `output` (`.docx`) with Word, which keeps the
/// layout as editable text. `output` is replaced if it exists, in one step; its folder must exist.
/// Waits up to [`WORD_SURESI`].
pub fn pdf_to_word(input: &Path, output: &Path) -> Result<(), OfficeHatasi> {
    migez_office::pdften_worde(input, output)
}

/// Whether trying again may help: the conversion ran out of time, or another one was still
/// using Office.
pub fn retryable(error: &OfficeHatasi) -> bool {
    matches!(error, OfficeHatasi::ZamanAsimi { .. } | OfficeHatasi::Mesgul)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Plain text stays Pdfrp's own conversion, an unknown file goes nowhere, and only an
    /// installed app takes a file (whatever this computer has).
    #[test]
    fn only_office_files_an_installed_app_converts_go_to_office() {
        assert_eq!(converts_to_pdf(Path::new("not.txt")), None);
        assert_eq!(converts_to_pdf(Path::new("picture.png")), None);
        assert_eq!(converts_to_pdf(Path::new("scan.pdf")), None);
        let installed = status();
        assert_eq!(converts_to_pdf(Path::new("Rapor.DOCX")).is_some(), installed.word);
        assert_eq!(converts_to_pdf(Path::new("Tablo.xlsx")).is_some(), installed.excel);
        assert_eq!(converts_to_pdf(Path::new("Sunum.pptx")).is_some(), installed.powerpoint);
        assert_eq!(converts_pdf_to_word(), installed.word);
        assert_eq!(status_now(), Some(installed), "remembered once read");
    }

    /// "Try again" is offered exactly for a conversion that ran out of time or found Office busy.
    #[test]
    fn only_a_timeout_or_a_busy_office_is_worth_retrying() {
        assert!(retryable(&OfficeHatasi::ZamanAsimi { saniye: 120 }));
        assert!(retryable(&OfficeHatasi::Mesgul));
        assert!(!retryable(&OfficeHatasi::ParolaKorumali));
        assert!(!retryable(&OfficeHatasi::UygulamaYok(Uygulama::Word)));
    }
}
