//! Pdfrp: UDF files in the app. A file with an electronic signature asks before it is converted;
//! one without opens at once. Both tabs, and Combine's list, say what the PDF is not. The files
//! are the synthetic ones of the engine's tests (made-up text only).

use egui_kittest::kittest::Queryable;

use crate::MigezApp;

const SIGNED: &[u8] = include_bytes!("../../../../engine/tests/data/udf/01-dilekce-iki-yana-imzali.udf");
const UNSIGNED: &[u8] = include_bytes!("../../../../engine/tests/data/udf/02-dar-kenar-listeler.udf");

fn english() -> MigezApp {
    let mut app = MigezApp::new();
    app.set_option("language", "en").unwrap();
    app
}

fn note(app: &MigezApp, index: usize) -> String {
    app.views[index].converted_note.clone().unwrap_or_default()
}

#[test]
fn a_signed_udf_is_converted_only_once_the_warning_is_agreed_to() {
    let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(|_cc| {
        let mut app = english();
        app.open_bytes("a.udf", None, SIGNED.to_vec()).unwrap();
        app.open_bytes("b.udf", None, SIGNED.to_vec()).unwrap();
        app
    });
    assert!(h.state().views.is_empty(), "nothing is converted before the warning is answered");
    h.run_steps(3);
    assert_eq!(h.state().dialog, Some(crate::Dialog::UdfSigned));
    h.get_by_label("a.udf");
    h.get_by_label(migez_engine::UDF_SIGNED_NOTE);
    // Cancelled: the first file is not converted, and the second one asks in turn.
    h.get_by_label("Cancel").click();
    h.run_steps(3);
    assert!(h.state().views.is_empty());
    assert_eq!(h.state().dialog, Some(crate::Dialog::UdfSigned));
    h.get_by_label("b.udf");
    h.get_by_label("Convert to PDF").click();
    h.run_steps(3);
    let app = h.state();
    assert!(app.pending_udf.is_empty() && app.dialog.is_none());
    assert_eq!(app.views.len(), 1);
    let doc = app.session.get(app.views[0].id).unwrap();
    assert_eq!(doc.name, "b.pdf");
    assert!(doc.path.is_none(), "a new, unsaved PDF: Save writes a PDF and the UDF file stays as it is");
    let said = note(app, 0);
    assert!(said.starts_with("b.udf was converted from a UDF file into a new PDF. Saving writes a PDF; the UDF file is not changed."), "{said}");
    assert!(said.ends_with(migez_engine::UDF_SIGNED_NOTE), "{said}");
    h.get_by_label_contains("b.udf was converted from a UDF file");
}

#[test]
fn an_unsigned_udf_opens_at_once_with_the_appearance_note() {
    let mut app = english();
    app.open_bytes("c.udf", None, UNSIGNED.to_vec()).unwrap();
    assert!(app.pending_udf.is_empty());
    assert_eq!(app.views.len(), 1);
    let said = note(&app, 0);
    assert!(said.ends_with(migez_engine::UDF_NOTE), "{said}");
    assert!(!said.contains("signature"), "{said}");
}

#[test]
fn combine_lists_what_a_udf_file_becomes() {
    let mut app = english();
    app.stage_combine(vec![("signed.udf".into(), SIGNED.to_vec()), ("plain.udf".into(), UNSIGNED.to_vec())]);
    assert_eq!(app.combine_draft.len(), 2);
    assert!(app.combine_draft.iter().all(|f| f.problem.is_none() && f.pages > 0), "both become PDFs");
    assert!(app.combine_draft[0].notes.iter().any(|n| n == migez_engine::UDF_SIGNED_NOTE), "{:?}", app.combine_draft[0].notes);
    assert!(app.combine_draft[1].notes.iter().any(|n| n == migez_engine::UDF_NOTE), "{:?}", app.combine_draft[1].notes);
    let toast = app.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default();
    assert_eq!(toast, format!("signed.udf: {}", migez_engine::UDF_SIGNED_NOTE));
}
