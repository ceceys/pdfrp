//! Pdfrp: UYAP UDF files converted to PDF and read back. The files in `tests/data/udf` are
//! synthetic: made-up text, written to cover every element the format has (margins, justified
//! paragraphs, lists, tables, pictures, headers, footers with page numbers, tabs, fields, Java's
//! logical fonts, a landscape page, a signed file). Each carries the word `ÇĞİÖŞÜçğıöşü`.
//! 07 to 09 copy, with made-up text and plain-colour logos, the three header and footer layouts
//! real files were seen with (only their margins, offsets and paragraph and picture counts).

use std::sync::Arc;

use crate::tests::page_texts;
use crate::{DocId, Session, SourceKind};

const FILES: [&str; 9] = [
    "01-dilekce-iki-yana-imzali.udf",
    "02-dar-kenar-listeler.udf",
    "03-tablolar-ustbilgi-altbilgi.udf",
    "04-resim.udf",
    "05-cok-sayfa-sekme-alan.udf",
    "06-yatay-mantiksal-yazitipi.udf",
    "07-ustbilgi-logo-cizgi-altbilgi-alti.udf",
    "08-eksi-ust-kenar-logo.udf",
    "09-varsayilan-sablon.udf",
];

const TURKISH: &str = "ÇĞİÖŞÜçğıöşü";

