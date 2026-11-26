//! Word, Excel ve PowerPoint ile bir dönüştürme. Uygulamayı başlatır (ya da çalışanına bağlanır),
//! belgenin geçici kopyasını salt okunur açar, dışa aktarır, yalnız kendi açtığı belgeyi kaydetmeden
//! kapatır ve uygulamayı yalnız kendisi başlattıysa kapatır.
//!
//! Uygulama "bizim" sayılır, ancak bağlanıldığı anda görünmüyorsa ve hiç açık belgesi (Korumalı
//! Görünüm pencereleri dahil) yoksa. İş bitince yine görünmüyorsa ve açık belge sayısı yine sıfırsa
//! `Quit` çağrılır; bunlardan biri okunamazsa uygulama açık bırakılır. PowerPoint tek örneklidir:
//! kullanıcının açık PowerPoint'ine bağlanılırsa yalnız bizim sunumumuz eklenip çıkarılır.

use std::path::Path;
use std::time::Duration;

use crate::calistirici::{self, Iptal};
use crate::com::{Arg, ComHatasi, Daire, Deger, Nesne};
use crate::dosya::{self, CalismaKlasoru};
use crate::kayit;
use crate::{OfficeHatasi, Uygulama};

/// Parolalı belgede Office'in parola penceresi açmasını önleyen, bilerek yanlış bir değer. Parolasız
/// belgede yok sayılır; parolalı belgede açma, pencere açılmadan "parola yanlış" hatasıyla biter.
const YANLIS_PAROLA: &str = "migez-office-sormadan-ac";

/// `msoAutomationSecurityForceDisable`: kodla açılan belgelerde makrolar çalışmaz.
const MAKROLAR_KAPALI: i32 = 3;
/// `msoTrue` (MsoTriState).
const MSO_EVET: i32 = -1;
/// `msoFalse` (MsoTriState).
const MSO_HAYIR: i32 = 0;

/// `wdAlertsNone`.
const WORD_UYARI_YOK: i32 = 0;
/// `wdDoNotSaveChanges`.
const WORD_KAYDETME: i32 = 0;
/// `wdExportFormatPDF`.
const WORD_PDF: i32 = 17;
/// `wdExportOptimizeForPrint`.
const WORD_BASKI_KALITESI: i32 = 0;
/// `wdExportAllDocument`.
const WORD_TUM_BELGE: i32 = 0;
/// `wdExportDocumentContent`.
const WORD_BELGE_ICERIGI: i32 = 0;
/// `wdExportCreateHeadingBookmarks`: başlıklar PDF'te yer imi olur.
const WORD_BASLIK_YER_IMLERI: i32 = 1;
/// `wdFormatDocumentDefault`: `.docx`.
const WORD_DOCX: i32 = 16;

/// `xlTypePDF`.
const EXCEL_PDF: i32 = 0;
/// `xlQualityStandard`.
const EXCEL_STANDART_KALITE: i32 = 0;
/// `UpdateLinks`: dış bağlantılar güncellenmez.
const EXCEL_BAGLANTI_GUNCELLEME: i32 = 0;

/// `ppAlertsNone`.
const POWERPOINT_UYARI_YOK: i32 = 1;
/// `ppSaveAsPDF`.
const POWERPOINT_PDF: i32 = 32;

/// Bir Office uygulamasının bu pakete gereken farkları.
pub(crate) struct Profil {
    pub(crate) uygulama: Uygulama,
    /// Açık belgeleri tutan koleksiyon.
    koleksiyon: &'static str,
    /// `DisplayAlerts`'in "hiçbir uyarı gösterme" değeri: Word ve PowerPoint'te sayı, Excel'de mantık.
    uyari_yok: Arg<'static>,
    /// Kendi başlattığımız uygulamada `Visible = false` yazılsın mı. PowerPoint'te yazılmaz: kodla
    /// başlatılan PowerPoint zaten görünmez, sunumu da penceresiz açarız.
    gizlenebilir: bool,
}

