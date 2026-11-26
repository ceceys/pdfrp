//! Pdfrp: Word, Excel and PowerPoint through the Microsoft Office installed on this computer
//! (`migez_engine::ms_office`, COM automation, Windows only).
//!
//! - **To PDF.** File ▸ Open and Create a PDF ▸ Single file hand a Word, Excel or PowerPoint file
//!   whose app is installed to that app. The PDF it makes opens as a new, unsaved tab named after
//!   the file, as the LibreOffice way does, which stays for computers without Office (and for the
//!   apps Office lacks).
//! - **To Word.** Export a PDF ▸ Microsoft Word (.docx), with Word installed, gives Word the
//!   document as it is now; Word rebuilds the pages as editable text and keeps their layout.
//!   Without Word the export stays Pdfrp's own.
//!
//! Office can take a while, so conversions run one at a time on a worker thread; a card at the
//! bottom of the window names the one running and for how long, and the interface keeps working.
//! Before a conversion starts: a file that came from the internet is asked about (Office opens it
//! without Protected View, macros off), and an output file that exists is asked about before it
//! is replaced. The result of a file from the internet carries its mark (Zone.Identifier). A
//! conversion that ran out of time or found Office busy offers to try again. Errors are
//! `OfficeHatasi`'s own Turkish messages.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use egui::{Align, Layout};
use migez_engine::DocId;
use migez_engine::ms_office::{self, OfficeHatasi, Uygulama};

#[cfg(not(target_arch = "wasm32"))]
use crate::file_filter::AddFilterAnyCase as _;
use crate::theme::{self, Tokens};
use crate::{MigezApp, widgets};

/// The largest PDF read back from Office into a tab, as for LibreOffice's.
const MAX_PDF: u64 = 512 * 1024 * 1024;

/// One conversion, with the answers already given for it.
#[derive(Clone, Debug)]
pub(crate) struct OfficeTask {
    pub(crate) kind: TaskKind,
    /// The user agreed to convert a file from the internet.
    internet_ok: bool,
    /// The user agreed to replace the output file (or the save dialog asked already).
    overwrite_ok: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum TaskKind {
    /// `input` to PDF with `app`, opened as a new, unsaved tab; `then` runs on it.
    ToPdf { input: PathBuf, app: Uygulama, then: Option<&'static str> },
    /// The document `name` (`pdf`, its bytes as they were when the export was asked for) to the
    /// Word file `output`. `zone` is the file whose internet mark the document carries, if any.
    ToWord { pdf: Arc<Vec<u8>>, name: String, output: PathBuf, zone: Option<PathBuf> },
}

impl OfficeTask {
    fn new(kind: TaskKind) -> OfficeTask {
        OfficeTask { kind, internet_ok: false, overwrite_ok: false }
    }

    /// The Office app that does it.
    fn app(&self) -> Uygulama {
        match &self.kind {
            TaskKind::ToPdf { app, .. } => *app,
            TaskKind::ToWord { .. } => Uygulama::Word,
        }
    }

    /// The file it is about, as the card and the messages name it.
    fn file_name(&self) -> String {
        match &self.kind {
            TaskKind::ToPdf { input, .. } => file_name(input),
            TaskKind::ToWord { name, .. } => name.clone(),
        }
    }

    /// The file whose internet mark decides the warning and marks the result.
    fn zone_source(&self) -> Option<&Path> {
        match &self.kind {
            TaskKind::ToPdf { input, .. } => Some(input),
            TaskKind::ToWord { zone, .. } => zone.as_deref(),
        }
    }

    /// The file it writes where the user chose (a PDF opens as a tab, so it writes none).
    fn output(&self) -> Option<&Path> {
        match &self.kind {
            TaskKind::ToPdf { .. } => None,
            TaskKind::ToWord { output, .. } => Some(output),
        }
    }
}

/// What a finished conversion gives back.
enum Outcome {
    /// The PDF's bytes, to open as a tab.
    Pdf(Arc<Vec<u8>>),
    /// The Word file written.
    Word(PathBuf),
}

type Slot = Arc<Mutex<Option<Result<Outcome, OfficeHatasi>>>>;

/// The conversion on the worker thread.
struct Running {
    task: OfficeTask,
    started: Instant,
    result: Slot,
}

/// A question asked before a conversion starts.
enum Question {
    /// The input came from the internet: Office opens it without Protected View.
    Internet(OfficeTask),
    /// The output file exists.
    Overwrite(OfficeTask),
}

/// The Office conversions of this window: those waiting, the one running, the questions and the
/// last failure.
#[derive(Default)]
pub struct OfficeState {
    /// Microsoft Office takes File ▸ Open of its files and Export ▸ Word in this window. The
    /// desktop app turns it on at startup ([`MigezApp::start_platform_services`]); a window
    /// made any other way (the tests, the web) keeps Pdfrp's own ways and never starts Office
    /// unasked.
    pub(crate) enabled: bool,
    queue: VecDeque<OfficeTask>,
    running: Option<Running>,
    questions: VecDeque<Question>,
    failure: Option<(OfficeTask, OfficeHatasi)>,
    /// Tabs converted from a file with an internet mark, and that file: saving such a tab gives
    /// the saved PDF the same mark.
    zone_sources: Vec<(DocId, PathBuf)>,
}

impl OfficeState {
    /// A question or a failure is showing: it is a dialog over the window.
    pub(crate) fn asks(&self) -> bool {
        !self.questions.is_empty() || self.failure.is_some()
    }

    /// The file a tab was converted from, when that file came from the internet.
    pub(crate) fn zone_source(&self, doc: DocId) -> Option<&Path> {
        self.zone_sources.iter().find(|(id, _)| *id == doc).map(|(_, path)| path.as_path())
    }

    /// Whether `input` is already being converted to PDF or waits to be (one file, one tab).
    fn converting(&self, input: &Path) -> bool {
        let same = |task: &OfficeTask| matches!(&task.kind, TaskKind::ToPdf { input: i, .. } if crate::same_file(&i.to_string_lossy(), &input.to_string_lossy()));
        self.running.as_ref().is_some_and(|r| same(&r.task))
            || self.queue.iter().any(same)
            || self.questions.iter().any(|q| match q {
                Question::Internet(t) | Question::Overwrite(t) => same(t),
            })
    }
}

/// `path`'s file name, or the whole path when it has none.
fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.to_string_lossy().into_owned(), |n| n.to_string_lossy().into_owned())
}

