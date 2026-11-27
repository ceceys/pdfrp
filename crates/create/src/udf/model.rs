//! `content.xml` of a UDF file read into a document model: the whole text is one CDATA block, and
//! paragraphs, runs, tables, headers and footers point into it by `startOffset` and `length`.
//! Offsets count UTF-16 code units (the editor is a Java Swing program), attribute names are
//! Swing's `StyleConstants` names, lengths are points and colours are signed 32-bit ARGB.

use std::collections::HashMap;

use base64::Engine as _;

/// A style or a node's own formatting; `None` leaves the value to the style below it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Attrs {
    pub family: Option<String>,
    pub size: Option<f64>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    /// Signed 32-bit ARGB.
    pub foreground: Option<i64>,
    /// 0 left, 1 centre, 2 right, 3 justified (Swing's `ALIGN_*`).
    pub alignment: Option<i64>,
    pub left: Option<f64>,
    pub right: Option<f64>,
    pub first: Option<f64>,
    pub above: Option<f64>,
    pub below: Option<f64>,
    /// Swing's line spacing: a share of the line height added to it (0 = single).
    pub line_spacing: Option<f64>,
    pub tabs: Option<Vec<f64>>,
    pub bulleted: Option<bool>,
    pub numbered: Option<bool>,
    pub list_id: Option<String>,
    pub list_level: Option<i64>,
    /// The style this node takes its other values from.
    pub resolver: Option<String>,
}

impl Attrs {
    fn read(node: roxmltree::Node<'_, '_>) -> Self {
        let text = |name: &str| node.attribute(name).map(str::trim).filter(|v| !v.is_empty());
        let num = |name: &str| text(name).and_then(|v| v.replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite());
        let int = |name: &str| num(name).map(|v| v as i64);
        let flag = |name: &str| text(name).map(|v| v.eq_ignore_ascii_case("true") || v == "1");
        Self {
            family: text("family").map(str::to_string),
            size: num("size").filter(|s| *s > 0.0),
            bold: flag("bold"),
            italic: flag("italic"),
            underline: flag("underline"),
            foreground: int("foreground"),
            alignment: int("Alignment"),
            left: num("LeftIndent"),
            right: num("RightIndent"),
            first: num("FirstLineIndent"),
            above: num("SpaceAbove"),
            below: num("SpaceBelow"),
            line_spacing: num("LineSpacing"),
            tabs: text("TabSet").map(tab_stops),
            bulleted: flag("Bulleted"),
            numbered: flag("Numbered"),
            list_id: text("ListId").map(str::to_string),
            list_level: int("ListLevel"),
            resolver: text("resolver").map(str::to_string),
        }
    }

    /// These values, and `base`'s where these have none.
    pub fn over(&self, base: &Attrs) -> Attrs {
        macro_rules! pick {
            ($($f:ident),*) => { Attrs { $($f: self.$f.clone().or_else(|| base.$f.clone())),* } };
        }
        pick!(
            family,
            size,
            bold,
            italic,
            underline,
            foreground,
            alignment,
            left,
            right,
            first,
            above,
            below,
            line_spacing,
            tabs,
            bulleted,
            numbered,
            list_id,
            list_level,
            resolver
        )
    }
}

/// The tab stops a `TabSet` value names, in points: every number in it, after `@` where the
/// value marks positions that way (Swing's `TabStop` text), sorted.
fn tab_stops(value: &str) -> Vec<f64> {
    let marked = value.contains('@');
    let mut stops = Vec::new();
    let mut chars = value.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        let starts = c.is_ascii_digit() || (c == '-' && chars.peek().is_some_and(|(_, d)| d.is_ascii_digit()));
        if !starts {
            continue;
        }
        let before = value.get(..i).unwrap_or_default().trim_end();
        let mut end = i + c.len_utf8();
        while let Some(&(j, d)) = chars.peek() {
            if d.is_ascii_digit() || d == '.' {
                end = j + d.len_utf8();
                chars.next();
            } else {
                break;
            }
        }
        if marked && !before.ends_with('@') {
            continue;
        }
        if let Some(v) = value.get(i..end).and_then(|s| s.parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0) {
            stops.push(v);
        }
    }
    stops.sort_by(f64::total_cmp);
    stops.dedup();
    stops
}

