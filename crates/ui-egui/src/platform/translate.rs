//! Pdfrp 1.2.0: offline translation, through `migez_translate`.
//!
//! - Edit ▸ Translate…, the page's right-click menu and the selection bar open the Translate panel
//!   (on the right) with the selected text made ready ([`migez_translate::text`]): lines of a
//!   paragraph joined, words divided at a line end mended, paragraphs kept. The text can be edited
//!   or typed there too.
//! - The source language is the user's choice (Pdfrp detects none) and is remembered, as is the
//!   target. With a remembered source the translation starts at once.
//! - A language pair needs one module (one of them English) or two, through English. A module that
//!   isn't installed asks first ([`Dialog::TranslateDownload`], the legal review's wording, with
//!   the size): nothing is downloaded before Download. Downloads go through `migez_update`'s
//!   client, with the update check's User-Agent ([`crate::updates::user_agent`]).
//! - The helper (`pdfrp-ceviri`) starts on the first translation and stays open; Pdfrp stops it
//!   before a module is installed or removed (it keeps model files mapped) and as the app exits.
//! - Preferences ▸ Language modules lists every language with its state, size and Download or
//!   Remove (a language is its two modules, into English and out of it). Download asks the same
//!   question first, then goes back to Preferences.
//!
//! Work runs on threads of its own (inline with `run_inline`, for tests); nothing waits for it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};

use egui::{Align, Layout, RichText};
use migez_translate::{Direction, Helper, HelperError, InstallError, Manifest, ModelStore, Unpack, languages, manifest};
use migez_update::Transport;

use crate::dialogs::preferences::PrefSearchContext;
use crate::theme::{self, Tokens};
use crate::{Dialog, MigezApp, RightPanel, widgets};

/// The most characters translated at once (the helper's limit).
pub const MAX_TEXT: usize = 100_000;

/// The legal review's wordings (27.11.2026), English as the code writes them; the Turkish is in
/// the catalog. Wherever Mozilla is named, [`NOT_AFFILIATED`](wording::NOT_AFFILIATED) is said as
/// prominently (Mozilla's trademark policy).
pub mod wording {
    /// The download question's title, one module.
    pub const TITLE_ONE: &str = "Download language module?";
    /// The download question's title, two modules.
    pub const TITLE_TWO: &str = "Download language modules?";
    /// One module is missing: `{diller}` its direction, `{boyut}` its size.
    pub const BODY_ONE: &str = "The {diller} language module ({boyut}) will be downloaded from Pdfrp's GitHub page; it contains Mozilla's openly licensed (MPL-2.0) translation model, and translation runs entirely on your computer.";
    /// Both modules of a pair through English are missing: `{diller}` the pair, `{boyut}` both.
    pub const BODY_TWO: &str = "Because {diller} is translated via English, two language modules ({boyut}) will be downloaded from Pdfrp's GitHub page; they contain Mozilla's openly licensed (MPL-2.0) translation models, and translation runs entirely on your computer.";
    /// Preferences ▸ Language modules ▸ Download, both modules of a language missing: `{dil}` the
    /// language's name as the list shows it, `{boyut}` both.
    pub const BODY_LANGUAGE: &str = "Two {dil} language modules ({boyut}) will be downloaded from Pdfrp's GitHub page: {dil} → English and English → {dil}. They contain Mozilla's openly licensed (MPL-2.0) translation models, and translation runs entirely on your computer.";
    /// The line under the question, the same size and colour as the question (Mozilla's trademark
    /// policy: "equally prominently").
    pub const NOT_AFFILIATED: &str = "Pdfrp is not officially associated with Mozilla or its products.";
    /// The note always under a translation (the panel's short form).
    pub const MACHINE_NOTE: &str = "Machine translation may contain errors; for legal or official use, rely on the original.";
    /// About ▸ Licences.
    pub const FXTRANSLATE_LINE: &str = "Mozilla Public License 2.0 · modified for Pdfrp · Source code: {source}";
    pub const MODULES_LINE: &str = "Mozilla's Firefox Translations models, unmodified · Mozilla Public License 2.0 · Source: github.com/mozilla/translations · Pdfrp is not officially associated with Mozilla or its products.";
    /// NOTICE's paragraph, `{source}` the vendored source at this version's tag.
    pub const NOTICE: &str = "pdfrp-ceviri.exe contains a version of fxtranslate 0.5.0 modified for Pdfrp (Mozilla Public License 2.0). Source code, including the changes: {source}. The language modules are Mozilla's Firefox Translations models (MPL-2.0), distributed unmodified. Source: https://github.com/mozilla/translations (earlier model repository: https://github.com/mozilla/firefox-translations-models). Pdfrp is not officially associated with Mozilla or its products.";
}

