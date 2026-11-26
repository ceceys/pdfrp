# migez-ocr-windows

Scan & OCR with the text recognition built into Windows 10 and 11 (`Windows.Media.Ocr`), as a
`migez_ocr::Recognizer` beside the ocrs and Tesseract engines. It reads Turkish (ç ğ ı İ ö ş
ü), which the ocrs models cannot read and Tesseract's language allow-list does not offer. Nothing
is downloaded or bundled: Windows provides the engine and each language's recognition data.

**Windows only.** On every other platform the crate is empty and has no dependencies, so a
dependent names it under `[target.'cfg(windows)'.dependencies]`.

```rust
use migez_ocr::{OcrImage, RecognizeOptions, Recognizer};
use migez_ocr_windows::WindowsOcr;

let engine = WindowsOcr::new();                      // Turkish unless a call names a language
engine.check_language("tr")?;                        // Ok("tr"), or the error naming what to install
let image = OcrImage::new(width, height, rgba)?;     // the rendered page, as for any engine
let lines = engine.recognize(&image, &RecognizeOptions { language: "tr".into() })?;
for word in lines.iter().flat_map(|line| &line.words) {
    // word.text; word.rect = [left, top, right, bottom] in `image` pixels;
    // word.source == "windows"; word.confidence == Some(ASSUMED_CONFIDENCE)
}
```

## API

| Item | What it does |
| --- | --- |
| `WindowsOcr::new()`, `Default` | The engine: Turkish by default, `ASSUMED_CONFIDENCE` on every word. Holds no Windows object; share it as `Arc<dyn Recognizer>`. |
| `with_default_language(code)` | The language read when `RecognizeOptions::language` is empty (an empty code keeps `"tr"`). |
| `with_confidence(Option<f32>)` | The confidence every word carries; `None` reports none, like ocrs. |
| `with_max_side(px)` | Read at most `px` pixels on a side (Windows' own limit still applies): lighter, less accurate on small print. The test page (14 pt) reads as well at about 212 dpi as at 300, and starts losing capitals at 188. |
| `default_language()` | The language read when a call names none. |
| `check_language(code)` | The installed Windows tag that reads `code` (empty: the default), or the error `recognize` would give. |
| `WindowsOcr::max_image_dimension()` | Windows' largest readable side (`OcrEngine::MaxImageDimension`). Windows 11 build 26300 says 10 000, the same as `migez_ocr::MAX_SIDE`, so there only `with_max_side` shrinks a page. |
| `Recognizer::id()` | `"windows"` (`ENGINE_ID`), also every word's `source`. |
| `Recognizer::available()` | Windows OCR is present with at least one language installed. |
| `Recognizer::languages()` | The installed languages as (Windows tag, name in the Windows display language): `("tr", "Türkçe")`. Remembered for 30 s. |
| `Recognizer::recognize(image, options)` | The words in reading order, boxes in `image` pixels. |
| `DEFAULT_LANGUAGE`, `ASSUMED_CONFIDENCE`, `TIMEOUT`, `MAX_WORDS` | `"tr"`, `80.0`, 120 s, 50 000. |

## How it reads

- **Language.** `options.language` when set, else the default (`"tr"`). It is matched against
  `OcrEngine::AvailableRecognizerLanguages`: the same tag, then a narrower one (`en` → `en-US`),
  then the same language in another region (`en-GB` → `en-US`), never another script
  (`zh-Hans` is not read by `zh-Hant`). A language with no match is an error, never a fallback
  to another language: it names the language and the Windows setting, in English and in Turkish
  (Settings > Time & language > Language & region > *language* > Language options > Optical
  character recognition; Ayarlar > Saat ve dil > Dil ve bölge > *dil* > Dil seçenekleri > Optik
  karakter tanıma).
- **Threads.** Every Windows Runtime call runs on a short-lived thread that never initialises
  COM, so it is in the process's multithreaded apartment (windows-core creates the MTA when no
  thread has). The calling thread, whatever its apartment, only waits for that thread; a panic
  there comes back as an error. No `unsafe` code.
- **Waiting.** `RecognizeAsync` is awaited like windows-future's `join()` (the 0.62 name of the
  older `get()`): a completion handler and a blocking wait, here limited to `TIMEOUT`. Past it the
  operation is cancelled and the call fails.
