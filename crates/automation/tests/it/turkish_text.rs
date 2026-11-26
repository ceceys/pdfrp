//! Pdfrp: Turkish text through every path that writes text into a PDF, end to end through the
//! tools. A pangram with every Turkish letter (`ğ Ğ ı I İ i ş Ş ç Ç ö Ö ü Ü`) is written, the
//! document saved and opened again, and its text read back (`text_extract`, after flattening where
//! the text is a comment's or a field's appearance): it must come back exactly as typed. WinAnsi,
//! the standard fonts' encoding, has `ç ö ü` but not `ğ ı ş İ`; those used to come out as `?`, or
//! the text was refused. They are drawn in the bundled Inter face, embedded once per document.

use std::path::Path;

use migez_automation::{Automation, Content};
use serde_json::{Value, json};

/// Every Turkish letter, both cases, with the dotted and dotless i pairs side by side.
const PANGRAM: &str = "Pijamalı hasta yağız şoföre çabucak güvendi. PİJAMALI HASTA YAĞIZ ŞOFÖRE ÇABUCAK GÜVENDİ. ğĞıIİişŞçÇöÖüÜ";

fn auto(dir: &Path) -> Automation {
    Automation::new().with_root(dir).unwrap().with_clock(|| 1_700_000_000)
}

fn ok(a: &mut Automation, tool: &str, args: Value) -> Value {
    match a.call(tool, &args) {
        Ok(mut c) => match c.remove(0) {
            Content::Json(v) => v,
            other => panic!("{tool}: expected JSON, got {other:?}"),
        },
        Err(e) => panic!("{tool} {args}: {e}"),
    }
}

/// Text as copy reads it, with line breaks and runs of spaces as one space.
fn flat(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The text of every page of the saved file `name`, read after opening it again; comments and
/// fields are flattened into the page first when `flatten` (their appearances are what shows).
/// The first page is rendered too, so a font the renderer can't use fails here.
fn read_back(a: &mut Automation, name: &str, flatten: bool) -> String {
    let doc = ok(a, "doc_open", json!({ "path": name }))["doc"].as_u64().unwrap();
    let png = a.call("page_render", &json!({ "doc": doc, "page": 1, "dpi": 72 })).unwrap();
    assert!(png.iter().any(|c| matches!(c, Content::Png { .. })), "{name}: rendered");
    if flatten {
        ok(a, "doc_flatten", json!({ "doc": doc }));
    }
    let r = ok(a, "text_extract", json!({ "doc": doc }));
    ok(a, "doc_close", json!({ "doc": doc, "discard_changes": true }));
    r["pages"].as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap_or_default().to_string()).collect::<Vec<_>>().join("\n")
}

/// A blank page, `edit` applied to it (its result must carry no warning: nothing is drawn as `?`),
/// saved as `name`.
fn written(a: &mut Automation, name: &str, edit: impl FnOnce(&mut Automation, u64) -> Value) {
    let doc = ok(a, "doc_create", json!({ "from": "blank" }))["doc"].as_u64().unwrap();
    let result = edit(a, doc);
    assert!(result.get("warnings").is_none_or(|w| w.as_array().is_some_and(Vec::is_empty)), "{name}: {result}");
    ok(a, "doc_save", json!({ "doc": doc, "path": name }));
    ok(a, "doc_close", json!({ "doc": doc }));
}

#[test]
fn a_text_box_callout_and_fill_and_sign_text_read_back_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    written(&mut a, "kutu.pdf", |a, doc| {
        ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "textbox", "rect": [40, 60, 570, 150], "contents": PANGRAM, "font_size": 12 }))
    });
    assert_eq!(flat(&read_back(&mut a, "kutu.pdf", true)), PANGRAM);
    written(&mut a, "balon.pdf", |a, doc| {
        ok(
            a,
            "comment_add",
            json!({ "doc": doc, "page": 1, "type": "callout", "rect": [200, 200, 570, 300], "to": [80, 360], "contents": PANGRAM, "font_size": 12 }),
        )
    });
    assert_eq!(flat(&read_back(&mut a, "balon.pdf", true)), PANGRAM);
    written(&mut a, "imza.pdf", |a, doc| {
        ok(a, "fill_sign_add", json!({ "doc": doc, "page": 1, "type": "text", "at": [40, 400], "text": PANGRAM, "font_size": 10 }))
    });
    assert_eq!(flat(&read_back(&mut a, "imza.pdf", true)), PANGRAM);
    // Retyped, the text box draws the new text, and its size change keeps it.
    let doc = ok(&mut a, "doc_open", json!({ "path": "kutu.pdf" }))["doc"].as_u64().unwrap();
    let id = ok(&mut a, "comment_list", json!({ "doc": doc }))["comments"][0]["id"].clone();
    let edited = ok(&mut a, "comment_edit", json!({ "doc": doc, "id": id, "contents": "Iğdır, Muş, Şırnak", "font_size": 16 }));
    assert!(edited.get("warnings").is_none_or(|w| w.as_array().is_some_and(Vec::is_empty)), "{edited}");
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "kutu2.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    assert_eq!(flat(&read_back(&mut a, "kutu2.pdf", true)), "Iğdır, Muş, Şırnak");
}