/// Where the vendored fxtranslate source of this version is: its tag in the public repository.
pub fn fxtranslate_source() -> String {
    format!("https://github.com/ceceys/pdfrp/tree/v{}/vendor/fxtranslate", migez_engine::build_info::PRODUCT_VERSION)
}

/// How this copy translates.
#[derive(Clone, Default)]
pub enum TranslateSetup {
    /// It doesn't (tests, hosts that haven't turned it on): Translate… says so.
    #[default]
    Off,
    /// The desktop app: the helper builds under `dir` (Pdfrp.exe's folder), modules in `store`,
    /// downloads from GitHub (or the local folder `MIGEZ_MODELS_DIR` names, for testing).
    Installed { dir: PathBuf, store: ModelStore },
    /// Tests: this helper (none: starting it fails), this store, this transport at this address,
    /// this unpacker.
    Fake { exe: Option<PathBuf>, store: ModelStore, transport: Arc<dyn Transport>, base: String, unpack: Unpack },
}

impl TranslateSetup {
    /// The desktop app's: the helper builds under the running program's folder (the installer
    /// puts them in `ceviri\avx2\` and `ceviri\portable\`), the modules in
    /// `%LOCALAPPDATA%\Pdfrp\models`, or in a portable copy's own data folder.
    pub fn desktop() -> TranslateSetup {
        let dir = std::env::current_exe().ok().and_then(|exe| exe.parent().map(std::path::Path::to_path_buf));
        let root = crate::portable::data_dir().map(|d| d.join("models")).or_else(migez_translate::store::default_root);
        match (dir, root) {
            (Some(dir), Some(root)) => TranslateSetup::Installed { dir, store: ModelStore::new(root) },
            _ => TranslateSetup::Off,
        }
    }

    fn store(&self) -> Option<&ModelStore> {
        match self {
            TranslateSetup::Off => None,
            TranslateSetup::Installed { store, .. } | TranslateSetup::Fake { store, .. } => Some(store),
        }
    }

    /// The helper to start: the build this processor runs.
    fn helper_exe(&self) -> Option<PathBuf> {
        match self {
            TranslateSetup::Off => None,
            TranslateSetup::Installed { dir, .. } => migez_translate::helper::helper_exe(dir, migez_translate::helper::use_avx2()),
            TranslateSetup::Fake { exe, .. } => exe.clone(),
        }
    }
}

/// What the workers send back.
enum Message {
    Translated(Result<String, String>),
    /// Modules installed (or not); `then_translate` when a translation waits for them.
    Installed {
        result: Result<(), String>,
        then_translate: bool,
    },
    Removed(Result<(), String>),
}

/// A download or removal under way (one at a time).
struct Job {
    /// The language, for Preferences' row (a translation's download has none).
    language: Option<String>,
    removing: bool,
    received: Arc<AtomicU64>,
    total: u64,
    cancel: Arc<AtomicBool>,
}

/// The modules behind the download question: a translation waits for them, or Preferences'
/// Download asked for a language's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Confirm {
    /// The missing modules, as `tr-en`.
    pub ids: Vec<String>,
    /// The pair being translated (Preferences: the language and English).
    pub from: String,
    pub to: String,
    /// Preferences ▸ Language modules ▸ Download: the language whose modules these are. `None`: a
    /// translation waits for them.
    pub language: Option<String>,
}

pub(crate) struct Translation {
    setup: TranslateSetup,
    /// The source language last chosen (kept in the settings); `None` until one is.
    pub(crate) source: Option<String>,
    /// The target language (kept in the settings).
    pub(crate) target: String,
    /// The text to translate, as the panel shows it.
    pub(crate) text: String,
    /// The last translation, or why there is none.
    pub(crate) result: Option<Result<String, String>>,
    /// A translation is running.
    busy: bool,
    job: Option<Job>,
    pub(crate) confirm: Option<Confirm>,
    tx: Sender<Message>,
    rx: Receiver<Message>,
    helper: Arc<Mutex<Option<Helper>>>,
    /// Which modules are installed, by `tr-en`; filled as asked, cleared after a change.
    installed: HashMap<String, bool>,
    /// The last download or removal failure, for Preferences.
    pub(crate) notice: Option<String>,
}

impl Default for Translation {
    fn default() -> Translation {
        let (tx, rx) = std::sync::mpsc::channel();
        Translation {
            setup: TranslateSetup::Off,
            source: None,
            target: default_target(),
            text: String::new(),
            result: None,
            busy: false,
            job: None,
            confirm: None,
            tx,
            rx,
            helper: Arc::new(Mutex::new(None)),
            installed: HashMap::new(),
            notice: None,
        }
    }
}