/// Whether `path` came from the internet: its Zone.Identifier names the Internet zone (3) or
/// Restricted sites (4). Windows only; a file without a mark (or another platform) is local.
pub(crate) fn from_internet(path: &Path) -> bool {
    #[cfg(windows)]
    {
        migez_platform::zone::read_zone(path).is_some_and(|stream| zone_id(&stream).is_some_and(|id| id >= 3))
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        false
    }
}

/// The `ZoneId` of a Zone.Identifier stream, when it names one.
#[cfg_attr(not(windows), allow(dead_code))]
fn zone_id(stream: &[u8]) -> Option<u32> {
    String::from_utf8_lossy(stream).lines().find_map(|line| {
        let (key, value) = line.split_once('=')?;
        key.trim().eq_ignore_ascii_case("ZoneId").then(|| value.trim().parse().ok()).flatten()
    })
}

/// Give `output` the internet mark of `source` (best effort: a volume without alternate data
/// streams can't carry it). Only a source that has a mark passes one on.
fn carry_mark(output: &Path, source: Option<&Path>) {
    #[cfg(windows)]
    if let Some(source) = source.filter(|s| migez_platform::zone::read_zone(s).is_some()) {
        let _ = migez_platform::zone::mark_from(output, Some(source));
    }
    #[cfg(not(windows))]
    let _ = (output, source);
}

/// A folder of this conversion's own under the temporary folder; what it made there is removed,
/// file by file, when it is dropped.
struct WorkDir {
    dir: PathBuf,
    files: Vec<PathBuf>,
}