fn fixture(name: &str) -> Arc<Vec<u8>> {
    Arc::new(std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/udf").join(name)).unwrap())
}

/// `name` converted as File ▸ Open converts it, opened, and its pages' text.
fn convert(name: &str) -> (Session, DocId, Vec<String>) {
    let mut s = Session::new();
    let (kind, pdf) = s.convert_to_pdf(name, &fixture(name)).unwrap();
    assert_eq!(kind, SourceKind::Udf, "{name}");
    let id = s.open_new(name.replace(".udf", ".pdf"), pdf).unwrap();
    let texts = page_texts(&s, id);
    (s, id, texts)
}

#[test]
fn every_synthetic_udf_converts_and_reads_back_its_turkish_letters() {
    for name in FILES {
        let (_, _, texts) = convert(name);
        let all = texts.join("\n");
        assert!(all.contains(TURKISH), "{name}: {all}");
    }
}

#[test]
fn a_long_udf_fills_pages_each_with_its_header() {
    let (_, _, texts) = convert("05-cok-sayfa-sekme-alan.udf");
    assert!(texts.len() >= 3, "{} pages", texts.len());
    for (i, text) in texts.iter().enumerate() {
        assert!(text.contains("Çok sayfalı sentetik belge"), "page {}: {text}", i + 1);
        assert!(text.contains("Uydurma kurum adı"), "the footer, page {}", i + 1);
    }
    let all = texts.join("\n");
    assert!(all.contains("Sentetik Davacı") && all.contains("30."), "fields and the last numbered heading");
}

#[test]
fn footer_page_numbers_count_the_pages_and_table_cells_read_back() {
    let (_, _, texts) = convert("03-tablolar-ustbilgi-altbilgi.udf");
    let n = texts.len();
    for (i, text) in texts.iter().enumerate() {
        assert!(text.contains(&format!("Sayfa {}/{n}", i + 1)), "page {}: {text}", i + 1);
        assert!(text.contains("Sentetik Üstbilgi"), "the header, page {}", i + 1);
    }
    let all = texts.join(" ");
    for cell in ["Ad Soyad", "Sentetik Kişi", "Bilirkişi", "Değer A", "Sağ sütun"] {
        assert!(all.contains(cell), "{cell}: {all}");
    }
}

#[test]
fn lists_bullet_and_number_their_items() {
    let (_, _, texts) = convert("02-dar-kenar-listeler.udf");
    let all = texts.join("\n");
    assert!(all.contains('\u{2022}'), "bullets: {all}");
    for marker in ["1.", "2.", "3."] {
        assert!(all.contains(marker), "{marker}: {all}");
    }
    assert!(all.contains("Kırmızı uyarı metni") && all.contains("altı çizili"), "{all}");
}

#[test]
fn a_signed_udf_is_known_and_its_signature_stays_behind() {
    assert!(crate::udf_signed(&fixture("01-dilekce-iki-yana-imzali.udf")));
    assert!(!crate::udf_signed(&fixture("02-dar-kenar-listeler.udf")));
    let s = Session::new();
    let (pdf, info) = s.create_from_udf("01", &fixture("01-dilekce-iki-yana-imzali.udf")).unwrap();
    assert!(info.signed);
    let mut s = Session::new();
    let id = s.open_new("01.pdf", pdf).unwrap();
    let info = &s.get(id).unwrap().info;
    assert!(info.fields.is_empty(), "no signature field is made");
    // Only the document's Subject says the signature stayed behind.
    assert_eq!(info.subject.as_deref(), Some("UDF dosyasından dönüştürüldü; elektronik imza taşınmadı"));
    let (unsigned, _) = Session::new().create_from_udf("02", &fixture("02-dar-kenar-listeler.udf")).unwrap();
    let id = s.open_new("02.pdf", unsigned).unwrap();
    assert_eq!(s.get(id).unwrap().info.subject, None);
}

#[test]
fn a_landscape_page_and_a_missing_font_are_handled() {
    let s = Session::new();
    let (pdf, info) = s.create_from_udf("06", &fixture("06-yatay-mantiksal-yazitipi.udf")).unwrap();
    assert!(info.notes.iter().any(|n| n.contains("Sentetik Olmayan Font")), "{:?}", info.notes);
    let mut s = Session::new();
    let id = s.open_new("06.pdf", pdf).unwrap();
    let page = &s.get(id).unwrap().info.pages[0];
    assert!(page.width > page.height, "landscape: {} × {}", page.width, page.height);
}

#[test]
fn a_udf_with_a_picture_draws_it_and_combines_with_a_pdf() {
    let mut s = Session::new();
    let udf = s.as_pdf("04-resim.udf", &fixture("04-resim.udf")).unwrap();
    let id = s.open_new("04.pdf", udf.clone()).unwrap();
    assert_eq!(page_texts(&s, id).len(), 1);
    let text = s.convert_to_pdf("notes.txt", &Arc::new(b"hello".to_vec())).unwrap().1;
    let combined = s.combine_ranges(&[("04".into(), udf, None), ("notes".into(), text, None)]).unwrap();
    let id = s.open_new("Combined.pdf", combined).unwrap();
    assert_eq!(page_texts(&s, id).len(), 2);
}

/// The header and footer of a fixture: the text of each of their lines (each line a paragraph
/// of its own, as `deneme/udf-uret.py` writes them). The body never has these words.
struct Bands {
    file: &'static str,
    header: &'static [&'static str],
    footer: &'static [&'static str],
    /// The header draws a rule of underscores under its logo.
    rule: bool,
    /// The pictures each page draws, all of them in the header.
    pictures: usize,
    /// The footer's page number reads "Sayfa n/N".
    numbered: bool,
    /// The top margin, where the header fits in it: the text starts there.
    fits_top: Option<f32>,
}