/// The target a new user starts with: Turkish when the interface is Turkish, English otherwise.
fn default_target() -> String {
    if crate::i18n::current().code() == "tr" { "tr".into() } else { "en".into() }
}

/// Whether language names show in Turkish.
fn turkish() -> bool {
    crate::i18n::current().code() == "tr"
}

fn manifest() -> Option<&'static Manifest> {
    manifest::embedded().ok()
}

/// Every language offered, English first, the others by name.
fn language_list() -> Vec<&'static str> {
    let mut codes: Vec<&'static str> = manifest().map(Manifest::language_codes).unwrap_or_default();
    let tr = turkish();
    codes.sort_by(|a, b| languages::compare_names(languages::name(a, tr), languages::name(b, tr), tr));
    std::iter::once(languages::ENGLISH.code).chain(codes).collect()
}

/// A size as the download question writes it: "38 MB".
pub fn megabytes(bytes: u64) -> String {
    format!("{} MB", ((bytes as f64) / 1_048_576.0).round().max(1.0))
}

/// "Türkçe → Almanca".
fn pair_name(from: &str, to: &str) -> String {
    let tr = turkish();
    format!("{} → {}", languages::name(from, tr), languages::name(to, tr))
}

/// The helper, poisoned or not: a panicked worker leaves it as it was.
fn lock(helper: &Mutex<Option<Helper>>) -> std::sync::MutexGuard<'_, Option<Helper>> {
    helper.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Stops the helper (if it runs), freeing its model files.
fn stop_helper(helper: &Mutex<Option<Helper>>) {
    if let Some(h) = lock(helper).take() {
        h.stop();
    }
}

/// What to tell the user about a module that wasn't installed.
fn install_failure(e: &InstallError) -> String {
    match e {
        InstallError::Mismatch { .. } => tl!("A downloaded file didn't match its SHA-256, so it was deleted and nothing was installed.").to_owned(),
        InstallError::Size { .. } => tl!("A download was incomplete, so it was deleted and nothing was installed.").to_owned(),
        InstallError::Cancelled => tl!("The download was cancelled.").to_owned(),
        InstallError::Fetch(f) => {
            let reason = match migez_update::CheckError::from(f.clone()) {
                migez_update::CheckError::Offline(_) => tl!("no connection to GitHub").to_owned(),
                migez_update::CheckError::RateLimited => tl!("GitHub isn't taking requests right now").to_owned(),
                other => other.to_string(),
            };
            crate::i18n::fmt(tl!("Couldn't download the language module: {reason}"), &[("reason", &reason)])
        }
        InstallError::Unpack { reason, .. } => crate::i18n::fmt(tl!("Couldn't unpack the language module: {reason}"), &[("reason", reason)]),
        InstallError::Io(reason) => crate::i18n::fmt(tl!("Couldn't save the language module: {reason}"), &[("reason", reason)]),
    }
}

/// Installs modules: stops the helper first, then each module in turn.
fn install_work(
    setup: TranslateSetup,
    ids: &[String],
    helper: &Mutex<Option<Helper>>,
    received: &AtomicU64,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let m = manifest().ok_or_else(|| tl!("The list of language modules can't be read.").to_owned())?;
    let directions: Vec<&Direction> = ids.iter().filter_map(|id| m.directions.iter().find(|d| d.id() == *id)).collect();
    stop_helper(helper);
    let (store, transport, base, unpack): (ModelStore, Arc<dyn Transport>, String, Unpack) = match setup {
        TranslateSetup::Off => return Err(crate::i18n::with_app(tl!("This copy of {app} can't translate."))),
        TranslateSetup::Installed { store, .. } => {
            let (transport, base) = migez_translate::Source::from_env()
                .and_then(|s| s.open(m, &crate::updates::user_agent()))
                .map_err(|e| install_failure(&InstallError::Fetch(e)))?;
            (store, Arc::from(transport), base, migez_translate::store::unpack_zstd)
        }
        TranslateSetup::Fake { store, transport, base, unpack, .. } => (store, transport, base, unpack),
    };
    for d in directions {
        store.install(d, (transport.as_ref(), &base), unpack, received, cancel).map_err(|e| {
            log::warn!("translate: {} not installed: {e}", d.id());
            install_failure(&e)
        })?;
        log::info!("translate: {} {} installed", d.id(), d.version);
    }
    Ok(())
}