/// What a run draws.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum RunKind {
    Text(String),
    Tab,
    /// A PNG (base64 in the file) and the size the file gives it, in points.
    Image {
        png: Vec<u8>,
        width: Option<f64>,
        height: Option<f64>,
    },
    /// The picture once embedded: XObject `/Im<index>`, drawn `width × height` points.
    Placed {
        index: usize,
        width: f64,
        height: f64,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Run {
    pub kind: RunKind,
    pub attrs: Attrs,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Para {
    pub attrs: Attrs,
    pub runs: Vec<Run>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Row {
    pub height: Option<f64>,
    pub cells: Vec<Vec<Para>>,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct Table {
    /// Column widths as shares (`columnSpans`); empty when the file gives none.
    pub spans: Vec<f64>,
    pub columns: usize,
    pub border: bool,
    pub rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::large_enum_variant, reason = "nearly every block is a paragraph: boxing them would allocate once per paragraph for nothing")]
pub(super) enum Block {
    Para(Para),
    Table(Table),
}

/// A footer's page number (`pageNumber-*`).
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageNumber {
    pub spec: String,
    pub attrs: Attrs,
    pub before: String,
    pub separator: String,
    pub start: i64,
}

/// A header or a footer and the pages it is on.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Band {
    pub paras: Vec<Para>,
    pub start_page: i64,
    pub stop_page: i64,
    pub page_number: Option<PageNumber>,
}

impl Band {
    /// Whether the band is on page `page` (1-based). 0 or less for the start means the first
    /// page, for the stop the last.
    pub fn on(&self, page: usize) -> bool {
        let page = i64::try_from(page).unwrap_or(i64::MAX);
        page >= self.start_page.max(1) && (self.stop_page <= 0 || page <= self.stop_page)
    }
}

/// `pageFormat`, in points.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct PageFormat {
    pub width: f64,
    pub height: f64,
    pub left: f64,
    pub right: f64,
    pub top: f64,
    pub bottom: f64,
    pub header_offset: Option<f64>,
    pub footer_offset: Option<f64>,
}

/// A4 in points.
pub(super) const A4: (f64, f64) = (595.276, 841.89);

impl Default for PageFormat {
    fn default() -> Self {
        Self { width: A4.0, height: A4.1, left: 70.875, right: 70.875, top: 56.69, bottom: 56.69, header_offset: None, footer_offset: None }
    }
}

/// A UDF document.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Udf {
    pub page: PageFormat,
    pub blocks: Vec<Block>,
    pub headers: Vec<Band>,
    pub footers: Vec<Band>,
    pub styles: HashMap<String, Attrs>,
    /// `elements`' own `resolver`: the style every paragraph starts from.
    pub root_style: Option<String>,
    /// What was left out or assumed, for the user.
    pub notes: Vec<String>,
}

impl Udf {
    /// The style named `name` with the styles it resolves to below it (at most a few levels: a
    /// loop of styles ends there), and with `base`, then the document's root style and `default`
    /// below those. A run's own style is read without them, so they don't hide its paragraph's.
    pub fn style(&self, name: Option<&str>, base: bool) -> Attrs {
        let mut out = Attrs::default();
        let mut next = name.map(str::to_string);
        for _ in 0..8 {
            let Some(n) = next.take() else { break };
            let Some(s) = self.styles.get(&n) else { break };
            out = out.over(s);
            next = s.resolver.clone().filter(|r| *r != n);
        }
        if base {
            for fallback in [self.root_style.as_deref(), Some("default")].into_iter().flatten() {
                if name != Some(fallback)
                    && let Some(s) = self.styles.get(fallback)
                {
                    out = out.over(s);
                }
            }
        }
        out
    }

    /// Every paragraph's runs, mutably: the body's, its tables' cells', and the headers' and
    /// footers'.
    pub fn runs_mut(&mut self) -> impl Iterator<Item = &mut Run> {
        let body = self.blocks.iter_mut().flat_map(|b| match b {
            Block::Para(p) => std::slice::from_mut(p).iter_mut().flat_map(|p| p.runs.iter_mut()).collect::<Vec<_>>(),
            Block::Table(t) => t.rows.iter_mut().flat_map(|r| r.cells.iter_mut().flatten()).flat_map(|p| p.runs.iter_mut()).collect(),
        });
        let bands = self.headers.iter_mut().chain(self.footers.iter_mut()).flat_map(|b| b.paras.iter_mut()).flat_map(|p| p.runs.iter_mut());
        body.chain(bands)
    }
}

/// The text of `content.xml`: UTF-8 (with or without a byte order mark), else Windows-1254.
fn decode(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => encoding_rs::WINDOWS_1254.decode(bytes).0.into_owned(),
    }
}

