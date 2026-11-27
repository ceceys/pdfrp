//! Pdfrp 1.1.0: Help ▸ Check for updates… and the check at start, through `migez_update`.
//!
//! - At start, at most once a day, and only when Preferences ▸ General allows it (on by default),
//!   the desktop app asks GitHub for the latest release on a thread of its own. Nothing waits for
//!   it, and a failure (offline, GitHub's rate limit, no release yet) is only logged.
//! - Help ▸ Check for updates… asks at once and says what it found, a failure included.
//! - A newer release shows a card at the bottom right: "{app} {version} is ready", Update now /
//!   Later. Nothing is downloaded before Update now. Then the installer goes to a new folder in
//!   %TEMP% and its SHA-256 is checked against the release's SHA256SUMS.txt; a file that differs is
//!   deleted and never run. Once it matches, the app quits the usual way, asking about unsaved
//!   documents first, and the installer's wizard opens as the app exits (`on_exit`): never a
//!   silent install. The user starts the installation there, and its last page starts Pdfrp
//!   again. A quit that doesn't go through (Cancel at the "save changes?" question, a
//!   save cancelled or failed, an OCR review still open) calls the install off: the card offers
//!   Update now / Later again, and a later ordinary quit installs nothing.
//! - At start the desktop app removes what earlier updates left in %TEMP%: only folders named as
//!   `migez_update` names its own, holding nothing but installers, an hour old or more.
//!
//! Tests never reach the network: the source is [`UpdateSource::Off`] until the desktop app sets
//! it, and they stand in a transport of their own ([`UpdateSource::Fake`]).

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError};

use egui::{Align, Align2, Layout, vec2};
use migez_update::{CheckError, InstallError, Outcome, Release, Transport, Verified, Version};

use crate::MigezApp;
use crate::theme::{self, Tokens};
use crate::widgets;

/// The test hook: local files (or a status) instead of GitHub (`migez_update::OVERRIDE_ENV`).
pub use migez_update::OVERRIDE_ENV;

/// Where the check reads.
#[derive(Clone, Default)]
pub enum UpdateSource {
    /// Nowhere: tests, and any host that hasn't turned updates on. Help ▸ Check for updates says
    /// this copy doesn't check.
    #[default]
    Off,
    /// GitHub (`migez_update::LATEST_URL`), or the local test files `MIGEZ_UPDATE_URL` names.
    Environment,
    /// Tests: this transport, asked at this address.
    Fake(Arc<dyn Transport>, String),
}

/// Starts a checked installer as the app exits. The desktop app's runs the file
/// ([`system_launcher`]); tests record the call.
pub type Launcher = Box<dyn FnMut(Verified) -> std::io::Result<()>>;

/// The installer started the usual way: the update's switches (its wizard shows, no silent
/// install), and which copy it replaces (`migez_update::installer_args`).
pub fn system_launcher() -> Launcher {
    Box::new(|verified: Verified| {
        let exe = std::env::current_exe().unwrap_or_default();
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        let programs = std::env::var_os("ProgramFiles").map(PathBuf::from);
        let args = migez_update::installer_args(&exe, local.as_deref(), programs.as_deref());
        log::info!("update: starting {} {}", verified.path.display(), args.join(" "));
        verified.launch(&args)
    })
}

/// Where an update stands.
#[derive(Clone, Debug, Default)]
enum Phase {
    #[default]
    Idle,
    Checking,
    /// A newer release, on the card; nothing downloaded yet.
    Offered(Release),
    Downloading(Release),
    /// Downloaded and checked: the app is quitting to install it (`Updates::quitting`), or waits
    /// for Update now again after that quit didn't go through.
    Ready(Release),
    /// The download or its check failed; the card says why.
    Failed {
        release: Release,
        message: String,
    },
}

/// What the worker thread sends back.
enum Message {
    Checked { manual: bool, result: Result<Outcome, CheckError> },
    Downloaded(Result<Verified, InstallError>),
}

#[derive(Default)]
pub(crate) struct Updates {
    phase: Phase,
    source: UpdateSource,
    /// Check on the first frame, if Preferences allows and a day has passed (the desktop app
    /// turns it on; tests leave it off).
    startup_check: bool,
    rx: Option<Receiver<Message>>,
    /// Bytes of the installer received so far.
    received: Arc<AtomicU64>,
    /// The checked installer, held open until it starts.
    verified: Option<Verified>,
    /// Update now asked the app to quit and that quit is still going on: only then does the
    /// installer start as the app exits.
    quitting: bool,
    launcher: Option<Launcher>,
    /// Where downloads go: %TEMP% unless a test says otherwise.
    download_dir: Option<PathBuf>,
}