const WORD: Profil = Profil { uygulama: Uygulama::Word, koleksiyon: "Documents", uyari_yok: Arg::Tam(WORD_UYARI_YOK), gizlenebilir: true };
const EXCEL: Profil = Profil { uygulama: Uygulama::Excel, koleksiyon: "Workbooks", uyari_yok: Arg::Mantik(false), gizlenebilir: true };
const POWERPOINT: Profil =
    Profil { uygulama: Uygulama::PowerPoint, koleksiyon: "Presentations", uyari_yok: Arg::Tam(POWERPOINT_UYARI_YOK), gizlenebilir: false };

fn profil(uygulama: Uygulama) -> &'static Profil {
    match uygulama {
        Uygulama::Word => &WORD,
        Uygulama::Excel => &EXCEL,
        Uygulama::PowerPoint => &POWERPOINT,
    }
}

/// `girdi`'yi `uygulama` ile PDF'e çevirir ([`crate::pdfe_donustur_sureli`]).
pub(crate) fn pdfe_donustur(girdi: &Path, cikti: &Path, uygulama: Uygulama, sure: Duration) -> Result<(), OfficeHatasi> {
    let yollar = dosya::yollari_denetle(girdi, cikti)?;
    if !kayit::kurulu_mu(uygulama) {
        return Err(OfficeHatasi::UygulamaYok(uygulama));
    }
    calistirici::calistir(sure, move |daire, iptal| {
        let mut klasor = CalismaKlasoru::yeni()?;
        let kopya = klasor.kopyala(&yollar.girdi)?;
        let sonuc = klasor.dosya(&dosya::sonuc_adi(&kopya, "pdf"));
        pdf_uret(daire, uygulama, &kopya, &sonuc, iptal)?;
        dosya::imzayi_denetle(&sonuc, b"%PDF-").map_err(|ayrinti| OfficeHatasi::DonusturmeBasarisiz { uygulama, ayrinti })?;
        iptal.denetle()?;
        dosya::yerine_koy(&sonuc, &yollar.cikti)
    })
}

/// `girdi` PDF'ini Word ile `.docx`'e çevirir ([`crate::pdften_worde_sureli`]).
pub(crate) fn pdften_worde(girdi: &Path, cikti: &Path, sure: Duration) -> Result<(), OfficeHatasi> {
    let yollar = dosya::yollari_denetle(girdi, cikti)?;
    if !kayit::kurulu_mu(Uygulama::Word) {
        return Err(OfficeHatasi::UygulamaYok(Uygulama::Word));
    }
    calistirici::calistir(sure, move |daire, iptal| {
        let mut klasor = CalismaKlasoru::yeni()?;
        let kopya = klasor.kopyala(&yollar.girdi)?;
        let sonuc = klasor.dosya(&dosya::sonuc_adi(&kopya, "docx"));
        docx_uret(daire, &kopya, &sonuc, iptal)?;
        dosya::imzayi_denetle(&sonuc, b"PK\x03\x04").map_err(|ayrinti| OfficeHatasi::DonusturmeBasarisiz { uygulama: Uygulama::Word, ayrinti })?;
        iptal.denetle()?;
        dosya::yerine_koy(&sonuc, &yollar.cikti)
    })
}

fn pdf_uret(daire: &Daire, uygulama: Uygulama, girdi: &Path, cikti: &Path, iptal: &Iptal) -> Result<(), OfficeHatasi> {
    let oturum = Oturum::ac(daire, uygulama)?;
    iptal.denetle()?;
    let metin_dosyasi = girdi.extension().is_some_and(|uzanti| uzanti.eq_ignore_ascii_case("txt"));
    let kodlama = if uygulama == Uygulama::Word && metin_dosyasi { dosya::metin_kodlamasi(girdi) } else { None };
    let mut belge = oturum.belge_ac(girdi, kodlama)?;
    iptal.denetle()?;
    belge.pdf_kaydet(cikti).map_err(|hata| islem_hatasi(uygulama, &hata))?;
    belge.kapat_ve_gunluge_yaz();
    drop(belge);
    oturum.bitir();
    Ok(())
}

fn docx_uret(daire: &Daire, girdi: &Path, cikti: &Path, iptal: &Iptal) -> Result<(), OfficeHatasi> {
    let oturum = Oturum::ac(daire, Uygulama::Word)?;
    iptal.denetle()?;
    let mut belge = oturum.belge_ac(girdi, None)?;
    iptal.denetle()?;
    belge.docx_kaydet(cikti).map_err(|hata| islem_hatasi(Uygulama::Word, &hata))?;
    belge.kapat_ve_gunluge_yaz();
    drop(belge);
    oturum.bitir();
    Ok(())
}