/// Reads the model. `text` is the CDATA, as UTF-16 code units.
struct Reader<'t> {
    text: &'t [u16],
    notes: Vec<String>,
    skipped: HashMap<String, usize>,
}

impl Reader<'_> {
    fn slice(&self, node: roxmltree::Node<'_, '_>) -> String {
        let num = |name: &str| node.attribute(name).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0).max(0);
        let start = usize::try_from(num("startOffset")).unwrap_or(usize::MAX).min(self.text.len());
        let len = usize::try_from(num("length")).unwrap_or(0);
        let end = start.saturating_add(len).min(self.text.len());
        String::from_utf16_lossy(self.text.get(start..end).unwrap_or_default())
    }

    fn image(&mut self, node: roxmltree::Node<'_, '_>) -> Option<RunKind> {
        let data = node.attribute("imageData")?;
        let clean: String = data.chars().filter(|c| !c.is_ascii_whitespace()).collect();
        match base64::engine::general_purpose::STANDARD.decode(clean.as_bytes()) {
            Ok(png) => {
                let size = |name: &str| {
                    node.attribute(name).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite() && *v > 0.0)
                };
                Some(RunKind::Image { png, width: size("width"), height: size("height") })
            }
            Err(_) => {
                self.notes.push("a picture whose data could not be read was left out".into());
                None
            }
        }
    }

    fn para(&mut self, node: roxmltree::Node<'_, '_>) -> Para {
        let mut runs = Vec::new();
        for child in node.children().filter(roxmltree::Node::is_element) {
            let attrs = Attrs::read(child);
            let name = child.tag_name().name();
            if let Some(image) = self.image(child) {
                runs.push(Run { kind: image, attrs });
                continue;
            }
            if name == "image" {
                continue;
            }
            if child.attribute("startOffset").is_none() {
                *self.skipped.entry(name.to_string()).or_default() += 1;
                continue;
            }
            let text = self.slice(child);
            if name == "tab" {
                runs.push(Run { kind: RunKind::Tab, attrs });
                continue;
            }
            // Tabs inside a run are tabs; the paragraph's own line break ends it.
            let mut part = String::new();
            for c in text.chars() {
                match c {
                    '\t' => {
                        if !part.is_empty() {
                            runs.push(Run { kind: RunKind::Text(std::mem::take(&mut part)), attrs: attrs.clone() });
                        }
                        runs.push(Run { kind: RunKind::Tab, attrs: attrs.clone() });
                    }
                    '\n' | '\r' | '\u{2028}' | '\u{2029}' => {}
                    c => part.push(c),
                }
            }
            if !part.is_empty() {
                runs.push(Run { kind: RunKind::Text(part), attrs });
            }
        }
        Para { attrs: Attrs::read(node), runs }
    }

    /// Paragraphs under `node`, a table's cells flattened into paragraphs where they nest.
    fn paras(&mut self, node: roxmltree::Node<'_, '_>) -> Vec<Para> {
        let mut out = Vec::new();
        for child in node.children().filter(roxmltree::Node::is_element) {
            match child.tag_name().name() {
                "paragraph" => out.push(self.para(child)),
                "table" | "row" | "cell" => out.extend(self.paras(child)),
                other => *self.skipped.entry(other.to_string()).or_default() += 1,
            }
        }
        out
    }

    fn table(&mut self, node: roxmltree::Node<'_, '_>) -> Table {
        let spans: Vec<f64> = node
            .attribute("columnSpans")
            .map(|v| v.split([',', ';', ' ']).filter_map(|s| s.trim().parse::<f64>().ok()).filter(|w| w.is_finite() && *w > 0.0).collect())
            .unwrap_or_default();
        let columns = node.attribute("columnCount").and_then(|v| v.trim().parse::<usize>().ok()).unwrap_or(spans.len()).clamp(1, 64);
        let border = node.attribute("border").is_none_or(|b| !b.eq_ignore_ascii_case("borderNone"));
        let mut rows = Vec::new();
        for row in node.children().filter(|n| n.is_element() && n.tag_name().name() == "row") {
            let height = row.attribute("height").and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|h| h.is_finite() && *h > 0.0);
            let cells = row.children().filter(|n| n.is_element() && n.tag_name().name() == "cell").map(|cell| self.paras(cell)).collect();
            rows.push(Row { height, cells });
        }
        Table { spans, columns, border, rows }
    }

    fn band(&mut self, node: roxmltree::Node<'_, '_>) -> Band {
        let int = |name: &str| node.attribute(name).and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(0);
        let page_number = node.attribute("pageNumber-spec").map(str::trim).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("none")).map(|spec| {
            let get = |name: &str| node.attribute(name).map(str::to_string);
            let num = |name: &str| get(name).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite());
            let flag = |name: &str| get(name).map(|v| v.trim().eq_ignore_ascii_case("true"));
            PageNumber {
                spec: spec.to_string(),
                attrs: Attrs {
                    family: get("pageNumber-fontFace").filter(|f| !f.trim().is_empty()),
                    size: num("pageNumber-fontSize").filter(|s| *s > 0.0),
                    bold: flag("pageNumber-fontBold"),
                    italic: flag("pageNumber-fontItalic"),
                    foreground: num("pageNumber-color").map(|v| v as i64),
                    ..Attrs::default()
                },
                before: get("pageNumber-foreStr").unwrap_or_default(),
                separator: get("pageNumber-seperator").unwrap_or_default(),
                start: get("pageNumber-pageStartNumStr").and_then(|v| v.trim().parse::<i64>().ok()).unwrap_or(1),
            }
        });
        Band { paras: self.paras(node), start_page: int("startPage"), stop_page: int("stopPage"), page_number }
    }
}

