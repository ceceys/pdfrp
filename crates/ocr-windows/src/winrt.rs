//! The Windows Runtime side. Everything here runs on a thread [`on_mta`] started for it, and
//! every Windows object is made, used and released on that thread.

use std::sync::mpsc::{RecvTimeoutError, channel};
use std::time::Duration;

use migez_ocr::OcrError;
use windows::Globalization::Language;
use windows::Graphics::Imaging::{BitmapPixelFormat, SoftwareBitmap};
use windows::Media::Ocr::{OcrEngine, OcrResult};
use windows::Storage::Streams::DataWriter;
use windows::core::HSTRING;
use windows_future::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation};

use crate::raster::Grey;
use crate::{MAX_WORDS, language};

/// At most this many installed languages are listed (Windows offers a few dozen).
const MAX_LANGUAGES: u32 = 1024;

/// Run `work` on a new thread and wait for it. The thread never initialises COM, so its Windows
/// Runtime calls run in the process's multithreaded apartment: when no thread has created the
/// MTA yet, windows-core creates it (`CoIncrementMTAUsage`) the first time an activation
/// factory reports COM uninitialised. The caller's own apartment (a UI's STA, say) never sees a
/// Windows object or a blocking Windows wait; it only joins this thread. A panic on the thread
/// comes back as an error instead of unwinding into the caller.
pub(crate) fn on_mta<T: Send>(work: impl FnOnce() -> Result<T, OcrError> + Send) -> Result<T, OcrError> {
    std::thread::scope(|scope| {
        let thread = std::thread::Builder::new()
            .name("migez-ocr-windows".into())
            .spawn_scoped(scope, work)
            .map_err(|e| OcrError::Recognize(format!("Windows OCR: could not start its thread: {e}")))?;
        thread.join().unwrap_or_else(|_| Err(OcrError::Recognize("Windows OCR: its thread stopped unexpectedly".into())))
    })
}

/// A Windows error as an [`OcrError`] naming what was being done. A null result (Windows
/// returned no object) has no error code of its own and is said so.
fn failed(what: &'static str) -> impl Fn(windows::core::Error) -> OcrError {
    move |e| {
        let code = e.code().0;
        if code == 0 {
            OcrError::Recognize(format!("Windows OCR: {what} returned nothing"))
        } else {
            OcrError::Recognize(format!("Windows OCR: {what} failed ({:#010x}): {}", code as u32, e.message().trim()))
        }
    }
}

/// An installed recognition language: its Windows tag (`"tr"`, `"en-US"`), its name in the
/// Windows display language (`"Türkçe"`), and the Windows object.
pub(crate) struct Installed {
    pub tag: String,
    pub name: String,
    language: Language,
}

/// The languages Windows recognises here (`OcrEngine::AvailableRecognizerLanguages`).
fn installed() -> Result<Vec<Installed>, OcrError> {
    let list = OcrEngine::AvailableRecognizerLanguages().map_err(failed("listing the installed languages"))?;
    let count = list.Size().map_err(failed("counting the installed languages"))?.min(MAX_LANGUAGES);
    let mut out = Vec::with_capacity(count as usize);
    for i in 0..count {
        let language = list.GetAt(i).map_err(failed("reading an installed language"))?;
        let tag = language.LanguageTag().map_err(failed("reading a language tag"))?.to_string_lossy();
        let name = language.DisplayName().map(|n| n.to_string_lossy()).unwrap_or_default();
        let name = if name.trim().is_empty() { tag.clone() } else { name };
        out.push(Installed { tag, name, language });
    }
    Ok(out)
}

/// The installed languages as (tag, name) pairs.
pub(crate) fn languages() -> Result<Vec<(String, String)>, OcrError> {
    Ok(installed()?.into_iter().map(|l| (l.tag, l.name)).collect())
}

/// The installed language that reads `wanted` ([`language::pick`]), or the error naming the
/// language that is missing.
pub(crate) fn resolve(wanted: &str) -> Result<Installed, OcrError> {
    let mut all = installed()?;
    let tags: Vec<&str> = all.iter().map(|l| l.tag.as_str()).collect();
    match language::pick(wanted, &tags) {
        Some(i) if i < all.len() => Ok(all.swap_remove(i)),
        _ => Err(language::missing(wanted, display_name(wanted).as_deref())),
    }
}

/// What Windows calls the language `code` in its display language, when `code` is a
/// well-formed tag (only then is it handed to Windows).
fn display_name(code: &str) -> Option<String> {
    let code = code.trim();
    if !language::plausible(code) {
        return None;
    }
    let tag = HSTRING::from(code);
    if !Language::IsWellFormed(&tag).unwrap_or(false) {
        return None;
    }
    let name = Language::CreateLanguage(&tag).and_then(|l| l.DisplayName()).ok()?.to_string_lossy();
    (!name.trim().is_empty()).then_some(name)
}

/// The largest side Windows reads (`OcrEngine::MaxImageDimension`).
pub(crate) fn max_image_dimension() -> Result<u32, OcrError> {
    OcrEngine::MaxImageDimension().map_err(failed("reading the largest image size"))
}

