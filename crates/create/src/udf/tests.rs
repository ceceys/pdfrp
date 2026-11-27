use std::io::Write as _;
use std::sync::Arc;

use migez_cos::{Document, Object, SaveOptions, write_full};

use super::{from_udf, is_udf, model, udf_signed, zip};

/// The document written and read back.
fn reopen(doc: &Document) -> (Document, Vec<u8>) {
    let bytes = write_full(doc, &SaveOptions::default()).unwrap();
    (Document::open(Arc::new(bytes.clone())).unwrap(), bytes)
}

/// The decoded content of every page, one after the other.
fn contents(doc: &Document) -> Vec<u8> {
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let kids = doc.get(pages).as_dict().unwrap().get(b"Kids").unwrap().as_array().unwrap().clone();
    let mut out = Vec::new();
    for kid in kids {
        let page = doc.resolve(&kid).as_dict().cloned().unwrap();
        if let Some(c) = page.get(b"Contents")
            && let Object::Stream(s) = &*doc.resolve(c)
        {
            out.extend(s.decoded().unwrap());
        }
    }
    out
}

/// A ZIP archive of `entries` (name, bytes, deflated), as the UDF editor writes one.
pub(crate) fn zip_of(entries: &[(&str, &[u8], bool)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, deflate) in entries {
        let mut crc = flate2::Crc::new();
        crc.update(data);
        let body = if *deflate {
            let mut e = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(data).unwrap();
            e.finish().unwrap()
        } else {
            data.to_vec()
        };
        let method: u16 = if *deflate { 8 } else { 0 };
        let offset = out.len() as u32;
        let head = |sig: u32, out: &mut Vec<u8>, central: bool| {
            out.extend(sig.to_le_bytes());
            if central {
                out.extend(20u16.to_le_bytes());
            }
            out.extend(20u16.to_le_bytes());
            out.extend(0u16.to_le_bytes());
            out.extend(method.to_le_bytes());
            out.extend([0u8; 4]);
            out.extend(crc.sum().to_le_bytes());
            out.extend((body.len() as u32).to_le_bytes());
            out.extend((data.len() as u32).to_le_bytes());
            out.extend((name.len() as u16).to_le_bytes());
            out.extend(0u16.to_le_bytes());
        };
        head(0x0403_4b50, &mut out, false);
        out.extend(name.as_bytes());
        out.extend(&body);
        head(0x0201_4b50, &mut central, true);
        // Comment length, disk, internal and external attributes.
        central.extend([0u8; 10]);
        central.extend(offset.to_le_bytes());
        central.extend(name.as_bytes());
    }
    let dir = out.len() as u32;
    out.extend(&central);
    out.extend(0x0605_4b50u32.to_le_bytes());
    out.extend([0u8; 4]);
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((entries.len() as u16).to_le_bytes());
    out.extend((central.len() as u32).to_le_bytes());
    out.extend(dir.to_le_bytes());
    out.extend(0u16.to_le_bytes());
    out
}

/// `content.xml` with `text` as its CDATA and `elements` inside `<elements>`.
fn xml(text: &str, page: &str, elements: &str, styles: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<template format_id=\"1.8\"><content><![CDATA[{text}]]></content><properties>{page}</properties><elements resolver=\"hvl-default\">{elements}</elements><styles>{styles}</styles></template>"
    )
}

fn udf(content: &str, signed: bool) -> Vec<u8> {
    let mut entries: Vec<(&str, &[u8], bool)> = vec![("content.xml", content.as_bytes(), true)];
    if signed {
        entries.push(("sign.sgn", b"not a real signature", false));
    }
    zip_of(&entries)
}

/// UTF-16 offset and length of `part` in `text`.
fn at(text: &str, part: &str) -> (usize, usize) {
    let start = text.find(part).unwrap();
    (text[..start].encode_utf16().count(), part.encode_utf16().count())
}

fn content(text: &str, part: &str, attrs: &str) -> String {
    let (s, l) = at(text, part);
    format!("<content startOffset=\"{s}\" length=\"{l}\" {attrs}/>")
}

fn page_count(doc: &Document) -> usize {
    doc.root()
        .and_then(|r| doc.get(r).as_dict().and_then(|d| d.reference(b"Pages")))
        .and_then(|p| doc.get(p).as_dict().and_then(|d| d.get(b"Count").and_then(Object::as_int)))
        .unwrap_or(0) as usize
}