/// Reads `content.xml`.
pub(super) fn read(content_xml: &[u8]) -> Result<Udf, String> {
    let xml = decode(content_xml);
    let doc = roxmltree::Document::parse(&xml).map_err(|e| format!("the UDF file's content.xml can't be read: {e}"))?;
    let root = doc.root_element();
    let child = |name: &str| root.children().find(|n| n.is_element() && n.tag_name().name() == name);
    let cdata: String = child("content").map(|c| c.children().filter_map(|t| t.text()).collect()).unwrap_or_default();
    let text: Vec<u16> = cdata.encode_utf16().collect();
    let mut udf = Udf::default();
    if let Some(format) = child("properties").and_then(|p| p.children().find(|n| n.is_element() && n.tag_name().name() == "pageFormat")) {
        let num = |name: &str| format.attribute(name).and_then(|v| v.trim().replace(',', ".").parse::<f64>().ok()).filter(|v| v.is_finite());
        let page = &mut udf.page;
        // A negative margin (one template has a top margin of -15) is no margin.
        let margin = |v: Option<f64>, default: f64| v.map_or(default, |m| m.clamp(0.0, 400.0));
        page.left = margin(num("leftMargin"), page.left);
        page.right = margin(num("rightMargin"), page.right);
        page.top = margin(num("topMargin"), page.top);
        page.bottom = margin(num("bottomMargin"), page.bottom);
        page.header_offset = num("headerFOffset").filter(|v| *v > 0.0);
        page.footer_offset = num("footerFOffset").filter(|v| *v > 0.0);
        // Java's PageFormat: 1 portrait, 0 landscape, 2 reverse landscape.
        if matches!(num("paperOrientation").map(|v| v as i64), Some(0 | 2)) {
            std::mem::swap(&mut page.width, &mut page.height);
        }
        if num("mediaSizeName").is_some_and(|v| v as i64 != 1) {
            udf.notes.push("the page size code is not the usual A4 one; the pages are A4".into());
        }
    }
    if let Some(styles) = child("styles") {
        for style in styles.children().filter(|n| n.is_element() && n.tag_name().name() == "style") {
            if let Some(name) = style.attribute("name") {
                udf.styles.insert(name.to_string(), Attrs::read(style));
            }
        }
    }
    let mut reader = Reader { text: &text, notes: Vec::new(), skipped: HashMap::new() };
    if let Some(elements) = child("elements") {
        udf.root_style = elements.attribute("resolver").map(str::to_string);
        for node in elements.children().filter(roxmltree::Node::is_element) {
            match node.tag_name().name() {
                "paragraph" => udf.blocks.push(Block::Para(reader.para(node))),
                "table" => udf.blocks.push(Block::Table(reader.table(node))),
                "header" => udf.headers.push(reader.band(node)),
                "footer" => udf.footers.push(reader.band(node)),
                other => *reader.skipped.entry(other.to_string()).or_default() += 1,
            }
        }
    }
    udf.notes.append(&mut reader.notes);
    let mut skipped: Vec<_> = reader.skipped.into_iter().collect();
    skipped.sort();
    for (name, n) in skipped {
        udf.notes.push(format!("{n} \"{name}\" element(s) the converter doesn't know were left out"));
    }
    Ok(udf)
}
