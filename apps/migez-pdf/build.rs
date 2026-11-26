//! Windows only: embed the app icon and version info (VERSIONINFO) into `migez-pdf.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab. The installer gives .pdf files the
//! same icon (`"Pdfrp.exe",0`).
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `MIGEZ_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

#[path = "src/windows_manifest.rs"]
mod windows_manifest;

/// The in-box DLLs only printing uses (Windows.Data.Pdf draws the job, prntvpt reads the
/// driver's print capabilities). Trimmed Windows installations (NTLite) can lack them, and a
/// load-time import of a missing DLL stops the program before `main` (#2630). Delay-loaded, they
/// load on the first print; `migez_print::spool` checks they exist before calling in.
/// Same list in `apps/migez-cli/build.rs`, `crates/print/build.rs` and `crates/print/src/spool.rs`.
const DELAY_LOADED: [&str; 2] = ["windows.data.pdf.dll", "prntvpt.dll"];

/// Pdfrp's version (`migez_engine::build_info::PRODUCT_VERSION`, which since 1.1.0 is the
/// workspace version cargo gives this script): the string, and the fixed part (major << 48 |
/// minor << 32 | patch << 16). `apps/migez-cli/build.rs` does the same, and its test checks it.
fn product_version() -> (String, u64) {
    let part = |name: &str| std::env::var(name).ok().and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) & 0xFFFF;
    let number = (part("CARGO_PKG_VERSION_MAJOR") << 48) | (part("CARGO_PKG_VERSION_MINOR") << 32) | (part("CARGO_PKG_VERSION_PATCH") << 16);
    (std::env::var("CARGO_PKG_VERSION").unwrap_or_default(), number)
}

fn main() {
    let (product_version, product_version_number) = product_version();
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/pdfrp/pdfrp.ico");
    println!("cargo:rerun-if-changed=src/windows_manifest.rs");
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
    // Pdfrp: one icon (the exe, the title bar, Explorer, and .pdf files through the
    // installer); upstream's separate PDF file icon is gone (1.0.1). The product name is written
    // out: a build script can't reach `migez_engine::build_info`. FileDescription is what Task
    // Manager lists the process as. Explorer's Details tab names Pdfrp alone: the licences of
    // what it is built on are the texts in the installation's `lisanslar` folder. The installer
    // names the file "Pdfrp.exe".
    res.set_icon("../../assets/pdfrp/pdfrp.ico")
        .set_manifest(windows_manifest::WINDOWS_MANIFEST)
        .set("ProductName", "Pdfrp")
        .set("FileDescription", "Pdfrp")
        .set("CompanyName", "Friday")
        .set("LegalCopyright", "Open-source licences: the lisanslar folder of the installation.")
        .set("OriginalFilename", "Pdfrp.exe")
        .set("InternalName", "Pdfrp")
        // Pdfrp's own version (`migez_engine::build_info::PRODUCT_VERSION`), as the product
        // and as the file's version.
        .set("ProductVersion", &product_version)
        .set_version_info(winresource::VersionInfo::PRODUCTVERSION, product_version_number)
        .set("FileVersion", &product_version)
        .set_version_info(winresource::VersionInfo::FILEVERSION, product_version_number);
    if let Err(e) = res.compile() {
        if std::env::var_os("MIGEZ_REQUIRE_WINRES").is_some() {
            println!("cargo::error=embedding Windows resources failed: {e}");
            return;
        }
        println!("cargo:warning=migez-pdf.exe built without icon/version resources: {e}");
    }
}
