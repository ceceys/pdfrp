//! A UDF document laid out on pages and written as PDF content: fonts (installed faces, embedded
//! as subsets of the glyphs the document draws, else the bundled face), line breaking, alignment
//! (justified lines spread their spaces), tabs, list markers, tables, pictures, pagination, and
//! headers and footers with their page numbers.
//!
//! A header is drawn `headerFOffset` below the top of the page and a footer ends `footerFOffset`
//! above its bottom. The text stays inside the margins and clear of what they draw: where a
//! header or footer draws past its margin (a letterhead's logo and address lines, a footer of
//! several lines, a negative top margin), the text starts lower or ends higher, [`BAND_GAP`]
//! from it ([`Area`]). One that fits in its margin moves nothing, and empty paragraphs take no
//! room.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::rc::Rc;

use migez_cos::{Dict, Document, Object};
use migez_fonts::pdf::Metrics;

use super::model::{Attrs, Band, Block, Para, RunKind, Table, Udf};
use crate::CreateError;

/// What neither the file nor its styles name: the editor's body text.
const DEFAULT_FAMILY: &str = "Times New Roman";
const DEFAULT_SIZE: f64 = 12.0;
/// The page number's size when its footer gives none.
const PAGE_NUMBER_SIZE: f64 = 10.0;
/// Swing's tab stops where a paragraph sets none: every 72 points from its left edge.
const DEFAULT_TAB: f64 = 72.0;
/// Between a list's marker and its text.
const MARKER_GAP: f64 = 6.0;
/// Between a table cell's border and its text.
const CELL_PAD: f64 = 3.0;
/// The narrowest a line may be laid out to.
const MIN_WIDTH: f64 = 12.0;
const MAX_PAGES: usize = 10_000;
/// Between what a header or footer draws and the text, so the two don't touch.
const BAND_GAP: f64 = 6.0;
/// The least height the text keeps on a page. Where a header and footer leave it less, it keeps
/// the margins' room instead, and a note says so.
const MIN_BODY: f64 = 72.0;
/// The bullet of a bulleted list.
const BULLET: &str = "\u{2022}";

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct FontKey {
    family: String,
    bold: bool,
    italic: bool,
}

/// A run's resolved look.
#[derive(Clone, Debug, PartialEq)]
struct Look {
    key: FontKey,
    size: f64,
    color: [f64; 3],
    underline: bool,
}

/// Signed 32-bit ARGB as RGB in 0–1 (the alpha byte is left aside: text is opaque).
fn rgb(argb: Option<i64>) -> [f64; 3] {
    let Some(v) = argb else { return [0.0; 3] };
    let v = v as u32;
    [f64::from((v >> 16) & 0xFF) / 255.0, f64::from((v >> 8) & 0xFF) / 255.0, f64::from(v & 0xFF) / 255.0]
}

impl Look {
    fn of(a: &Attrs) -> Self {
        Self {
            key: FontKey {
                family: a.family.clone().filter(|f| !f.trim().is_empty()).unwrap_or_else(|| DEFAULT_FAMILY.to_string()),
                bold: a.bold.unwrap_or(false),
                italic: a.italic.unwrap_or(false),
            },
            size: a.size.unwrap_or(DEFAULT_SIZE).clamp(1.0, 400.0),
            color: rgb(a.foreground),
            underline: a.underline.unwrap_or(false),
        }
    }
}

// ── fonts ───────────────────────────────────────────────────────────────────────────────────

/// One embedded font.
struct Face {
    resource: String,
    /// Glyph advances per unit of font size, and the codes that draw each character.
    widths: HashMap<char, f64>,
    codes: HashMap<char, Vec<u8>>,
    ascent: f64,
    descent: f64,
    /// The face found has no bold or italic of its own: it is thickened or slanted when drawn.
    fake_bold: bool,
    fake_italic: bool,
}

impl Face {
    fn width(&self, s: &str, size: f64) -> f64 {
        s.chars().map(|c| self.widths.get(&c).or_else(|| self.widths.get(&'?')).copied().unwrap_or(0.5)).sum::<f64>() * size
    }

    /// The codes that draw `s`; a character the font lacks is drawn as `?`.
    fn encode(&self, s: &str) -> Vec<u8> {
        let mut out = Vec::new();
        for c in s.chars() {
            if let Some(code) = self.codes.get(&c).or_else(|| self.codes.get(&'?')) {
                out.extend_from_slice(code);
            }
        }
        out
    }
}

/// The fonts a document is drawn with, one per look, all in one font resource dictionary.
struct Fonts {
    faces: Vec<Face>,
    index: HashMap<FontKey, usize>,
    dict: Dict,
}

/// The families to look for, in order, for a family a UDF names: Java's logical font names stand
/// for the faces Java maps them to on Windows, then a serif or sans-serif family every Windows
/// has, then the same on Linux.
fn families(requested: &str) -> Vec<String> {
    let key: String = requested.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    let first: &str = match key.as_str() {
        "dialog" | "sansserif" | "sans" => "Arial",
        "serif" | "editor" | "timesroman" | "roman" => "Times New Roman",
        "monospaced" | "dialoginput" | "monospace" | "courier" => "Courier New",
        _ => requested.trim(),
    };
    let serif = (key.contains("serif") && !key.contains("sans"))
        || ["times", "roman", "georgia", "garamond", "book", "editor"].iter().any(|k| key.contains(k));
    let generic: [&str; 3] =
        if serif { ["Times New Roman", "Liberation Serif", "DejaVu Serif"] } else { ["Arial", "Liberation Sans", "DejaVu Sans"] };
    let mut out: Vec<String> = Vec::new();
    for f in std::iter::once(first).chain(generic) {
        if !f.is_empty() && !out.iter().any(|o| o.eq_ignore_ascii_case(f)) {
            out.push(f.to_string());
        }
    }
    out
}