impl WorkDir {
    fn new() -> Result<WorkDir, OfficeHatasi> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let dir = std::env::temp_dir().join(format!("migez-office-ui-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        std::fs::create_dir_all(&dir).map_err(|e| OfficeHatasi::Ic { ayrinti: e.to_string() })?;
        Ok(WorkDir { dir, files: Vec::new() })
    }

    /// A path in the folder, removed with it.
    fn file(&mut self, name: &str) -> PathBuf {
        let path = self.dir.join(name);
        self.files.push(path.clone());
        path
    }
}

impl Drop for WorkDir {
    fn drop(&mut self) {
        for file in &self.files {
            let _ = std::fs::remove_file(file);
        }
        // Only an empty folder goes: anything else in it is left alone.
        let _ = std::fs::remove_dir(&self.dir);
    }
}

/// Run one conversion (on the worker thread).
fn convert(task: &OfficeTask) -> Result<Outcome, OfficeHatasi> {
    let mut work = WorkDir::new()?;
    match &task.kind {
        TaskKind::ToPdf { input, .. } => {
            let out = work.file("donusum.pdf");
            ms_office::to_pdf(input, &out)?;
            let too_big = std::fs::metadata(&out).map(|m| m.len() > MAX_PDF).unwrap_or(false);
            if too_big {
                return Err(OfficeHatasi::Ic { ayrinti: format!("the PDF Office made is larger than {} MB", MAX_PDF / (1024 * 1024)) });
            }
            let bytes = std::fs::read(&out).map_err(|e| OfficeHatasi::Ic { ayrinti: e.to_string() })?;
            Ok(Outcome::Pdf(Arc::new(bytes)))
        }
        TaskKind::ToWord { pdf, output, .. } => {
            let source = work.file("belge.pdf");
            std::fs::write(&source, pdf.as_slice()).map_err(|e| OfficeHatasi::Ic { ayrinti: e.to_string() })?;
            ms_office::pdf_to_word(&source, output)?;
            Ok(Outcome::Word(output.clone()))
        }
    }
}

impl MigezApp {
    /// Pdfrp, once at startup, before the saved preferences are restored: find out which
    /// Microsoft Office apps are installed on a thread of its own and let them convert in this
    /// window, and start Recognize Text with Windows' own recognition and Turkish where Windows
    /// reads Turkish.
    pub fn start_platform_services(&mut self) {
        ms_office::read_status_in_background();
        self.office.enabled = true;
        self.ocr_draft.prefer(migez_engine::ocr::turkish_default());
    }

    /// The Office app that converts `path` to PDF in this window ([`ms_office::converts_to_pdf`]),
    /// when Office is in use here ([`OfficeState::enabled`]).
    pub(crate) fn office_app_for(&self, path: &Path) -> Option<Uygulama> {
        if !self.office.enabled {
            return None;
        }
        ms_office::converts_to_pdf(path)
    }

    /// Whether Export ▸ Word goes to Microsoft Word in this window: Office is in use here and Word
    /// is installed.
    pub(crate) fn office_exports_word(&self) -> bool {
        self.office.enabled && ms_office::converts_pdf_to_word()
    }

    /// Pdfrp: why the Word, Excel or PowerPoint file at `path` can't become a PDF here, when it
    /// can't: neither the Office app that converts it nor LibreOffice is installed. `None` for any
    /// other file, or when one of them is there (the conversion then says what went wrong, if
    /// anything does). Said before converting, in the interface language, instead of the
    /// converter's English error.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn no_office_converter(&self, path: &Path) -> Option<String> {
        let ext = path.extension()?.to_string_lossy().to_ascii_lowercase();
        if !migez_engine::OFFICE_EXTENSIONS.contains(&ext.as_str()) || self.office_app_for(path).is_some() || migez_engine::libreoffice().is_some() {
            return None;
        }
        let app = ms_office::uygulama_sec(path).map_or("Microsoft Office", Uygulama::ad);
        Some(crate::i18n::fmt(
            tl!("{name} can't be converted to PDF: that needs {app} or LibreOffice, and neither is installed on this computer."),
            &[("name", &file_name(path)), ("app", app)],
        ))
    }

    /// File ▸ Open of `input`, a file Office converts here ([`ms_office::converts_to_pdf`]):
    /// convert it with `app` in the background and open the PDF as a new tab, where `then` runs.
    pub(crate) fn office_to_pdf(&mut self, input: PathBuf, app: Uygulama, then: Option<&'static str>) {
        if self.office.converting(&input) {
            return;
        }
        self.submit_office(OfficeTask::new(TaskKind::ToPdf { input, app, then }));
    }

