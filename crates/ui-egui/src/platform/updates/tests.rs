//! The update check in the app, without the network: a transport of the test's own stands in
//! for GitHub, the work runs inline, and a recorder stands in for starting the installer.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Arc, Mutex};

use eframe::App as _;
use egui_kittest::Harness;
use egui_kittest::kittest::Queryable as _;
use migez_engine::{Edit, Session};
use migez_update::{FetchError, Transport};

use super::*;

const API: &str = "https://api.github.com/repos/ceceys/pdfrp/releases/latest";
const EXE: &str = "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe";
const SUMS: &str = "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/SHA256SUMS.txt";
const INSTALLER: &[u8] = b"MZ pretend installer 1.2.0";

/// Answers from a table (404 for anything else) and counts the requests.
#[derive(Default)]
struct Fake {
    answers: HashMap<String, Result<Vec<u8>, FetchError>>,
    asked: Mutex<Vec<String>>,
}

impl Transport for Fake {
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
        if let Ok(mut asked) = self.asked.lock() {
            asked.push(url.to_owned());
        }
        match self.answers.get(url) {
            Some(Ok(body)) if body.len() as u64 > limit => Err(FetchError::TooLarge),
            Some(Ok(body)) => sink.write_all(body).map(|()| body.len() as u64).map_err(|e| FetchError::Network(e.to_string())),
            Some(Err(e)) => Err(e.clone()),
            None => Err(FetchError::Status(404)),
        }
    }
}

fn release_json(tag: &str) -> Vec<u8> {
    serde_json::json!({
        "tag_name": tag,
        "html_url": format!("https://github.com/ceceys/pdfrp/releases/tag/{tag}"),
        "draft": false,
        "prerelease": false,
        "assets": [
            { "name": "Pdfrp.Kurulum.1.2.0.exe", "browser_download_url": EXE, "size": INSTALLER.len() },
            { "name": "SHA256SUMS.txt", "browser_download_url": SUMS, "size": 0 },
        ],
    })
    .to_string()
    .into_bytes()
}

/// GitHub with release `tag`, whose SHA256SUMS.txt gives `listed`'s SHA-256 for the installer.
fn github(tag: &str, listed: &[u8]) -> Arc<Fake> {
    let sums = format!("{}  Pdfrp Kurulum 1.2.0.exe\n", migez_update::sha256_hex(listed).unwrap());
    let mut fake = Fake::default();
    fake.answers.insert(API.into(), Ok(release_json(tag)));
    fake.answers.insert(EXE.into(), Ok(INSTALLER.to_vec()));
    fake.answers.insert(SUMS.into(), Ok(sums.into_bytes()));
    Arc::new(fake)
}

fn failing(error: FetchError) -> Arc<Fake> {
    let mut fake = Fake::default();
    fake.answers.insert(API.into(), Err(error));
    Arc::new(fake)
}

/// What the recorder saw started: the installer's path and what it held then.
type Started = Arc<Mutex<Vec<(PathBuf, Vec<u8>)>>>;

struct Setup {
    app: MigezApp,
    started: Started,
    dir: PathBuf,
    _folder: Folder,
}

