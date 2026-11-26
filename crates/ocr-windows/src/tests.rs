//! End to end through Windows: a Turkish line is set as a PDF page (migez-create), rendered
//! to pixels (migez-render) as the engine renders a page for recognition, and read back.
//! Every test here skips (with a note) where Windows has no Turkish text recognition installed.

use std::sync::{Arc, OnceLock};

use migez_ocr::{OcrImage, RecognizeOptions, Recognizer};

use super::*;

/// The line every reading is checked against: each Turkish letter, in words and alone.
const SAMPLE: &str = "Çığ Şişe İğne Güneş Öğrenci ılık ÇĞİÖŞÜ çğıöşü";

/// The letters Turkish adds to the Latin alphabet. Of the 25 in [`SAMPLE`], 16 (ğ ı ş Ğ İ Ş)
/// are outside Latin-1, so a reader without Turkish gets at most the other 9 right.
const TURKISH: &str = "çğıöşüÇĞİÖŞÜ";

/// How many of [`SAMPLE`]'s 25 Turkish letters a reading must get right: 20 (80%). An engine
/// without Turkish gets at most 9, so this proves Windows read Turkish, while leaving room for
/// five slips. They come where no dictionary helps and the marks are smallest: Windows 11
/// (build 26300) reads the capital row ÇĞİÖŞÜ as "ÇGIOŞÜ" (3 letters lost) and every other
/// letter right, 22 of 25; askew, or shrunk to some sizes, it also takes the İ of "İğne" for i
/// (21 of 25).
const TURKISH_LETTERS_READ: usize = 20;

/// The least character accuracy (1 − edits / 46 characters): at most 6 edits. Windows 11 makes
/// 3 on the sample page, 4 askew.
const ACCURACY: f64 = 0.85;

/// The resolution pages are read at (the engine's default "Downsample to").
const DPI: f32 = 300.0;

/// Whether Windows reads Turkish here; when not, the test that asked says it skipped.
fn turkish_installed() -> bool {
    let installed = WindowsOcr::new().check_language("tr").is_ok();
    if !installed {
        eprintln!("skipped: Windows has no Turkish text recognition installed here");
    }
    installed
}

fn options(language: &str) -> RecognizeOptions {
    RecognizeOptions { language: language.into() }
}

/// `text` set in `font_size` pt on a `size` page by migez-create, saved, and its first page
/// rendered at [`DPI`] by migez-render: black text on white, premultiplied RGBA.
fn page(text: &str, font_size: f64, size: (f64, f64)) -> OcrImage {
    let doc = migez_create::from_text("ocr-windows test", text, size, font_size).unwrap();
    let bytes = migez_cos::write_full(&doc, &migez_cos::SaveOptions::default()).unwrap();
    let mut renderer = migez_render::PageRenderer::new(Arc::new(bytes), migez_render::RenderConfig::default());
    let shot = renderer.render(migez_render::RenderRequest { page: 0, scale: DPI / 72.0, ..Default::default() });
    assert!(shot.error.is_none(), "{:?}", shot.error);
    OcrImage::new(shot.width, shot.height, shot.rgba.to_vec()).unwrap()
}

/// [`SAMPLE`] in 14 pt on a 612 × 300 pt page: 2550 × 1250 pixels, rendered once.
fn sample_page() -> &'static OcrImage {
    static PAGE: OnceLock<OcrImage> = OnceLock::new();
    PAGE.get_or_init(|| page(SAMPLE, 14.0, (612.0, 300.0)))
}

/// The sample page read in Turkish by a default engine, once.
fn sample_reading() -> &'static [Line] {
    static READING: OnceLock<Vec<Line>> = OnceLock::new();
    READING.get_or_init(|| WindowsOcr::new().recognize(sample_page(), &options("tr")).unwrap())
}

/// What Windows reads in `image` in Turkish before the boxes are mapped back, and the size it
/// read.
fn raw_reading(image: &OcrImage) -> (winrt::Reading, (u32, u32)) {
    winrt::on_mta(|| {
        let language = winrt::resolve("tr")?;
        let grey = raster::grey(image, u32::MAX);
        Ok((winrt::read(&language, &grey, TIMEOUT)?, (grey.width, grey.height)))
    })
    .unwrap()
}

