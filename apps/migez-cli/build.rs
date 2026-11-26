//! Windows only: embed version info (VERSIONINFO) into `migez-cli.exe`, so Explorer's
//! Details tab, `(Get-Item migez-cli.exe).VersionInfo` and inventory tools can read the version
//! and publisher, as they can for `migez-pdf.exe` (`apps/migez-pdf/build.rs`).
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `MIGEZ_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

/// The in-box DLLs only printing uses (Windows.Data.Pdf draws the job, prntvpt reads the
/// driver's print capabilities). Trimmed Windows installations (NTLite) can lack them, and a
/// load-time import of a missing DLL stops the program before `main` (#2630). Delay-loaded, they
/// load on the first print; `migez_print::spool` checks they exist before calling in.
/// Same list in `apps/migez-pdf/build.rs`, `crates/print/build.rs` and `crates/print/src/spool.rs`.
const DELAY_LOADED: [&str; 2] = ["windows.data.pdf.dll", "prntvpt.dll"];

/// Pdfrp's version (`migez_engine::build_info::PRODUCT_VERSION`, which since 1.1.0 is the
/// workspace version cargo gives this script; `tests/it/version_resource.rs` checks they agree):
/// the string, and the fixed part (major << 48 | minor << 32 | patch << 16). Same in
/// `apps/migez-pdf/build.rs`.
fn product_version() -> (String, u64) {
    let part = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) & 0xFFFF;
    let number = (part("CARGO_PKG_VERSION_MAJOR") << 48) | (part("CARGO_PKG_VERSION_MINOR") << 32) | (part("CARGO_PKG_VERSION_PATCH") << 16);
    (std::env::var("CARGO_PKG_VERSION").unwrap_or_default(), number)
}

fn main() {
    let (product_version, product_version_number) = product_version();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MIGEZ_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        for dll in DELAY_LOADED {
            println!("cargo:rustc-link-arg-bins=/DELAYLOAD:{dll}");
        }
        println!("cargo:rustc-link-arg-bins=delayimp.lib");
    }
    let mut res = winresource::WindowsResource::new();
    // The product name is written out: a build script can't reach `migez_engine::build_info`.
    // The copyright holders are named in LICENSE-MIT, LICENSE-APACHE and NOTICE, as for the app.
    res.set("ProductName", "Pdfrp")
        .set("FileDescription", "Pdfrp command-line tool")
        .set("LegalCopyright", "Open-source licences: LICENSE-MIT, LICENSE-APACHE and NOTICE.")
        .set("OriginalFilename", "migez-cli.exe")
        .set("InternalName", "migez-cli")
        // The file version is the crates' (winresource's default), the same number since 1.1.0.
        .set("ProductVersion", &product_version)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, product_version_number);
    if let Err(e) = res.compile() {
        if std::env::var_os("MIGEZ_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=migez-cli.exe built without version resources: {e}");
    }
}
