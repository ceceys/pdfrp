//! Names that earlier copies used for the preferences key and the per-user folders: Mig-EZ Pdf
//! 1.0.0 and 1.0.1 (the program's name before Pdfrp), the trial copies before it (under the
//! upstream internal name), and PrintCraft before that. Read only, never written: an upgrade picks
//! up preferences, recent files and unsaved work once. This is the one place the app keeps the old
//! names (with `migez_ui_egui::portable::FORMER_DATA_DIR` for a portable copy, and the installer's
//! clean-up in `kurulum/pdfrp.iss`); drop an entry once no copy older than 1.1.0 matters.

/// `app.ron` keys of earlier copies, newest first: 1.0.0 and its trial copies, then PrintCraft.
/// (1.0.1 already used `migez_ui_egui::STORAGE_KEY`.)
pub const STORAGE_KEYS: [&str; 2] = ["pdfcraft", "printcraft"];

/// The folder name of Mig-EZ Pdf 1.0.0 and 1.0.1 under %APPDATA% and %LOCALAPPDATA%: copied to
/// [`crate::DATA_FOLDER`] once and left as it was (`migrate_legacy_folders`), so an upgrade keeps
/// preferences, recent files and unsaved work and nothing of the former copy is removed.
pub const COPIED_FOLDERS: [&str; 1] = ["Mig-EZ Pdf"];

/// Folder names of earlier copies under %APPDATA% and %LOCALAPPDATA%, newest first: the trial
/// copies before 1.0.0, then PrintCraft. Moved, not copied, when nothing newer is there.
pub const FOLDERS: [&str; 2] = ["PdfCraft", "PrintCraft"];