/// Bir Office uygulamasıyla çalışma süresi. Düşünce (ya da [`Oturum::bitir`] ile) değiştirdiği
/// ayarları geri yükler ve uygulamayı yalnız kendisi başlattıysa ve hâlâ boşsa kapatır.
pub(crate) struct Oturum<'d> {
    uyg: Nesne<'d>,
    profil: &'static Profil,
    /// Uygulama bağlanıldığında görünmüyordu ve hiç açık belgesi yoktu.
    kendi: bool,
    eski_uyari: Option<Deger<'d>>,
    eski_guvenlik: Option<Deger<'d>>,
    bitti: bool,
}

impl<'d> Oturum<'d> {
    pub(crate) fn ac(daire: &'d Daire, uygulama: Uygulama) -> Result<Oturum<'d>, OfficeHatasi> {
        let profil = profil(uygulama);
        let uyg = Nesne::olustur(daire, kayit::progid(uygulama)).map_err(|hata| baslatma_hatasi(uygulama, &hata))?;
        let gorunur = gorunur_mu(&uyg);
        let belge_sayisi = acik_belge_sayisi(&uyg, profil);
        let kendi = !gorunur && belge_sayisi == Some(0);
        log::debug!("{}: bağlanıldı; görünür {gorunur}, açık belge {belge_sayisi:?}, bizim başlattığımız {kendi}", uygulama.ad());
        let mut oturum = Oturum { uyg, profil, kendi, eski_uyari: None, eski_guvenlik: None, bitti: false };
        if !gorunur {
            // Görünmeyen Office'te iletişim kutusunu yanıtlayacak kimse yok: uyarılar kapatılır. Görünen
            // (kullanıcının) Office'te bu ayara dokunulmaz: uygulamanın tamamına işler, kullanıcının
            // oturumu değiştirilmez.
            match degistir(&oturum.uyg, "DisplayAlerts", profil.uyari_yok) {
                Ok(eski) => oturum.eski_uyari = eski,
                Err(hata) => log::warn!("{}: uyarılar kapatılamadı: {hata}", uygulama.ad()),
            }
        }
        // Kodla açılan belgede Office'in varsayılanı makroları çalıştırmaktır; kapatılamazsa açılmaz.
        oturum.eski_guvenlik = degistir(&oturum.uyg, "AutomationSecurity", Arg::Tam(MAKROLAR_KAPALI))
            .map_err(|hata| OfficeHatasi::Ic { ayrinti: format!("{} makroları kapatılamadı: {hata}", uygulama.ad()) })?;
        if oturum.kendi
            && profil.gizlenebilir
            && let Err(hata) = oturum.uyg.ata("Visible", Arg::Mantik(false))
        {
            log::debug!("{}: Visible yazılamadı: {hata}", uygulama.ad());
        }
        Ok(oturum)
    }

    /// Uygulama nesnesi (testler belge üretmek için kullanır).
    #[cfg(test)]
    pub(crate) fn uygulama(&self) -> &Nesne<'d> {
        &self.uyg
    }