/// What Windows read: for each line, each word's text and box `[x, y, width, height]` in the
/// pixels of the bitmap it read, and the text's slant in degrees when it reported one (the boxes
/// are then measured with the slant straightened; see `geometry`).
pub(crate) struct Reading {
    pub lines: Vec<Vec<(String, [f32; 4])>>,
    pub angle: Option<f64>,
}

/// Read `grey` in `language`, waiting at most `timeout`.
pub(crate) fn read(language: &Installed, grey: &Grey, timeout: Duration) -> Result<Reading, OcrError> {
    let engine = OcrEngine::TryCreateFromLanguage(&language.language).map_err(failed("starting the recogniser"))?;
    let bitmap = bitmap(grey)?;
    let operation = engine.RecognizeAsync(&bitmap).map_err(failed("starting recognition"))?;
    // On an error (a timeout above all) the bitmap is only released, never closed: a cancelled
    // reading may still be using it inside Windows, which keeps its own reference.
    let result = wait(&operation, timeout)?;
    // Read through: the bitmap's memory goes now rather than whenever Windows lets go of it.
    let _ = bitmap.Close();
    collect(&result)
}

/// `grey` as a Windows bitmap (8-bit grey, rows packed).
fn bitmap(grey: &Grey) -> Result<SoftwareBitmap, OcrError> {
    let size = |side: u32| i32::try_from(side).map_err(|_| OcrError::ImageTooLarge);
    let (width, height) = (size(grey.width)?, size(grey.height)?);
    let writer = DataWriter::new().map_err(failed("making the image buffer"))?;
    // At most `migez_ocr::MAX_PIXELS` bytes (one per pixel), well under the `u32` length
    // `WriteBytes` converts it to.
    writer.WriteBytes(&grey.pixels).map_err(failed("filling the image buffer"))?;
    let buffer = writer.DetachBuffer().map_err(failed("taking the image buffer"))?;
    SoftwareBitmap::CreateCopyFromBuffer(&buffer, BitmapPixelFormat::Gray8, width, height).map_err(failed("making the bitmap"))
}

/// Wait for `operation` the way windows-future's `join()` does (a completion handler, then
/// `GetResults`), but at most `timeout`: past it the operation is cancelled and the wait fails.
fn wait(operation: &IAsyncOperation<OcrResult>, timeout: Duration) -> Result<OcrResult, OcrError> {
    let status = operation.Status().map_err(failed("asking how recognition is going"))?;
    if status == AsyncStatus::Started {
        let (done, finished) = channel();
        // Windows calls the handler once, on a thread of its own (or at once, when the reading
        // has already finished). It cannot panic: a failed send only means the wait gave up.
        let handler = AsyncOperationCompletedHandler::<OcrResult>::new(move |_, status| {
            let _ = done.send(status);
            Ok(())
        });
        operation.SetCompleted(&handler).map_err(failed("waiting for recognition"))?;
        match finished.recv_timeout(timeout) {
            Ok(AsyncStatus::Canceled) => return Err(OcrError::Recognize("Windows OCR: recognition was cancelled".into())),
            // Completed, or failed: `GetResults` returns the result or the failure.
            Ok(_) => {}
            Err(RecvTimeoutError::Timeout) => {
                let _ = operation.Cancel();
                return Err(OcrError::Recognize(format!("Windows OCR did not finish within {} seconds", timeout.as_secs())));
            }
            // The handler was released without being called: the status says what happened.
            Err(RecvTimeoutError::Disconnected) => {}
        }
    }
    let result = operation.GetResults().map_err(failed("recognition"));
    let _ = operation.Close();
    result
}

/// The lines and words of `result`, at most [`MAX_WORDS`] words: a reading with more is an
/// error, not a truncated page.
fn collect(result: &OcrResult) -> Result<Reading, OcrError> {
    let too_many = || OcrError::Recognize(format!("Windows OCR: the {MAX_WORDS}-word limit was exceeded; the incomplete reading was rejected"));
    let angle = result.TextAngle().ok().and_then(|a| a.Value().ok()).filter(|a| a.is_finite());
    let lines = result.Lines().map_err(failed("reading the lines"))?;
    let count = lines.Size().map_err(failed("counting the lines"))?;
    if count as usize > MAX_WORDS {
        return Err(too_many());
    }
    let mut out = Vec::with_capacity(count as usize);
    let mut total = 0usize;
    for i in 0..count {
        let words = lines.GetAt(i).and_then(|line| line.Words()).map_err(failed("reading a line"))?;
        let n = words.Size().map_err(failed("counting a line's words"))?;
        total = total.saturating_add(n as usize);
        if total > MAX_WORDS {
            return Err(too_many());
        }
        let mut line = Vec::with_capacity(n as usize);
        for j in 0..n {
            let word = words.GetAt(j).map_err(failed("reading a word"))?;
            let text = word.Text().map_err(failed("reading a word's text"))?.to_string_lossy();
            let r = word.BoundingRect().map_err(failed("reading a word's box"))?;
            line.push((text, [r.X, r.Y, r.Width, r.Height]));
        }
        out.push(line);
    }
    Ok(Reading { lines: out, angle })
}