    /// Export a PDF ▸ Microsoft Word with Word installed: ask where (`save_override` in tests),
    /// then have Word convert the document as it is then. The save dialog asks before replacing a
    /// file; a path given another way is asked about here.
    pub(crate) fn office_word_export(&mut self, id: DocId) {
        let Some(doc) = self.session.get(id) else { return };
        let stem = doc.name.trim_end_matches(".pdf").trim_end_matches(".PDF").to_string();
        let folder = doc.path.clone();
        let export = move |app: &mut Self, output: PathBuf, asked: bool| {
            // What was typed while the save panel was open is part of the document (#166).
            if !app.commit_typing_in(id) {
                return;
            }
            let Some(doc) = app.session.get(id) else { return };
            let name = doc.name.clone();
            let zone = doc.path.clone().map(PathBuf::from).filter(|p| from_internet(p)).or_else(|| app.office.zone_source(id).map(Path::to_path_buf));
            match app.session.save_bytes(id) {
                Ok(pdf) => {
                    let mut task = OfficeTask::new(TaskKind::ToWord { pdf, name, output, zone });
                    task.overwrite_ok = asked;
                    app.submit_office(task);
                }
                Err(e) => app.notify_fmt("Couldn't write {name}: {e}", &[("name", &name), ("e", &e.to_string())]),
            }
        };
        #[cfg(not(target_arch = "wasm32"))]
        match self.save_override.clone() {
            Some(p) => export(self, p.into(), false),
            None => {
                let dialog = rfd::AsyncFileDialog::new()
                    .set_title(tl!("Export").to_string())
                    .add_filter_any_case("DOCX", &["docx"])
                    .set_file_name(format!("{stem}.docx"));
                let dialog = crate::editing::in_document_folder(dialog, folder.as_deref());
                // Windows' save dialog asks before it hands back a file that exists.
                self.ask_one(crate::pickers::Ask::Save(dialog), None, move |app, path| export(app, path, cfg!(windows)));
            }
        }
        #[cfg(target_arch = "wasm32")]
        let _ = (export, stem, folder);
    }

    /// Queue `task`, asking first what it needs asked.
    fn submit_office(&mut self, task: OfficeTask) {
        if !task.internet_ok && task.zone_source().is_some_and(from_internet) {
            self.office.questions.push_back(Question::Internet(task));
            return;
        }
        if !task.overwrite_ok && task.output().is_some_and(Path::exists) {
            self.office.questions.push_back(Question::Overwrite(task));
            return;
        }
        self.office.queue.push_back(task);
        self.start_next_office();
    }

    /// Start the next conversion when none is running.
    fn start_next_office(&mut self) {
        if self.office.running.is_some() {
            return;
        }
        let Some(task) = self.office.queue.pop_front() else { return };
        let result: Slot = Arc::new(Mutex::new(None));
        let slot = result.clone();
        let job = task.clone();
        let ctx = self.ctx.clone();
        let work = move || {
            // A panic in the conversion is its error: the queue goes on.
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| convert(&job)))
                .unwrap_or_else(|_| Err(OfficeHatasi::Ic { ayrinti: "the conversion stopped unexpectedly".into() }));
            if let Ok(mut s) = slot.lock() {
                *s = Some(outcome);
            }
            if let Some(ctx) = ctx {
                ctx.request_repaint();
            }
        };
        if self.run_inline {
            work();
        } else if let Err(e) = std::thread::Builder::new().name("migez-office".into()).spawn(work)
            && let Ok(mut s) = result.lock()
        {
            *s = Some(Err(OfficeHatasi::Ic { ayrinti: e.to_string() }));
        }
        self.office.running = Some(Running { task, started: Instant::now(), result });
    }

    /// Every frame: take a finished conversion's result and start the next.
    pub(crate) fn poll_office(&mut self) {
        let Some(running) = &self.office.running else {
            self.start_next_office();
            return;
        };
        let finished = match running.result.lock() {
            Ok(mut slot) => slot.take(),
            // The worker stopped while holding the slot: it will never answer.
            Err(_) => Some(Err(OfficeHatasi::Ic { ayrinti: "the conversion stopped unexpectedly".into() })),
        };
        let Some(result) = finished else {
            if let Some(ctx) = &self.ctx {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            return;
        };
        if let Some(Running { task, .. }) = self.office.running.take() {
            self.finish_office(task, result);
        }
        self.start_next_office();
    }

    /// A conversion is over: open or report its result, or keep its failure to show.
    fn finish_office(&mut self, task: OfficeTask, result: Result<Outcome, OfficeHatasi>) {
        let app = task.app().ad();
        let outcome = match result {
            Ok(outcome) => outcome,
            Err(error) => {
                self.office.failure = Some((task, error));
                return;
            }
        };
        match (outcome, &task.kind) {
            (Outcome::Pdf(bytes), TaskKind::ToPdf { input, then, .. }) => {
                let name = file_name(input);
                let before = self.views.len();
                match self.open_converted_pdf(&name, Ok(bytes)) {
                    Ok(()) => {
                        if let Some(view) = self.views.get(before) {
                            let id = view.id;
                            if from_internet(input) {
                                self.office.zone_sources.push((id, input.clone()));
                            }
                            // Save proposes the source file's folder.
                            self.converted_from.insert(id, input.to_string_lossy().into_owned());
                        }
                        self.notify(crate::i18n::fmt(tl!("Converted {name} to PDF with {app}"), &[("name", &name), ("app", app)]));
                        if let Some(command) = then {
                            self.execute(command);
                        }
                    }
                    Err(e) => self.notify_fmt("Couldn't open {name}: {e}", &[("name", &name), ("e", &e)]),
                }
            }
            (Outcome::Word(path), _) => {
                carry_mark(&path, task.zone_source());
                self.notify_fmt("Exported to {path}", &[("path", &path.display().to_string())]);
            }
            (Outcome::Pdf(_), TaskKind::ToWord { .. }) => {
                self.notify_error(OfficeHatasi::Ic { ayrinti: "Office returned another kind of file".into() })
            }
        }
    }

    /// Save As of a tab converted from a file from the internet: the saved PDF keeps its mark.
    pub(crate) fn office_mark_saved(&self, id: DocId, dest: &str) {
        carry_mark(Path::new(dest), self.office.zone_source(id));
    }
}