/// Every word read, in reading order, one space apart.
fn text_of(lines: &[Line]) -> String {
    lines.iter().map(Line::text).collect::<Vec<_>>().join(" ")
}

/// How a reading compares with what was set: the edit distance in characters, and how many of
/// the Turkish letters set came back as the same letter in an alignment of least edits.
#[derive(Debug)]
struct Score {
    edits: usize,
    chars: usize,
    turkish: usize,
    turkish_read: usize,
}

impl Score {
    fn accuracy(&self) -> f64 {
        1.0 - self.edits as f64 / self.chars as f64
    }

    /// Whether the reading passes both thresholds.
    fn passes(&self) -> bool {
        self.turkish_read >= TURKISH_LETTERS_READ && self.accuracy() >= ACCURACY
    }
}

fn score(want: &str, got: &str) -> Score {
    let a: Vec<char> = want.chars().collect();
    let b: Vec<char> = got.chars().collect();
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for (i, row) in d.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in d[0].iter_mut().enumerate() {
        *cell = j;
    }
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            let replace = d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]);
            d[i][j] = replace.min(d[i - 1][j] + 1).min(d[i][j - 1] + 1);
        }
    }
    let (mut i, mut j, mut turkish_read) = (a.len(), b.len(), 0);
    while i > 0 && j > 0 {
        if d[i][j] == d[i - 1][j - 1] + usize::from(a[i - 1] != b[j - 1]) {
            turkish_read += usize::from(a[i - 1] == b[j - 1] && TURKISH.contains(a[i - 1]));
            (i, j) = (i - 1, j - 1);
        } else if d[i][j] == d[i - 1][j] + 1 {
            i -= 1;
        } else {
            j -= 1;
        }
    }
    Score { edits: d[a.len()][b.len()], chars: a.len(), turkish: a.iter().filter(|c| TURKISH.contains(**c)).count(), turkish_read }
}

/// The scorer itself: exact text scores perfectly, a lost accent costs one edit and one letter,
/// and an English reading of the sample stays far below the bar.
#[test]
fn the_scorer_counts_edits_and_turkish_letters() {
    let exact = score(SAMPLE, SAMPLE);
    assert_eq!((exact.edits, exact.chars, exact.turkish, exact.turkish_read), (0, 46, 25, 25));
    let windows11 = score(SAMPLE, "Çığ Şişe İğne Güneş Öğrenci ılık ÇGIOŞÜ çğıöşü");
    assert_eq!((windows11.edits, windows11.turkish_read), (3, 22));
    assert!(windows11.passes());
    let english = score(SAMPLE, "Cig Sise Igne Gunes Ogrenci ilik CGIOSU cgiosu");
    assert_eq!(english.turkish_read, 0);
    assert!(!english.passes());
    let latin1 = score(SAMPLE, "Çig Sise Igne Günes Ögrenci ilik ÇGIÖSÜ çgiösü");
    assert_eq!(latin1.turkish_read, 9, "Latin-1 has 9 of the 25");
    assert!(!latin1.passes());
}

/// Whether the pixel at (`x`, `y`) is ink.
fn ink_at(image: &OcrImage, x: u32, y: u32) -> bool {
    image.gray(i64::from(x), i64::from(y)).is_some_and(|g| g < 128)
}

/// The share of the ink in `image` that lies inside one of `boxes` grown by `margin` pixels.
fn ink_inside(image: &OcrImage, boxes: &[[f32; 4]], margin: f32) -> f64 {
    let (mut ink, mut inside) = (0usize, 0usize);
    for y in 0..image.height() {
        for x in 0..image.width() {
            if !ink_at(image, x, y) {
                continue;
            }
            ink += 1;
            let (px, py) = (x as f32 + 0.5, y as f32 + 0.5);
            inside += usize::from(boxes.iter().any(|b| px >= b[0] - margin && px <= b[2] + margin && py >= b[1] - margin && py <= b[3] + margin));
        }
    }
    if ink == 0 { 0.0 } else { inside as f64 / ink as f64 }
}