/// Translates through the helper, starting it if it isn't running.
fn translate_work(exe: Option<PathBuf>, root: PathBuf, chain: &[Direction], text: &str, helper: &Mutex<Option<Helper>>) -> Result<String, String> {
    let mut guard = lock(helper);
    if !guard.as_mut().is_some_and(Helper::running) {
        *guard = None;
        let exe = exe.ok_or_else(|| crate::i18n::with_app(tl!("This copy of {app} can't translate: its translation helper isn't installed.")))?;
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        *guard = Some(Helper::start(&exe, &root).map_err(|e| e.to_string())?);
    }
    let Some(h) = guard.as_mut() else { return Err(tl!("The translation helper didn't start.").to_owned()) };
    let refs: Vec<&Direction> = chain.iter().collect();
    match h.translate(&refs, text) {
        Ok(translation) => Ok(translation),
        Err(HelperError::Answer(e)) => Err(e),
        Err(e @ HelperError::Process(_)) => {
            // It stopped: the next translation starts a new one.
            *guard = None;
            Err(e.to_string())
        }
    }
}

impl MigezApp {
    /// How this copy translates (the desktop app: [`TranslateSetup::Installed`]).
    pub fn set_translate_setup(&mut self, setup: TranslateSetup) {
        self.translation.setup = setup;
        self.translation.installed.clear();
    }

    /// The Translate panel's text, its last translation and the languages (tests and automation).
    pub fn translation_text(&self) -> &str {
        &self.translation.text
    }

    pub fn translation_result(&self) -> Option<&Result<String, String>> {
        self.translation.result.as_ref()
    }

    pub fn translation_languages(&self) -> (Option<&str>, &str) {
        (self.translation.source.as_deref(), &self.translation.target)
    }

    /// Sets the languages (automation, tests; the panel's pickers do it too).
    pub fn set_translation_languages(&mut self, source: Option<&str>, target: &str) {
        self.translation.source = source.map(str::to_owned);
        self.translation.target = target.to_owned();
    }

    /// The modules waiting behind the download question.
    pub fn translation_download_question(&self) -> Option<&Confirm> {
        self.translation.confirm.as_ref()
    }

    /// Whether a translation, a download or a removal is under way.
    pub fn translation_busy(&self) -> bool {
        self.translation.busy || self.translation.job.is_some()
    }

    /// Whether the module is installed (asked once, then remembered until something changes).
    fn module_installed(&mut self, d: &Direction) -> bool {
        let Some(store) = self.translation.setup.store() else { return false };
        let id = d.id();
        if let Some(&known) = self.translation.installed.get(&id) {
            return known;
        }
        let known = store.is_installed(d);
        self.translation.installed.insert(id, known);
        known
    }

    /// Edit ▸ Translate…: the panel, with the selection's text if there is one; and the
    /// translation at once when the source language is known.
    pub fn open_translate(&mut self) {
        let selected = self.active.and_then(|i| self.views.get(i)).and_then(|v| match v.selected_lines() {
            Some(lines) => {
                let lines: Vec<migez_translate::text::Line> = lines
                    .into_iter()
                    .map(|(text, r)| migez_translate::text::Line { text, left: r[0], top: r[1], right: r[2], bottom: r[3] })
                    .collect();
                Some(migez_translate::text::from_lines(&lines))
            }
            None => v.selected_text().map(|t| migez_translate::text::prepare(&t)),
        });
        if let Some(text) = selected.filter(|t| !t.trim().is_empty()) {
            self.translation.text = text;
            self.translation.result = None;
        }
        self.right = Some(RightPanel::Translate);
        if self.translation.source.is_some() && !self.translation.text.trim().is_empty() {
            self.start_translation();
        }
    }

    /// Translates the panel's text: asks to download what is missing first.
    pub fn start_translation(&mut self) {
        if self.translation.busy || self.translation.job.is_some() {
            return;
        }
        let Some(m) = manifest() else {
            self.translation.result = Some(Err(tl!("The list of language modules can't be read.").to_owned()));
            return;
        };
        if matches!(self.translation.setup, TranslateSetup::Off) {
            self.translation.result = Some(Err(crate::i18n::with_app(tl!("This copy of {app} can't translate."))));
            return;
        }
        let Some(from) = self.translation.source.clone() else {
            self.translation.result = Some(Err(tl!("Choose the language of the text.").to_owned()));
            return;
        };
        let to = self.translation.target.clone();
        let text = self.translation.text.clone();
        if text.trim().is_empty() {
            self.translation.result = Some(Err(tl!("Select text on the page, or type or paste it here.").to_owned()));
            return;
        }
        if text.chars().count() > MAX_TEXT {
            self.translation.result = Some(Err(crate::i18n::fmt(
                tl!("The text is too long to translate at once (at most {n} characters)."),
                &[("n", &MAX_TEXT.to_string())],
            )));
            return;
        }
        let route = match m.route(&from, &to) {
            Ok(route) => route,
            Err(migez_translate::RouteError::Same) => {
                self.translation.result = Some(Err(tl!("The two languages are the same.").to_owned()));
                return;
            }
            Err(e) => {
                self.translation.result = Some(Err(e.to_string()));
                return;
            }
        };
        let missing: Vec<String> = route.iter().filter(|d| !self.module_installed(d)).map(|d| d.id()).collect();
        if !missing.is_empty() {
            self.translation.confirm = Some(Confirm { ids: missing, from, to, language: None });
            self.dialog = Some(Dialog::TranslateDownload);
            return;
        }
        let chain: Vec<Direction> = route.into_iter().cloned().collect();
        let (exe, root) = (self.translation.setup.helper_exe(), self.translation.setup.store().map(|s| s.root().to_path_buf()).unwrap_or_default());
        let helper = self.translation.helper.clone();
        self.translation.busy = true;
        self.translation.result = None;
        self.spawn_translate_work("migez-translate", move || Message::Translated(translate_work(exe, root, &chain, &text, &helper)));
    }

