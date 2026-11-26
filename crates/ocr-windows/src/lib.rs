//! migez-ocr-windows — Scan & OCR with the text recognition built into Windows
//! (`Windows.Media.Ocr`), as a [`Recognizer`] beside the ocrs and Tesseract engines (L4).
//!
//! Windows reads every language whose optical character recognition feature is installed, and
//! this engine asks for Turkish unless told otherwise ([`DEFAULT_LANGUAGE`]). Nothing is
//! downloaded or bundled: the engine is part of Windows 10 and 11. The crate is empty on every
//! other platform.
//!
//! - **Apartments.** Every Windows Runtime call runs on a short-lived thread of its own that
//!   never initialises COM, so it is in the process's multithreaded apartment (windows-core
//!   creates the MTA on first use when nothing has). The caller may be any thread, a UI's
//!   single-threaded apartment included: no Windows object or call ever reaches the caller's
//!   apartment, so waiting cannot deadlock it. (The caller is still blocked while the page is
//!   read: keep recognition off the UI thread.) No `unsafe` is needed (the workspace forbids it).
//! - **Waiting.** `RecognizeAsync` is awaited the way windows-future's `join()` (formerly `get()`)
//!   waits, a completion handler and a blocking wait, but with a time limit ([`TIMEOUT`]); past
//!   it the operation is cancelled and the call fails.
//! - **Size.** An image larger than `OcrEngine::MaxImageDimension` is shrunk (area average)
//!   before it is read, and every box is mapped back to the caller's pixels, as is the slant
//!   Windows reports for skewed text.
//! - **Confidence.** Windows reports none: every word carries [`ASSUMED_CONFIDENCE`], or nothing
//!   with [`WindowsOcr::with_confidence`]`(None)`. See the README before relying on it.
//! - **Languages.** A language that is not installed is an error naming it and the Windows
//!   setting that installs it, never a silent fallback to another language.

#![cfg(windows)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod geometry;
mod language;
mod raster;
mod winrt;

use std::sync::{Mutex, PoisonError};
use std::time::{Duration, Instant};

use migez_ocr::{Line, OcrError, OcrImage, RecognizeOptions, Recognizer, Word};

/// The engine's name: what its words carry as [`Word::source`] and what settings name it by.
pub const ENGINE_ID: &str = "windows";

/// The language read when [`RecognizeOptions::language`] is empty (and no other default was set
/// with [`WindowsOcr::with_default_language`]).
pub const DEFAULT_LANGUAGE: &str = "tr";

/// The confidence every word carries, on the 0–100 scale of [`Word::confidence`]. Windows
/// reports no confidence at all, so this is not a measurement: it is a fixed value in the
/// Medium band (70–89, `migez_ocr::band_for`). It keeps the words out of the Low band and
/// the batch flag (60 and below), lets them win a ROVER merge against an engine that reports
/// no confidence (ocrs; the bar there is 70), and never claims High. Callers that would rather
/// treat the confidence as unknown use [`WindowsOcr::with_confidence`]`(None)`.
pub const ASSUMED_CONFIDENCE: f32 = 80.0;

/// How long one recognition may run before it is cancelled and reported as an error. A page
/// takes well under a second; this only stops a reading that never ends.
pub const TIMEOUT: Duration = Duration::from_secs(120);

/// The most words one reading may hold. A reading with more is rejected rather than truncated,
/// so a page is never applied half read.
pub const MAX_WORDS: usize = 50_000;

/// How long the installed-language list behind [`Recognizer::languages`] and
/// [`Recognizer::available`] is remembered: those may be asked often (an interface checks them
/// as it draws), and an OCR feature installed while the app runs shows up after this.
const LANGUAGES_TTL: Duration = Duration::from_secs(30);

/// The installed languages as (Windows tag, display name), and when they were listed.
type ListedLanguages = Option<(Instant, Vec<(String, String)>)>;

/// The Windows OCR engine. Cheap to build and to share (`Arc<dyn Recognizer>`): it holds no
/// Windows object, each call starts its own.
pub struct WindowsOcr {
    default_language: String,
    confidence: Option<f32>,
    /// A tighter limit on the side of the image Windows reads than Windows' own.
    max_side: Option<u32>,
    timeout: Duration,
    /// The installed languages and when they were listed.
    languages: Mutex<ListedLanguages>,
}

impl Default for WindowsOcr {
    fn default() -> Self {
        WindowsOcr::new()
    }
}

impl WindowsOcr {
    /// The engine, reading Turkish ([`DEFAULT_LANGUAGE`]) when a call names no language, with
    /// [`ASSUMED_CONFIDENCE`] on every word.
    pub fn new() -> WindowsOcr {
        WindowsOcr {
            default_language: DEFAULT_LANGUAGE.to_string(),
            confidence: Some(ASSUMED_CONFIDENCE),
            max_side: None,
            timeout: TIMEOUT,
            languages: Mutex::new(None),
        }
    }

