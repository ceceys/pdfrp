//! Export a PDF ▸ Image and Text from the real shell (egui_kittest).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use migez_ui_egui::MigezApp;

const FIXTURE: &[u8] = b"%PDF-1.7
1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj
2 0 obj << /Type /Pages /Kids [3 0 R 4 0 R] /Count 2 /MediaBox [0 0 200 100] >> endobj
3 0 obj << /Type /Page /Parent 2 0 R >> endobj
4 0 obj << /Type /Page /Parent 2 0 R >> endobj
trailer << /Root 1 0 R >>
%%EOF";

#[test]
fn export_dialogs_write_images_and_text() {
    let dir = std::env::temp_dir().join(format!("migez-export-ui-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = MigezApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, FIXTURE.to_vec()).unwrap();
        app.export_dir_override = Some(d.to_string_lossy().into_owned());
        app
    });
    h.run_steps(4);
    assert!(h.state_mut().execute("export.image"));
    h.run_steps(2);
    h.get_by_label("Export to Image");
    h.get_by_label("Export").click();
    h.run_steps(4);
    assert!(dir.join("doc_page_1.png").exists() && dir.join("doc_page_2.png").exists());
    h.get_by_label_contains("Exported 2 images");
    assert!(h.state_mut().execute("export.text"));
    h.run_steps(2);
    h.get_by_label("Export").click();
    h.run_steps(4);
    assert!(dir.join("doc.txt").exists());
    // Export all images: this document has none, and says so.
    assert!(h.state_mut().execute("export.all_images"));
    h.run_steps(2);
    h.get_by_label("Export All Images");
    h.get_by_label("Export").click();
    h.run_steps(4);
    h.get_by_label_contains("Exported 0 images");
}

/// Pdfrp: the page thumbnails' "Save page as image…" opens Export as Image on the picked
/// pages only; the same dialog opened from the menu afterwards is back on the page range.
#[test]
fn save_page_as_image_exports_the_picked_pages() {
    let dir = std::env::temp_dir().join(format!("migez-save-page-image-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let d = dir.clone();
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = MigezApp::new();
        app.set_option("language", "en").unwrap();
        app.open_bytes("doc.pdf", None, FIXTURE.to_vec()).unwrap();
        app.export_dir_override = Some(d.to_string_lossy().into_owned());
        app
    });
    h.run_steps(4);
    // Page 2 picked in the thumbnails, then its context menu's "Save page as image…".
    h.state_mut().views[0].select_pages(&[1]);
    h.state_mut().views[0].pending_action = Some(migez_ui_egui::canvas::ViewAction::SaveAsImage);
    h.run_steps(3);
    h.get_by_label("Export to Image");
    assert_eq!(h.state().export_draft.pages(2), [1], "the picked page only");
    h.get_by_label("Export").click();
    h.run_steps(4);
    assert!(dir.join("doc_page_2.png").exists() && !dir.join("doc_page_1.png").exists(), "only page 2 was written");
    h.get_by_label_contains("Exported 1 image");
    // From the menu: the range again, not the earlier pick.
    assert!(h.state_mut().execute("export.image"));
    h.run_steps(2);
    assert_eq!(h.state().export_draft.pages(2), [0, 1]);
    let _ = std::fs::remove_dir_all(&dir);
}