/// The face embedded as `resource`, measured for the characters of `text`.
fn face_from(doc: &Document, fonts: &Dict, resource: &str, text: &str, fake_bold: bool, fake_italic: bool) -> Option<Face> {
    let r = fonts.reference(resource.as_bytes())?;
    let dict = doc.dict(&Object::Ref(r))?;
    let m = Metrics::from_dict(doc, &dict);
    let mut widths = HashMap::new();
    let mut codes = HashMap::new();
    for c in text.chars().chain(['?', ' ']) {
        if codes.contains_key(&c) {
            continue;
        }
        let mut buf = [0u8; 4];
        if let Some(bytes) = m.encode(c.encode_utf8(&mut buf)) {
            let w: f64 = m.codes(&bytes).iter().map(|(code, _)| m.width(*code)).sum();
            widths.insert(c, w);
            codes.insert(c, bytes);
        }
    }
    let ascent = if m.ascent.is_finite() && m.ascent.abs() > 0.3 { m.ascent.abs().min(1.5) } else { 0.89 };
    let descent = if m.descent.is_finite() && m.descent.abs() > 0.05 { m.descent.abs().min(0.8) } else { 0.22 };
    Some(Face { resource: resource.to_string(), widths, codes, ascent, descent, fake_bold, fake_italic })
}

impl Fonts {
    /// Embed one font per look the document uses, each a subset of the characters drawn in it.
    /// What stood in for a family that isn't installed goes to `notes`.
    fn embed(doc: &mut Document, texts: &HashMap<FontKey, String>, notes: &mut Vec<String>) -> Result<Self, CreateError> {
        let mut fonts = Fonts { faces: Vec::new(), index: HashMap::new(), dict: Dict::new() };
        let mut lookups = migez_fonts::installed::Lookups::default();
        let mut keys: Vec<&FontKey> = texts.keys().collect();
        keys.sort_by(|a, b| (&a.family, a.bold, a.italic).cmp(&(&b.family, b.bold, b.italic)));
        let mut said: Vec<String> = Vec::new();
        for key in keys {
            let text: String = texts.get(key).map(String::as_str).unwrap_or_default().chars().filter(|c| !c.is_control()).chain(['?', ' ']).collect();
            let (face, stood_in) = match fonts.installed(doc, &mut lookups, key, &text) {
                Some((face, family)) => (face, (!family.eq_ignore_ascii_case(key.family.trim())).then_some(family)),
                None => (
                    fonts.bundled(doc, key, &text).ok_or_else(|| CreateError::Invalid(format!("no font could be found for {}", key.family)))?,
                    Some("Inter".to_string()),
                ),
            };
            if let Some(family) = stood_in
                && !said.contains(&key.family)
            {
                said.push(key.family.clone());
                notes.push(format!("{} is not installed: its text is set in {family}", key.family));
            }
            fonts.index.insert(key.clone(), fonts.faces.len());
            fonts.faces.push(face);
        }
        Ok(fonts)
    }

    /// An installed face for `key` and the family it is of: `key`'s family with its style, else
    /// without it (drawn thickened or slanted), then the families after it ([`families`]).
    fn installed(&mut self, doc: &mut Document, lookups: &mut migez_fonts::installed::Lookups, key: &FontKey, text: &str) -> Option<(Face, String)> {
        for family in families(&key.family) {
            let mut tries = vec![(key.bold, key.italic)];
            for t in [(key.bold, false), (false, key.italic), (false, false)] {
                if !tries.contains(&t) {
                    tries.push(t);
                }
            }
            for (bold, italic) in tries {
                let Some((face, bytes)) = lookups.usable(&family, bold, italic) else { continue };
                if !migez_fonts::embedding::covers(&bytes, text) {
                    continue;
                }
                let Some(embedded) = migez_fonts::installed::embed_found(doc, &mut self.dict, &face, &bytes, text) else { continue };
                let Some(f) = face_from(doc, &self.dict, &embedded.resource, text, key.bold && !bold, key.italic && !italic) else { continue };
                return Some((f, family));
            }
        }
        None
    }

    /// The bundled face (Inter), for a family no installed face stands in for. Characters it
    /// can't draw are drawn as `?`.
    fn bundled(&mut self, doc: &mut Document, key: &FontKey, text: &str) -> Option<Face> {
        let face = migez_fonts::document_latin_fonts(key.bold).into_iter().next()?;
        let mut buf = [0u8; 4];
        let drawable: String = text.chars().filter(|c| migez_fonts::embedding::covers(face.bytes, c.encode_utf8(&mut buf))).collect();
        let (resource, _) = migez_fonts::embedding::embed_installed(doc, &mut self.dict, face.bytes, &drawable, false).ok()?;
        face_from(doc, &self.dict, &resource, &drawable, false, key.italic)
    }

    fn get(&self, key: &FontKey) -> Option<&Face> {
        self.faces.get(*self.index.get(key)?)
    }
}