- **Pixels.** The RGBA raster (premultiplied, as the renderer makes it) is laid over white paper
  and turned into 8-bit grey (the weights `OcrImage::gray` uses), then handed to Windows as a
  `Gray8` bitmap. Larger than Windows' limit (or `with_max_side`), it is shrunk by area
  averaging first.
- **Boxes.** Windows measures boxes in the bitmap it read, and for slanted text in that bitmap
  turned straight (`OcrResult::TextAngle`). Each box is turned back by the slant about the
  bitmap's centre, scaled back to the caller's size on each axis, and clamped to the image; a
  box that is not finite or covers nothing is dropped, and so is a word without text. Boxes are
  axis-aligned: a slanted word gets the box around it.
- **Bounds.** At most `MAX_WORDS` words: a reading with more is rejected, not truncated.

## Wiring it into the interface

Not done in this crate: the engine (`EngineChoice::Windows` in `crates/engine/src/ocr.rs`), the
Recognize Text dialog, the OCR Verify screen and the `ocr_recognize` tool (`engine: "windows"`)
use it, with `with_confidence(None)`. What to watch for:

1. **Layering.** `cargo xtask layers` fails until the crate is registered in
   `xtask/src/layers.rs`: `("ocr-windows", Class::Layer(4))` in `TABLE`, and in `SIDEWAYS`
   `("ocr-windows", "ocr")` and, for the tests, `("ocr-windows", "create")`.
2. **Engine choice.** In `crates/engine/src/ocr.rs`: an `EngineChoice` for it (name
   `"windows"`), a `recognizers` arm that builds `Arc::new(WindowsOcr::new())`, and an
   `engines_status_for` entry whose `why_not` is `check_language`'s error. The dependency goes
   under `[target.'cfg(windows)'.dependencies]`.
3. **Language.** `OcrSettings::default()` and the dialog's default are `"en"`, and the dialog
   lists only `migez_ocr::LANGUAGES` (`en`) and drops a saved language outside it
   (`crates/ui-egui/src/workflows/ocr_ui.rs`). With this engine the list should come from
   `WindowsOcr::languages()` (Windows tags: `"tr"`, `"en-US"`), the default should be `"tr"` (or
   empty, which means `DEFAULT_LANGUAGE`), and `check_language` should run when the engine or
   the language is chosen, so a missing language is said before a job starts. Asked for `"en"`
   on a machine with only Turkish installed, the engine fails rather than reading Turkish.
4. **Confidence.** Windows reports none. Every word carries `ASSUMED_CONFIDENCE` (80), in the
   review's Medium band: no word is flagged Low or by the batch rule (60), the band colour says
   nothing about the word, and in a merge 80 beats an ocrs word (unknown confidence, bar 70) and
   loses to a Tesseract word above 80. `with_confidence(None)` makes the words unknown instead,
   the preferred reading of unreported confidences; then an ocrs word keeps its place in a
   merge. For Turkish pages read the engine alone (`MergeStrategy::PrimaryOnly`): ocrs cannot read
   Turkish, and a merge would put its guesses beside Windows' words.
5. **Threads.** `recognize` blocks its caller for the reading (a page takes well under a
   second): call it from the OCR job's worker thread, never the UI thread. `languages` and
   `available` start a thread and wait a few milliseconds the first time in 30 seconds.
6. **Output text.** Searchable output keeps every Turkish letter (the engine's invisible layer
   writes characters beyond WinAnsi through Type3 fonts with `/ToUnicode`). The editable output
   draws ğ Ğ ı İ ş Ş only from a craft-fonts face that has them: in a build without
   `MIGEZ_FONTS_DIR` they become `?` (counted in the output notes).
7. **Messages.** Errors are English, like the rest of `OcrError`; the missing-language one adds
   the Turkish Windows path. The interface can translate them or show `check_language`'s error
   in its own words.

## Limits

- Needs Windows 10 or 11 with the language's optical character recognition feature installed;
  Windows editions without `Windows.Media.Ocr` report the engine unavailable.
- No confidence (see above), no paragraphs or blocks: lines and words only, in the order
  Windows reads them.
- Word boxes are axis-aligned even for slanted text.
- Cancelling (past `TIMEOUT`) is a request to Windows: the reading may go on inside Windows for a
  while; the call has already returned its error.
- Accents on capitals in words no dictionary knows are the weakest spot: Windows 11 (build
  26300) reads the test's capital row ÇĞİÖŞÜ as "ÇGIOŞÜ", and every Turkish letter of the real
  words right.