    /// The download question's answer: Download fetches the missing modules, then translates (a
    /// translation's question) or only fetches them (Preferences'). Preferences' question goes
    /// back to Preferences either way, where the language's row shows the download.
    pub fn answer_translate_download(&mut self, download: bool) {
        let Some(confirm) = self.translation.confirm.take() else { return };
        if confirm.language.is_some() {
            self.dialog = Some(Dialog::Preferences);
        }
        if !download {
            return;
        }
        let then_translate = confirm.language.is_none();
        self.start_install(confirm.ids, confirm.language, then_translate);
    }

    /// Preferences ▸ Language modules ▸ Download: asks first, with the same question a translation
    /// asks ([`Dialog::TranslateDownload`]), for the language's modules that aren't installed.
    pub fn download_language(&mut self, code: &str) {
        if self.translation.job.is_some() || self.translation.busy {
            return;
        }
        let Some(m) = manifest() else { return };
        let ids: Vec<String> = m.modules_of(code).into_iter().filter(|d| !self.module_installed(d)).map(|d| d.id()).collect();
        if !ids.is_empty() {
            self.translation.confirm =
                Some(Confirm { ids, from: code.to_owned(), to: languages::ENGLISH.code.to_owned(), language: Some(code.to_owned()) });
            self.dialog = Some(Dialog::TranslateDownload);
        }
    }

    fn start_install(&mut self, ids: Vec<String>, language: Option<String>, then_translate: bool) {
        if self.translation.job.is_some() || self.translation.busy {
            return;
        }
        let Some(m) = manifest() else { return };
        let total = ids.iter().filter_map(|id| m.directions.iter().find(|d| d.id() == *id)).map(Direction::download_size).sum();
        let received = Arc::new(AtomicU64::new(0));
        let cancel = Arc::new(AtomicBool::new(false));
        self.translation.job = Some(Job { language, removing: false, received: received.clone(), total, cancel: cancel.clone() });
        self.translation.notice = None;
        let (setup, helper) = (self.translation.setup.clone(), self.translation.helper.clone());
        self.spawn_translate_work("migez-translate-download", move || Message::Installed {
            result: install_work(setup, &ids, &helper, &received, &cancel),
            then_translate,
        });
    }

    /// Stops a download under way; what it fetched is deleted.
    pub fn cancel_translate_download(&mut self) {
        if let Some(job) = &self.translation.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Preferences ▸ Language modules: removes both modules of a language (the helper first).
    pub fn remove_language(&mut self, code: &str) {
        if self.translation.job.is_some() || self.translation.busy {
            return;
        }
        let (Some(m), Some(store)) = (manifest(), self.translation.setup.store().cloned()) else { return };
        let directions: Vec<Direction> = m.modules_of(code).into_iter().cloned().collect();
        let helper = self.translation.helper.clone();
        self.translation.job = Some(Job {
            language: Some(code.to_owned()),
            removing: true,
            received: Arc::new(AtomicU64::new(0)),
            total: 0,
            cancel: Arc::new(AtomicBool::new(false)),
        });
        self.spawn_translate_work("migez-translate-remove", move || {
            stop_helper(&helper);
            let result = directions.iter().try_for_each(|d| store.remove(d)).map_err(|e| install_failure(&e));
            Message::Removed(result)
        });
    }

    fn spawn_translate_work(&mut self, name: &str, work: impl FnOnce() -> Message + Send + 'static) {
        let tx = self.translation.tx.clone();
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
            log::warn!("translate: {e}");
            self.translation.busy = false;
            self.translation.job = None;
        }
    }