    /// `yol`'daki belgeyi salt okunur, pencere ve iletişim kutusu açmadan açar. `kodlama` yalnız Word
    /// ile açılan metin dosyaları için.
    pub(crate) fn belge_ac(&self, yol: &Path, kodlama: Option<i32>) -> Result<AcikBelge<'d>, OfficeHatasi> {
        let uygulama = self.profil.uygulama;
        let koleksiyon = self.uyg.al(self.profil.koleksiyon).and_then(|deger| deger.nesne(self.profil.koleksiyon));
        let koleksiyon = koleksiyon.map_err(|hata| acma_hatasi(uygulama, yol, &hata))?;
        let acilan = match uygulama {
            Uygulama::Word => word_ac(&koleksiyon, yol, kodlama),
            Uygulama::Excel => excel_ac(&koleksiyon, yol),
            Uygulama::PowerPoint => powerpoint_ac(&koleksiyon, yol),
        };
        match acilan {
            Ok(Deger::Nesne(belge)) => Ok(AcikBelge { belge: Some(belge), uygulama }),
            Ok(_) => Err(OfficeHatasi::DosyaAcilamadi { uygulama, ayrinti: "Office açtığı belgeyi döndürmedi".into() }),
            Err(hata) => Err(acma_hatasi(uygulama, yol, &hata)),
        }
    }

    /// Ayarları geri yükler ya da uygulamayı kapatır (bkz. modül belgesi).
    pub(crate) fn bitir(mut self) {
        self.kapat_ic();
    }

    fn kapat_ic(&mut self) {
        if std::mem::replace(&mut self.bitti, true) {
            return;
        }
        let ad = self.profil.uygulama.ad();
        let gorunur = gorunur_mu(&self.uyg);
        let belge_sayisi = acik_belge_sayisi(&self.uyg, self.profil);
        if self.kendi && !gorunur && belge_sayisi == Some(0) {
            let cikis = match self.profil.uygulama {
                Uygulama::Word => self.uyg.cagir("Quit", &[Arg::Tam(WORD_KAYDETME)]),
                Uygulama::Excel | Uygulama::PowerPoint => self.uyg.cagir("Quit", &[]),
            };
            match cikis {
                Ok(_) => return,
                Err(hata) => log::warn!("{ad}: kapatılamadı: {hata}"),
            }
        } else if self.kendi {
            log::warn!("{ad}: iş bitince görünür {gorunur}, açık belge {belge_sayisi:?}; uygulama açık bırakıldı");
        }
        self.geri_yukle();
    }

    fn geri_yukle(&mut self) {
        let ad = self.profil.uygulama.ad();
        for (ozellik, eski) in [("DisplayAlerts", self.eski_uyari.take()), ("AutomationSecurity", self.eski_guvenlik.take())] {
            if let Some(arg) = eski.as_ref().and_then(Deger::arg)
                && let Err(hata) = self.uyg.ata(ozellik, arg)
            {
                log::warn!("{ad}: {ozellik} geri yüklenemedi: {hata}");
            }
        }
    }
}

impl Drop for Oturum<'_> {
    fn drop(&mut self) {
        self.kapat_ic();
    }
}

/// Bu paketin açtığı bir belge. Düşünce kaydetmeden kapatılır.
pub(crate) struct AcikBelge<'d> {
    belge: Option<Nesne<'d>>,
    uygulama: Uygulama,
}