// ── lines ───────────────────────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
enum Atom {
    Word(String, Rc<Look>),
    Space(Rc<Look>),
    Tab(Rc<Look>),
    Image { index: usize, width: f64, height: f64 },
}

#[derive(Clone, Debug)]
enum Item {
    Text {
        x: f64,
        text: String,
        look: Rc<Look>,
        width: f64,
    },
    /// A space, or the gap a tab leaves (`tab`).
    Space {
        x: f64,
        look: Rc<Look>,
        width: f64,
        tab: bool,
    },
    Image {
        x: f64,
        index: usize,
        width: f64,
        height: f64,
    },
}

impl Item {
    fn right(&self) -> f64 {
        match self {
            Item::Text { x, width, .. } | Item::Space { x, width, .. } | Item::Image { x, width, .. } => x + width,
        }
    }

    fn is_space(&self) -> bool {
        matches!(self, Item::Space { .. })
    }
}

/// One laid-out line: items placed from its left edge, which is `left` points into its column.
#[derive(Clone, Debug, Default)]
struct Line {
    left: f64,
    avail: f64,
    items: Vec<Item>,
    ascent: f64,
    descent: f64,
    last: bool,
    tabbed: bool,
    /// A list's marker: its place in the column, its text and look.
    marker: Option<(f64, String, Rc<Look>)>,
}

impl Line {
    /// The width of what the line draws, trailing spaces aside.
    fn width(&self) -> f64 {
        self.items.iter().rev().find(|i| !i.is_space()).map_or(0.0, Item::right)
    }

    fn x(&self) -> f64 {
        self.items.last().map_or(0.0, Item::right)
    }

    fn has_content(&self) -> bool {
        self.items.iter().any(|i| !i.is_space())
    }

    /// Whether the line puts anything on the page: a word, a picture or a list marker. Spaces
    /// and tabs draw nothing, so neither does an empty paragraph's line.
    fn draws(&self) -> bool {
        self.marker.is_some()
            || self.items.iter().any(|i| match i {
                Item::Text { text, .. } => !text.trim().is_empty(),
                Item::Image { .. } => true,
                Item::Space { .. } => false,
            })
    }
}

/// An empty line starting `left` points into a column whose text ends at `column_right`.
fn new_line(left: f64, column_right: f64) -> Line {
    Line { left, avail: (column_right - left).max(MIN_WIDTH), ..Line::default() }
}

/// Fills a paragraph's lines with its items, breaking a line where the next word doesn't fit.
struct Builder<'a, 'b> {
    layout: &'a Layout<'b>,
    lines: Vec<Line>,
    cur: Line,
    /// The paragraph's left indent: where every line after its first starts, and what its tab
    /// stops count from.
    left: f64,
    column_right: f64,
    tabs: Vec<f64>,
}

impl Builder<'_, '_> {
    fn break_line(&mut self) {
        let next = new_line(self.left, self.column_right);
        self.lines.push(std::mem::replace(&mut self.cur, next));
    }

    /// Put `item` at the end of the current line (its `x` is set here).
    fn push(&mut self, mut item: Item) {
        let at = self.cur.x();
        match &mut item {
            Item::Text { x, .. } | Item::Space { x, .. } | Item::Image { x, .. } => *x = at,
        }
        self.cur.items.push(item);
    }

    fn word(&mut self, word: String, look: Rc<Look>) {
        let w = self.layout.width(&word, &look);
        if self.cur.x() + w > self.cur.avail && self.cur.has_content() {
            self.break_line();
        }
        if w <= self.cur.avail {
            self.push(Item::Text { x: 0.0, text: word, look, width: w });
            return;
        }
        // A word wider than a line is broken where it fills one.
        let mut part = String::new();
        for c in word.chars() {
            let mut next = part.clone();
            next.push(c);
            if !part.is_empty() && self.cur.x() + self.layout.width(&next, &look) > self.cur.avail {
                let width = self.layout.width(&part, &look);
                self.push(Item::Text { x: 0.0, text: std::mem::take(&mut part), look: look.clone(), width });
                self.break_line();
            }
            part.push(c);
        }
        if !part.is_empty() {
            let width = self.layout.width(&part, &look);
            self.push(Item::Text { x: 0.0, text: part, look, width });
        }
    }

    /// The next tab stop after the end of the current line, from the line's left edge.
    fn tab_stop(&self) -> f64 {
        let from = self.cur.left - self.left;
        let at = from + self.cur.x();
        let next = self.tabs.iter().copied().find(|s| *s > at + 0.01).unwrap_or_else(|| ((at / DEFAULT_TAB).floor() + 1.0) * DEFAULT_TAB);
        next - from
    }

    fn tab(&mut self, look: Rc<Look>) {
        let mut to = self.tab_stop();
        if to > self.cur.avail && self.cur.has_content() {
            self.break_line();
            to = self.tab_stop();
        }
        let width = (to - self.cur.x()).max(0.0);
        self.push(Item::Space { x: 0.0, look, width, tab: true });
        self.cur.tabbed = true;
    }

    /// A picture, made smaller when it is wider than a line.
    fn image(&mut self, index: usize, width: f64, height: f64) {
        let room = self.cur.avail.max(MIN_WIDTH);
        let scale = if width > room { room / width } else { 1.0 };
        let (w, h) = (width * scale, height * scale);
        if self.cur.x() + w > self.cur.avail && self.cur.has_content() {
            self.break_line();
        }
        self.push(Item::Image { x: 0.0, index, width: w, height: h });
    }
}