/// Draw the Office card, question and failure (called every frame with the progress notices).
pub(crate) fn show(app: &mut MigezApp, ctx: &egui::Context) {
    progress_card(app, ctx);
    question(app, ctx);
    failure(app, ctx);
}

/// The card at the bottom while a conversion runs: which file, with which app, for how long, and
/// how many wait. Above the other progress notice, so the two never cover each other.
fn progress_card(app: &MigezApp, ctx: &egui::Context) {
    let Some(running) = &app.office.running else { return };
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let seconds = running.started.elapsed().as_secs().to_string();
    let label = crate::i18n::fmt(tl!("Converting {name} with {app}…"), &[("name", &running.task.file_name()), ("app", running.task.app().ad())]);
    let waiting = app.office.queue.len();
    egui::Area::new(egui::Id::unique("office-progress"))
        .order(egui::Order::Foreground)
        .pivot(egui::Align2::CENTER_BOTTOM)
        .fixed_pos(screen.center_bottom() - egui::vec2(0.0, 104.0))
        .show(ctx, |ui| {
            egui::Frame::NONE
                .fill(t.card)
                .stroke(egui::Stroke::new(1.0, t.border))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(16, 12))
                .shadow(ctx.global_style().visuals.popup_shadow)
                .show(ui, |ui| {
                    ui.set_max_width(420.0_f32.min(screen.width() - 64.0).max(200.0));
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new().size(16.0).color(t.accent));
                        ui.add(egui::Label::new(egui::RichText::new(label).color(t.text)).wrap());
                    });
                    let mut detail = crate::i18n::fmt(tl!("{s} s"), &[("s", &seconds)]);
                    if waiting > 0 {
                        detail.push_str(" · ");
                        detail.push_str(&crate::i18n::fmt(tl!("{n} waiting"), &[("n", &waiting.to_string())]));
                    }
                    ui.label(egui::RichText::new(detail).small().color(t.text_muted));
                });
        });
}

