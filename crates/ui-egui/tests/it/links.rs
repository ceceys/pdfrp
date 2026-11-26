//! No community links (Pdfrp): no Discord button in the top bar, no community card on Home,
//! no community, web page or GitHub links in Help or About, and nothing opened on the web of the
//! app's own accord. About names Pdfrp alone and says where the open-source licences are
//! (office release 1.0.0: no other program's name, credits or logo).

use egui_kittest::Harness;
use egui_kittest::kittest::Queryable;
use migez_ui_egui::{APP_NAME, Dialog, MigezApp};

fn harness(setup: impl FnOnce(&mut MigezApp) + 'static) -> Harness<'static, MigezApp> {
    let mut h = Harness::builder().with_size(egui::vec2(1400.0, 900.0)).build_eframe(move |_cc| {
        let mut app = MigezApp::new();
        app.set_option("language", "en").unwrap();
        setup(&mut app);
        app
    });
    h.run_steps(4);
    h
}

/// The labels upstream's community links and card had (with the app's name in them).
const GONE: [&str; 5] = [
    "Discord",
    "Join our Discord",
    "Get help, share feedback and follow development on Discord.",
    "Pdfrp web page",
    "Pdfrp on GitHub",
];

#[test]
fn home_and_the_top_bar_have_no_community_links() {
    let mut h = harness(|_| {});
    h.get_by_label("Welcome");
    for label in GONE {
        assert!(h.query_by_label(label).is_none(), "{label:?} is not shown");
    }
    for word in ["Discord", "community", "website"] {
        assert!(h.query_by_label_contains(word).is_none(), "{word:?} is not shown");
    }
    // Pdfrp 1.1.0: GitHub is named once, as plain text, in the privacy note that says the
    // update check is the one thing that goes online. No link or button names it.
    h.get_by_label_contains("It goes online only to check GitHub for updates");
    assert_eq!(h.query_all_by_label_contains("GitHub").count(), 1, "only the privacy note names GitHub");
    h.run_steps(2);
    assert_eq!(h.state().last_opened_url, None, "nothing was opened");
}

#[test]
fn the_link_commands_are_gone() {
    for id in ["help.discord", "help.app_page", "help.github", "help.website"] {
        let mut h = harness(|_| {});
        assert!(!h.state_mut().execute(id), "{id} is not a command");
        h.run_steps(2);
        assert_eq!(h.state().last_opened_url, None, "{id} opened nothing");
    }
    // Help keeps its own: the shortcuts, Check for updates (1.1.0) and About. Check for updates
    // opens no page, and without a source (tests) it asks nothing either.
    assert!(harness(|_| {}).state_mut().execute("help.about"));
    let mut h = harness(|_| {});
    assert!(h.state_mut().execute("help.check_updates"));
    h.run_steps(2);
    assert_eq!(h.state().last_opened_url, None);
    assert!(!h.state().update_busy(), "tests never reach the network");
}

#[test]
fn about_names_the_app_alone_and_where_its_licences_are() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    // With a document open, so Home is not on screen.
    let mut h = harness(move |app| {
        app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
        app.dialog = Some(Dialog::About);
    });
    settled(&mut h, "Open-source licences: in the lisanslar folder of the installation.");
    assert!(h.query_all_by_label(APP_NAME).count() >= 2, "the app's icon (alt text) and its name");
    // No other program is named or credited, and there are no credit tabs.
    for gone in ["Part of", "based on", "Discord", "GitHub", "website", "Rendering:", "A clean-room, open-source PDF application"] {
        assert!(h.query_by_label_contains(gone).is_none(), "{gone:?} is not shown");
    }
    for label in GONE.into_iter().chain(["Contributors", "Models"]) {
        assert!(h.query_by_label(label).is_none(), "{label:?}: no mark, credit tab or link");
    }
}

/// Runs frames until the node labelled `label` exists and has stopped moving, then returns it.
///
/// The About dialog is a modal centred on its own size, and switching tabs changes that size, so
/// for a few frames afterwards the modal grows and recentres and every control in it moves. A
/// fixed number of frames is a guess at how long that takes: it was too few after the 0.5.0
/// contributors refresh (78 names instead of 5) on some machines, the click on Table landed where
/// the button had been, and the table never opened (#671).
fn settled<'a>(h: &'a mut Harness<'static, MigezApp>, label: &'a str) -> egui_kittest::Node<'a> {
    let mut last = None;
    for _ in 0..60 {
        h.run_steps(1);
        let now = h.query_by_label(label).map(|n| n.rect());
        if now.is_some() && now == last {
            return h.get_by_label(label);
        }
        last = now;
    }
    panic!("{label:?} never appeared or never stopped moving");
}

/// Pdfrp: About has no Contributors or Models tab (upstream's credits are not shown), only the
/// app's own version.
#[test]
fn about_dialog_has_no_credit_tabs() {
    let mut h = harness(|app| app.dialog = Some(Dialog::About));
    settled(&mut h, &format!("Version {}", migez_engine::build_info::PRODUCT_VERSION));
    for label in ["Contributors", "Models", "Table", "@echelon", "Anthropic"] {
        assert!(h.query_by_label(label).is_none(), "{label:?} is not shown");
    }
}

/// Pdfrp: the status bar along the bottom of the main window ends in "by Friday", in English
/// whatever the interface language, low on the right of the window, on Home and with a document
/// open, in a narrow window too.
#[test]
fn the_status_bar_ends_in_by_friday() {
    let pdf = b"%PDF-1.7\n1 0 obj << /Type /Catalog /Pages 2 0 R >> endobj\n2 0 obj << /Type /Pages /Kids [3 0 R] /Count 1 >> endobj\n3 0 obj << /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] >> endobj\ntrailer << /Root 1 0 R >>\n%%EOF";
    for (open, width, language) in [(false, 1400.0, "en"), (true, 1400.0, "tr"), (true, migez_ui_egui::MIN_WINDOW_SIZE[0], "tr")] {
        let mut h = Harness::builder().with_size(egui::vec2(width, 900.0)).build_eframe(move |_cc| {
            let mut app = MigezApp::new();
            app.set_option("language", language).unwrap();
            if open {
                app.open_bytes("one.pdf", None, pdf.to_vec()).unwrap();
            }
            app
        });
        h.run_steps(4);
        let rect = h.get_by_label("by Friday").rect();
        let window = egui::vec2(width, 900.0);
        assert!(rect.right() <= window.x && rect.right() > window.x - 40.0, "{open} {width}: at the right end, {rect:?}");
        assert!(rect.bottom() <= window.y && rect.top() > window.y - 24.0, "{open} {width}: in the bottom strip, {rect:?}");
    }
}
