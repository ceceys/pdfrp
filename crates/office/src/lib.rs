//! migez-office: Word, Excel ve PowerPoint belgelerini bilgisayarda kurulu Microsoft Office ile
//! PDF'e, PDF'i de Word ile `.docx`'e çevirir. Yalnız Windows'ta çalışır (COM otomasyonu); başka
//! sistemlerde derlenir ama [`OfficeHatasi::Desteklenmiyor`] döner.
//!
//! - [`mevcut`]: hangi Office uygulamalarının kurulu olduğu. Kayıt defterine bakar, Office'i başlatmaz.
//! - [`pdfe_donustur`]: uzantıya göre Word, Excel ya da PowerPoint ile PDF.
//! - [`pdften_worde`]: Word'ün PDF açma özelliğiyle `.docx`.
//!
//! Her dönüştürme kendi STA iş parçacığında, görünmez bir Office ile çalışır; çağıran en çok verilen
//! süre kadar bekler. Belge geçici bir klasöre kopyalanır ve oradan salt okunur açılır, sonuç önce
//! geçici klasöre yazılır ve hedefe tek adımda taşınır. Kullanıcının açık belgelerine dokunulmaz:
//! yalnız bu paketin açtığı belge kapatılır, Office yalnız iş başlarken görünmüyorsa ve hiç açık
//! belgesi yoksa, iş bitince de yoksa kapatılır (README, "Kullanıcının belgelerini koruma").

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::path::Path;
use std::time::Duration;

mod hata;

pub use hata::OfficeHatasi;

#[cfg(windows)]
mod calistirici;
#[cfg(windows)]
mod com;
#[cfg(windows)]
mod dosya;
#[cfg(windows)]
mod islem;
#[cfg(windows)]
mod kayit;
#[cfg(all(test, windows))]
mod testler;

/// Word ile PDF'e çevrilen dosyaların uzantıları.
pub const WORD_UZANTILARI: &[&str] = &["doc", "docx", "rtf", "odt", "txt"];
/// Excel ile PDF'e çevrilen dosyaların uzantıları.
pub const EXCEL_UZANTILARI: &[&str] = &["xls", "xlsx", "ods", "csv"];
/// PowerPoint ile PDF'e çevrilen dosyaların uzantıları.
pub const POWERPOINT_UZANTILARI: &[&str] = &["ppt", "pptx", "odp"];

/// [`pdfe_donustur`]'ün bekleme süresi. LibreOffice ile dönüştürmenin süresiyle aynı.
pub const PDF_SURESI: Duration = Duration::from_secs(120);
/// [`pdften_worde`]'nin bekleme süresi: Word'ün PDF'i yeniden dizmesi uzun belgede dakikalar sürer.
pub const WORD_SURESI: Duration = Duration::from_secs(300);

/// Bir Microsoft Office uygulaması.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Uygulama {
    Word,
    Excel,
    PowerPoint,
}

impl Uygulama {
    /// Üç uygulama, bu sırayla.
    pub const HEPSI: [Uygulama; 3] = [Uygulama::Word, Uygulama::Excel, Uygulama::PowerPoint];

    /// Kullanıcıya gösterilen adı: "Microsoft Word" gibi.
    pub fn ad(self) -> &'static str {
        match self {
            Uygulama::Word => "Microsoft Word",
            Uygulama::Excel => "Microsoft Excel",
            Uygulama::PowerPoint => "Microsoft PowerPoint",
        }
    }

    /// Bu uygulamayla PDF'e çevrilen dosyaların uzantıları (küçük harf, noktasız).
    pub fn uzantilar(self) -> &'static [&'static str] {
        match self {
            Uygulama::Word => WORD_UZANTILARI,
            Uygulama::Excel => EXCEL_UZANTILARI,
            Uygulama::PowerPoint => POWERPOINT_UZANTILARI,
        }
    }
}

/// Hangi Office uygulamalarının kurulu olduğu ([`mevcut`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct OfficeDurumu {
    pub word: bool,
    pub excel: bool,
    pub powerpoint: bool,
}

impl OfficeDurumu {
    /// `uygulama` kurulu mu.
    pub fn var_mi(self, uygulama: Uygulama) -> bool {
        match uygulama {
            Uygulama::Word => self.word,
            Uygulama::Excel => self.excel,
            Uygulama::PowerPoint => self.powerpoint,
        }
    }

    /// En az bir Office uygulaması kurulu mu.
    pub fn herhangi(self) -> bool {
        self.word || self.excel || self.powerpoint
    }
}

/// `girdi`'yi PDF'e hangi uygulamanın çevireceği, uzantısına göre (büyük küçük harf fark etmez).
/// Uzantı tanınmıyorsa `None`.
pub fn uygulama_sec(girdi: &Path) -> Option<Uygulama> {
    let uzanti = girdi.extension()?.to_str()?;
    Uygulama::HEPSI.into_iter().find(|u| u.uzantilar().iter().any(|e| uzanti.eq_ignore_ascii_case(e)))
}

/// Kurulu Office uygulamaları. Kayıt defterindeki ProgID kayıtlarına (ve kayıtlı programın
/// diskte olup olmadığına) bakar; Office'i başlatmaz, hızlıdır. Windows dışında hepsi `false`.
pub fn mevcut() -> OfficeDurumu {
    #[cfg(windows)]
    {
        kayit::durum()
    }
    #[cfg(not(windows))]
    {
        OfficeDurumu::default()
    }
}