impl<'d> AcikBelge<'d> {
    fn nesne(&self) -> Result<&Nesne<'d>, ComHatasi> {
        self.belge.as_ref().ok_or_else(|| ComHatasi::yeni("belge", windows::Win32::Foundation::E_POINTER, "belge kapatılmış"))
    }

    /// Belgenin nesnesi (testler belgeyi okumak için kullanır).
    #[cfg(test)]
    pub(crate) fn belge(&self) -> Option<&Nesne<'d>> {
        self.belge.as_ref()
    }

    /// Testin Office'te yeni açtığı bir belgeyi sarar: test yarıda kalsa da belge kaydedilmeden
    /// kapanır ve Office kapatılabilir.
    #[cfg(test)]
    pub(crate) fn sar(belge: Nesne<'d>, uygulama: Uygulama) -> AcikBelge<'d> {
        AcikBelge { belge: Some(belge), uygulama }
    }

    /// PDF olarak dışa aktarır: Word ve Excel `ExportAsFixedFormat`, PowerPoint `SaveAs` (ppSaveAsPDF).
    pub(crate) fn pdf_kaydet(&self, cikti: &Path) -> Result<(), ComHatasi> {
        let belge = self.nesne()?;
        let sonuc = match self.uygulama {
            Uygulama::Word => belge.cagir_adli(
                "ExportAsFixedFormat",
                &[
                    ("OutputFileName", Arg::Yol(cikti)),
                    ("ExportFormat", Arg::Tam(WORD_PDF)),
                    ("OpenAfterExport", Arg::Mantik(false)),
                    ("OptimizeFor", Arg::Tam(WORD_BASKI_KALITESI)),
                    ("Range", Arg::Tam(WORD_TUM_BELGE)),
                    ("Item", Arg::Tam(WORD_BELGE_ICERIGI)),
                    ("IncludeDocProps", Arg::Mantik(true)),
                    ("KeepIRM", Arg::Mantik(true)),
                    ("CreateBookmarks", Arg::Tam(WORD_BASLIK_YER_IMLERI)),
                    ("DocStructureTags", Arg::Mantik(true)),
                    ("BitmapMissingFonts", Arg::Mantik(true)),
                    ("UseISO19005_1", Arg::Mantik(false)),
                ],
            ),
            Uygulama::Excel => belge.cagir_adli(
                "ExportAsFixedFormat",
                &[
                    ("Type", Arg::Tam(EXCEL_PDF)),
                    ("Filename", Arg::Yol(cikti)),
                    ("Quality", Arg::Tam(EXCEL_STANDART_KALITE)),
                    ("IncludeDocProperties", Arg::Mantik(true)),
                    ("IgnorePrintAreas", Arg::Mantik(false)),
                    ("OpenAfterPublish", Arg::Mantik(false)),
                ],
            ),
            Uygulama::PowerPoint => belge.cagir("SaveAs", &[Arg::Yol(cikti), Arg::Tam(POWERPOINT_PDF)]),
        };
        sonuc.map(drop)
    }

    /// Word belgesini `.docx` olarak kaydeder.
    pub(crate) fn docx_kaydet(&self, cikti: &Path) -> Result<(), ComHatasi> {
        let belge = self.nesne()?;
        let argumanlar = [("FileName", Arg::Yol(cikti)), ("FileFormat", Arg::Tam(WORD_DOCX)), ("AddToRecentFiles", Arg::Mantik(false))];
        let sonuc = match belge.cagir_adli("SaveAs2", &argumanlar) {
            // Word 2007'de SaveAs2 yok.
            Err(hata) if hata.uye_yok() => belge.cagir_adli("SaveAs", &argumanlar),
            sonuc => sonuc,
        };
        sonuc.map(drop)
    }

    /// Yalnız bu belgeyi, kaydetmeden kapatır.
    pub(crate) fn kapat(&mut self) -> Result<(), ComHatasi> {
        let Some(belge) = self.belge.take() else { return Ok(()) };
        let sonuc = match self.uygulama {
            Uygulama::Word => belge.cagir("Close", &[Arg::Tam(WORD_KAYDETME)]),
            Uygulama::Excel => belge.cagir("Close", &[Arg::Mantik(false)]),
            Uygulama::PowerPoint => {
                // Değişmiş sayılmasın: kapatırken kaydetme sorusu çıkmasın.
                if let Err(hata) = belge.ata("Saved", Arg::Tam(MSO_EVET)) {
                    log::debug!("PowerPoint: Saved yazılamadı: {hata}");
                }
                belge.cagir("Close", &[])
            }
        };
        sonuc.map(drop)
    }

    fn kapat_ve_gunluge_yaz(&mut self) {
        if let Err(hata) = self.kapat() {
            log::warn!("{}: açtığımız belge kapatılamadı: {hata}", self.uygulama.ad());
        }
    }
}

impl Drop for AcikBelge<'_> {
    fn drop(&mut self) {
        self.kapat_ve_gunluge_yaz();
    }
}

fn word_ac<'d>(belgeler: &Nesne<'d>, yol: &Path, kodlama: Option<i32>) -> Result<Deger<'d>, ComHatasi> {
    let mut argumanlar = vec![
        ("FileName", Arg::Yol(yol)),
        ("ConfirmConversions", Arg::Mantik(false)),
        ("ReadOnly", Arg::Mantik(true)),
        ("AddToRecentFiles", Arg::Mantik(false)),
        ("PasswordDocument", Arg::Metin(YANLIS_PAROLA)),
        ("PasswordTemplate", Arg::Metin(YANLIS_PAROLA)),
        // Aynı ad açıksa onu yeniden yükleme (kullanıcının kaydedilmemiş değişikliği gitmesin).
        ("Revert", Arg::Mantik(false)),
        ("Visible", Arg::Mantik(false)),
        ("OpenAndRepair", Arg::Mantik(false)),
        ("NoEncodingDialog", Arg::Mantik(true)),
    ];
    if let Some(kodlama) = kodlama {
        argumanlar.push(("Encoding", Arg::Tam(kodlama)));
    }
    // OpenNoRepairDialog, bozuk belgede onarım penceresi açmaz (Word 2010 ve sonrası).
    match belgeler.cagir_adli("OpenNoRepairDialog", &argumanlar) {
        Err(hata) if hata.uye_yok() => belgeler.cagir_adli("Open", &argumanlar),
        sonuc => sonuc,
    }
}

