//! Pdfrp: Turkish text recognition end to end through the tools, with the recognition built
//! into Windows. A Turkish page is set as text, exported as a picture, made into a PDF that is only
//! that picture, read with `ocr_recognize` (engine windows, language tr), and searched for Turkish
//! words. Skips (with a note) where Windows has no Turkish text recognition installed.

use std::path::Path;

use migez_automation::{Automation, Content, ToolError};
use serde_json::{Value, json};

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

/// The page every reading is checked against: Turkish words with every Turkish letter.
const TURKISH: &str = "Öğrenci Güneş Çığ Şişe İğne ılık";

#[test]
fn a_turkish_scan_becomes_searchable_with_windows_text_recognition() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    let status = ok(&mut a, "ocr_status", json!({}));
    let windows = status["engines"].as_array().unwrap().iter().find(|e| e["id"] == "windows").cloned().expect("windows is listed");
    assert!(windows["languages"].is_array(), "{windows}");
    if windows["available"] != true {
        eprintln!("skipped: Windows has no Turkish text recognition here ({})", windows["unavailable"]);
        return;
    }
    assert!(windows["languages"].as_array().unwrap().iter().any(|l| l["code"].as_str().is_some_and(|c| c.starts_with("tr"))), "{windows}");

    // The page as a picture only: no text of its own, as a scan.
    let text = ok(&mut a, "doc_create", json!({ "from": "text", "text": TURKISH }))["doc"].as_u64().unwrap();
    ok(&mut a, "doc_export_images", json!({ "doc": text, "folder": "scan", "dpi": 300, "pages": [1] }));
    let png = std::fs::read_dir(dir.path().join("scan")).unwrap().next().unwrap().unwrap().path();
    let scan = ok(&mut a, "doc_create", json!({ "from": "images", "paths": [png.to_string_lossy()] }))["doc"].as_u64().unwrap();
    assert_eq!(ok(&mut a, "text_find", json!({ "doc": scan, "query": "Öğrenci" }))["count"], 0, "the picture has no text yet");

    let r = ok(&mut a, "ocr_recognize", json!({ "doc": scan, "engine": "windows", "language": "tr" }));
    assert!(r["words"].as_u64().unwrap() >= 5, "{r}");
    for word in ["Öğrenci", "Güneş"] {
        let found = ok(&mut a, "text_find", json!({ "doc": scan, "query": word }));
        assert!(found["count"].as_u64().unwrap() >= 1, "{word} is searchable: {found}");
        assert_eq!(found["matches"][0]["page"], 1, "{found}");
    }
    // Saved and opened again, the words are still there.
    ok(&mut a, "doc_save", json!({ "doc": scan, "path": "aranabilir.pdf" }));
    let reopened = ok(&mut a, "doc_open", json!({ "path": "aranabilir.pdf" }))["doc"].as_u64().unwrap();
    assert!(ok(&mut a, "text_find", json!({ "doc": reopened, "query": "Güneş" }))["count"].as_u64().unwrap() >= 1);

    // A language Windows doesn't have is named before anything is read, and junk is refused.
    let missing = a.call("ocr_recognize", &json!({ "doc": text, "engine": "windows", "language": "tlh", "force_ocr": true })).unwrap_err();
    assert!(matches!(missing, ToolError::Failed(_)) && missing.to_string().contains("tlh"), "{missing}");
    assert!(matches!(a.call("ocr_recognize", &json!({ "doc": scan, "engine": "windows", "language": "tr;del" })), Err(ToolError::InvalidArgs(_))));
}
