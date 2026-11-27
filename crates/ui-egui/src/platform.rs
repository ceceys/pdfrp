pub(super) mod drag_pointer;
pub(super) mod file_filter;
#[cfg(not(target_arch = "wasm32"))]
pub mod file_watch;
#[cfg(target_os = "macos")]
pub(super) mod macos_association;
pub mod motion;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod open_worker;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod pickers;
pub mod portable;
#[cfg(not(target_arch = "wasm32"))]
pub(super) mod system_fonts;
pub mod ui_scale;
// Pdfrp 1.1.0: Help ▸ Check for updates… and the check at start, its own (not upstream's).
#[cfg(not(target_arch = "wasm32"))]
pub mod updates;
// Pdfrp 1.2.0: offline translation (Edit ▸ Translate…, Preferences ▸ Language modules).
#[cfg(not(target_arch = "wasm32"))]
pub mod translate;
pub mod window_controls;