#[test]
fn a_note_keeps_its_text_and_stamps_draw_their_line() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    // A note's text is its /Contents, shown in the pop-up: a Unicode text string.
    written(&mut a, "not.pdf", |a, doc| ok(a, "comment_add", json!({ "doc": doc, "page": 1, "type": "note", "at": [20, 20], "contents": PANGRAM })));
    let doc = ok(&mut a, "doc_open", json!({ "path": "not.pdf" }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "comment_list", json!({ "doc": doc }))["comments"][0]["contents"], PANGRAM);
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    // A dynamic stamp's By line names the author.
    written(&mut a, "damga.pdf", |a, doc| {
        ok(
            a,
            "comment_add",
            json!({ "doc": doc, "page": 1, "type": "stamp", "at": [300, 600], "stamp": "approved", "dynamic": true, "author": PANGRAM }),
        )
    });
    let text = flat(&read_back(&mut a, "damga.pdf", true));
    assert!(text.contains(PANGRAM), "{text}");
    // A custom stamp's line under its picture.
    let picture = dir.path().join("resim.png");
    let blank = ok(&mut a, "doc_create", json!({ "from": "blank" }))["doc"].as_u64().unwrap();
    let png = a.call("page_render", &json!({ "doc": blank, "page": 1, "dpi": 10 })).unwrap();
    let Some(Content::Png { data, .. }) = png.into_iter().find(|c| matches!(c, Content::Png { .. })) else { panic!("a picture") };
    std::fs::write(&picture, data).unwrap();
    ok(&mut a, "doc_close", json!({ "doc": blank, "discard_changes": true }));
    written(&mut a, "ozel.pdf", |a, doc| {
        ok(a, "stamp_custom", json!({ "doc": doc, "page": 1, "path": "resim.png", "rect": [40, 500, 570, 700], "line": PANGRAM }))
    });
    assert_eq!(flat(&read_back(&mut a, "ozel.pdf", true)), PANGRAM);
}

#[test]
fn a_form_field_value_reads_back_and_names_its_font_for_other_viewers() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    written(&mut a, "form.pdf", |a, doc| {
        ok(a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [40, 450, 570, 520], "name": "ad", "multiline": true }));
        ok(a, "form_add_field", json!({ "doc": doc, "page": 1, "type": "text", "rect": [40, 400, 570, 430], "name": "tek" }));
        ok(a, "form_fill", json!({ "doc": doc, "values": { "ad": PANGRAM, "tek": "Iğdır" } }))
    });
    let doc = ok(&mut a, "doc_open", json!({ "path": "form.pdf" }))["doc"].as_u64().unwrap();
    let fields = ok(&mut a, "form_fields", json!({ "doc": doc }));
    let value = |name: &str| fields["fields"].as_array().unwrap().iter().find(|f| f["name"] == name).map(|f| f["value"].clone());
    assert_eq!(value("ad"), Some(json!(PANGRAM)));
    assert_eq!(value("tek"), Some(json!("Iğdır")));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    // form_fill gave no warning (`written`): nothing was drawn as `?`. The /DA and /DR naming the
    // face are checked in migez-forms (`turkish_values_are_drawn_in_the_bundled_face_named_in_da_and_dr`).
    let text = read_back(&mut a, "form.pdf", true);
    assert!(flat(&text).contains(PANGRAM) && flat(&text).contains("Iğdır"), "{text}");
}