/// A paragraph laid out to a column width.
#[derive(Clone, Debug, Default)]
struct ParaBox {
    lines: Vec<Line>,
    above: f64,
    below: f64,
    spacing: f64,
    align: i64,
}

impl ParaBox {
    fn line_height(&self, line: &Line) -> f64 {
        (line.ascent + line.descent) * (1.0 + self.spacing)
    }

    fn height(&self) -> f64 {
        self.above + self.lines.iter().map(|l| self.line_height(l)).sum::<f64>() + self.below
    }
}

/// Where `boxes`, stacked from the top down as [`Painter::para_at`] draws them, put something on
/// the page: how far below the top of the first box the first line that draws starts, and how
/// far the last one ends (its descent included, the line spacing after it not). `None` when
/// they draw nothing: empty paragraphs only.
fn drawn_span<'p>(boxes: impl IntoIterator<Item = &'p ParaBox>) -> Option<(f64, f64)> {
    let mut y = 0.0;
    let mut span: Option<(f64, f64)> = None;
    for pb in boxes {
        y += pb.above;
        for line in &pb.lines {
            if line.draws() {
                let end = y + line.ascent + line.descent;
                span = Some(span.map_or((y, end), |(start, last)| (start, last.max(end))));
            }
            y += pb.line_height(line);
        }
        y += pb.below;
    }
    span
}

/// Numbered lists' counters, by list and level.
#[derive(Default)]
struct Lists(HashMap<(String, i64), u32>);

struct Layout<'a> {
    udf: &'a Udf,
    fonts: &'a Fonts,
}

impl Layout<'_> {
    /// A paragraph's own values over its style, the document's and `default`.
    fn para_attrs(&self, para: &Para) -> Attrs {
        para.attrs.over(&self.udf.style(para.attrs.resolver.as_deref(), true))
    }

    /// A run's values: its own, its own style's, then its paragraph's.
    fn run_attrs(&self, run: &Attrs, para: &Attrs) -> Attrs {
        run.over(&self.udf.style(run.resolver.as_deref(), false)).over(para)
    }

    fn face(&self, look: &Look) -> Option<&Face> {
        self.fonts.get(&look.key)
    }

    fn width(&self, s: &str, look: &Look) -> f64 {
        self.face(look).map_or(s.chars().count() as f64 * look.size * 0.5, |f| f.width(s, look.size))
    }

    fn metrics(&self, look: &Look) -> (f64, f64) {
        self.face(look).map_or((0.89 * look.size, 0.22 * look.size), |f| (f.ascent * look.size, f.descent * look.size))
    }

    /// A list item's marker: a bullet, or the next number of its list at its level.
    fn marker(&self, pa: &Attrs, lists: &mut Lists) -> Option<String> {
        if pa.bulleted == Some(true) {
            return Some(BULLET.to_string());
        }
        if pa.numbered == Some(true) {
            let level = pa.list_level.unwrap_or(0);
            let list = pa.list_id.clone().unwrap_or_default();
            // An item at a shallower level restarts the deeper levels of its list.
            lists.0.retain(|(l, lv), _| *l != list || *lv <= level);
            let n = lists.0.entry((list, level)).or_insert(0);
            *n = n.saturating_add(1);
            return Some(format!("{n}."));
        }
        None
    }

    fn atoms(&self, para: &Para, pa: &Attrs) -> Vec<Atom> {
        let mut out = Vec::new();
        for run in &para.runs {
            let look = Rc::new(Look::of(&self.run_attrs(&run.attrs, pa)));
            match &run.kind {
                RunKind::Tab => out.push(Atom::Tab(look)),
                RunKind::Placed { index, width, height } => out.push(Atom::Image { index: *index, width: *width, height: *height }),
                RunKind::Image { .. } => {}
                RunKind::Text(text) => {
                    let mut word = String::new();
                    for c in text.chars() {
                        if c == ' ' {
                            if !word.is_empty() {
                                out.push(Atom::Word(std::mem::take(&mut word), look.clone()));
                            }
                            out.push(Atom::Space(look.clone()));
                        } else {
                            word.push(c);
                        }
                    }
                    if !word.is_empty() {
                        out.push(Atom::Word(word, look.clone()));
                    }
                }
            }
        }
        out
    }

    /// The look of a paragraph's first text run (or the paragraph's own): its marker's and an
    /// empty paragraph's.
    fn base_look(&self, para: &Para, pa: &Attrs) -> Rc<Look> {
        para.runs
            .iter()
            .find(|r| matches!(r.kind, RunKind::Text(_)))
            .map(|r| Rc::new(Look::of(&self.run_attrs(&r.attrs, pa))))
            .unwrap_or_else(|| Rc::new(Look::of(pa)))
    }

    /// Lay `para` out to a column `width` points wide.
    fn para(&self, para: &Para, width: f64, lists: &mut Lists) -> ParaBox {
        let pa = self.para_attrs(para);
        let left = pa.left.unwrap_or(0.0).clamp(0.0, (width - MIN_WIDTH).max(0.0));
        let right = pa.right.unwrap_or(0.0).clamp(0.0, (width - left - MIN_WIDTH).max(0.0));
        let first = pa.first.unwrap_or(0.0);
        let tabs = pa.tabs.clone().unwrap_or_default();
        let base_look = self.base_look(para, &pa);
        let column_right = (width - right).max(MIN_WIDTH);
        let mut line_left = (left + first).clamp(0.0, (column_right - MIN_WIDTH).max(0.0));
        let mut marker = None;
        if let Some(text) = self.marker(&pa, lists) {
            let mw = self.width(&text, &base_look);
            marker = Some((line_left, text, base_look.clone()));
            line_left = if first < 0.0 { (line_left + mw + MARKER_GAP).max(left) } else { line_left + mw + MARKER_GAP };
        }
        let mut builder = Builder { layout: self, lines: Vec::new(), cur: new_line(line_left, column_right), left, column_right, tabs };
        builder.cur.marker = marker;
        for atom in self.atoms(para, &pa) {
            match atom {
                Atom::Word(word, look) => builder.word(word, look),
                Atom::Space(look) => {
                    let width = self.width(" ", &look);
                    builder.push(Item::Space { x: 0.0, look, width, tab: false });
                }
                Atom::Tab(look) => builder.tab(look),
                Atom::Image { index, width, height } => builder.image(index, width, height),
            }
        }
        builder.cur.last = true;
        let mut lines = builder.lines;
        lines.push(builder.cur);
        let (base_ascent, base_descent) = self.metrics(&base_look);
        for line in &mut lines {
            let (mut a, mut d) = (0.0f64, 0.0f64);
            for item in &line.items {
                match item {
                    Item::Text { look, .. } | Item::Space { look, .. } => {
                        let (ia, id) = self.metrics(look);
                        a = a.max(ia);
                        d = d.max(id);
                    }
                    Item::Image { height, .. } => a = a.max(*height),
                }
            }
            if a == 0.0 {
                (a, d) = (base_ascent, base_descent);
            }
            line.ascent = a;
            line.descent = d;
        }
        ParaBox {
            lines,
            above: pa.above.unwrap_or(0.0).max(0.0),
            below: pa.below.unwrap_or(0.0).max(0.0),
            spacing: pa.line_spacing.unwrap_or(0.0).clamp(0.0, 10.0),
            align: pa.alignment.unwrap_or(0),
        }
    }
}

