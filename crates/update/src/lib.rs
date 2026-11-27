//! migez-update — Pdfrp's update check (1.1.0).
//!
//! Asks GitHub for the latest release of the public repository (`ceceys/pdfrp`), compares
//! its tag (`v1.2.0`) with the running version, and for a newer one downloads the installer
//! (`Pdfrp Kurulum 1.2.0.exe`) to %TEMP% and checks its SHA-256 against the release's
//! `SHA256SUMS.txt` before anything starts it.
//!
//! What leaves the computer: HTTPS requests to `api.github.com` (the release), `github.com` (only
//! this repository's release downloads, [`fetch::DOWNLOAD_PATH`]) and GitHub's file hosts (the two
//! downloads), carrying a `User-Agent: Pdfrp/<version>` header and nothing else of the app's:
//! no identifier, no settings, no document data. Addresses outside those hosts and that path,
//! plain HTTP, and redirects elsewhere are refused ([`fetch::allowed`]).
//!
//! Pdfrp 1.2.0: the language modules of offline translation download through the same client
//! ([`github_transport`]), from this repository's `models-v3.0` release, with the same User-Agent.
//!
//! The installer is never started silently: its wizard shows and the user starts the installation
//! ([`INSTALLER_ARGS`]).
//!
//! No UI here (L7, below the egui shell); `migez-ui-egui` runs it on its own thread.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod fetch;
#[cfg(windows)]
mod https;
mod install;
mod release;
mod version;

#[cfg(test)]
mod tests;

pub use fetch::{ALLOWED_HOSTS, DOWNLOAD_PATH, FetchError, FileTransport, LATEST_URL, OVERRIDE_ENV, Source, Transport, allowed, github_transport};
pub use install::{
    DOWNLOAD_PREFIX, INSTALLER_ARGS, InstallError, Verified, clean_old_downloads, download, installer_args, is_download_folder, sha256_hex,
};
pub use release::{
    Asset, CheckError, Latest, Outcome, Release, SUMS_NAME, check, expected_sha256, installer_name, parse_latest, parse_sums, same_name,
};
pub use version::{Identifier, Version};

/// The User-Agent the requests carry: `Pdfrp/1.1.0`. GitHub's API refuses requests without one.
pub fn user_agent(version: &str) -> String {
    format!("Pdfrp/{version}")
}

/// Whether an automatic check is due: never checked, a day (86 400 s) or more since `last`, or the
/// clock now reads earlier than `last` (it was set back). Both are seconds since 1970.
pub fn due(last: Option<u64>, now: u64) -> bool {
    const DAY: u64 = 24 * 60 * 60;
    match last {
        None => true,
        Some(last) => now < last || now - last >= DAY,
    }
}

/// Seconds since 1970 now (0 if the clock reads earlier).
pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}