/// The running version, as the check compares it.
fn current_version() -> Result<Version, CheckError> {
    Version::parse(migez_engine::build_info::PRODUCT_VERSION).ok_or_else(|| CheckError::Invalid("the running version isn't a version".into()))
}

/// The User-Agent every request of the app carries (`Pdfrp/<version>`): the update check's and,
/// since 1.2.0, the language modules' downloads (`translate`), through the same client
/// (`migez_update::github_transport`).
pub(crate) fn user_agent() -> String {
    migez_update::user_agent(migez_engine::build_info::PRODUCT_VERSION)
}

/// The transport and address `source` reads from.
fn open(source: &UpdateSource) -> Result<(Arc<dyn Transport>, String), CheckError> {
    match source {
        UpdateSource::Off => Err(CheckError::Refused("this copy doesn't check for updates".into())),
        UpdateSource::Environment => {
            let (transport, url) = migez_update::Source::from_env()?.open(&user_agent())?;
            Ok((Arc::from(transport), url))
        }
        UpdateSource::Fake(transport, url) => Ok((transport.clone(), url.clone())),
    }
}

/// What to tell the user about a check that found nothing (Help ▸ Check for updates).
fn check_failure(e: &CheckError) -> String {
    match e {
        CheckError::Offline(_) => tl!("Couldn't check for updates: no connection to GitHub.").to_owned(),
        CheckError::RateLimited => tl!("Couldn't check for updates: GitHub isn't taking requests right now. Try again later.").to_owned(),
        CheckError::NotFound => crate::i18n::with_app(tl!("No release of {app} has been published yet.")),
        CheckError::Invalid(_) => tl!("Couldn't check for updates: the release on GitHub is incomplete or can't be read.").to_owned(),
        CheckError::Status(code) => crate::i18n::fmt(tl!("Couldn't check for updates: {reason}"), &[("reason", &format!("HTTP {code}"))]),
        CheckError::Refused(reason) => crate::i18n::fmt(tl!("Couldn't check for updates: {reason}"), &[("reason", reason)]),
    }
}

/// What the card says about a download that failed.
fn install_failure(e: &InstallError) -> String {
    match e {
        InstallError::Mismatch { .. } => {
            tl!("The downloaded file didn't match its SHA-256 in SHA256SUMS.txt, so it was deleted and not run.").to_owned()
        }
        InstallError::NotListed(_) => tl!("SHA256SUMS.txt doesn't list the installer, so it wasn't downloaded.").to_owned(),
        InstallError::Size { .. } => tl!("The download was incomplete, so it was deleted and not run.").to_owned(),
        InstallError::Fetch(f) => {
            let reason = match CheckError::from(f.clone()) {
                CheckError::Offline(_) => tl!("no connection to GitHub").to_owned(),
                CheckError::RateLimited => tl!("GitHub isn't taking requests right now").to_owned(),
                other => other.to_string(),
            };
            crate::i18n::fmt(tl!("Couldn't download the update: {reason}"), &[("reason", &reason)])
        }
        InstallError::Io(reason) => crate::i18n::fmt(tl!("Couldn't save the update in the temporary folder: {reason}"), &[("reason", reason)]),
    }
}

/// Removes a download the user put off: the installer and the folder made for it.
fn discard(verified: Verified) {
    let path = verified.path.clone();
    drop(verified);
    let _ = std::fs::remove_file(&path);
    if let Some(dir) = path.parent().filter(|d| d.file_name().and_then(|n| n.to_str()).is_some_and(migez_update::is_download_folder)) {
        let _ = std::fs::remove_dir(dir);
    }
}

/// At start (the desktop app): remove the download folders earlier updates left in %TEMP%, on a
/// thread of its own so the window doesn't wait (`migez_update::clean_old_downloads`: only our own
/// folders, holding nothing but installers, an hour old or more).
pub fn clean_old_downloads_in_background() {
    let spawned = std::thread::Builder::new().name("migez-update-clean".into()).spawn(|| {
        let removed = migez_update::clean_old_downloads(&std::env::temp_dir(), std::time::Duration::from_secs(60 * 60));
        if removed > 0 {
            log::info!("update: removed {removed} old download folder(s) from the temporary folder");
        }
    });
    if let Err(e) = spawned {
        log::warn!("update: {e}");
    }
}