// ── drawing ─────────────────────────────────────────────────────────────────────────────────

/// A number for a content stream: at most three decimals, no `-0`.
fn n(v: f64) -> String {
    let v = if v.is_finite() { v } else { 0.0 };
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" || s.is_empty() { "0".to_string() } else { s.to_string() }
}

/// A string being collected for one text-showing operation.
struct Segment {
    x: f64,
    text: String,
    look: Rc<Look>,
    end: f64,
}

struct Painter<'a> {
    layout: &'a Layout<'a>,
    pictures: usize,
}

impl Painter<'_> {
    fn text(&self, out: &mut Vec<u8>, text: &str, look: &Look, x: f64, baseline: f64) {
        let Some(face) = self.layout.face(look) else { return };
        let codes = face.encode(text);
        if codes.is_empty() {
            return;
        }
        let [r, g, b] = look.color;
        let mut s = format!("BT /{} {} Tf {} {} {} rg ", face.resource, n(look.size), n(r), n(g), n(b));
        if face.fake_bold {
            let _ = write!(s, "2 Tr {} w {} {} {} RG ", n(look.size * 0.03), n(r), n(g), n(b));
        }
        let skew = if face.fake_italic { 0.2 } else { 0.0 };
        let _ = write!(s, "1 0 {} 1 {} {} Tm <", n(skew), n(x), n(baseline));
        for b in codes {
            let _ = write!(s, "{b:02X}");
        }
        s.push_str("> Tj ET\n");
        out.extend_from_slice(s.as_bytes());
    }

    fn flush(&self, out: &mut Vec<u8>, segment: &mut Option<Segment>, baseline: f64) {
        let Some(seg) = segment.take() else { return };
        let shown = seg.text.trim_end();
        if shown.trim().is_empty() {
            return;
        }
        self.text(out, shown, &seg.look, seg.x, baseline);
        if seg.look.underline {
            let trailing = self.layout.width(seg.text.get(shown.len()..).unwrap_or_default(), &seg.look);
            underline(out, &seg.look, seg.x, baseline, (seg.end - seg.x - trailing).max(0.0));
        }
    }

    /// Draw `line` of `pb` with its column's left edge at `column_x` and its top at `top`.
    /// Returns the line's height.
    fn line(&self, out: &mut Vec<u8>, pb: &ParaBox, line: &Line, column_x: f64, top: f64) -> f64 {
        let baseline = top - line.ascent;
        let free = (line.avail - line.width()).max(0.0);
        let (offset, extra) = match pb.align {
            1 => (free / 2.0, 0.0),
            2 => (free, 0.0),
            3 if !line.last && !line.tabbed => {
                let last_word = line.items.iter().rposition(|i| !i.is_space()).unwrap_or(0);
                let gaps = line.items.iter().take(last_word).filter(|i| i.is_space()).count();
                (0.0, if gaps > 0 { free / gaps as f64 } else { 0.0 })
            }
            _ => (0.0, 0.0),
        };
        let x0 = column_x + line.left + offset;
        if let Some((mx, text, look)) = &line.marker {
            let shift = if matches!(pb.align, 1 | 2) { offset } else { 0.0 };
            self.text(out, text, look, column_x + mx + shift, baseline);
        }
        // Neighbouring words and spaces of one look are one string, so copied text keeps its
        // spaces; a justified line draws each word at its own place, and a tab ends a string.
        let mut shift = 0.0;
        let mut segment: Option<Segment> = None;
        for item in &line.items {
            match item {
                Item::Text { x, text, look, width } => {
                    let at = x0 + x + shift;
                    match &mut segment {
                        Some(seg) if *seg.look == **look => {
                            seg.text.push_str(text);
                            seg.end = at + width;
                        }
                        _ => {
                            self.flush(out, &mut segment, baseline);
                            segment = Some(Segment { x: at, text: text.clone(), look: look.clone(), end: at + width });
                        }
                    }
                }
                Item::Space { x, look, width, tab } => {
                    if *tab || extra > 0.0 {
                        self.flush(out, &mut segment, baseline);
                        if !*tab {
                            shift += extra;
                        }
                        continue;
                    }
                    let at = x0 + x + shift;
                    match &mut segment {
                        Some(seg) if *seg.look == **look => {
                            seg.text.push(' ');
                            seg.end = at + width;
                        }
                        _ => {
                            self.flush(out, &mut segment, baseline);
                            segment = Some(Segment { x: at, text: " ".to_string(), look: look.clone(), end: at + width });
                        }
                    }
                }
                Item::Image { x, index, width, height } => {
                    self.flush(out, &mut segment, baseline);
                    if *index < self.pictures {
                        out.extend_from_slice(
                            format!("q {} 0 0 {} {} {} cm /Im{} Do Q\n", n(*width), n(*height), n(x0 + x + shift), n(baseline), index).as_bytes(),
                        );
                    }
                }
            }
        }
        self.flush(out, &mut segment, baseline);
        pb.line_height(line)
    }

    /// Draw `pb`'s lines from `top` down; returns the height used (above and below included).
    fn para_at(&self, out: &mut Vec<u8>, pb: &ParaBox, column_x: f64, top: f64) -> f64 {
        let mut y = top - pb.above;
        for line in &pb.lines {
            y -= self.line(out, pb, line, column_x, y);
        }
        top - (y - pb.below)
    }
}