/// The question at the front: convert a file from the internet? Replace the output?
fn question(app: &mut MigezApp, ctx: &egui::Context) {
    let Some(front) = app.office.questions.front() else { return };
    let t = Tokens::get(ctx);
    let (title, body, go) = match front {
        Question::Internet(task) => (
            tl!("This file came from the internet"),
            crate::i18n::fmt(
                tl!(
                    "{name} came from the internet. {app} opens it to convert it without Protected View (its macros stay off). Convert it only if you trust where it came from."
                ),
                &[("name", &task.file_name()), ("app", task.app().ad())],
            ),
            tl!("Convert"),
        ),
        Question::Overwrite(task) => (
            tl!("Replace the file?"),
            crate::i18n::fmt(
                tl!("{name} already exists. Replace it with the converted document?"),
                &[("name", &task.output().map(|p| p.display().to_string()).unwrap_or_default())],
            ),
            tl!("Replace"),
        ),
    };
    let mut answer: Option<bool> = None;
    let modal = widgets::show_modal(ctx, "office_question", |ui| {
        ui.set_width(460.0);
        ui.horizontal(|ui| {
            ui.add(crate::icons::image("triangle-alert", 22.0, t.accent));
            ui.label(egui::RichText::new(title).font(theme::semibold(16.0)));
        });
        ui.add_space(6.0);
        ui.add(egui::Label::new(egui::RichText::new(body).color(t.text)).wrap());
        ui.add_space(14.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill_button(ui, tl!("Cancel"), true).clicked() {
                answer = Some(false);
            }
            if widgets::pill_button(ui, go, false).clicked() {
                answer = Some(true);
            }
        });
    });
    if answer.is_none() && modal.should_close() {
        answer = Some(false);
    }
    let Some(yes) = answer else { return };
    let Some(question) = app.office.questions.pop_front() else { return };
    if !yes {
        return;
    }
    let task = match question {
        Question::Internet(mut task) => {
            task.internet_ok = true;
            task
        }
        Question::Overwrite(mut task) => {
            task.overwrite_ok = true;
            task
        }
    };
    app.submit_office(task);
}

