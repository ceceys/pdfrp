//! What this build calls itself. A local or patched build sets `MIGEZ_BUILD_LABEL` at compile
//! time (`MIGEZ_BUILD_LABEL=titlebar cargo build`, or through `packaging/linux/package.sh`), so
//! it can be told from the release of the same version. The version itself stays the release's.

/// The product name people see: the window title, the menus, Home, About, notices and the MCP
/// server's title. This is the one place it is written for the engine and everything above it
/// (`migez_ui_egui::APP_NAME` is [`NAME`]). Crate, binary and settings-folder names are
/// `migez-*`: they are internal ids. A macro, so `&'static str` tables can `concat!` it; use
/// [`NAME`] anywhere else.
///
/// The crates below the engine (sign, ocr, annot, redact, xfa) and the executables' `build.rs`
/// can't depend on this crate and write the name out; keep them in step when it changes.
#[macro_export]
macro_rules! product_name {
    () => {
        "Pdfrp"
    };
}

/// The product name ([`product_name!`]).
pub const NAME: &str = product_name!();

/// The crates' version from the workspace `Cargo.toml` (`[workspace.package] version`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Pdfrp's version, the one people see (About, `--version`, the executables' product and
/// file versions, the installer and the update check). Since 1.1.0 it is the crates' [`VERSION`]:
/// one number in the workspace `Cargo.toml`, which the executables' `build.rs` read too, and which
/// the installer (`kurulum/pdfrp.iss`) and the release tag (`vX.Y.Z`) repeat. 1.0.0: the
/// first office release (20.11.2026); 1.0.1: internal names (25.11.2026); 1.1.0: the update
/// check and the public repository (25.11.2026).
pub const PRODUCT_VERSION: &str = VERSION;

/// The compile-time build label, if any.
pub const LABEL: Option<&str> = option_env!("MIGEZ_BUILD_LABEL");

/// The crates' version as shown to people: `0.7.1`, or `0.7.1 (titlebar)` for a labelled build.
pub fn version() -> String {
    describe(VERSION, LABEL)
}

/// The product version as shown to people: `0.9.0`, or `0.9.0 (titlebar)` for a labelled build.
pub fn product_version() -> String {
    describe(PRODUCT_VERSION, LABEL)
}

fn describe(version: &str, label: Option<&str>) -> String {
    match label.map(str::trim).filter(|l| !l.is_empty()) {
        Some(label) => format!("{version} ({label})"),
        None => version.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::{PRODUCT_VERSION, VERSION, describe};

    #[test]
    fn a_label_follows_the_version_and_a_blank_one_is_ignored() {
        assert_eq!(describe("0.7.1", None), "0.7.1");
        assert_eq!(describe("0.7.1", Some("")), "0.7.1");
        assert_eq!(describe("0.7.1", Some("  ")), "0.7.1");
        assert_eq!(describe("0.7.1", Some("titlebar")), "0.7.1 (titlebar)");
    }

    /// Pdfrp's version is three numbers, and since 1.1.0 the crates' own: the installer, the
    /// About window and the update check read one number.
    #[test]
    fn the_product_version_is_pdfrps_own() {
        assert_eq!(PRODUCT_VERSION, "1.2.0");
        assert_eq!(PRODUCT_VERSION.split('.').filter(|n| n.parse::<u16>().is_ok()).count(), 3);
        assert_eq!(PRODUCT_VERSION, VERSION, "one version for the crates and the product");
    }
}
