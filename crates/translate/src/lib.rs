//! migez-translate — Pdfrp's offline translation (1.2.0), below the UI.
//!
//! - [`manifest`]: the language modules Pdfrp offers, embedded at build time
//!   (`models-v3.0.json`, made by `kurulum/ceviri-modelleri.ps1`): Mozilla's Firefox Translations
//!   models, republished unmodified in this repository's `models-v3.0` GitHub release, with
//!   Mozilla's own SHA-256 of each file. English and every other language have a module each way;
//!   two other languages are translated through English ([`manifest::Manifest::route`]).
//! - [`languages`]: the languages' names, our own table.
//! - [`store`]: where modules are installed (`%LOCALAPPDATA%\Pdfrp\models\<from>-<to>\<version>\`)
//!   and how: each file downloads to a `.part`, its SHA-256 is checked, it is unpacked (zstd), the
//!   unpacked file's SHA-256 is checked, and only then is the module moved into place.
//! - [`helper`]: the `pdfrp-ceviri` helper process that translates (fxtranslate): which build the
//!   processor can run, and its line-by-line JSON protocol.
//! - [`text`]: selected PDF text made ready to translate (lines of a paragraph joined, words
//!   divided at a line end mended, paragraph breaks kept).
//!
//! What leaves the computer: only the module downloads, through `migez_update`'s client
//! ([`migez_update::github_transport`]): HTTPS to this repository's release downloads on GitHub,
//! with the User-Agent `Pdfrp/<version>` and nothing else of the app's. Translation itself runs in
//! the helper, which has no network code; the text never leaves the computer.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod helper;
pub mod languages;
pub mod manifest;
pub mod store;
pub mod text;

#[cfg(test)]
mod tests;

pub use helper::{Helper, HelperError};
pub use manifest::{Direction, Manifest, ModelFile, RouteError};
pub use store::{InstallError, ModelStore, Source, Unpack};