fn excel_ac<'d>(kitaplar: &Nesne<'d>, yol: &Path) -> Result<Deger<'d>, ComHatasi> {
    kitaplar.cagir_adli(
        "Open",
        &[
            ("Filename", Arg::Yol(yol)),
            ("UpdateLinks", Arg::Tam(EXCEL_BAGLANTI_GUNCELLEME)),
            ("ReadOnly", Arg::Mantik(true)),
            ("Password", Arg::Metin(YANLIS_PAROLA)),
            ("IgnoreReadOnlyRecommended", Arg::Mantik(true)),
            ("Notify", Arg::Mantik(false)),
            ("AddToMru", Arg::Mantik(false)),
            // CSV, kullanıcının bölge ayarlarıyla okunur (Türkçede alan ayracı ";", ondalık ",").
            ("Local", Arg::Mantik(true)),
        ],
    )
}

fn powerpoint_ac<'d>(sunumlar: &Nesne<'d>, yol: &Path) -> Result<Deger<'d>, ComHatasi> {
    // PowerPoint'in Open'ında parola yok; "dosya::parola::" biçimi parola sorulmadan açar.
    let mut ad = yol.as_os_str().to_owned();
    ad.push("::");
    ad.push(YANLIS_PAROLA);
    ad.push("::");
    // FileName, ReadOnly, Untitled, WithWindow.
    sunumlar.cagir("Open", &[Arg::Yol(Path::new(&ad)), Arg::Tam(MSO_EVET), Arg::Tam(MSO_HAYIR), Arg::Tam(MSO_HAYIR)])
}

/// `ad` özelliğinin eski değerini okur ve `yeni`yi yazar; eski değer (okunabildiyse) geri yükleme için.
fn degistir<'d>(uyg: &Nesne<'d>, ad: &str, yeni: Arg<'_>) -> Result<Option<Deger<'d>>, ComHatasi> {
    let eski = uyg.al(ad).ok();
    uyg.ata(ad, yeni)?;
    Ok(eski)
}

/// Uygulama ekranda mı. Okunamazsa görünür sayılır: kullanıcınınmış gibi davranılır.
fn gorunur_mu(uyg: &Nesne<'_>) -> bool {
    uyg.al("Visible").ok().and_then(|deger| deger.dogru_mu()).unwrap_or(true)
}

/// Açık belge, kitap ya da sunum sayısı, Korumalı Görünüm pencereleri dahil. Okunamazsa `None`.
fn acik_belge_sayisi(uyg: &Nesne<'_>, profil: &Profil) -> Option<i32> {
    let belgeler = uyg.al(profil.koleksiyon).ok()?.nesne(profil.koleksiyon).ok()?;
    let sayi = belgeler.al("Count").ok()?.tam()?;
    let korumali = match uyg.al("ProtectedViewWindows") {
        Ok(deger) => deger.nesne("ProtectedViewWindows").ok()?.al("Count").ok()?.tam()?,
        // Office 2007'de Korumalı Görünüm yok.
        Err(hata) if hata.uye_yok() => 0,
        Err(_) => return None,
    };
    sayi.checked_add(korumali)
}

fn baslatma_hatasi(uygulama: Uygulama, hata: &ComHatasi) -> OfficeHatasi {
    match u32::from_ne_bytes(hata.kod.to_ne_bytes()) {
        // REGDB_E_CLASSNOTREG, CO_E_CLASSSTRING: kayıtlı değil.
        0x8004_0154 | 0x8004_01F3 => OfficeHatasi::UygulamaYok(uygulama),
        _ => OfficeHatasi::UygulamaBaslatilamadi { uygulama, ayrinti: hata.aciklama.clone() },
    }
}