fn underline(out: &mut Vec<u8>, look: &Look, x: f64, baseline: f64, width: f64) {
    let [r, g, b] = look.color;
    let y = baseline - look.size * 0.11;
    out.extend_from_slice(
        format!("{} {} {} RG {} w {} {} m {} {} l S\n", n(r), n(g), n(b), n((look.size * 0.06).max(0.3)), n(x), n(y), n(x + width), n(y)).as_bytes(),
    );
}

// ── pages ───────────────────────────────────────────────────────────────────────────────────

/// Where the text goes on each page: inside the margins, and clear of what the page's header and
/// footer draw. Points from the bottom of the page.
struct Area<'u> {
    /// The margins' edges.
    top: f64,
    bottom: f64,
    /// Each header, and how low on the page it draws: the bottom of its last line that draws.
    /// A header that draws nothing isn't here.
    headers: Vec<(&'u Band, f64)>,
    /// Each footer, and how high on the page it draws: the top of its first line that draws.
    footers: Vec<(&'u Band, f64)>,
}

impl Area<'_> {
    /// The top and bottom of the text on page `page` (1-based), and whether the page's header
    /// and footer left it [`MIN_BODY`] of room; where they didn't, it keeps the margins'.
    fn on(&self, page: usize) -> (f64, f64, bool) {
        let top = self.headers.iter().filter(|(b, _)| b.on(page)).fold(self.top, |top, (_, low)| top.min(low - BAND_GAP));
        let bottom = self.footers.iter().filter(|(b, _)| b.on(page)).fold(self.bottom, |bottom, (_, high)| bottom.max(high + BAND_GAP));
        let moved = top < self.top || bottom > self.bottom;
        if !moved || top - bottom >= MIN_BODY { (top, bottom, true) } else { (self.top, self.bottom, false) }
    }
}

/// The pages being filled: their content, and where the next line goes.
struct Flow<'a> {
    pages: Vec<Vec<u8>>,
    area: &'a Area<'a>,
    bottom: f64,
    y: f64,
    /// Nothing is on the current page yet: a paragraph's space above is left out, and a line
    /// that doesn't fit is placed all the same rather than starting an empty page.
    fresh: bool,
    /// On some page the header and footer left the text too little room.
    squeezed: bool,
}

impl Flow<'_> {
    fn new_page(&mut self) -> Result<(), CreateError> {
        if self.pages.len() >= MAX_PAGES {
            return Err(CreateError::Invalid(format!("the UDF document is longer than {MAX_PAGES} pages")));
        }
        self.pages.push(Vec::new());
        let (top, bottom, roomy) = self.area.on(self.pages.len());
        self.y = top;
        self.bottom = bottom;
        self.squeezed |= !roomy;
        self.fresh = true;
        Ok(())
    }

    fn out(&mut self) -> &mut Vec<u8> {
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
        }
        let last = self.pages.len() - 1;
        &mut self.pages[last]
    }

    /// Room for `h` points, on a new page if this one has too little (unless it is empty).
    fn room(&mut self, h: f64) -> Result<(), CreateError> {
        if self.y - h < self.bottom - 0.01 && !self.fresh {
            self.new_page()?;
        }
        Ok(())
    }
}