/// The share of a box's pixels that are ink.
fn ink_share(image: &OcrImage, b: [f32; 4]) -> f64 {
    let (x0, y0) = (b[0].floor() as u32, b[1].floor() as u32);
    let (x1, y1) = ((b[2].ceil() as u32).min(image.width()), (b[3].ceil() as u32).min(image.height()));
    let all = (x1.saturating_sub(x0) * y1.saturating_sub(y0)).max(1);
    let ink = (y0..y1).flat_map(|y| (x0..x1).map(move |x| (x, y))).filter(|&(x, y)| ink_at(image, x, y)).count();
    ink as f64 / f64::from(all)
}

fn boxes(lines: &[Line]) -> Vec<[f32; 4]> {
    lines.iter().flat_map(|l| l.words.iter().map(|w| w.rect)).collect()
}

/// Every box lies in `image`, holds some ink, and together they hold (nearly) all of it.
fn assert_boxes_on_the_ink(image: &OcrImage, lines: &[Line], margin: f32) {
    let (w, h) = (image.width() as f32, image.height() as f32);
    for b in boxes(lines) {
        assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= w && b[3] <= h && b[0] < b[2] && b[1] < b[3], "{b:?} outside {w} × {h}");
        assert!(ink_share(image, b) >= 0.05, "{b:?} holds almost no ink: {}", ink_share(image, b));
    }
    let inside = ink_inside(image, &boxes(lines), margin);
    assert!(inside >= 0.98, "only {inside:.3} of the ink is inside the boxes: {lines:?}");
}

/// `image` turned `degrees` clockwise (image y pointing down) about its centre, bilinear, with
/// paper outside: a page scanned askew.
fn rotate(image: &OcrImage, degrees: f64) -> OcrImage {
    let (w, h) = (image.width(), image.height());
    let (cx, cy) = (f64::from(w) / 2.0, f64::from(h) / 2.0);
    let (sin, cos) = degrees.to_radians().sin_cos();
    let inside = |x: f64, y: f64| x >= 0.0 && y >= 0.0 && x < f64::from(w) && y < f64::from(h);
    let at = |x: f64, y: f64| if inside(x, y) { f64::from(image.gray(x as i64, y as i64).unwrap()) } else { 255.0 };
    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h {
        for x in 0..w {
            // Where this pixel's centre came from: turned back by `degrees`.
            let (dx, dy) = (f64::from(x) + 0.5 - cx, f64::from(y) + 0.5 - cy);
            let (sx, sy) = (cx + dx * cos + dy * sin - 0.5, cy - dx * sin + dy * cos - 0.5);
            let (x0, y0) = (sx.floor(), sy.floor());
            let (fx, fy) = (sx - x0, sy - y0);
            let top = at(x0, y0) * (1.0 - fx) + at(x0 + 1.0, y0) * fx;
            let bottom = at(x0, y0 + 1.0) * (1.0 - fx) + at(x0 + 1.0, y0 + 1.0) * fx;
            let g = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
            rgba.extend_from_slice(&[g, g, g, 255]);
        }
    }
    OcrImage::new(w, h, rgba).unwrap()
}

/// The task's check: the Turkish line comes back with (nearly) every Turkish letter right.
#[test]
fn reads_the_turkish_letters() {
    if !turkish_installed() {
        return;
    }
    let text = text_of(sample_reading());
    let s = score(SAMPLE, &text);
    assert_eq!(s.turkish, 25);
    assert!(
        s.turkish_read >= TURKISH_LETTERS_READ,
        "{} of 25 Turkish letters read (at least {TURKISH_LETTERS_READ} expected): {text:?} ({s:?})",
        s.turkish_read
    );
    assert!(s.accuracy() >= ACCURACY, "character accuracy {:.3} (at least {ACCURACY}): {text:?} ({s:?})", s.accuracy());
}

