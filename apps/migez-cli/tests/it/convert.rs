//! Pdfrp: `convert` makes a new PDF of a UDF file, and says first, on standard error, what the
//! PDF is not. The files are the synthetic ones of the engine's tests (made-up text only).

use std::path::PathBuf;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_migez-cli");

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../crates/engine/tests/data/udf").join(name)
}

#[test]
fn convert_writes_a_pdf_of_a_udf_file_and_warns_of_its_signature_first() {
    let dir = std::env::temp_dir().join(format!("migez-cli-convert-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    for (file, signed) in [("01-dilekce-iki-yana-imzali.udf", true), ("02-dar-kenar-listeler.udf", false)] {
        let out = dir.join(file.replace(".udf", ".pdf"));
        let output = Command::new(BIN).arg("convert").arg(fixture(file)).arg("--out").arg(&out).output().unwrap();
        let err = String::from_utf8_lossy(&output.stderr);
        assert!(output.status.success(), "{err}");
        assert!(std::fs::read(&out).unwrap().starts_with(b"%PDF-"));
        let first = err.lines().next().unwrap_or_default();
        assert!(first.contains("The appearance may differ from how it looks in the UYAP Document Editor."), "{err}");
        assert_eq!(first.contains("Pdfrp does not verify the signature and does not carry it over to the PDF"), signed, "{err}");
        let text = Command::new(BIN).arg("text").arg(&out).output().unwrap();
        assert!(String::from_utf8_lossy(&text.stdout).contains("ÇĞİÖŞÜçğıöşü"), "{file}: the Turkish letters read back");
    }
    std::fs::remove_dir_all(dir).unwrap();
}