    /// Read `code` (a language tag: `"tr"`, `"en-US"`) when a call names no language. An empty
    /// code keeps [`DEFAULT_LANGUAGE`].
    #[must_use]
    pub fn with_default_language(mut self, code: &str) -> WindowsOcr {
        let code = code.trim();
        self.default_language = if code.is_empty() { DEFAULT_LANGUAGE } else { code }.to_string();
        self
    }

    /// The confidence every word carries: `Some(c)` (clamped to 0–100, as
    /// [`Word::with_confidence`] does), or `None` to report none, like the ocrs engine.
    #[must_use]
    pub fn with_confidence(mut self, confidence: Option<f32>) -> WindowsOcr {
        self.confidence = confidence;
        self
    }

    /// Read images at most `max_side` pixels on a side (at least 1), shrinking larger ones;
    /// Windows' own limit still applies when it is smaller. Faster and lighter on memory, at
    /// some cost in accuracy for small print. Boxes always come back in the caller's pixels.
    #[must_use]
    pub fn with_max_side(mut self, max_side: u32) -> WindowsOcr {
        self.max_side = Some(max_side.max(1));
        self
    }

    /// The language read when a call names none.
    pub fn default_language(&self) -> &str {
        &self.default_language
    }

    /// The installed Windows language that reads `code` (empty: the default language), as its
    /// Windows tag (`"tr"` → `"tr"`, `"en"` → `"en-US"`), or the error [`Recognizer::recognize`]
    /// would return: which language is missing and the Windows setting that installs it. An
    /// interface can ask this before it starts a job, to say so up front.
    pub fn check_language(&self, code: &str) -> Result<String, OcrError> {
        let wanted = self.wanted(code);
        winrt::on_mta(|| winrt::resolve(wanted).map(|installed| installed.tag))
    }

    /// The largest side, in pixels, Windows reads without this crate shrinking the image first
    /// (`OcrEngine::MaxImageDimension`).
    pub fn max_image_dimension() -> Result<u32, OcrError> {
        winrt::on_mta(winrt::max_image_dimension)
    }

    /// The language a call asks for: its own when it names one, else the default.
    fn wanted<'a>(&'a self, code: &'a str) -> &'a str {
        let code = code.trim();
        if code.is_empty() { &self.default_language } else { code }
    }
}

impl Recognizer for WindowsOcr {
    fn id(&self) -> &'static str {
        ENGINE_ID
    }

    /// Whether Windows can recognise text here: its OCR is present and has at least one
    /// language installed. Whether it has the language a job asks for is
    /// [`WindowsOcr::check_language`].
    fn available(&self) -> bool {
        !self.languages().is_empty()
    }

    /// The installed recognition languages as (Windows tag, display name in the Windows
    /// language): `("tr", "Türkçe")`. Empty when Windows has no OCR or it could not be asked.
    /// Remembered for 30 seconds.
    fn languages(&self) -> Vec<(String, String)> {
        if let Some((at, list)) = self.languages.lock().unwrap_or_else(PoisonError::into_inner).as_ref()
            && at.elapsed() < LANGUAGES_TTL
        {
            return list.clone();
        }
        // Asked without the lock held: concurrent callers may both ask, and both answers are
        // valid.
        let list = winrt::on_mta(winrt::languages).unwrap_or_default();
        *self.languages.lock().unwrap_or_else(PoisonError::into_inner) = Some((Instant::now(), list.clone()));
        list
    }

    /// Read the words in `image`, in Windows' reading order, with boxes in `image`'s pixels.
    fn recognize(&self, image: &OcrImage, options: &RecognizeOptions) -> Result<Vec<Line>, OcrError> {
        let wanted = self.wanted(&options.language);
        let (reading, read) = winrt::on_mta(|| {
            // The language first: a missing one is the likeliest failure, and costs nothing.
            let language = winrt::resolve(wanted)?;
            let limit = winrt::max_image_dimension()?;
            if limit == 0 {
                return Err(OcrError::Recognize("Windows OCR reports no image size it can read".into()));
            }
            let grey = raster::grey(image, self.max_side.map_or(limit, |side| side.min(limit)));
            Ok((winrt::read(&language, &grey, self.timeout)?, (grey.width, grey.height)))
        })?;
        Ok(lines(reading, read, (image.width(), image.height()), self.confidence))
    }
}

/// The words Windows read, as [`Line`]s in the original image's pixels: words without text or
/// with a box that covers nothing are left out, and so are lines left without words.
fn lines(reading: winrt::Reading, read: (u32, u32), original: (u32, u32), confidence: Option<f32>) -> Vec<Line> {
    let angle = reading.angle;
    reading
        .lines
        .into_iter()
        .map(|words| Line {
            words: words
                .into_iter()
                .filter(|(text, _)| !text.trim().is_empty())
                .filter_map(|(text, rect)| {
                    let rect = geometry::to_original(rect, angle, read, original)?;
                    let word = Word::new(text, rect, ENGINE_ID);
                    Some(match confidence {
                        Some(c) => word.with_confidence(c),
                        None => word,
                    })
                })
                .collect(),
        })
        .filter(|line| !line.words.is_empty())
        .collect()
}

#[cfg(test)]
mod tests;