#[test]
fn watermarks_headers_and_added_text_read_back_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    // Level and small enough to fit the page's width, so none of it is cut off.
    written(&mut a, "filigran.pdf", |a, doc| ok(a, "doc_watermark", json!({ "doc": doc, "text": PANGRAM, "font_size": 8, "rotation": 0 })));
    assert_eq!(flat(&read_back(&mut a, "filigran.pdf", false)), PANGRAM);
    written(&mut a, "ustbilgi.pdf", |a, doc| ok(a, "doc_header_footer", json!({ "doc": doc, "header_left": PANGRAM, "font_size": 8 })));
    assert_eq!(flat(&read_back(&mut a, "ustbilgi.pdf", false)), PANGRAM);
    written(&mut a, "metin.pdf", |a, doc| {
        ok(a, "page_add_text", json!({ "doc": doc, "page": 1, "at": [40, 200], "width": 530, "text": PANGRAM, "size": 12 }))
    });
    assert_eq!(flat(&read_back(&mut a, "metin.pdf", false)), PANGRAM);
    // Justified, bold, two paragraphs: the bundled face's spaces are moved by TJ, not Tw.
    let two = format!("{PANGRAM}\n{PANGRAM}");
    written(&mut a, "iki.pdf", |a, doc| {
        ok(
            a,
            "page_add_text",
            json!({ "doc": doc, "page": 1, "at": [40, 200], "width": 300, "text": two, "size": 11, "bold": true, "align": "justify" }),
        )
    });
    assert_eq!(flat(&read_back(&mut a, "iki.pdf", false)), flat(&two));
    // Retyped, the item keeps drawing it.
    let doc = ok(&mut a, "doc_open", json!({ "path": "metin.pdf" }))["doc"].as_u64().unwrap();
    ok(&mut a, "content_update", json!({ "doc": doc, "page": 1, "index": 1, "text": "Şişli İzmir Ağrı" }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "metin2.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    assert_eq!(flat(&read_back(&mut a, "metin2.pdf", false)), "Şişli İzmir Ağrı");
    // Text no bundled face has is still refused, naming the character.
    let doc = ok(&mut a, "doc_create", json!({ "from": "blank" }))["doc"].as_u64().unwrap();
    let e = a.call("page_add_text", &json!({ "doc": doc, "page": 1, "at": [40, 200], "text": "ğ 签" })).unwrap_err().to_string();
    assert!(e.contains("签") && e.contains("U+7B7E"), "{e}");
}

#[test]
fn existing_text_edited_into_turkish_reads_back_exactly() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    let doc = ok(&mut a, "doc_create", json!({ "from": "text", "text": "Merhaba dunya, bu bir deneme paragrafidir." }))["doc"].as_u64().unwrap();
    ok(&mut a, "text_paragraphs", json!({ "doc": doc, "page": 1 }));
    ok(&mut a, "text_edit", json!({ "doc": doc, "page": 1, "paragraph": 1, "text": PANGRAM }));
    ok(&mut a, "doc_save", json!({ "doc": doc, "path": "duzenle.pdf" }));
    ok(&mut a, "doc_close", json!({ "doc": doc }));
    assert_eq!(flat(&read_back(&mut a, "duzenle.pdf", false)), PANGRAM);
}

/// OCR's editable output writes what recognition read as visible text: the letters it read come
/// back from the file as they were read. Skips where Windows has no Turkish text recognition.
#[cfg(windows)]
#[test]
fn ocr_editable_output_writes_turkish_letters_as_read() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    let status = ok(&mut a, "ocr_status", json!({}));
    let windows = status["engines"].as_array().unwrap().iter().find(|e| e["id"] == "windows").cloned().expect("windows is listed");
    if windows["available"] != true {
        eprintln!("skipped: Windows has no Turkish text recognition here ({})", windows["unavailable"]);
        return;
    }
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": PANGRAM }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "tarama", "dpi": 300, "pages": [1] }));
    let png = std::fs::read_dir(dir.path().join("tarama")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    let r = ok(&mut a, "ocr_recognize", json!({ "doc": scan, "engine": "windows", "language": "tr", "output_mode": "editable", "out": "ocr.pdf" }));
    let read =
        flat(&r["pages"].as_array().unwrap().iter().map(|p| p["text"].as_str().unwrap_or_default().to_string()).collect::<Vec<_>>().join("\n"));
    assert!(read.contains("yağız") && read.contains("şoföre"), "recognition read the Turkish words: {read}");
    let written = flat(&read_back(&mut a, "ocr.pdf", false));
    assert_eq!(written, read, "the output holds exactly what was read");
    assert!(!written.contains('?'), "{written}");
}