#[test]
fn the_zip_reader_reads_stored_and_deflated_entries_and_refuses_others() {
    let bytes = zip_of(&[("a.txt", b"stored", false), ("dir/content.xml", b"deflated deflated deflated", true)]);
    let archive = zip::Archive::open(&bytes).unwrap();
    assert_eq!(archive.entries.len(), 2);
    assert_eq!(archive.read(archive.find("a.txt").unwrap()).unwrap(), b"stored");
    assert_eq!(archive.read(archive.find("DIR/CONTENT.XML").unwrap()).unwrap(), b"deflated deflated deflated");
    assert!(zip::Archive::open(b"%PDF-1.7").is_err());
    assert!(zip::Archive::open(b"PK\x03\x04 and nothing else").is_err());
    // Cut short: the directory is gone.
    assert!(zip::Archive::open(&bytes[..bytes.len() - 30]).is_err());
}

#[test]
fn a_udf_is_known_by_its_name_and_its_zip_signature() {
    let bytes = udf(&xml("a\n", "", "", ""), false);
    assert!(is_udf("Dilekçe.UDF", &bytes));
    assert!(!is_udf("arşiv.zip", &bytes));
    assert!(!is_udf("boş.udf", b"plain text"));
    assert_eq!(crate::source_kind("dava.udf", &bytes), Some(crate::SourceKind::Udf));
    assert!(crate::CONVERTIBLE.contains(&"udf"));
}

#[test]
fn the_signature_is_seen_and_never_carried() {
    let text = "İmzalı belge\n";
    let el = format!("<paragraph>{}</paragraph>", content(text, "İmzalı belge", ""));
    let signed = udf(&xml(text, "", &el, ""), true);
    assert!(udf_signed(&signed));
    let (doc, info) = from_udf("imzalı", &signed, &[]).unwrap();
    assert!(info.signed);
    let (reopened, bytes) = reopen(&doc);
    assert!(!bytes.windows(4).any(|w| w == b"/Sig"), "no signature in the PDF");
    // Only the document's Subject says so; the page itself says nothing of it.
    assert_eq!(info_text(&reopened, b"Subject").as_deref(), Some("UDF dosyasından dönüştürüldü; elektronik imza taşınmadı"));
    assert_eq!(info_text(&reopened, b"Title").as_deref(), Some("imzalı"));
    let unsigned = udf(&xml(text, "", &el, ""), false);
    assert!(!udf_signed(&unsigned));
    let (doc, info) = from_udf("imzasız", &unsigned, &[]).unwrap();
    assert!(!info.signed);
    let plain = reopen(&doc).0;
    assert_eq!(info_text(&plain, b"Subject"), None, "an unsigned file's PDF has no Subject");
    // The signed file's page is drawn exactly as the unsigned one's: nothing on it about the signature.
    assert_eq!(contents(&reopened), contents(&plain));
}

/// The text of the document information entry `key`, if there is one.
fn info_text(doc: &Document, key: &[u8]) -> Option<String> {
    let info = doc.resolve(doc.trailer().get(b"Info")?);
    info.as_dict()?.get(key)?.as_string().map(|s| s.to_text())
}

#[test]
fn offsets_count_utf16_units_and_runs_take_their_text() {
    // An emoji takes two UTF-16 units: the run after it must still start at "Çay".
    let text = "Ağ😀Çay ılık\n";
    let el = format!("<paragraph>{}{}</paragraph>", content(text, "Ağ😀", ""), content(text, "Çay ılık", "bold=\"true\""));
    let model = model::read(xml(text, "", &el, "").as_bytes()).unwrap();
    let model::Block::Para(p) = &model.blocks[0] else { panic!("a paragraph") };
    let texts: Vec<_> = p.runs.iter().map(|r| r.kind.clone()).collect();
    assert_eq!(texts, [model::RunKind::Text("Ağ😀".into()), model::RunKind::Text("Çay ılık".into())]);
    assert_eq!(p.runs[1].attrs.bold, Some(true));
    // Out-of-range offsets are cut to the text, never a panic.
    let bad = xml("kısa\n", "", "<paragraph><content startOffset=\"3\" length=\"999\"/><content startOffset=\"-4\" length=\"2\"/></paragraph>", "");
    let model = model::read(bad.as_bytes()).unwrap();
    let model::Block::Para(p) = &model.blocks[0] else { panic!("a paragraph") };
    assert_eq!(p.runs[0].kind, model::RunKind::Text("a".into()));
    assert_eq!(p.runs[1].kind, model::RunKind::Text("kı".into()));
}

