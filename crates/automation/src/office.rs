//! Pdfrp: Word, Excel and PowerPoint to PDF, and PDF to Word, through the Microsoft Office
//! installed on this computer (`migez_engine::ms_office`; Windows only). The interface's File ▸
//! Open and Export ▸ Word use the same engine calls.
//!
//! Safe by default: an output file that exists is never replaced unless `overwrite` says so, and a
//! file from the internet (its Zone.Identifier names the Internet or Restricted zone), which Office
//! opens without Protected View, is refused unless `trust_internet_file` says so; its output then
//! carries the same mark. The conversion waits for Office (up to 120 s for a PDF, 300 s for Word).

use std::path::Path;

use migez_engine::ms_office::{self, OfficeHatasi};

use super::{Args, Automation, Result, ToolError, Value, failed, json};

/// Whether `path` came from the internet (Windows' Zone.Identifier: zone 3 or 4).
fn from_internet(path: &Path) -> bool {
    #[cfg(windows)]
    {
        migez_platform::zone::read_zone(path).is_some_and(|stream| {
            String::from_utf8_lossy(&stream).lines().any(|line| {
                line.split_once('=')
                    .is_some_and(|(key, value)| key.trim().eq_ignore_ascii_case("ZoneId") && value.trim().parse::<u32>().is_ok_and(|id| id >= 3))
            })
        })
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// Office's error as the tool's: what was wrong is in Office's (Turkish) words, and whether to try
/// again is said.
fn office_failed(error: &OfficeHatasi) -> ToolError {
    if ms_office::retryable(error) { failed(format!("{error} (Microsoft Office was busy or slow: try again)")) } else { failed(error.to_string()) }
}

impl Automation {
    /// `office_status`: which Office apps are installed.
    pub(crate) fn office_status(&self) -> Result<Value> {
        let s = ms_office::status();
        Ok(json!({ "windows": cfg!(windows), "word": s.word, "excel": s.excel, "powerpoint": s.powerpoint, "any": s.herhangi() }))
    }

    /// The input and output of an Office conversion, resolved against the roots (`self.resolve`)
    /// and checked before Office is started: the input exists inside the root, the output is a new
    /// file (or `overwrite`) with `extension`, and a file from the internet is converted only when
    /// trusted. (Its name says `resolve`: `file_access`'s test looks for that before a tool touches
    /// the file system.)
    fn resolve_office_paths(&self, a: &Args, extension: &str) -> Result<(std::path::PathBuf, std::path::PathBuf, bool)> {
        let input = self.resolve(a.str("path")?, false)?;
        if !input.is_file() {
            return Err(failed(format!("{} is not a file", input.to_string_lossy())));
        }
        let out = self.resolve(a.str("out")?, true)?;
        if !out.extension().is_some_and(|e| e.eq_ignore_ascii_case(extension)) {
            return Err(ToolError::InvalidArgs(format!("out must end in .{extension}")));
        }
        if out.exists() && !a.opt_bool("overwrite")?.unwrap_or(false) {
            return Err(failed(format!("out already exists ({}); pass overwrite: true to replace it", out.to_string_lossy())));
        }
        let internet = from_internet(&input);
        if internet && !a.opt_bool("trust_internet_file")?.unwrap_or(false) {
            return Err(failed(
                "the input came from the internet, and Office opens it to convert it without Protected View (macros stay off); pass trust_internet_file: true only for a file you trust",
            ));
        }
        if let Some(folder) = out.parent().filter(|f| !f.as_os_str().is_empty()) {
            std::fs::create_dir_all(folder).map_err(failed)?;
        }
        Ok((input, out, internet))
    }

    /// `office_to_pdf`: a Word, Excel or PowerPoint file to PDF with the installed Office.
    pub(crate) fn office_to_pdf(&self, a: &Args) -> Result<Value> {
        let (input, out, internet) = self.resolve_office_paths(a, "pdf")?;
        let app = ms_office::uygulama_sec(&input).ok_or_else(|| {
            ToolError::InvalidArgs(
                "path must be a Word (doc, docx, rtf, odt, txt), Excel (xls, xlsx, ods, csv) or PowerPoint (ppt, pptx, odp) file".into(),
            )
        })?;
        ms_office::to_pdf(&input, &out).map_err(|e| office_failed(&e))?;
        mark(&out, &input, internet);
        let pages = std::fs::read(&out).ok().and_then(|bytes| migez_engine::source_page_count(&std::sync::Arc::new(bytes), None));
        Ok(json!({ "path": out.to_string_lossy(), "app": app.ad(), "pages": pages, "marked_from_internet": internet }))
    }

    /// `office_pdf_to_word`: a PDF to a Word document with the installed Word, which rebuilds the
    /// pages as editable text and keeps their layout.
    pub(crate) fn office_pdf_to_word(&self, a: &Args) -> Result<Value> {
        let (input, out, internet) = self.resolve_office_paths(a, "docx")?;
        if !input.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
            return Err(ToolError::InvalidArgs("path must be a PDF file".into()));
        }
        ms_office::pdf_to_word(&input, &out).map_err(|e| office_failed(&e))?;
        mark(&out, &input, internet);
        let bytes = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
        Ok(json!({ "path": out.to_string_lossy(), "app": "Microsoft Word", "bytes": bytes, "marked_from_internet": internet }))
    }
}

/// Give `out` the internet mark of `input` when it came from the internet (best effort: a volume
/// without alternate data streams can't carry it).
fn mark(out: &Path, input: &Path, internet: bool) {
    if internet {
        let _ = migez_platform::zone::mark_from(out, Some(input));
    }
}