    /// Each frame: the workers' answers.
    pub(crate) fn poll_translate(&mut self) {
        // Empty or disconnected: nothing more this frame.
        while let Ok(message) = self.translation.rx.try_recv() {
            match message {
                Message::Translated(result) => {
                    self.translation.busy = false;
                    if let Err(e) = &result {
                        log::info!("translate: {e}");
                    }
                    self.translation.result = Some(result.map_err(|e| crate::i18n::fmt(tl!("Couldn't translate: {reason}"), &[("reason", &e)])));
                }
                Message::Installed { result, then_translate } => {
                    self.translation.job = None;
                    self.translation.installed.clear();
                    match result {
                        Ok(()) if then_translate => self.start_translation(),
                        Ok(()) => {}
                        Err(e) if then_translate => self.translation.result = Some(Err(e)),
                        Err(e) => self.translation.notice = Some(e),
                    }
                }
                Message::Removed(result) => {
                    self.translation.job = None;
                    self.translation.installed.clear();
                    if let Err(e) = result {
                        self.translation.notice = Some(e);
                    }
                }
            }
        }
        if self.translation.job.is_some()
            && let Some(ctx) = &self.ctx
        {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
        }
    }

    /// The app is exiting: the helper goes with it.
    pub(crate) fn translate_exiting(&mut self) {
        if let Some(job) = &self.translation.job {
            job.cancel.store(true, Ordering::Relaxed);
        }
        stop_helper(&self.translation.helper);
    }

    /// The panel's Translate, Swap and Copy.
    pub(crate) fn translate_panel_action(&mut self, action: PanelAction, ctx: &egui::Context) {
        match action {
            PanelAction::Translate => self.start_translation(),
            PanelAction::Swap => {
                if let Some(source) = self.translation.source.take() {
                    self.translation.source = Some(std::mem::replace(&mut self.translation.target, source));
                    if let Some(Ok(done)) = self.translation.result.take() {
                        self.translation.text = done;
                    }
                }
            }
            PanelAction::Copy => {
                if let Some(Ok(done)) = &self.translation.result {
                    ctx.copy_text(done.clone());
                    self.notify_tr("Translation copied");
                }
            }
        }
    }

    /// The settings' languages.
    pub(crate) fn translation_settings(&self) -> serde_json::Value {
        serde_json::json!({ "from": self.translation.source, "to": self.translation.target })
    }

    pub(crate) fn restore_translation_settings(&mut self, v: &serde_json::Value) {
        let offered = |code: &str| code == languages::ENGLISH.code || manifest().is_some_and(|m| m.language_codes().contains(&code));
        if let Some(from) = v["from"].as_str().filter(|c| offered(c)) {
            self.translation.source = Some(from.to_owned());
        }
        if let Some(to) = v["to"].as_str().filter(|c| offered(c)) {
            self.translation.target = to.to_owned();
        }
    }
}

/// What the panel asks the app to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PanelAction {
    Translate,
    Swap,
    Copy,
}

/// A language picker. `None` stands for "Choose…" (the source before one is chosen).
fn picker(ui: &mut egui::Ui, id: &str, label: &str, value: &mut Option<String>, allow_none: bool) {
    let tr = turkish();
    let selected = value.as_deref().map_or_else(|| tl!("Choose…").to_owned(), |c| languages::name(c, tr).to_owned());
    let label = ui.label(label);
    craft_ui::ComboBox::from_id_salt(id)
        .selected_text(selected)
        .width(ui.available_width().min(220.0))
        .show_ui(ui, |ui| {
            if allow_none && value.is_none() {
                let _ = ui.selectable_label(true, tl!("Choose…"));
            }
            for code in language_list() {
                if ui.selectable_label(value.as_deref() == Some(code), languages::name(code, tr)).clicked() {
                    *value = Some(code.to_owned());
                    ui.close();
                }
            }
        })
        .response
        .labelled_by(label.id);
}