/// `girdi`'yi uzantısına göre Word, Excel ya da PowerPoint ile PDF'e çevirip `cikti`'ya yazar
/// ([`PDF_SURESI`] bekler). `cikti` varsa yerine yazılır; klasörü var olmalıdır.
///
/// Word: doc, docx, rtf, odt, txt. Excel: xls, xlsx, ods, csv (bütün sayfalar). PowerPoint: ppt,
/// pptx, odp (slaytlar).
pub fn pdfe_donustur(girdi: &Path, cikti: &Path) -> Result<(), OfficeHatasi> {
    pdfe_donustur_sureli(girdi, cikti, PDF_SURESI)
}

/// [`pdfe_donustur`], bekleme süresi verilerek. Süre önceki bir dönüştürmenin bitmesini beklemeyi de
/// kapsar; dolarsa [`OfficeHatasi::ZamanAsimi`] ya da [`OfficeHatasi::Mesgul`] döner.
pub fn pdfe_donustur_sureli(girdi: &Path, cikti: &Path, sure: Duration) -> Result<(), OfficeHatasi> {
    let uygulama = uygulama_sec(girdi).ok_or_else(|| OfficeHatasi::BicimDesteklenmiyor { uzanti: uzanti(girdi) })?;
    #[cfg(windows)]
    {
        islem::pdfe_donustur(girdi, cikti, uygulama, sure)
    }
    #[cfg(not(windows))]
    {
        let _ = (cikti, sure, uygulama);
        Err(OfficeHatasi::Desteklenmiyor)
    }
}

/// `girdi` PDF'ini Word'ün PDF açma özelliğiyle düzenlenebilir bir belgeye çevirip `cikti`'ya
/// `.docx` olarak yazar ([`WORD_SURESI`] bekler). `cikti` varsa yerine yazılır; klasörü var olmalıdır.
pub fn pdften_worde(girdi: &Path, cikti: &Path) -> Result<(), OfficeHatasi> {
    pdften_worde_sureli(girdi, cikti, WORD_SURESI)
}

/// [`pdften_worde`], bekleme süresi verilerek (bkz. [`pdfe_donustur_sureli`]).
pub fn pdften_worde_sureli(girdi: &Path, cikti: &Path, sure: Duration) -> Result<(), OfficeHatasi> {
    if !girdi.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf")) {
        return Err(OfficeHatasi::BicimDesteklenmiyor { uzanti: uzanti(girdi) });
    }
    #[cfg(windows)]
    {
        islem::pdften_worde(girdi, cikti, sure)
    }
    #[cfg(not(windows))]
    {
        let _ = (cikti, sure);
        Err(OfficeHatasi::Desteklenmiyor)
    }
}

/// `yol`'un uzantısı, küçük harfle ve noktasız; yoksa boş.
fn uzanti(yol: &Path) -> String {
    yol.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uzanti_uygulamayi_secer() {
        assert_eq!(uygulama_sec(Path::new("rapor.docx")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("RAPOR.DOC")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("not.txt")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("mektup.rtf")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("yazi.odt")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("tablo.xlsx")), Some(Uygulama::Excel));
        assert_eq!(uygulama_sec(Path::new("eski.XLS")), Some(Uygulama::Excel));
        assert_eq!(uygulama_sec(Path::new("veri.csv")), Some(Uygulama::Excel));
        assert_eq!(uygulama_sec(Path::new("hesap.ods")), Some(Uygulama::Excel));
        assert_eq!(uygulama_sec(Path::new("sunum.pptx")), Some(Uygulama::PowerPoint));
        assert_eq!(uygulama_sec(Path::new("eski.ppt")), Some(Uygulama::PowerPoint));
        assert_eq!(uygulama_sec(Path::new("slayt.odp")), Some(Uygulama::PowerPoint));
    }

    #[test]
    fn taninmayan_dosyanin_uygulamasi_yoktur() {
        assert_eq!(uygulama_sec(Path::new("belge.pdf")), None);
        assert_eq!(uygulama_sec(Path::new("docx")), None);
        assert_eq!(uygulama_sec(Path::new("")), None);
        assert_eq!(uygulama_sec(Path::new("arsiv.docx.zip")), None);
        // Türkçe harfli ad, uzantı ASCII.
        assert_eq!(uygulama_sec(Path::new("Çığ Şişe.docx")), Some(Uygulama::Word));
        assert_eq!(uygulama_sec(Path::new("rapor.dócx")), None);
    }

    #[test]
    fn desteklenmeyen_bicim_office_baslatmadan_reddedilir() {
        let hata = pdfe_donustur(Path::new("resim.png"), Path::new("cikti.pdf")).unwrap_err();
        assert_eq!(hata, OfficeHatasi::BicimDesteklenmiyor { uzanti: "png".into() });
        let hata = pdfe_donustur(Path::new("uzantisiz"), Path::new("cikti.pdf")).unwrap_err();
        assert_eq!(hata, OfficeHatasi::BicimDesteklenmiyor { uzanti: String::new() });
        let hata = pdften_worde(Path::new("belge.docx"), Path::new("cikti.docx")).unwrap_err();
        assert_eq!(hata, OfficeHatasi::BicimDesteklenmiyor { uzanti: "docx".into() });
    }

    #[test]
    fn durum_uygulamalari_tek_tek_soyler() {
        let durum = OfficeDurumu { word: true, excel: false, powerpoint: true };
        assert!(durum.var_mi(Uygulama::Word));
        assert!(!durum.var_mi(Uygulama::Excel));
        assert!(durum.var_mi(Uygulama::PowerPoint));
        assert!(durum.herhangi());
        assert!(!OfficeDurumu::default().herhangi());
    }

    #[test]
    fn mevcut_cokmeden_doner() {
        // Office kurulu olsun olmasın: yalnız kayıt defterini okur.
        let durum = mevcut();
        eprintln!("Office durumu: {durum:?}");
        #[cfg(not(windows))]
        assert_eq!(durum, OfficeDurumu::default());
    }
}