const BANDS: [Bands; 3] = [
    // Letterhead: a 177 × 60 logo, a red rule and two address lines in the header, six lines in
    // the footer; top and bottom margins 70.875, header and footer 20 from the edges.
    Bands {
        file: "07-ustbilgi-logo-cizgi-altbilgi-alti.udf",
        header: &["Sentetik Antet Hukuk Bürosu", "Uydurma Mah. Örnek Sok. No: 0, Sentetik İlçe"],
        footer: &[
            "Sentetik altbilgi 1: Uydurma Plaza, Kat 0",
            "Sentetik altbilgi 2: Telefon 000 000 00 00",
            "Sentetik altbilgi 3: Faks 000 000 00 01",
            "Sentetik altbilgi 4: e-posta yok",
            "Sentetik altbilgi 5: Vergi no 0000000000",
            "Sentetik altbilgi 6: Bu belge sınama içindir",
        ],
        rule: true,
        pictures: 1,
        numbered: false,
        fits_top: None,
    },
    // A top margin of -15 and a 130 × 70 logo 28 from the top; four footer lines and the page
    // number 27 from the bottom, over a bottom margin of 7.
    Bands {
        file: "08-eksi-ust-kenar-logo.udf",
        header: &[],
        footer: &[
            "Sentetik altbilgi A: Uydurma Bulvarı No: 0",
            "Sentetik altbilgi B: Sentetik İş Merkezi",
            "Sentetik altbilgi C: Telefon 000 000 00 00",
            "Sentetik altbilgi D: web adresi yok",
        ],
        rule: false,
        pictures: 1,
        numbered: true,
        fits_top: None,
    },
    // The editor's own template: a one-line header inside its top margin of 42.52; a footer 60
    // from the bottom (one line and an empty paragraph) over a bottom margin of 14.17.
    Bands {
        file: "09-varsayilan-sablon.udf",
        header: &["Sentetik üstbilgi: uydurma kurum"],
        footer: &["Sentetik altbilgi: uydurma adres"],
        rule: false,
        pictures: 0,
        numbered: false,
        fits_top: Some(42.52),
    },
];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Part {
    Body,
    Header,
    Footer,
}

/// Every glyph and picture page `p` of `pages` draws: its part of the page, its text ("picture"
/// for a picture) and its box in points, y down from the top of the page. Glyph boxes are the
/// text layer's (the em box, from 0.2 em below the baseline to 0.8 em above it), picture boxes
/// the placements the page's content makes: what the PDF draws, read back, not what the
/// converter meant to draw.
fn page_parts(
    doc: &crate::Document,
    renderer: &mut migez_render::PageRenderer,
    bands: &Bands,
    p: usize,
    pages: usize,
) -> Result<Vec<(Part, String, [f32; 4])>, String> {
    let at = format!("{}, page {}", bands.file, p + 1);
    let request = migez_render::RenderRequest { page: p, kind: migez_render::RequestKind::Text, scale: 1.0, ..Default::default() };
    let text = renderer.render(request).text.ok_or_else(|| format!("{at}: no text layer"))?;
    let mut part = vec![Part::Body; text.glyphs.len()];
    let mut phrases: Vec<(String, Part)> = bands.header.iter().map(|h| (h.to_string(), Part::Header)).collect();
    phrases.extend(bands.footer.iter().map(|f| (f.to_string(), Part::Footer)));
    if bands.numbered {
        phrases.push((format!("Sayfa {}/{pages}", p + 1), Part::Footer));
    }
    for (phrase, as_part) in phrases {
        let found = text.find(&phrase);
        if found.len() != 1 {
            return Err(format!("{at}: \"{phrase}\" reads back whole {} times, not once (drawn into the text?)", found.len()));
        }
        for i in found.into_iter().flatten() {
            part[i] = as_part;
        }
    }
    if bands.rule {
        let rule: Vec<usize> = text.glyphs.iter().enumerate().filter(|(_, g)| g.text == "_").map(|(i, _)| i).collect();
        if rule.len() != 70 {
            return Err(format!("{at}: the header's rule has {} of its 70 underscores", rule.len()));
        }
        for i in rule {
            part[i] = Part::Header;
        }
    }
    let mut boxes: Vec<(Part, String, [f32; 4])> =
        text.glyphs.iter().zip(&part).filter(|(g, _)| !g.text.trim().is_empty()).map(|(g, part)| (*part, g.text.clone(), g.rect)).collect();
    let top = doc.info.pages[p].crop[3];
    let pictures = doc.page_images(p);
    if pictures.len() != bands.pictures {
        return Err(format!("{at}: {} pictures, not the header's {}", pictures.len(), bands.pictures));
    }
    for picture in pictures {
        let [x0, y0, x1, y1] = picture.rect.map(|v| v as f32);
        boxes.push((Part::Header, "picture".into(), [x0, top - y1, x1, top - y0]));
    }
    Ok(boxes)
}