/// The Translate panel's body (the right panel). Its buttons come back as a [`PanelAction`].
pub(crate) fn panel(ui: &mut egui::Ui, t: &Tokens, translation: &mut Translation) -> Option<PanelAction> {
    let mut action = None;
    let busy = translation.busy || translation.job.is_some();
    ui.add_enabled_ui(!busy, |ui| {
        picker(ui, "translate-from", tl!("Source language"), &mut translation.source, true);
        ui.add_space(4.0);
        let mut target = Some(translation.target.clone());
        picker(ui, "translate-to", tl!("Target language"), &mut target, false);
        if let Some(target) = target {
            translation.target = target;
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if widgets::pill_button(ui, tl!("Translate"), true).clicked() {
                action = Some(PanelAction::Translate);
            }
            if translation.source.is_some() && crate::icons::button(ui, "arrow-left-right", 26.0, false, tl!("Swap languages")).clicked() {
                action = Some(PanelAction::Swap);
            }
        });
    });
    ui.add_space(8.0);
    let half = (ui.available_height() - 120.0).max(160.0) / 2.0;
    egui::ScrollArea::vertical().id_salt("translate-source").max_height(half).show(ui, |ui| {
        let edit = egui::TextEdit::multiline(&mut translation.text)
            .hint_text(tl!("Select text on the page, or type or paste it here."))
            .desired_rows(5)
            .desired_width(f32::INFINITY);
        ui.add_enabled(!busy, edit);
    });
    ui.add_space(8.0);
    ui.separator();
    ui.add_space(4.0);
    if let Some(job) = translation.job.as_ref().filter(|j| j.language.is_none()) {
        download_progress(ui, t, job);
    } else if translation.busy {
        ui.horizontal(|ui| {
            ui.add(egui::Spinner::new().size(16.0).color(t.accent));
            ui.label(tl!("Translating…"));
        });
    }
    match &translation.result {
        Some(Ok(done)) => {
            ui.horizontal(|ui| {
                ui.label(RichText::new(tl!("Translation")).font(theme::semibold(13.0)));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if crate::icons::button(ui, "copy", 26.0, false, tl!("Copy")).clicked() {
                        action = Some(PanelAction::Copy);
                    }
                });
            });
            egui::ScrollArea::vertical().id_salt("translate-result").max_height(half).show(ui, |ui| {
                ui.add(egui::Label::new(done.as_str()).selectable(true).wrap());
            });
        }
        Some(Err(e)) => {
            ui.add(egui::Label::new(RichText::new(e.as_str()).color(t.text_muted)).wrap());
        }
        None => {}
    }
    ui.add_space(10.0);
    ui.add(egui::Label::new(RichText::new(tl!(wording::MACHINE_NOTE)).small().color(t.text_muted)).wrap());
    action
}

fn download_progress(ui: &mut egui::Ui, t: &Tokens, job: &Job) {
    let mb = |bytes: u64| format!("{:.1}", bytes as f64 / 1_048_576.0);
    let got = job.received.load(Ordering::Relaxed).min(job.total);
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new().size(14.0).color(t.accent));
        ui.label(crate::i18n::fmt(tl!("Downloading… {received} of {total} MB"), &[("received", &mb(got)), ("total", &mb(job.total))]));
    });
    let fraction = if job.total > 0 { got as f32 / job.total as f32 } else { 0.0 };
    ui.add(egui::ProgressBar::new(fraction).desired_width(ui.available_width().min(260.0)));
}

/// The download question for `confirm`: its title (English, for `tl!`) and its body, filled in.
/// - One module missing: [`wording::TITLE_ONE`], [`wording::BODY_ONE`] with the module's
///   direction ("English → German"), whoever asks.
/// - A translation through English, both modules missing: [`wording::TITLE_TWO`],
///   [`wording::BODY_TWO`] with the pair.
/// - Preferences, both modules of a language missing: [`wording::TITLE_TWO`],
///   [`wording::BODY_LANGUAGE`] with the language's name as the list shows it.
///
/// The size is the missing modules' downloads together, as Preferences' row writes it.
pub(crate) fn question(confirm: &Confirm) -> Option<(&'static str, String)> {
    let m = manifest()?;
    let missing: Vec<&Direction> = confirm.ids.iter().filter_map(|id| m.directions.iter().find(|d| d.id() == *id)).collect();
    let size = megabytes(missing.iter().map(|d| d.download_size()).sum());
    let fill = |body: &str, key: &str, value: &str| crate::i18n::fmt(tl!(body), &[(key, value), ("boyut", &size)]);
    Some(match (missing.as_slice(), confirm.language.as_deref()) {
        ([], _) => return None,
        ([d], _) => (wording::TITLE_ONE, fill(wording::BODY_ONE, "diller", &pair_name(&d.from, &d.to))),
        (_, Some(code)) => (wording::TITLE_TWO, fill(wording::BODY_LANGUAGE, "dil", languages::name(code, turkish()))),
        (_, None) => (wording::TITLE_TWO, fill(wording::BODY_TWO, "diller", &pair_name(&confirm.from, &confirm.to))),
    })
}

