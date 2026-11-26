//! Pdfrp: Word to PDF and PDF to Word through the tools, with the Microsoft Office installed
//! on this computer. They start Word (unseen; migez-office leaves documents the user has open
//! alone), so they are ignored by default; run them on a machine with Office:
//!
//! ```text
//! cargo test -p migez-automation --test it -- --ignored ms_office
//! ```
//!
//! The Word file is written here (the smallest package Word reads), with Turkish text, so no
//! binary fixture is committed.

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

/// How many times `word` is found in the PDF at `path`.
fn found(a: &mut Automation, path: &str, word: &str) -> u64 {
    let doc = ok(a, "doc_open", json!({ "path": path }))["doc"].as_u64().unwrap();
    let count = ok(a, "text_find", json!({ "doc": doc, "query": word }))["count"].as_u64().unwrap();
    ok(a, "doc_close", json!({ "doc": doc }));
    count
}

const TURKISH: &str = "Öğrenci Güneş Çığ Şişe İğne ılık";

#[test]
#[ignore = "needs Microsoft Word, which it starts unseen"]
fn ms_office_word_converts_a_turkish_letter_to_pdf_and_back_to_word() {
    let dir = tempfile::tempdir().unwrap();
    let mut a = auto(dir.path());
    let status = ok(&mut a, "office_status", json!({}));
    if status["word"] != true {
        eprintln!("skipped: Microsoft Word is not installed here ({status})");
        return;
    }
    std::fs::write(dir.path().join("Mektup.docx"), super::office::docx(TURKISH)).unwrap();

    // Word to PDF: the Turkish words are in the PDF's text.
    let pdf = ok(&mut a, "office_to_pdf", json!({ "path": "Mektup.docx", "out": "Mektup.pdf" }));
    assert_eq!(pdf["app"], "Microsoft Word", "{pdf}");
    assert_eq!(pdf["pages"], 1, "{pdf}");
    assert_eq!(pdf["marked_from_internet"], false);
    for word in ["Öğrenci", "Güneş"] {
        assert!(found(&mut a, "Mektup.pdf", word) >= 1, "{word} is in the PDF");
    }

    // The PDF is never replaced unasked, and only a PDF is made.
    let again = a.call("office_to_pdf", &json!({ "path": "Mektup.docx", "out": "Mektup.pdf" })).unwrap_err();
    assert!(matches!(again, ToolError::Failed(_)) && again.to_string().contains("already exists"), "{again}");
    assert!(matches!(a.call("office_to_pdf", &json!({ "path": "Mektup.docx", "out": "Mektup.docx" })), Err(ToolError::InvalidArgs(_))));
    ok(&mut a, "office_to_pdf", json!({ "path": "Mektup.docx", "out": "Mektup.pdf", "overwrite": true }));

    // PDF to Word: a Word package, whose text comes back through Word again.
    let word = ok(&mut a, "office_pdf_to_word", json!({ "path": "Mektup.pdf", "out": "Geri.docx" }));
    assert!(word["bytes"].as_u64().unwrap() > 0, "{word}");
    assert!(std::fs::read(dir.path().join("Geri.docx")).unwrap().starts_with(b"PK"), "a .docx is a ZIP package");
    ok(&mut a, "office_to_pdf", json!({ "path": "Geri.docx", "out": "Geri.pdf" }));
    assert!(found(&mut a, "Geri.pdf", "Güneş") >= 1, "the Turkish text survived PDF → Word → PDF");

    // A file from the internet is converted only when trusted, and its result carries the mark.
    #[cfg(windows)]
    {
        let input = dir.path().join("Mektup.docx");
        std::fs::write(format!("{}:Zone.Identifier", input.display()), migez_platform::zone::INTERNET_ZONE).unwrap();
        let refused = a.call("office_to_pdf", &json!({ "path": "Mektup.docx", "out": "Internet.pdf" })).unwrap_err();
        assert!(refused.to_string().contains("internet"), "{refused}");
        assert!(!dir.path().join("Internet.pdf").exists(), "nothing was converted");
        let trusted = ok(&mut a, "office_to_pdf", json!({ "path": "Mektup.docx", "out": "Internet.pdf", "trust_internet_file": true }));
        assert_eq!(trusted["marked_from_internet"], true);
        assert!(migez_platform::zone::read_zone(&dir.path().join("Internet.pdf")).is_some(), "the PDF carries the mark");
    }
}