/// The column widths of a table `width` points wide: its `columnSpans` as shares, else equal.
fn columns(table: &Table, width: f64) -> Vec<f64> {
    let count = table.rows.iter().map(|r| r.cells.len()).max().unwrap_or(0).max(table.columns).max(1);
    let mut spans: Vec<f64> = table.spans.iter().copied().take(count).collect();
    if spans.len() < count {
        let mean = if spans.is_empty() { 1.0 } else { spans.iter().sum::<f64>() / spans.len() as f64 };
        spans.resize(count, mean);
    }
    let total: f64 = spans.iter().sum();
    spans.into_iter().map(|s| width * s / total).collect()
}

/// The page number a footer shows on page `page` of `pages`: the text before it, the number,
/// and, when the footer names a separator, the separator and the page count.
fn page_number_text(pn: &super::model::PageNumber, page: usize, pages: usize) -> String {
    let first = pn.start;
    let n = first.saturating_add(i64::try_from(page).unwrap_or(0)).saturating_sub(1);
    let total = first.saturating_add(i64::try_from(pages).unwrap_or(0)).saturating_sub(1);
    if pn.separator.is_empty() { format!("{}{n}", pn.before) } else { format!("{}{n}{}{total}", pn.before, pn.separator) }
}

/// Where a footer's page number sits: its spec names right or left (`SAG`, `SOL`, `RIGHT`,
/// `LEFT`), else the centre.
fn page_number_align(spec: &str) -> i64 {
    let s = spec.to_uppercase();
    if s.contains("SAG") || s.contains("SAĞ") || s.contains("RIGHT") {
        2
    } else if s.contains("SOL") || s.contains("LEFT") {
        0
    } else {
        1
    }
}

/// Every text a document draws, by font, and the looks of its page numbers: what [`Fonts::embed`]
/// needs before anything is laid out.
fn texts(udf: &Udf) -> HashMap<FontKey, String> {
    // Looks only: no font is measured here.
    let none = Fonts { faces: Vec::new(), index: HashMap::new(), dict: Dict::new() };
    let layout = Layout { udf, fonts: &none };
    let mut texts: HashMap<FontKey, String> = HashMap::new();
    let mut add = |look: &Look, s: &str| texts.entry(look.key.clone()).or_default().push_str(s);
    let mut paras: Vec<&Para> = Vec::new();
    for block in &udf.blocks {
        match block {
            Block::Para(p) => paras.push(p),
            Block::Table(t) => paras.extend(t.rows.iter().flat_map(|r| r.cells.iter().flatten())),
        }
    }
    paras.extend(udf.headers.iter().chain(&udf.footers).flat_map(|b| b.paras.iter()));
    for para in paras {
        let pa = layout.para_attrs(para);
        let base = layout.base_look(para, &pa);
        add(&base, " ");
        if pa.bulleted == Some(true) {
            add(&base, BULLET);
        }
        if pa.numbered == Some(true) {
            add(&base, "0123456789.");
        }
        for run in &para.runs {
            let look = Look::of(&layout.run_attrs(&run.attrs, &pa));
            match &run.kind {
                RunKind::Text(t) => add(&look, t),
                _ => add(&look, " "),
            }
        }
    }
    for band in &udf.footers {
        if let Some(pn) = &band.page_number {
            let look = page_number_look(udf, pn);
            add(&look, &format!("{}{}0123456789", pn.before, pn.separator));
        }
    }
    texts
}

fn page_number_look(udf: &Udf, pn: &super::model::PageNumber) -> Look {
    let mut a = pn.attrs.over(&udf.style(None, true));
    if pn.attrs.size.is_none() {
        a.size = Some(PAGE_NUMBER_SIZE);
    }
    Look::of(&a)
}

/// A page as laid out: its width and height in points, its resources and its content stream.
pub(super) type PageOut = (f64, f64, Dict, Vec<u8>);