/// One line of eight words, left to right on one band, each box on its word's ink.
#[test]
fn the_words_and_the_line_sit_on_their_ink() {
    if !turkish_installed() {
        return;
    }
    let lines = sample_reading();
    assert_eq!(lines.len(), 1, "{lines:?}");
    let words = &lines[0].words;
    assert_eq!(words.len(), 8, "{words:?}");
    for pair in words.windows(2) {
        assert!(pair[1].rect[0] >= pair[0].rect[2] - 1.0, "out of reading order: {pair:?}");
    }
    let band = words[0].rect;
    assert!(words.iter().all(|w| w.rect[1] < band[3] && w.rect[3] > band[1]), "not one line: {words:?}");
    assert_boxes_on_the_ink(sample_page(), lines, 3.0);
}

/// Every word names the engine and carries the fixed confidence; without one it carries none.
#[test]
fn words_name_the_engine_and_carry_the_assumed_confidence() {
    if !turkish_installed() {
        return;
    }
    let words: Vec<&Word> = sample_reading().iter().flat_map(|l| &l.words).collect();
    assert!(!words.is_empty());
    assert!(words.iter().all(|w| w.source == ENGINE_ID && w.confidence == Some(ASSUMED_CONFIDENCE)), "{words:?}");
    let unknown = WindowsOcr::new().with_confidence(None).recognize(sample_page(), &options("tr")).unwrap();
    assert!(unknown.iter().flat_map(|l| &l.words).all(|w| w.confidence.is_none()), "{unknown:?}");
}

/// No language named: the default, Turkish, is read; and an empty default stays Turkish.
#[test]
fn an_empty_language_reads_the_default_one() {
    if !turkish_installed() {
        return;
    }
    let engine = WindowsOcr::new().with_default_language("  ");
    assert_eq!(engine.default_language(), "tr");
    let lines = engine.recognize(sample_page(), &options("")).unwrap();
    assert_eq!(text_of(&lines), text_of(sample_reading()));
}

/// A language Windows has no recognition for is an error that names it and the Windows setting
/// that installs it; nothing is read in another language instead.
#[test]
fn a_missing_language_names_itself_and_the_setting() {
    if !turkish_installed() {
        return;
    }
    let engine = WindowsOcr::new();
    let error = engine.check_language("tlh").unwrap_err();
    let message = error.to_string();
    assert!(matches!(error, OcrError::Recognize(_)), "{error:?}");
    for part in ["tlh", "Language options > Optical character recognition", "Dil seçenekleri > Optik karakter tanıma"] {
        assert!(message.contains(part), "{part:?} missing from {message}");
    }
    let read = engine.recognize(sample_page(), &options("tlh")).unwrap_err().to_string();
    assert_eq!(read, message, "recognize refuses the same way");
    let default = WindowsOcr::new().with_default_language("tlh").recognize(sample_page(), &options("")).unwrap_err();
    assert!(default.to_string().contains("tlh"), "{default}");
}

/// `check_language` answers with the Windows tag that will be read.
#[test]
fn check_language_names_the_windows_tag() {
    if !turkish_installed() {
        return;
    }
    let engine = WindowsOcr::new();
    for code in ["tr", " TR ", "", "tr-TR", "tr_tr"] {
        assert_eq!(engine.check_language(code).unwrap(), "tr", "{code:?}");
    }
}

/// The installed languages come from Windows, Turkish among them, with Windows' name for it.
#[test]
fn languages_and_availability_come_from_windows() {
    if !turkish_installed() {
        return;
    }
    let engine = WindowsOcr::new();
    assert!(engine.available());
    let languages = engine.languages();
    assert!(languages.iter().any(|(code, name)| code == "tr" && !name.trim().is_empty()), "{languages:?}");
    assert_eq!(engine.languages(), languages, "asked again, the remembered list");
    assert_eq!(engine.id(), "windows");
    assert!(WindowsOcr::max_image_dimension().unwrap() >= 1000);
}