/// A failed conversion: Office's reason in Turkish (`OfficeHatasi`'s own text), and "Try again"
/// when it ran out of time or found Office busy.
fn failure(app: &mut MigezApp, ctx: &egui::Context) {
    // One dialog at a time: a question first.
    if !app.office.questions.is_empty() {
        return;
    }
    let Some((task, error)) = &app.office.failure else { return };
    let t = Tokens::get(ctx);
    let retry = ms_office::retryable(error);
    let title = crate::i18n::fmt(tl!("Couldn't convert {name}"), &[("name", &task.file_name())]);
    let message = error.to_string();
    let mut choice: Option<bool> = None;
    let modal = widgets::show_modal(ctx, "office_failure", |ui| {
        ui.set_width(460.0);
        ui.horizontal(|ui| {
            ui.add(crate::icons::image("circle-x", 22.0, t.accent));
            ui.label(egui::RichText::new(title).font(theme::semibold(16.0)));
        });
        ui.add_space(6.0);
        ui.add(egui::Label::new(egui::RichText::new(message).color(t.text)).wrap().selectable(true));
        ui.add_space(14.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if widgets::pill_button(ui, tl!("Close"), !retry).clicked() {
                choice = Some(false);
            }
            if retry && widgets::pill_button(ui, tl!("Try again"), true).clicked() {
                choice = Some(true);
            }
        });
    });
    if choice.is_none() && modal.should_close() {
        choice = Some(false);
    }
    let Some(again) = choice else { return };
    if let Some((task, _)) = app.office.failure.take()
        && again
    {
        // Asked once already: the answers stand.
        app.office.queue.push_back(task);
        app.start_next_office();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word_task(output: PathBuf) -> OfficeTask {
        OfficeTask::new(TaskKind::ToWord { pdf: Arc::new(b"%PDF-1.7".to_vec()), name: "a.pdf".into(), output, zone: None })
    }

    /// A window made in a test (or on the web) never hands a file to Office, whatever is
    /// installed; the desktop app's startup turns Office on, and then only an installed app
    /// takes its files.
    #[test]
    fn office_is_used_only_once_the_desktop_app_turns_it_on() {
        let mut app = MigezApp::new();
        assert!(app.office_app_for(Path::new("Rapor.docx")).is_none());
        assert!(!app.office_exports_word());
        app.office.enabled = true;
        let installed = ms_office::status();
        assert_eq!(app.office_app_for(Path::new("Rapor.docx")).is_some(), installed.word);
        assert_eq!(app.office_app_for(Path::new("Sunum.pptx")).is_some(), installed.powerpoint);
        assert!(app.office_app_for(Path::new("not.txt")).is_none(), "plain text stays the app's own");
        assert_eq!(app.office_exports_word(), installed.word);
    }

    #[test]
    fn a_zone_stream_names_its_zone() {
        assert_eq!(zone_id(b"[ZoneTransfer]\r\nZoneId=3\r\nHostUrl=https://example.test/\r\n"), Some(3));
        assert_eq!(zone_id(b"[ZoneTransfer]\nzoneid = 4\n"), Some(4));
        assert_eq!(zone_id(b"[ZoneTransfer]\r\nZoneId=x\r\n"), None);
        assert_eq!(zone_id(b""), None);
    }

    /// A fresh folder in the temporary folder for one test.
    fn test_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("migez-office-ui-test-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("fixture");
        dir
    }

    /// An output that exists is asked about before anything runs; agreeing queues it, and an
    /// output the save dialog asked about already isn't asked about again.
    #[test]
    fn an_existing_output_is_asked_about_before_it_is_replaced() {
        let dir = test_dir("asked");
        let output = dir.join("Rapor.docx");
        std::fs::write(&output, b"PK").expect("fixture");
        let mut app = MigezApp::new();
        app.submit_office(word_task(output.clone()));
        assert!(app.office.asks() && app.office.running.is_none() && app.office.queue.is_empty(), "asked, nothing started");
        assert!(matches!(app.office.questions.front(), Some(Question::Overwrite(_))));
        assert!(app.modal_open(), "the question is a dialog over the window");
        let mut asked = word_task(output.clone());
        asked.overwrite_ok = true;
        let mut other = MigezApp::new();
        other.office.running = Some(Running { task: word_task(output.clone()), started: Instant::now(), result: Arc::new(Mutex::new(None)) });
        other.submit_office(asked);
        assert!(!other.office.asks() && other.office.queue.len() == 1, "asked by the save dialog: queued behind the running one");
        assert_eq!(std::fs::read(&output).expect("fixture"), b"PK", "nothing was replaced");
        let _ = std::fs::remove_file(&output);
        let _ = std::fs::remove_dir(&dir);
    }

    /// End to end with the real Word: File ▸ Open of a Word file converts it with Word and opens the
    /// PDF as a new, unsaved tab named after it, with the Turkish text in it. Starts Word unseen.
    #[test]
    #[ignore = "needs Microsoft Word, which it starts unseen"]
    fn ms_office_opening_a_word_file_converts_it_into_a_new_tab() {
        if !ms_office::status().word {
            eprintln!("skipped: Microsoft Word is not installed here");
            return;
        }
        let dir = test_dir("open");
        // A Word file, written by the app's own Word export from a Turkish text page.
        let mut app = MigezApp::new();
        app.run_inline = true;
        // As the desktop app does at startup; a window made in a test leaves Office alone.
        app.office.enabled = true;
        let pdf = migez_engine::Session::new().create_from_text("Mektup", "Öğrenci Güneş Çığ Şişe").expect("fixture");
        app.open_bytes("kaynak.pdf", None, pdf.to_vec()).expect("fixture");
        let source = app.active_ids().expect("fixture").1;
        let docx = app.session.get(source).expect("fixture").export_office(migez_engine::compare::OfficeFormat::Docx);
        let input = dir.join("Mektup.docx");
        std::fs::write(&input, docx.as_slice()).expect("fixture");

        let before = app.views.len();
        app.open_path(&input.to_string_lossy());
        assert!(app.office.running.is_some(), "Word took the file");
        app.poll_office();
        assert!(app.office.failure.is_none(), "{:?}", app.office.failure.as_ref().map(|(_, e)| e.to_string()));
        assert_eq!(app.views.len(), before + 1, "a new tab");
        let doc = app.session.get(app.views[before].id).expect("the converted document");
        assert_eq!((doc.name.as_str(), doc.path.as_deref()), ("Mektup.pdf", None), "named after the file, not saved");
        assert!(!doc.info.pages.is_empty());
        let toast = app.toast.clone().map(|t| t.0).unwrap_or_default();
        assert!(toast.contains("Mektup.docx") && toast.contains("Microsoft Word"), "{toast}");
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_dir(&dir);
    }

    /// A finished conversion that failed keeps its error to show, with "Try again" only for a
    /// timeout or a busy Office; one that ran out of time goes back in the queue on retry.
    #[test]
    fn a_failure_is_kept_and_only_a_timeout_or_a_busy_office_is_retried() {
        let mut app = MigezApp::new();
        let task = word_task(std::env::temp_dir().join("migez-office-ui-test-never-written.docx"));
        app.office.running =
            Some(Running { task, started: Instant::now(), result: Arc::new(Mutex::new(Some(Err(OfficeHatasi::ZamanAsimi { saniye: 300 })))) });
        app.poll_office();
        assert!(app.office.running.is_none(), "the run is over");
        let (_, error) = app.office.failure.as_ref().expect("its failure is kept");
        assert!(ms_office::retryable(error));
        assert!(error.to_string().contains("300 saniyede bitmedi"), "{error}");
        assert!(app.modal_open());
    }
}