impl MigezApp {
    /// Where the update check reads (the desktop app: [`UpdateSource::Environment`]).
    pub fn set_update_source(&mut self, source: UpdateSource) {
        self.updates.source = source;
    }

    /// What starts a checked installer as the app exits (the desktop app: [`system_launcher`]).
    pub fn set_update_launcher(&mut self, launcher: Launcher) {
        self.updates.launcher = Some(launcher);
    }

    /// Where downloaded installers go instead of %TEMP% (tests).
    pub fn set_update_download_dir(&mut self, dir: PathBuf) {
        self.updates.download_dir = Some(dir);
    }

    /// Whether the app checks on its first frame (off until the desktop app turns it on). The
    /// check still waits for Preferences and for a day since the last one.
    pub fn set_startup_update_check(&mut self, on: bool) {
        self.updates.startup_check = on;
    }

    /// The version the card offers, if it offers one (tests and automation).
    pub fn update_offered(&self) -> Option<String> {
        match &self.updates.phase {
            Phase::Offered(r) | Phase::Downloading(r) | Phase::Ready(r) | Phase::Failed { release: r, .. } => Some(r.version.to_string()),
            Phase::Idle | Phase::Checking => None,
        }
    }

    /// Whether a checked installer starts as the app exits: Update now's quit is going on.
    pub fn update_ready(&self) -> bool {
        self.updates.quitting && self.updates.verified.is_some() && matches!(self.updates.phase, Phase::Ready(_))
    }

    /// Whether a checked installer is downloaded, whether or not it is about to start.
    pub fn update_downloaded(&self) -> bool {
        self.updates.verified.is_some() && matches!(self.updates.phase, Phase::Ready(_))
    }

    /// Something of a quit is still going on: the app is closing, the "save changes?" question
    /// or another dialog is open, a Save As picker is showing (or its answer waits for the
    /// frame), or a save waits behind the signature-lock or overwrite question.
    fn quit_under_way(&self) -> bool {
        self.allow_quit || self.modal_open() || self.pickers.pending() || self.pending_lock.is_some() || self.pending_overwrite.is_some()
    }

    /// Whether a check or a download is running.
    pub fn update_busy(&self) -> bool {
        self.updates.rx.is_some()
    }

    /// Help ▸ Check for updates…: ask now and say what was found.
    pub fn check_for_updates(&mut self) {
        self.start_update_check(true);
    }

    fn start_update_check(&mut self, manual: bool) {
        if self.updates.rx.is_some() || matches!(self.updates.phase, Phase::Ready(_)) {
            if manual {
                self.notify_tr("An update check is already under way.");
            }
            return;
        }
        if matches!(self.updates.source, UpdateSource::Off) {
            if manual {
                self.notify(crate::i18n::with_app(tl!("This copy of {app} doesn't check for updates.")));
            }
            return;
        }
        self.update_last_check = Some(migez_update::now_secs());
        self.updates.phase = Phase::Checking;
        if manual {
            self.notify_tr("Checking for updates…");
        }
        let source = self.updates.source.clone();
        self.spawn_update_work("migez-update-check", move || {
            let result = current_version().and_then(|current| {
                let (transport, url) = open(&source)?;
                migez_update::check(transport.as_ref(), &url, &current)
            });
            Message::Checked { manual, result }
        });
    }

    /// Update now: download the offered installer and check it.
    fn start_update_download(&mut self) {
        let Phase::Offered(release) = &self.updates.phase else { return };
        let release = release.clone();
        let source = self.updates.source.clone();
        let parent = self.updates.download_dir.clone().unwrap_or_else(std::env::temp_dir);
        let received = Arc::new(AtomicU64::new(0));
        self.updates.received = received.clone();
        self.updates.phase = Phase::Downloading(release.clone());
        self.spawn_update_work("migez-update-download", move || {
            let result = open(&source)
                .map_err(|e| InstallError::Io(e.to_string()))
                .and_then(|(transport, _)| migez_update::download(transport.as_ref(), &release, &parent, &received));
            Message::Downloaded(result)
        });
    }

    /// Runs `work` on its own thread (inline with `run_inline`, for tests), its answer picked up
    /// by [`Self::poll_updates`].
    fn spawn_update_work(&mut self, name: &str, work: impl FnOnce() -> Message + Send + 'static) {
        let (tx, rx) = std::sync::mpsc::channel();
        self.updates.rx = Some(rx);
        let ctx = self.ctx.clone();
        let job = move || {
            let _ = tx.send(work());
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        };
        if self.run_inline {
            job();
        } else if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(job) {
            log::warn!("update: {e}");
            self.updates.rx = None;
            self.updates.phase = Phase::Idle;
        }
    }

