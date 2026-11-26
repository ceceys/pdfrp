//! Office'in kurulu olup olmadığı, kayıt defterinden: ProgID → CLSID → LocalServer32 → program.
//! Office başlatılmaz; COM gerekmez.

use std::path::{Path, PathBuf};

use windows_registry::CLASSES_ROOT;

use crate::{OfficeDurumu, Uygulama};

/// Uygulamanın otomasyon ProgID'si.
pub(crate) fn progid(uygulama: Uygulama) -> &'static str {
    match uygulama {
        Uygulama::Word => "Word.Application",
        Uygulama::Excel => "Excel.Application",
        Uygulama::PowerPoint => "PowerPoint.Application",
    }
}

pub(crate) fn durum() -> OfficeDurumu {
    OfficeDurumu { word: kurulu_mu(Uygulama::Word), excel: kurulu_mu(Uygulama::Excel), powerpoint: kurulu_mu(Uygulama::PowerPoint) }
}

/// `uygulama` kayıtlı mı: ProgID'nin bir CLSID'si, o CLSID'nin bir yerel sunucusu (LocalServer32)
/// var ve kayıtlı program (tam yolu yazılıysa) diskte duruyor.
pub(crate) fn kurulu_mu(uygulama: Uygulama) -> bool {
    let Ok(clsid) = CLASSES_ROOT.open(format!(r"{}\CLSID", progid(uygulama))).and_then(|anahtar| anahtar.get_string("")) else {
        return false;
    };
    let clsid = clsid.trim();
    if !gecerli_clsid(clsid) {
        return false;
    }
    // 64 bitlik süreç kayıt defterinin 64 bitlik görünümünü okur; 32 bitlik Office'in yerel sunucusu
    // yalnız WOW6432Node altında da kayıtlı olabilir.
    for kok in [r"CLSID", r"WOW6432Node\CLSID"] {
        if let Ok(anahtar) = CLASSES_ROOT.open(format!(r"{kok}\{clsid}\LocalServer32")) {
            let komut = anahtar.get_string("").unwrap_or_default();
            return program_yolu(&komut).is_none_or(|program| program.is_file());
        }
    }
    false
}

/// `{xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx}` biçiminde mi; kayıt yolu kurarken başka bir şey kullanılmaz.
fn gecerli_clsid(metin: &str) -> bool {
    let Some(ic) = metin.strip_prefix('{').and_then(|m| m.strip_suffix('}')) else { return false };
    let parcalar: Vec<&str> = ic.split('-').collect();
    let uzunluklar = [8, 4, 4, 4, 12];
    parcalar.len() == uzunluklar.len()
        && parcalar.iter().zip(uzunluklar).all(|(parca, uzunluk)| parca.len() == uzunluk && parca.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// LocalServer32 komutundaki programın tam yolu: `"C:\…\WINWORD.EXE" /Automation` ya da tırnaksız
/// `C:\…\WINWORD.EXE /Automation`. Tam yol okunamıyorsa `None`: o zaman COM'un kendisine güvenilir.
fn program_yolu(komut: &str) -> Option<PathBuf> {
    let komut = komut.trim();
    let yol = if let Some(tirnakli) = komut.strip_prefix('"') {
        tirnakli.split('"').next()?
    } else {
        // `.exe` ASCII: küçük harfe çevirmek bayt konumlarını değiştirmez.
        let son = komut.to_ascii_lowercase().find(".exe")?.checked_add(4)?;
        komut.get(..son)?
    };
    let yol = Path::new(yol);
    yol.is_absolute().then(|| yol.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn program_yolu_tirnakli_ve_tirnaksiz_okunur() {
        assert_eq!(
            program_yolu(r"C:\Program Files\Microsoft Office\Root\Office16\WINWORD.EXE /Automation"),
            Some(PathBuf::from(r"C:\Program Files\Microsoft Office\Root\Office16\WINWORD.EXE"))
        );
        assert_eq!(
            program_yolu(r#""C:\Program Files (x86)\Office\EXCEL.EXE" /automation"#),
            Some(PathBuf::from(r"C:\Program Files (x86)\Office\EXCEL.EXE"))
        );
        assert_eq!(program_yolu(r"  C:\Office\POWERPNT.exe"), Some(PathBuf::from(r"C:\Office\POWERPNT.exe")));
        // Tam yol yoksa ya da okunamıyorsa karar COM'a kalır.
        assert_eq!(program_yolu("WINWORD.EXE /Automation"), None);
        assert_eq!(program_yolu(""), None);
        assert_eq!(program_yolu("\""), None);
        assert_eq!(program_yolu(r"C:\Ofis\Çığ.EXE"), Some(PathBuf::from(r"C:\Ofis\Çığ.EXE")));
    }

    #[test]
    fn yalniz_clsid_bicimindeki_metin_kabul_edilir() {
        assert!(gecerli_clsid("{000209FF-0000-0000-C000-000000000046}"));
        assert!(gecerli_clsid("{91493441-5a91-11cf-8700-00aa0060263b}"));
        assert!(!gecerli_clsid("000209FF-0000-0000-C000-000000000046"));
        assert!(!gecerli_clsid(r"{000209FF-0000-0000-C000-000000000046}\..\..\Bir"));
        assert!(!gecerli_clsid("{000209FF-0000-0000-C000-00000000004G}"));
        assert!(!gecerli_clsid("{}"));
    }

    #[test]
    fn kayitsiz_uygulama_kurulu_sayilmaz() {
        // Kayıt defterinde olmayan bir ProgID'nin CLSID'si okunamaz.
        assert!(CLASSES_ROOT.open(r"MigEZPdf.YokBoyleBirUygulama\CLSID").is_err());
    }
}