#[test]
fn styles_resolve_run_over_paragraph_over_document() {
    let text = "Metin\n";
    let styles = "<style name=\"default\" family=\"Dialog\" size=\"12\"/><style name=\"hvl-default\" family=\"Times New Roman\" size=\"12\" Alignment=\"3\"/><style name=\"başlık\" size=\"16\" bold=\"true\"/>";
    let el = format!("<paragraph resolver=\"başlık\" LeftIndent=\"25\">{}</paragraph>", content(text, "Metin", "italic=\"true\""));
    let m = model::read(xml(text, "", &el, styles).as_bytes()).unwrap();
    let model::Block::Para(p) = &m.blocks[0] else { panic!("a paragraph") };
    let pa = p.attrs.over(&m.style(p.attrs.resolver.as_deref(), true));
    assert_eq!(pa.size, Some(16.0), "the paragraph's style");
    assert_eq!(pa.family.as_deref(), Some("Times New Roman"), "the document's root style, over default");
    assert_eq!(pa.alignment, Some(3));
    assert_eq!(pa.left, Some(25.0));
    let ra = p.runs[0].attrs.over(&m.style(p.runs[0].attrs.resolver.as_deref(), false)).over(&pa);
    assert_eq!((ra.italic, ra.bold, ra.size), (Some(true), Some(true), Some(16.0)));
}

#[test]
fn page_format_is_in_points_with_no_negative_margin() {
    let page = "<pageFormat mediaSizeName=\"1\" leftMargin=\"42.52\" rightMargin=\"56.69\" topMargin=\"-15\" bottomMargin=\"70.875\" paperOrientation=\"1\"/>";
    let m = model::read(xml("a\n", page, "", "").as_bytes()).unwrap();
    assert_eq!((m.page.left, m.page.right, m.page.top, m.page.bottom), (42.52, 56.69, 0.0, 70.875));
    assert_eq!((m.page.width, m.page.height), model::A4);
    let landscape = "<pageFormat paperOrientation=\"0\"/>";
    let m = model::read(xml("a\n", landscape, "", "").as_bytes()).unwrap();
    assert!(m.page.width > m.page.height);
}

/// A long justified document with a list, a table, a picture, a header and a footer converts,
/// fills more than one page and draws each kind of thing.
#[test]
fn a_full_document_converts_to_several_pages() {
    let para = "Bu paragraf iki yana yaslanmış uzun bir örnek metindir; çığır, şölen, ılgaz, İzmir ve Ağrı gibi Türkçe sözcükler içerir. ";
    let mut text = String::from("Üst bilgi\n");
    let mut elements =
        format!("<header startPage=\"1\">{}</header>", "<paragraph Alignment=\"1\"><content startOffset=\"0\" length=\"9\"/></paragraph>");
    for i in 0..40 {
        let p = format!("{i}. {}{}\n", para, para);
        let (s, l) = (text.encode_utf16().count(), p.trim_end().encode_utf16().count());
        text.push_str(&p);
        elements.push_str(&format!("<paragraph Alignment=\"3\" FirstLineIndent=\"35.4375\" SpaceBelow=\"6\"><content startOffset=\"{s}\" length=\"{l}\" family=\"Times New Roman\" size=\"12\"/></paragraph>"));
    }
    for item in ["Birinci madde", "İkinci madde"] {
        let s = text.encode_utf16().count();
        text.push_str(item);
        text.push('\n');
        elements.push_str(&format!(
            "<paragraph Numbered=\"true\" NumberType=\"NUMBER_TYPE_NUMBER_DOT\" ListId=\"1\" ListLevel=\"0\" LeftIndent=\"36\" FirstLineIndent=\"-18\"><content startOffset=\"{s}\" length=\"{}\"/></paragraph>",
            item.encode_utf16().count()
        ));
    }
    let mut cells = String::new();
    for cell in ["Esas No", "2026/1", "Karar"] {
        let s = text.encode_utf16().count();
        text.push_str(cell);
        text.push('\n');
        cells.push_str(&format!("<cell><paragraph><content startOffset=\"{s}\" length=\"{}\"/></paragraph></cell>", cell.encode_utf16().count()));
    }
    elements.push_str(&format!(
        "<table border=\"borderCell\" columnCount=\"3\" columnSpans=\"80,121,99\"><row rowType=\"dataRow\">{cells}</row></table>"
    ));
    let png = {
        let mut out = Vec::new();
        {
            let mut e = png::Encoder::new(&mut out, 4, 2);
            e.set_color(png::ColorType::Rgb);
            e.set_depth(png::BitDepth::Eight);
            let mut w = e.write_header().unwrap();
            w.write_image_data(&[200u8; 24]).unwrap();
        }
        out
    };
    let b64 = {
        use base64::Engine as _;
        base64::engine::general_purpose::STANDARD.encode(&png)
    };
    let s = text.encode_utf16().count();
    text.push_str(" \n");
    elements.push_str(&format!(
        "<paragraph Alignment=\"1\"><image imageData=\"{b64}\" width=\"40\" height=\"20\" startOffset=\"{s}\" length=\"1\"/></paragraph>"
    ));
    elements.push_str(
        "<footer pageNumber-spec=\"BSP32_ALT_ORTA\" pageNumber-foreStr=\"Sayfa \" pageNumber-seperator=\"/\" pageNumber-fontSize=\"9\"></footer>",
    );
    let page = "<pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"56.69\" topMargin=\"56.69\" bottomMargin=\"56.69\" paperOrientation=\"1\" headerFOffset=\"20\" footerFOffset=\"20\"/>";
    let (doc, info) = from_udf("uzun", &udf(&xml(&text, page, &elements, ""), false), &[]).unwrap();
    assert!(info.pages >= 2, "{info:?}");
    assert_eq!(page_count(&doc), info.pages);
    let (doc, _) = reopen(&doc);
    let contents = contents(&doc);
    let has = |needle: &[u8]| contents.windows(needle.len()).any(|w| w == needle);
    assert!(has(b"Tj"), "text");
    assert!(has(b" re S"), "the table's borders");
    assert!(has(b"/Im0 Do"), "the picture");
    assert!(info.notes.iter().all(|n| !n.contains("left out")), "{:?}", info.notes);
}