fn acma_hatasi(uygulama: Uygulama, yol: &Path, hata: &ComHatasi) -> OfficeHatasi {
    log::debug!("{}: açılamadı: {hata}", uygulama.ad());
    if hata.baglanti_koptu() {
        return OfficeHatasi::BaglantiKoptu { uygulama };
    }
    if parola_hatasi_mi(uygulama, hata) || dosya::sifreli_office_dosyasi_mi(yol) {
        return OfficeHatasi::ParolaKorumali;
    }
    OfficeHatasi::DosyaAcilamadi { uygulama, ayrinti: hata.aciklama.clone() }
}

fn islem_hatasi(uygulama: Uygulama, hata: &ComHatasi) -> OfficeHatasi {
    log::debug!("{}: dönüştürülemedi: {hata}", uygulama.ad());
    if hata.baglanti_koptu() {
        OfficeHatasi::BaglantiKoptu { uygulama }
    } else {
        OfficeHatasi::DonusturmeBasarisiz { uygulama, ayrinti: hata.aciklama.clone() }
    }
}

/// Office'in "parola yanlış" hatası mı (eski biçimli parolalı dosyalarda tek ipucu bu).
fn parola_hatasi_mi(uygulama: Uygulama, hata: &ComHatasi) -> bool {
    match uygulama {
        // Word 5408: "Parola yanlış. Word belgeyi açamıyor."
        Uygulama::Word => hata.office_numarasi() == Some(5408),
        // Excel'in parola hatası genel 1004 (0x800A03EC), PowerPoint'inki genel E_FAIL (0x80004005):
        // yalnız iletisi söylüyor. Türkçe ve İngilizce Office'te tanınır; öteki dillerde Office'in
        // iletisi DosyaAcilamadi'de görünür.
        Uygulama::Excel | Uygulama::PowerPoint => parola_iletisi_mi(&hata.aciklama),
    }
}

fn parola_iletisi_mi(ileti: &str) -> bool {
    let ileti = ileti.to_lowercase();
    ileti.contains("parola") || ileti.contains("password")
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::Foundation::DISP_E_EXCEPTION;
    use windows::core::HRESULT;

    #[test]
    fn parola_hatasi_taninir() {
        let word = ComHatasi::yeni("Open", HRESULT(0x800A_1520_u32.cast_signed()), "Parola yanlış.");
        assert!(parola_hatasi_mi(Uygulama::Word, &word));
        // Word'de yalnız hata numarası sayılır, ileti değil.
        assert!(!parola_hatasi_mi(Uygulama::Word, &ComHatasi::yeni("Open", DISP_E_EXCEPTION, "parola")));
        let excel = ComHatasi::yeni("Open", HRESULT(0x800A_03EC_u32.cast_signed()), "Yazdığınız PAROLA doğru değil.");
        assert!(parola_hatasi_mi(Uygulama::Excel, &excel));
        let powerpoint = ComHatasi::yeni("Open", DISP_E_EXCEPTION, "Presentations.Open : The password is incorrect.");
        assert!(parola_hatasi_mi(Uygulama::PowerPoint, &powerpoint));
        let bozuk = ComHatasi::yeni("Open", HRESULT(0x800A_03EC_u32.cast_signed()), "Dosya bozuk.");
        assert!(!parola_hatasi_mi(Uygulama::Excel, &bozuk));
    }

    #[test]
    fn kayitsiz_uygulama_uygulama_yok_sayilir() {
        let kayitsiz = ComHatasi::yeni("CoCreateInstance", HRESULT(0x8004_0154_u32.cast_signed()), "Sınıf kayıtlı değil");
        assert_eq!(baslatma_hatasi(Uygulama::Excel, &kayitsiz), OfficeHatasi::UygulamaYok(Uygulama::Excel));
        let baslamadi = ComHatasi::yeni("CoCreateInstance", HRESULT(0x8008_0005_u32.cast_signed()), "Sunucu yürütme başarısız");
        assert!(matches!(baslatma_hatasi(Uygulama::Word, &baslamadi), OfficeHatasi::UygulamaBaslatilamadi { .. }));
        let koptu = ComHatasi::yeni("ExportAsFixedFormat", HRESULT(0x8001_0108_u32.cast_signed()), "bağlantı kesildi");
        assert_eq!(islem_hatasi(Uygulama::Word, &koptu), OfficeHatasi::BaglantiKoptu { uygulama: Uygulama::Word });
    }
}
