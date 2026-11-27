//! Text selected in a PDF, made ready to translate.
//!
//! A PDF has no paragraphs, only lines: Copy gives one line per line on the page. The translation
//! engine ends a sentence at every line break, so a sentence that runs over three lines came out as
//! three broken pieces. Here the lines of a paragraph are joined into one, a word divided by a
//! hyphen at a line end is mended, and paragraphs stay apart, one blank line between them (which
//! the engine keeps in its output).
//!
//! Where the selection's geometry is known ([`from_lines`]) a paragraph ends at a wider gap between
//! lines, at a short line that ends a sentence, and before a list item. From plain text ([`prepare`])
//! a blank line ends one, and so does a short line that ends a sentence, and a list item.

/// One line of the selection and where it is on the page (any units, y growing downwards).
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    pub left: f32,
    pub right: f32,
    pub top: f32,
    pub bottom: f32,
}

/// Characters that end a sentence (a closing quote or bracket may follow).
fn ends_sentence(line: &str) -> bool {
    let trimmed = line.trim_end().trim_end_matches(['"', '\'', '”', '’', '»', ')', ']']);
    trimmed.ends_with(['.', '!', '?', ':', ';', '…', '。', '！', '？', '：'])
}

/// A list item's start: a bullet or dash and a space, `1.` / `1)` / `a)` / `(a)` and a space.
fn starts_list_item(line: &str) -> bool {
    let line = line.trim_start();
    let mut chars = line.chars();
    let Some(first) = chars.next() else { return false };
    if matches!(first, '•' | '◦' | '▪' | '‣' | '·' | '-' | '–' | '—' | '*') {
        return chars.next().is_some_and(char::is_whitespace);
    }
    let marker = line.split_whitespace().next().unwrap_or_default();
    if marker.len() == line.len() {
        return false;
    }
    let inner = marker.strip_prefix('(').unwrap_or(marker);
    let Some(body) = inner.strip_suffix([')', '.']) else { return false };
    let numbered = !body.is_empty() && body.len() <= 3 && body.bytes().all(|b| b.is_ascii_digit());
    let lettered = body.chars().count() == 1 && body.chars().all(char::is_alphabetic) && (inner.ends_with(')') || marker.starts_with('('));
    numbered || lettered
}

/// Han, kana: scripts written without spaces between words.
fn unspaced(c: char) -> bool {
    matches!(c as u32, 0x3040..=0x30FF | 0x3400..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF66..=0xFF9F)
}

/// Joins two lines of one paragraph.
fn join(out: &mut String, next: &str) {
    let next = next.trim();
    if next.is_empty() {
        return;
    }
    if out.is_empty() {
        out.push_str(next);
        return;
    }
    let first = next.chars().next().unwrap_or(' ');
    // A soft hyphen only marks where the word was divided: it goes.
    if out.ends_with('\u{AD}') {
        out.pop();
        out.push_str(next);
        return;
    }
    // A hyphen after a letter, with the word going on in lower case on the next line: the word
    // was divided ("sözleş-" + "mesi"). Before a capital or a digit it stays, with no space
    // ("Ankara-" + "İstanbul").
    if let Some(stem) = out.strip_suffix(['-', '\u{2010}']) {
        if stem.chars().next_back().is_some_and(char::is_alphabetic) && first.is_lowercase() {
            out.truncate(stem.len());
        }
        out.push_str(next);
        return;
    }
    if out.chars().next_back().is_some_and(unspaced) && unspaced(first) {
        out.push_str(next);
        return;
    }
    out.push(' ');
    out.push_str(next);
}

/// Lines into paragraphs: `breaks[i]` says a paragraph ends before line `i`.
fn assemble(lines: &[&str], breaks: &[bool]) -> String {
    let mut paragraphs: Vec<String> = Vec::new();
    let mut current = String::new();
    for (i, line) in lines.iter().enumerate() {
        if breaks.get(i).copied().unwrap_or(false) && !current.is_empty() {
            paragraphs.push(std::mem::take(&mut current));
        }
        join(&mut current, line);
    }
    if !current.is_empty() {
        paragraphs.push(current);
    }
    paragraphs.join("\n\n")
}

/// Plain text (Copy's): blank lines end paragraphs, and so do a short line that ends a sentence
/// (under three quarters of the longest line) and the start of a list item.
pub fn prepare(text: &str) -> String {
    let lines: Vec<&str> = text.lines().map(|l| l.trim_end_matches('\r')).collect();
    let longest = lines.iter().map(|l| l.trim().chars().count()).max().unwrap_or(0);
    let mut kept: Vec<&str> = Vec::with_capacity(lines.len());
    let mut kept_breaks: Vec<bool> = Vec::with_capacity(lines.len());
    let mut blank = false;
    for line in &lines {
        if line.trim().is_empty() {
            blank = true;
            continue;
        }
        let previous = kept.last().copied();
        let short_end = previous.is_some_and(|p| ends_sentence(p) && (p.trim().chars().count() as f32) < longest as f32 * 0.75);
        kept.push(line);
        kept_breaks.push(blank || short_end || starts_list_item(line));
        blank = false;
    }
    assemble(&kept, &kept_breaks)
}

/// The median of some values (0 for none).
fn median(mut values: Vec<f32>) -> f32 {
    values.retain(|v| v.is_finite());
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(f32::total_cmp);
    values.get(values.len() / 2).copied().unwrap_or(0.0)
}

/// The selection's lines with their places: a paragraph also ends where the gap to the next line
/// is clearly wider than the usual one, where a line that ends a sentence stops well short of the
/// block's right edge, before a list item, and where the next line is not below this one (another
/// column).
pub fn from_lines(lines: &[Line]) -> String {
    let lines: Vec<&Line> = lines.iter().filter(|l| !l.text.trim().is_empty()).collect();
    let height = median(lines.iter().map(|l| (l.bottom - l.top).abs()).collect());
    let step = median(lines.windows(2).filter_map(|w| w.first().zip(w.get(1))).map(|(a, b)| b.top - a.top).filter(|g| *g > 0.0).collect());
    let left = lines.iter().map(|l| l.left).fold(f32::INFINITY, f32::min);
    let right = lines.iter().map(|l| l.right).fold(f32::NEG_INFINITY, f32::max);
    let width = (right - left).max(0.0);
    let mut breaks = vec![false; lines.len()];
    for (i, pair) in lines.windows(2).enumerate() {
        let (Some(a), Some(b)) = (pair.first(), pair.get(1)) else { continue };
        let gap = b.top - a.top;
        let wide = if step > 0.0 { gap > step * 1.45 + height * 0.1 } else { gap > height * 1.8 };
        let short_end = ends_sentence(&a.text) && width > 0.0 && a.right < right - width * 0.12;
        let other_column = gap <= 0.0;
        if let Some(slot) = breaks.get_mut(i + 1) {
            *slot = wide || short_end || other_column || starts_list_item(&b.text);
        }
    }
    let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
    assemble(&texts, &breaks)
}