/// `Dialog::TranslateDownload` content: the legal review's question ([`question`]), and under it
/// that Pdfrp isn't associated with Mozilla, as prominent as the question. Returns the answer
/// when one is given: `true` for Download.
pub(crate) fn download_dialog(ui: &mut egui::Ui, app: &MigezApp) -> Option<bool> {
    let (title, body) = question(app.translation.confirm.as_ref()?)?;
    ui.label(RichText::new(tl!(title)).font(theme::semibold(18.0)));
    ui.add_space(8.0);
    ui.label(body);
    ui.add_space(6.0);
    ui.label(tl!(wording::NOT_AFFILIATED));
    ui.add_space(12.0);
    let mut answer = None;
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        if widgets::pill_button(ui, tl!("Download"), true).clicked() {
            answer = Some(true);
        }
        // "Vazgeç" in Turkish: the shared catalog's "Cancel" is "İptal".
        if widgets::pill_button(ui, tl_ctx!("module", "Cancel"), false).clicked() {
            answer = Some(false);
        }
    });
    answer
}

/// Preferences ▸ Language modules.
pub(crate) fn preferences_card(ui: &mut egui::Ui, app: &mut MigezApp, t: &Tokens, ctx: &mut PrefSearchContext) {
    let Some(m) = manifest() else { return };
    let count = m.languages.to_string();
    ctx.label_secondary(
        ui,
        &crate::i18n::fmt(
            tl!("Translation works between English and {n} languages; other pairs go through English. Each language is two modules, into English and out of it. Modules download from Pdfrp's GitHub page."),
            &[("n", &count)],
        ),
        t,
    );
    if app.translation.setup.store().is_none() {
        ctx.label_secondary(ui, &crate::i18n::with_app(tl!("This copy of {app} can't translate.")), t);
        return;
    }
    if let Some(notice) = app.translation.notice.clone() {
        ui.add(egui::Label::new(RichText::new(notice).color(t.accent_text)).wrap());
    }
    ui.add_space(6.0);
    let tr = turkish();
    let busy = app.translation.job.is_some() || app.translation.busy;
    let mut codes = m.language_codes();
    codes.sort_by(|a, b| languages::compare_names(languages::name(a, tr), languages::name(b, tr), tr));
    let mut download = None;
    let mut remove = None;
    let mut cancel = false;
    egui::ScrollArea::vertical().id_salt("language-modules").max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
        for code in codes {
            let modules = m.modules_of(code);
            let installed = modules.iter().filter(|d| app.module_installed(d)).count();
            let download_size: u64 =
                modules.iter().filter(|d| !app.translation.installed.get(&d.id()).copied().unwrap_or(false)).map(|d| d.download_size()).sum();
            let disk: u64 = modules.iter().map(|d| d.installed_size()).sum();
            let job = app.translation.job.as_ref().filter(|j| j.language.as_deref() == Some(code));
            ui.horizontal(|ui| {
                let name = languages::name(code, tr);
                ctx.label_primary(ui, name, t);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(job) = job {
                        if job.removing {
                            ui.add(egui::Spinner::new().size(14.0).color(t.accent));
                        } else {
                            if widgets::pill_button(ui, tl_ctx!("module", "Cancel"), false).clicked() {
                                cancel = true;
                            }
                            let fraction =
                                if job.total > 0 { job.received.load(Ordering::Relaxed).min(job.total) as f32 / job.total as f32 } else { 0.0 };
                            ui.add(egui::ProgressBar::new(fraction).desired_width(90.0).show_percentage());
                        }
                        return;
                    }
                    if installed > 0
                        && ui
                            .add_enabled(!busy, egui::Button::new(tl!("Remove")))
                            .on_hover_text(crate::i18n::fmt(tl!("Remove {language}"), &[("language", name)]))
                            .clicked()
                    {
                        remove = Some(code.to_owned());
                    }
                    if installed < modules.len()
                        && ui
                            .add_enabled(!busy, egui::Button::new(tl!("Download")))
                            .on_hover_text(crate::i18n::fmt(tl!("Download {language}"), &[("language", name)]))
                            .clicked()
                    {
                        download = Some(code.to_owned());
                    }
                    let state = if installed == modules.len() {
                        crate::i18n::fmt(tl!("Installed · {size}"), &[("size", &megabytes(disk))])
                    } else if installed > 0 {
                        crate::i18n::fmt(tl!("Partly installed · {size} to download"), &[("size", &megabytes(download_size))])
                    } else {
                        crate::i18n::fmt(tl!("Not installed · {size} to download"), &[("size", &megabytes(download_size))])
                    };
                    ui.label(RichText::new(state).color(t.text_muted).small());
                });
            });
        }
    });
    if cancel {
        app.cancel_translate_download();
    }
    if let Some(code) = download {
        app.download_language(&code);
    }
    if let Some(code) = remove {
        app.remove_language(&code);
    }
}

#[cfg(test)]
mod tests;