fn overlap(a: &[f32; 4], b: &[f32; 4]) -> bool {
    a[0] < b[2] && b[0] < a[2] && a[1] < b[3] && b[1] < a[3]
}

/// What is wrong with a page (`boxes`, [`page_parts`]) of a fixture, `at` naming it: text drawn
/// over the header or footer, or not between them.
fn page_problem(boxes: &[(Part, String, [f32; 4])], bands: &Bands, at: &str) -> Result<(), String> {
    let of = |part: Part| boxes.iter().filter(move |(q, _, _)| *q == part);
    let (header, footer): (Vec<_>, Vec<_>) = boxes.iter().filter(|(q, _, _)| *q != Part::Body).partition(|(q, _, _)| *q == Part::Header);
    if of(Part::Body).count() == 0 || footer.is_empty() {
        return Err(format!("{at}: no text or no footer"));
    }
    for (_, glyph, b) in of(Part::Body) {
        if let Some((part, what, other)) = header.iter().chain(&footer).find(|(_, _, other)| overlap(b, other)) {
            return Err(format!("{at}: the text's \"{glyph}\" at {b:?} is drawn over the {part:?}'s \"{what}\" at {other:?}"));
        }
    }
    // The text lies between them: below all of the header, above all of the footer.
    let text_top = of(Part::Body).map(|(_, _, b)| b[1]).fold(f32::MAX, f32::min);
    let text_bottom = of(Part::Body).map(|(_, _, b)| b[3]).fold(f32::MIN, f32::max);
    let header_bottom = header.iter().map(|(_, _, b)| b[3]).fold(f32::MIN, f32::max);
    let footer_top = footer.iter().map(|(_, _, b)| b[1]).fold(f32::MAX, f32::min);
    if !header.is_empty() && header_bottom > text_top {
        return Err(format!("{at}: the header ends at {header_bottom}, below the text's top {text_top}"));
    }
    if text_bottom > footer_top {
        return Err(format!("{at}: the text ends at {text_bottom}, below the footer's top {footer_top}"));
    }
    match bands.fits_top {
        // A header that fits in its top margin moves nothing: the text starts at the margin (its
        // first glyph's em box a little lower), as it did before headers took room from the text.
        Some(margin) if !(margin..margin + 3.0).contains(&text_top) => {
            Err(format!("{at}: the text starts at {text_top}, not at the top margin {margin}"))
        }
        // Clear of the header, and not much further down than that.
        None if text_top - header_bottom >= 24.0 => Err(format!("{at}: the text starts {} below the header", text_top - header_bottom)),
        _ => Ok(()),
    }
}

/// Real files whose header holds a logo and several lines, whose footer has four to six lines,
/// or whose top margin is negative had their text drawn over the header and footer. On every
/// page of the three fixtures that copy them, no glyph of the text touches anything the header
/// or footer draws, and the text lies between them. Each file's first problem is reported.
#[test]
fn the_text_never_runs_over_a_header_or_a_footer() {
    let mut problems = Vec::new();
    for bands in &BANDS {
        let (s, id, _) = convert(bands.file);
        let doc = s.get(id).unwrap();
        let mut renderer = migez_render::PageRenderer::new(doc.bytes.clone(), migez_render::RenderConfig::default());
        let pages = doc.info.pages.len();
        if pages < 2 {
            problems.push(format!("{}: the text fills {pages} page, not more", bands.file));
        }
        let checked = (0..pages)
            .try_for_each(|p| page_problem(&page_parts(doc, &mut renderer, bands, p, pages)?, bands, &format!("{}, page {}", bands.file, p + 1)));
        problems.extend(checked.err());
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// Files whose header and footer fit in their margins are laid out as before headers and footers
/// took room from the text: the same number of pages as the converter's first version made.
#[test]
fn headers_and_footers_that_fit_their_margins_move_nothing() {
    for (name, pages) in FILES.iter().take(6).zip([1, 1, 1, 1, 4, 1]) {
        let (_, _, texts) = convert(name);
        assert_eq!(texts.len(), pages, "{name}");
    }
}