/// Read smaller than the page (1801 × 883 for 2550 × 1250, odd on both sides: about 212 dpi),
/// the boxes still come back in the page's own pixels, on the ink, and the text reads as at full
/// size. (Smaller still, 1601 px or 188 dpi, Windows starts reading the capital row as lower
/// case: 19 of 25 letters.)
#[test]
fn a_shrunk_reading_maps_its_boxes_back() {
    if !turkish_installed() {
        return;
    }
    let image = sample_page();
    let lines = WindowsOcr::new().with_max_side(1801).recognize(image, &options("tr")).unwrap();
    let text = text_of(&lines);
    let s = score(SAMPLE, &text);
    assert!(s.passes(), "{text:?} ({s:?})");
    // Unscaled boxes would end before x = 1145 (1620 px × 1801 / 2550).
    assert!(boxes(&lines).iter().any(|b| b[2] > 1500.0), "{lines:?}");
    // A shrunk pixel is 1.4 source pixels: a box may be off by a few.
    assert_boxes_on_the_ink(image, &lines, 4.0);
}

/// A page scanned 3° askew: Windows reads the slant and measures the boxes straightened; turned
/// back, they sit on the slanted ink. Without turning them back they miss most of it, so the
/// check below would catch a slant that is ignored or turned the wrong way.
#[test]
fn a_skewed_page_keeps_its_boxes_on_the_ink() {
    if !turkish_installed() {
        return;
    }
    let image = rotate(sample_page(), 3.0);
    let (raw, read) = raw_reading(&image);
    let angle = raw.angle.unwrap_or_default();
    assert!((1.5..=4.5).contains(&angle), "Windows read a slant of {angle}° for 3°");
    let unturned = lines(winrt::Reading { lines: raw.lines.clone(), angle: None }, read, read, None);
    let missed = ink_inside(&image, &boxes(&unturned), 3.0);
    assert!(missed < 0.9, "boxes left straight should miss the slanted ink, but hold {missed:.3}");
    let turned = WindowsOcr::new().recognize(&image, &options("tr")).unwrap();
    assert_boxes_on_the_ink(&image, &turned, 3.0);
    let s = score(SAMPLE, &text_of(&turned));
    assert!(s.passes(), "{:?} ({s:?})", text_of(&turned));
}

/// Nothing to read is no words, not an error, down to a single pixel.
#[test]
fn a_blank_page_reads_nothing() {
    if !turkish_installed() {
        return;
    }
    for (w, h) in [(2550u32, 1250u32), (40, 40), (3, 2), (1, 1)] {
        let blank = OcrImage::new(w, h, vec![255; (w * h * 4) as usize]).unwrap();
        assert_eq!(WindowsOcr::new().recognize(&blank, &options("tr")).unwrap(), Vec::<Line>::new(), "{w} × {h}");
    }
}

/// One engine shared by several threads (as `Arc<dyn Recognizer>`) reads the same page alike
/// on each: every call runs on its own MTA thread.
#[test]
fn several_threads_read_alike() {
    if !turkish_installed() {
        return;
    }
    let engine: Arc<dyn Recognizer> = Arc::new(WindowsOcr::new());
    let want = text_of(sample_reading());
    let readers: Vec<_> = (0..3)
        .map(|_| {
            let engine = Arc::clone(&engine);
            std::thread::spawn(move || engine.recognize(sample_page(), &options("tr")).map(|lines| text_of(&lines)))
        })
        .collect();
    for reader in readers {
        assert_eq!(reader.join().unwrap().unwrap(), want);
    }
}

/// A reading that outlasts its time is cancelled and reported, and the engine reads normally
/// afterwards.
#[test]
fn a_reading_past_its_time_is_cancelled() {
    if !turkish_installed() {
        return;
    }
    let hasty = WindowsOcr { timeout: Duration::ZERO, ..WindowsOcr::new() };
    let error = hasty.recognize(sample_page(), &options("tr")).unwrap_err();
    assert!(matches!(&error, OcrError::Recognize(m) if m.contains("did not finish within 0 seconds")), "{error:?}");
    let again = WindowsOcr::new().recognize(sample_page(), &options("tr")).unwrap();
    assert_eq!(text_of(&again), text_of(sample_reading()));
}