/// Appends `line` to `text` and returns a paragraph of it, with `attrs` on the paragraph and
/// `run` on its text.
fn para(text: &mut String, line: &str, attrs: &str, run: &str) -> String {
    let s = text.encode_utf16().count();
    text.push_str(line);
    text.push('\n');
    format!("<paragraph {attrs}><content startOffset=\"{s}\" length=\"{}\" {run}/></paragraph>", line.encode_utf16().count())
}

/// `count` long justified paragraphs, appended to `text`.
fn long_body(text: &mut String, count: usize) -> String {
    let sentence = "Bu paragraf iki yana yaslanmış uzun bir örnek metindir; çığır, şölen, ılgaz, İzmir ve Ağrı gibi Türkçe sözcükler içerir. ";
    (0..count).map(|i| para(text, &format!("{i}. {sentence}{sentence}"), "Alignment=\"3\" SpaceBelow=\"6\"", "")).collect()
}

/// The editor's own template: the header 15 below the top of the page over a top margin of
/// 42.52, the footer 60 above its bottom over a bottom margin of 14.17.
const TEMPLATE_PAGE: &str = "<pageFormat mediaSizeName=\"1\" leftMargin=\"70.875\" rightMargin=\"56.69\" topMargin=\"42.52\" bottomMargin=\"14.17\" paperOrientation=\"1\" headerFOffset=\"15\" footerFOffset=\"60\"/>";