    /// Each frame: the check at start, and the worker's answer.
    pub(crate) fn poll_updates(&mut self) {
        if std::mem::take(&mut self.updates.startup_check)
            && self.update_check_at_start
            && migez_update::due(self.update_last_check, migez_update::now_secs())
        {
            self.start_update_check(false);
        }
        // Update now's quit didn't go through (Cancel at the "save changes?" question, a save
        // cancelled or failed, an OCR review that holds the quit): the install is called off, and
        // the card offers Update now / Later again. A later ordinary quit installs nothing.
        if self.updates.quitting && !self.quit_under_way() {
            self.updates.quitting = false;
            log::info!("update: the quit was called off, so is the install");
        }
        let Some(rx) = &self.updates.rx else { return };
        let message = match rx.try_recv() {
            Ok(message) => message,
            Err(TryRecvError::Empty) => {
                // The download's progress moves without a message.
                if let Some(ctx) = &self.ctx {
                    ctx.request_repaint_after(std::time::Duration::from_millis(250));
                }
                return;
            }
            Err(TryRecvError::Disconnected) => {
                self.updates.rx = None;
                self.updates.phase = match std::mem::take(&mut self.updates.phase) {
                    Phase::Downloading(release) => Phase::Failed { release, message: tl!("The update stopped unexpectedly.").to_owned() },
                    _ => Phase::Idle,
                };
                return;
            }
        };
        self.updates.rx = None;
        self.answer_update(message);
    }

    fn answer_update(&mut self, message: Message) {
        match message {
            Message::Checked { manual, result } => match result {
                Ok(Outcome::Newer(release)) => {
                    log::info!("update: {} is available", release.version);
                    self.updates.phase = Phase::Offered(release);
                }
                Ok(Outcome::UpToDate(latest)) => {
                    log::info!("update: up to date (latest release {latest})");
                    self.updates.phase = Phase::Idle;
                    if manual {
                        let version = migez_engine::build_info::PRODUCT_VERSION;
                        self.notify(crate::i18n::fmt(tl!("{app} is up to date ({version})."), &[("app", crate::APP_NAME), ("version", version)]));
                    }
                }
                Err(e) => {
                    log::info!("update: check failed: {e}");
                    self.updates.phase = Phase::Idle;
                    if manual {
                        self.notify(check_failure(&e));
                    }
                }
            },
            Message::Downloaded(result) => {
                let Phase::Downloading(release) = std::mem::take(&mut self.updates.phase) else { return };
                match result {
                    Ok(verified) => {
                        log::info!("update: {} downloaded, SHA-256 {} matches", verified.path.display(), verified.sha256);
                        self.updates.verified = Some(verified);
                        self.updates.phase = Phase::Ready(release);
                        self.quit_to_update();
                    }
                    Err(e) => {
                        log::warn!("update: download failed: {e}");
                        self.updates.phase = Phase::Failed { release, message: install_failure(&e) };
                    }
                }
            }
        }
    }

    /// Quit so the checked installer starts: the "save changes?" question first when something is
    /// unsaved, as for any quit.
    fn quit_to_update(&mut self) {
        if let Some(ctx) = self.ctx.clone() {
            self.updates.quitting = true;
            let _ = self.request_quit(&ctx);
        }
    }

    /// Later: the card goes; a downloaded installer is deleted.
    fn put_off_update(&mut self) {
        self.updates.quitting = false;
        self.updates.phase = Phase::Idle;
        if let Some(verified) = self.updates.verified.take() {
            discard(verified);
        }
    }

    /// The app is exiting (`on_exit`): start a checked installer, but only when this exit is the
    /// quit Update now asked for. Otherwise a downloaded installer is deleted, not run.
    pub(crate) fn updates_exiting(&mut self) {
        let Some(verified) = self.updates.verified.take() else { return };
        if !(self.updates.quitting && matches!(self.updates.phase, Phase::Ready(_))) {
            log::info!("update: exiting without Update now's quit; the installer is not started");
            discard(verified);
            return;
        }
        self.updates.quitting = false;
        match self.updates.launcher.as_mut() {
            Some(launch) => match launch(verified) {
                Ok(()) => log::info!("update: installer started"),
                Err(e) => log::error!("update: couldn't start the installer: {e}"),
            },
            None => log::warn!("update: no launcher; the installer was not started"),
        }
        self.updates.phase = Phase::Idle;
    }
}