/// Lay `udf` out and write it into `doc`: the page contents, with the shared font and picture
/// resources. `pictures` is the number of picture XObjects (`/Im0` …) the resources hold.
pub(super) fn write(doc: &mut Document, udf: &Udf, pictures: &Dict, notes: &mut Vec<String>) -> Result<Vec<PageOut>, CreateError> {
    let fonts = Fonts::embed(doc, &texts(udf), notes)?;
    let layout = Layout { udf, fonts: &fonts };
    let painter = Painter { layout: &layout, pictures: pictures.len() };
    let page = &udf.page;
    let body = (page.width - page.left - page.right).max(72.0);
    let x0 = page.left.min(page.width - body);
    let top = page.height - page.top;
    let header_top = page.height - page.header_offset.unwrap_or((page.top / 2.0).max(12.0));
    let footer_bottom = page.footer_offset.unwrap_or((page.bottom / 2.0).max(12.0));
    // Headers and footers first: the text keeps clear of what they draw.
    let headers: Vec<(&Band, Vec<ParaBox>)> = udf.headers.iter().map(|b| (b, band_boxes(&layout, b, body))).collect();
    let footers: Vec<(&Band, Vec<ParaBox>)> = udf.footers.iter().map(|b| (b, band_boxes(&layout, b, body))).collect();
    let area = Area {
        top,
        bottom: page.bottom.min(top - MIN_BODY).max(0.0),
        headers: headers.iter().filter_map(|(band, boxes)| Some((*band, header_top - drawn_span(boxes)?.1))).collect(),
        footers: footers
            .iter()
            .filter_map(|(band, boxes)| {
                // A page number is as tall on every page: the first page's stands for all.
                let number = band.page_number.as_ref().map(|pn| page_number_box(&layout, pn, body, 1, 1));
                let height: f64 = boxes.iter().chain(number.iter()).map(ParaBox::height).sum();
                Some((*band, footer_bottom + height - drawn_span(boxes.iter().chain(number.iter()))?.0))
            })
            .collect(),
    };
    let mut flow = Flow { pages: Vec::new(), area: &area, bottom: area.bottom, y: top, fresh: true, squeezed: false };
    flow.new_page()?;
    let mut lists = Lists::default();
    for block in &udf.blocks {
        match block {
            Block::Para(para) => {
                let pb = layout.para(para, body, &mut lists);
                if !flow.fresh {
                    flow.y -= pb.above;
                }
                for line in &pb.lines {
                    flow.room(pb.line_height(line))?;
                    let y = flow.y;
                    let h = painter.line(flow.out(), &pb, line, x0, y);
                    flow.y -= h;
                    flow.fresh = false;
                }
                flow.y -= pb.below;
            }
            Block::Table(table) => {
                let widths = columns(table, body);
                for row in &table.rows {
                    let cells: Vec<Vec<ParaBox>> = row
                        .cells
                        .iter()
                        .zip(&widths)
                        .map(|(cell, w)| cell.iter().map(|p| layout.para(p, (w - 2.0 * CELL_PAD).max(MIN_WIDTH), &mut lists)).collect())
                        .collect();
                    let content = cells.iter().map(|c| c.iter().map(ParaBox::height).sum::<f64>()).fold(0.0, f64::max);
                    let height = (content + 2.0 * CELL_PAD).max(row.height.unwrap_or(0.0));
                    flow.room(height)?;
                    let y = flow.y;
                    let mut x = x0;
                    for (i, w) in widths.iter().enumerate() {
                        if let Some(boxes) = cells.get(i) {
                            let mut cy = y - CELL_PAD;
                            for pb in boxes {
                                cy -= painter.para_at(flow.out(), pb, x + CELL_PAD, cy);
                            }
                        }
                        if table.border {
                            flow.out()
                                .extend_from_slice(format!("0 0 0 RG 0.5 w {} {} {} {} re S\n", n(x), n(y - height), n(*w), n(height)).as_bytes());
                        }
                        x += w;
                    }
                    flow.y -= height;
                    flow.fresh = false;
                }
            }
        }
    }
    if flow.squeezed {
        notes.push("on some pages the header and footer leave the text too little room: there it keeps to the margins and may run over them".into());
    }
    let total = flow.pages.len();
    // Headers and footers, now that the page count is known.
    for (i, content) in flow.pages.iter_mut().enumerate() {
        let number = i + 1;
        for (_, boxes) in headers.iter().filter(|(b, _)| b.on(number)) {
            let mut y = header_top;
            for pb in boxes {
                y -= painter.para_at(content, pb, x0, y);
            }
        }
        for (band, boxes) in footers.iter().filter(|(b, _)| b.on(number)) {
            footer(&painter, content, band, boxes, x0, body, footer_bottom, number, total);
        }
    }
    let mut resources = Dict::new();
    resources.set(b"Font".to_vec(), Object::Dict(fonts.dict.clone()));
    if !pictures.is_empty() {
        resources.set(b"XObject".to_vec(), Object::Dict(pictures.clone()));
    }
    Ok(flow.pages.into_iter().map(|content| (page.width, page.height, resources.clone(), content)).collect())
}

/// A header's or footer's paragraphs laid out to the text's width, each list counted on its own.
fn band_boxes(layout: &Layout<'_>, band: &Band, width: f64) -> Vec<ParaBox> {
    band.paras.iter().map(|p| layout.para(p, width, &mut Lists::default())).collect()
}

/// A footer's page number on page `page` of `pages`: a line of its own, `width` points wide.
fn page_number_box(layout: &Layout<'_>, pn: &super::model::PageNumber, width: f64, page: usize, pages: usize) -> ParaBox {
    let look = Rc::new(page_number_look(layout.udf, pn));
    let text = page_number_text(pn, page, pages);
    let text_width = layout.width(&text, &look);
    let (ascent, descent) = layout.metrics(&look);
    let line = Line {
        left: 0.0,
        avail: width,
        items: vec![Item::Text { x: 0.0, text, look, width: text_width }],
        ascent,
        descent,
        last: true,
        ..Line::default()
    };
    ParaBox { lines: vec![line], align: page_number_align(&pn.spec), ..ParaBox::default() }
}

/// A footer on one page: its paragraphs (`boxes`), then its page number below them, ending at
/// `bottom`.
#[allow(clippy::too_many_arguments)]
fn footer(painter: &Painter<'_>, out: &mut Vec<u8>, band: &Band, boxes: &[ParaBox], x0: f64, body: f64, bottom: f64, page: usize, pages: usize) {
    let number = band.page_number.as_ref().map(|pn| page_number_box(painter.layout, pn, body, page, pages));
    let height: f64 = boxes.iter().chain(number.iter()).map(ParaBox::height).sum();
    let mut y = bottom + height;
    for pb in boxes.iter().chain(number.iter()) {
        y -= painter.para_at(out, pb, x0, y);
    }
}