/// A header or footer that draws nothing (empty paragraphs, a run of spaces) takes no room from
/// the text: the pages are drawn exactly as without it. A footer line that draws does: the text
/// ends above it.
#[test]
fn a_header_or_footer_that_draws_nothing_takes_no_room_from_the_text() {
    let mut text = String::new();
    let body = long_body(&mut text, 30);
    let spaces = para(&mut text, "   ", "", "");
    let line = para(&mut text, "Alt bilgi satırı", "Alignment=\"1\"", "size=\"8\"");
    let convert = |bands: &str| {
        let (doc, info) = from_udf("şablon", &udf(&xml(&text, TEMPLATE_PAGE, &format!("{bands}{body}"), ""), false), &[]).unwrap();
        (info.pages, page_contents(&reopen(&doc).0))
    };
    let bare = convert("");
    assert!(bare.0 >= 2, "{} pages", bare.0);
    let blank = convert(&format!("<header><paragraph/></header><footer><paragraph/>{spaces}<paragraph/></footer>"));
    assert_eq!(blank.0, bare.0);
    assert!(blank.1 == bare.1, "the pages are drawn as without the header and footer");
    // The footer's line (8 points) is drawn above an empty paragraph, 60 above the bottom; the
    // text (12 points) on the full first page ends above it, where it ran down to 14.17 before.
    let (pages, drawn) = convert(&format!("<footer>{line}<paragraph/></footer>"));
    assert!(pages >= bare.0, "{pages} pages, {} without the footer", bare.0);
    let ops = text_ops(&drawn[0]);
    let lowest = |size: f64| ops.iter().filter(|(s, _)| *s == size).map(|(_, y)| *y).fold(f64::MAX, f64::min);
    let (footer, text) = (lowest(8.0), lowest(12.0));
    assert!(footer > 60.0 && footer < 100.0, "the footer's baseline: {footer}");
    assert!(text > footer + 10.0, "the text's lowest baseline {text} is clear of the footer's {footer}");
}

/// The decoded content of each page.
fn page_contents(doc: &Document) -> Vec<Vec<u8>> {
    let pages = doc.get(doc.root().unwrap()).as_dict().unwrap().reference(b"Pages").unwrap();
    let kids = doc.get(pages).as_dict().unwrap().get(b"Kids").unwrap().as_array().unwrap().clone();
    kids.iter()
        .map(|kid| {
            let page = doc.resolve(kid).as_dict().cloned().unwrap();
            match &*doc.resolve(page.get(b"Contents").unwrap()) {
                Object::Stream(s) => s.decoded().unwrap(),
                _ => panic!("a content stream"),
            }
        })
        .collect()
}

/// Each text the page content shows (one `BT … ET` a line, as the converter writes them): its
/// font size and its baseline, from the bottom of the page.
fn text_ops(content: &[u8]) -> Vec<(f64, f64)> {
    String::from_utf8_lossy(content)
        .lines()
        .filter_map(|l| {
            let t: Vec<&str> = l.split_whitespace().collect();
            let before = |op: &str| t.iter().position(|w| *w == op).and_then(|i| t.get(i.checked_sub(1)?)).and_then(|v| v.parse::<f64>().ok());
            Some((before("Tf")?, before("Tm")?))
        })
        .collect()
}

/// A header and footer that leave the text less than an inch of the page: it keeps the margins'
/// room (and may run over them), and a note says so. Nothing breaks.
#[test]
fn a_header_and_footer_that_leave_no_room_keep_the_margins_and_say_so() {
    let mut text = String::new();
    let body = long_body(&mut text, 12);
    let header: String = (1..=30).map(|i| para(&mut text, &format!("Üst bilgi satırı {i}"), "", "")).collect();
    let footer: String = (1..=30).map(|i| para(&mut text, &format!("Alt bilgi satırı {i}"), "", "")).collect();
    let page = "<pageFormat topMargin=\"56.69\" bottomMargin=\"56.69\" headerFOffset=\"20\" footerFOffset=\"20\"/>";
    let (_, bare) = from_udf("boş", &udf(&xml(&text, page, &body, ""), false), &[]).unwrap();
    let bands = format!("<header>{header}</header>{body}<footer>{footer}</footer>");
    let (doc, info) = from_udf("dolu", &udf(&xml(&text, page, &bands, ""), false), &[]).unwrap();
    assert!(info.notes.iter().any(|n| n.contains("too little room")), "{:?}", info.notes);
    assert!(bare.notes.iter().all(|n| !n.contains("too little room")), "{:?}", bare.notes);
    assert_eq!(info.pages, bare.pages, "the text keeps the margins' room");
    assert_eq!(page_count(&doc), info.pages);
}

#[test]
fn broken_files_are_refused_with_a_reason() {
    let no_content = zip_of(&[("sign.sgn", b"x", false)]);
    assert!(from_udf("x", &no_content, &[]).unwrap_err().to_string().contains("content.xml"));
    let bad_xml = zip_of(&[("content.xml", b"<template><content>", false)]);
    assert!(from_udf("x", &bad_xml, &[]).unwrap_err().to_string().contains("content.xml"));
    // An empty document is one blank page.
    let (_, info) = from_udf("boş", &udf(&xml("", "", "", ""), false), &[]).unwrap();
    assert_eq!(info.pages, 1);
}