/// The test's download folder, removed at the end of the test.
struct Folder(PathBuf);

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn setup(test: &str, fake: Arc<Fake>, language: &str) -> Setup {
    let dir = std::env::temp_dir().join(format!("migez-ui-update-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut app = MigezApp::new();
    app.set_option("language", language).unwrap();
    app.run_inline = true;
    app.set_update_source(UpdateSource::Fake(fake, API.into()));
    app.set_update_download_dir(dir.clone());
    let started: Started = Arc::default();
    let record = started.clone();
    app.set_update_launcher(Box::new(move |verified: Verified| {
        let bytes = std::fs::read(&verified.path)?;
        record.lock().unwrap().push((verified.path.clone(), bytes));
        Ok(())
    }));
    Setup { app, started, _folder: Folder(dir.clone()), dir }
}

fn toast(app: &MigezApp) -> String {
    app.toast.as_ref().map(|t| t.0.clone()).unwrap_or_default()
}

fn pdf(text: &str) -> Vec<u8> {
    Session::new().create_from_text("t", text).unwrap().to_vec()
}

#[test]
fn the_running_version_reads_as_a_release() {
    let current = current_version().unwrap();
    assert!(!current.is_pre_release());
    assert_eq!(current.to_string(), migez_engine::build_info::PRODUCT_VERSION);
}

#[test]
fn help_check_for_updates_says_when_the_app_is_up_to_date() {
    let fake = github(&format!("v{}", migez_engine::build_info::PRODUCT_VERSION), INSTALLER);
    let mut s = setup("current", fake.clone(), "en");
    assert!(s.app.execute("help.check_updates"));
    s.app.poll_updates();
    assert_eq!(toast(&s.app), format!("Pdfrp is up to date ({}).", migez_engine::build_info::PRODUCT_VERSION));
    assert_eq!(s.app.update_offered(), None);
    assert_eq!(*fake.asked.lock().unwrap(), [API], "one request, to the GitHub API");
    // An older release on GitHub is no update either.
    let mut s = setup("older", github("v1.0.1", INSTALLER), "en");
    s.app.check_for_updates();
    s.app.poll_updates();
    assert!(toast(&s.app).contains("is up to date"), "{}", toast(&s.app));
}

#[test]
fn a_failed_check_is_quiet_at_start_and_said_on_request() {
    for (error, said) in [
        (FetchError::Status(404), "No release of Pdfrp has been published yet."),
        (FetchError::Status(403), "Couldn't check for updates: GitHub isn't taking requests right now. Try again later."),
        (FetchError::Status(429), "Couldn't check for updates: GitHub isn't taking requests right now. Try again later."),
        (FetchError::Network("dns error".into()), "Couldn't check for updates: no connection to GitHub."),
        (FetchError::Status(500), "Couldn't check for updates: HTTP 500"),
    ] {
        // At start: nothing is said and nothing is offered.
        let mut s = setup("quiet", failing(error.clone()), "en");
        s.app.set_startup_update_check(true);
        s.app.poll_updates();
        s.app.poll_updates();
        assert_eq!(toast(&s.app), "", "{error:?} at start");
        assert_eq!(s.app.update_offered(), None);
        assert!(s.app.update_last_check.is_some(), "the attempt counts for the day");
        // On request: said.
        let mut s = setup("said", failing(error.clone()), "en");
        s.app.check_for_updates();
        s.app.poll_updates();
        assert_eq!(toast(&s.app), said, "{error:?} on request");
    }
}

#[test]
fn the_check_at_start_waits_for_the_preference_and_a_day() {
    let now = migez_update::now_secs();
    for (on, last, asks) in [(true, None, true), (false, None, false), (true, Some(now - 60), false), (true, Some(now - 2 * 86_400), true)] {
        let fake = github("v1.2.0", INSTALLER);
        let mut s = setup("start", fake.clone(), "en");
        s.app.update_check_at_start = on;
        s.app.update_last_check = last;
        s.app.set_startup_update_check(true);
        s.app.poll_updates();
        assert_eq!(fake.asked.lock().unwrap().len(), usize::from(asks), "on {on}, last {last:?}");
        // Only on the first frame.
        s.app.poll_updates();
        assert!(fake.asked.lock().unwrap().len() <= 1);
        assert_eq!(s.app.update_offered().is_some(), asks);
    }
    // Without a source (tests, any host that didn't turn it on) nothing is asked, even on request.
    let mut app = MigezApp::new();
    app.set_option("language", "en").unwrap();
    app.set_startup_update_check(true);
    app.poll_updates();
    assert!(!app.update_busy());
    app.check_for_updates();
    assert_eq!(toast(&app), "This copy of Pdfrp doesn't check for updates.");
}

#[test]
fn the_preference_and_the_last_check_are_kept_in_the_settings() {
    let mut app = MigezApp::new();
    assert!(app.update_check_at_start, "on by default");
    app.update_check_at_start = false;
    app.update_last_check = Some(1_764_000_000);
    let saved = app.persist();
    let mut again = MigezApp::new();
    again.restore(&saved);
    assert!(!again.update_check_at_start);
    assert_eq!(again.update_last_check, Some(1_764_000_000));
    // Nonsense keeps the defaults.
    let mut odd = MigezApp::new();
    odd.restore(r#"{"update_check_at_start":"no","update_last_check":-5}"#);
    assert!(odd.update_check_at_start);
    assert_eq!(odd.update_last_check, None);
    // The control channel can turn it off, as Preferences does.
    let mut app = MigezApp::new();
    app.set_option("update_check_at_start", "false").unwrap();
    assert!(!app.update_check_at_start);
    assert!(app.set_option("update_check_at_start", "maybe").is_err());
}

#[test]
fn update_now_downloads_checks_and_quits_then_the_installer_starts_on_exit() {
    let fake = github("v1.2.0", INSTALLER);
    let mut s = setup("update", fake.clone(), "en");
    let ctx = egui::Context::default();
    s.app.ctx = Some(ctx.clone());
    s.app.check_for_updates();
    s.app.poll_updates();
    assert_eq!(s.app.update_offered().as_deref(), Some("1.2.0"));
    assert_eq!(fake.asked.lock().unwrap().len(), 1, "nothing is downloaded before Update now");
    s.app.start_update_download();
    s.app.poll_updates();
    assert!(s.app.update_ready());
    assert!(s.app.allow_quit, "nothing unsaved: the app quits at once");
    assert_eq!(*fake.asked.lock().unwrap(), [API, SUMS, EXE]);
    assert!(s.started.lock().unwrap().is_empty(), "the installer waits for the exit");
    s.app.on_exit(None);
    let started = s.started.lock().unwrap().clone();
    assert_eq!(started.len(), 1);
    assert_eq!(started[0].1, INSTALLER, "the checked file");
    assert_eq!(started[0].0.file_name().unwrap(), "Pdfrp Kurulum 1.2.0.exe");
    assert!(started[0].0.starts_with(&s.dir), "in the download folder");
    assert!(!s.app.update_ready());
    // A second exit starts nothing more.
    s.app.on_exit(None);
    assert_eq!(s.started.lock().unwrap().len(), 1);
}

#[test]
fn a_sha256_mismatch_is_shown_and_nothing_is_started() {
    let fake = github("v1.2.0", b"MZ the file the release meant");
    let mut s = setup("mismatch", fake, "en");
    s.app.ctx = Some(egui::Context::default());
    s.app.check_for_updates();
    s.app.poll_updates();
    s.app.start_update_download();
    s.app.poll_updates();
    assert!(!s.app.update_ready());
    assert!(!s.app.allow_quit, "the app stays open");
    assert!(
        matches!(&s.app.updates.phase, Phase::Failed { message, .. } if message.contains("didn't match its SHA-256")),
        "{:?}",
        s.app.updates.phase
    );
    s.app.on_exit(None);
    assert!(s.started.lock().unwrap().is_empty(), "never run");
    let left: Vec<_> = std::fs::read_dir(&s.dir).unwrap().collect();
    assert!(left.is_empty(), "the download is deleted: {left:?}");
    s.app.put_off_update();
    assert_eq!(s.app.update_offered(), None, "Close puts the card away");
}

#[test]
fn cancel_at_the_save_question_calls_the_install_off() {
    let mut s = setup("cancel", github("v1.2.0", INSTALLER), "en");
    let ctx = egui::Context::default();
    s.app.ctx = Some(ctx.clone());
    s.app.open_bytes("a.pdf", None, pdf("Draft")).unwrap();
    assert!(s.app.apply_edit(Edit::RotatePages { pages: vec![0], degrees: 90 }));
    s.app.check_for_updates();
    s.app.poll_updates();
    s.app.start_update_download();
    s.app.poll_updates();
    assert!(s.app.update_ready(), "Update now's quit is under way");
    assert_eq!(s.app.close_request, Some(crate::CloseRequest::Quit), "the save question comes first");
    assert!(!s.app.allow_quit);
    s.app.poll_updates();
    assert!(s.app.update_ready(), "the open question is part of the quit");
    // Cancel: the install is called off, the card offers Update now / Later again.
    s.app.resolve_close(&ctx, None);
    s.app.poll_updates();
    assert!(!s.app.allow_quit, "Cancel keeps the app open");
    assert!(!s.app.update_ready(), "nothing waits to install");
    assert!(s.app.update_downloaded(), "the checked download is kept for Update now");
    assert_eq!(s.app.update_offered().as_deref(), Some("1.2.0"));
    // A later ordinary quit installs nothing, and deletes the download.
    let path = s.app.updates.verified.as_ref().map(|v| v.path.clone()).unwrap();
    assert!(!s.app.request_quit(&ctx), "File ▸ Quit asks about the document");
    s.app.resolve_close(&ctx, Some(false));
    assert!(s.app.allow_quit);
    s.app.poll_updates();
    assert!(!s.app.update_ready());
    s.app.on_exit(None);
    assert!(s.started.lock().unwrap().is_empty(), "no install without Update now");
    assert!(!path.exists(), "the downloaded installer is deleted");
}

#[test]
fn update_now_again_after_cancel_installs_on_that_quit() {
    let mut s = setup("again", github("v1.2.0", INSTALLER), "en");
    let ctx = egui::Context::default();
    s.app.ctx = Some(ctx.clone());
    s.app.open_bytes("a.pdf", None, pdf("Draft")).unwrap();
    assert!(s.app.apply_edit(Edit::RotatePages { pages: vec![0], degrees: 90 }));
    s.app.check_for_updates();
    s.app.poll_updates();
    s.app.start_update_download();
    s.app.poll_updates();
    s.app.resolve_close(&ctx, None);
    s.app.poll_updates();
    assert!(!s.app.update_ready());
    // Update now on the card again: no second download, the save question again, Don't save.
    s.app.quit_to_update();
    assert!(s.app.update_ready());
    assert_eq!(s.app.close_request, Some(crate::CloseRequest::Quit));
    s.app.resolve_close(&ctx, Some(false));
    assert!(s.app.allow_quit, "the document is closed unsaved and the app quits");
    s.app.poll_updates();
    assert!(s.app.update_ready(), "the quit is under way");
    s.app.on_exit(None);
    assert_eq!(s.started.lock().unwrap().len(), 1, "installed on Update now's quit");
}

#[test]
fn later_deletes_the_download() {
    let mut s = setup("later-file", github("v1.2.0", INSTALLER), "en");
    let ctx = egui::Context::default();
    s.app.ctx = Some(ctx.clone());
    s.app.open_bytes("a.pdf", None, pdf("Draft")).unwrap();
    assert!(s.app.apply_edit(Edit::RotatePages { pages: vec![0], degrees: 90 }));
    s.app.check_for_updates();
    s.app.poll_updates();
    s.app.start_update_download();
    s.app.poll_updates();
    s.app.resolve_close(&ctx, None);
    s.app.poll_updates();
    let path = s.app.updates.verified.as_ref().map(|v| v.path.clone()).unwrap();
    s.app.put_off_update();
    assert!(!path.exists(), "the downloaded installer is deleted");
    assert_eq!(s.app.update_offered(), None);
    s.app.on_exit(None);
    assert!(s.started.lock().unwrap().is_empty());
}

#[test]
fn the_card_offers_update_now_and_later_in_turkish() {
    if !crate::i18n::needs_catalogs() {
        return;
    }
    let Setup { mut app, started, _folder, .. } = setup("card", github("v1.2.0", INSTALLER), "tr");
    app.check_for_updates();
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 700.0)).build_eframe(move |_cc| app);
    h.run_steps(4);
    h.get_by_label("Pdfrp 1.2.0 hazır");
    h.get_by_label("Sonra");
    h.get_by_label("Şimdi güncelle").click();
    h.run_steps(3);
    assert!(h.state().update_ready(), "downloaded and checked");
    assert!(h.state().allow_quit, "quitting to install");
    h.state_mut().on_exit(None);
    assert_eq!(started.lock().unwrap().len(), 1);
}

#[test]
fn the_card_says_later_puts_it_away_in_english() {
    let Setup { mut app, _folder, .. } = setup("later", github("v1.2.0", INSTALLER), "en");
    app.check_for_updates();
    let mut h = Harness::builder().with_size(egui::vec2(1100.0, 700.0)).build_eframe(move |_cc| app);
    h.run_steps(4);
    h.get_by_label("Pdfrp 1.2.0 is ready");
    h.get_by_label("Later").click();
    h.run_steps(2);
    assert!(h.query_by_label("Pdfrp 1.2.0 is ready").is_none());
    assert_eq!(h.state().update_offered(), None);
}

#[test]
fn help_lists_check_for_updates() {
    let spec = migez_engine::commands::command("help.check_updates").unwrap();
    assert_eq!(spec.label, "Check for updates…");
    assert_eq!(spec.menu, Some("Help"));
}