/// The update card at the bottom right, over the status bar: a newer release, its download, a
/// failure, or a checked update waiting for the app to quit. Hidden under dialogs and while the
/// "save changes?" question or the quit is under way.
pub(crate) fn show(app: &mut MigezApp, ctx: &egui::Context) {
    if app.modal_open() || app.allow_quit || app.updates.quitting {
        return;
    }
    let (release, busy) = match &app.updates.phase {
        Phase::Offered(r) | Phase::Ready(r) => (r.clone(), false),
        Phase::Downloading(r) => (r.clone(), true),
        Phase::Failed { release, .. } => (release.clone(), false),
        Phase::Idle | Phase::Checking => return,
    };
    let failed = match &app.updates.phase {
        Phase::Failed { message, .. } => Some(message.clone()),
        _ => None,
    };
    let ready = matches!(app.updates.phase, Phase::Ready(_));
    let version = release.version.to_string();
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let title = match (&failed, busy) {
        (Some(_), _) => tl!("The update wasn't installed").to_owned(),
        (None, true) => crate::i18n::fmt(tl!("Downloading {app} {version}…"), &[("app", crate::APP_NAME), ("version", &version)]),
        (None, false) => crate::i18n::fmt(tl!("{app} {version} is ready"), &[("app", crate::APP_NAME), ("version", &version)]),
    };
    let body = match (&failed, busy, ready) {
        (Some(message), _, _) => message.clone(),
        (None, true, _) => {
            let mb = |bytes: u64| format!("{:.1}", bytes as f64 / 1_048_576.0);
            let got = app.updates.received.load(Ordering::Relaxed);
            match release.installer.size {
                0 => crate::i18n::fmt(tl!("{received} MB"), &[("received", &mb(got))]),
                total => {
                    crate::i18n::fmt(tl!("{received} of {total} MB, then checked with SHA-256"), &[("received", &mb(got)), ("total", &mb(total))])
                }
            }
        }
        (None, false, true) => crate::i18n::with_app(tl!("The update is downloaded and checked. {app} closes to install it.")),
        (None, false, false) => tl!("A new version is on GitHub. It is downloaded and checked with SHA-256 before it installs.").to_owned(),
    };
    #[derive(PartialEq)]
    enum Click {
        Update,
        Later,
        Close,
    }
    let mut click = None;
    egui::Area::new(egui::Id::unique("migez-update-card"))
        .order(egui::Order::Foreground)
        .pivot(Align2::RIGHT_BOTTOM)
        .fixed_pos(screen.right_bottom() - vec2(16.0, crate::STATUS_BAR_HEIGHT + 16.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(t.card)
                .stroke(egui::Stroke::new(1.0, t.border))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(16, 12))
                .shadow(ctx.global_style().visuals.popup_shadow)
                .show(ui, |ui| {
                    ui.set_max_width(360.0_f32.min(screen.width() - 64.0).max(200.0));
                    ui.horizontal(|ui| {
                        if busy {
                            ui.add(egui::Spinner::new().size(18.0).color(t.accent));
                        } else if failed.is_some() {
                            ui.add(crate::icons::image("triangle-alert", 18.0, t.accent));
                        } else {
                            widgets::app_mark(ui, 20.0);
                        }
                        ui.add(egui::Label::new(egui::RichText::new(&title).font(theme::semibold(14.0)).color(t.text)).wrap());
                    });
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(egui::RichText::new(&body).color(t.text_muted)).wrap());
                    if busy {
                        return;
                    }
                    ui.add_space(10.0);
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if failed.is_some() {
                            if widgets::pill_button(ui, tl!("Close"), false).clicked() {
                                click = Some(Click::Close);
                            }
                            return;
                        }
                        // "Sonra" in Turkish: the shared catalog's "Later" is "Daha sonra".
                        if widgets::pill_button(ui, tl_ctx!("update", "Later"), false).clicked() {
                            click = Some(Click::Later);
                        }
                        if widgets::pill_button(ui, tl!("Update now"), true).clicked() {
                            click = Some(Click::Update);
                        }
                    });
                });
        });
    match click {
        Some(Click::Update) if ready => app.quit_to_update(),
        Some(Click::Update) => app.start_update_download(),
        Some(Click::Later | Click::Close) => app.put_off_update(),
        None => {}
    }
}

#[cfg(test)]
mod tests;
