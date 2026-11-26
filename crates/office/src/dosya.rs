//! Dosya işleri: yolların denetimi, geçici çalışma klasörü, sonucun hedefe tek adımda taşınması ve
//! Office'e verilmeden önce dosyaya bakılan birkaç şey (metin kodlaması, şifreli Office dosyası).

use std::ffi::OsStr;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::OfficeHatasi;

/// Office'in bu sayıdan uzun yolları açamadığı yerler var (Excel 218 karakter); kopyanın adı kısa tutulur.
const AD_SINIRI: usize = 120;
/// Metin dosyasının kodlamasına bakılırken okunan en çok bayt.
const KODLAMA_OKUMASI: u64 = 4 * 1024 * 1024;
/// Word'ün UTF-8 kodlama numarası (`msoEncodingUTF8`).
const UTF8: i32 = 65001;
/// Bileşik belge (OLE) imzası: eski Office dosyaları ve parolayla şifrelenmiş yeni Office dosyaları.
const OLE_IMZASI: &[u8] = &[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
/// Şifrelenince bileşik belgeye dönüşen, normalde ZIP olan Office biçimleri.
const OOXML_UZANTILARI: &[&str] = &["docx", "docm", "dotx", "xlsx", "xlsm", "xltx", "pptx", "pptm", "potx", "ppsx"];

/// Denetlenmiş, mutlak girdi ve çıktı yolları.
pub(crate) struct Yollar {
    pub(crate) girdi: PathBuf,
    pub(crate) cikti: PathBuf,
}

/// Girdi var ve bir dosya mı, çıktının klasörü var mı, ikisi aynı dosya mı. Office'e gitmeden önce.
pub(crate) fn yollari_denetle(girdi: &Path, cikti: &Path) -> Result<Yollar, OfficeHatasi> {
    let girdi_mutlak = std::path::absolute(girdi).map_err(|_| OfficeHatasi::DosyaYok(girdi.to_path_buf()))?;
    if !fs::metadata(&girdi_mutlak).is_ok_and(|bilgi| bilgi.is_file()) {
        return Err(OfficeHatasi::DosyaYok(girdi_mutlak));
    }
    let cikti_mutlak = std::path::absolute(cikti).map_err(|e| OfficeHatasi::CiktiYazilamadi { ayrinti: e.to_string() })?;
    if cikti_mutlak.file_name().is_none() {
        return Err(OfficeHatasi::CiktiYazilamadi { ayrinti: format!("geçerli bir dosya adı değil: {}", cikti_mutlak.display()) });
    }
    let klasor = cikti_mutlak.parent().filter(|klasor| klasor.is_dir());
    if klasor.is_none() {
        return Err(OfficeHatasi::CiktiYazilamadi { ayrinti: format!("klasör bulunamadı: {}", cikti_mutlak.display()) });
    }
    if cikti_mutlak.is_dir() {
        return Err(OfficeHatasi::CiktiYazilamadi { ayrinti: format!("bu adda bir klasör var: {}", cikti_mutlak.display()) });
    }
    if ayni_dosya(&girdi_mutlak, &cikti_mutlak) {
        return Err(OfficeHatasi::AyniDosya);
    }
    Ok(Yollar { girdi: girdi_mutlak, cikti: cikti_mutlak })
}

/// İki yol aynı dosyayı mı gösteriyor: harf büyüklüğüne bakmadan, çıktı zaten varsa gerçek yollarıyla.
fn ayni_dosya(a: &Path, b: &Path) -> bool {
    if a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase() {
        return true;
    }
    match (fs::canonicalize(a), fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => false,
    }
}

/// `%TEMP%` altında bu dönüştürmeye ait bir klasör. Düşünce yalnız kaydettiği dosyaları tek tek siler,
/// sonra boşalan klasörü kaldırır; Office başka bir şey bıraktıysa klasör kalır ve günlüğe yazılır.
pub(crate) struct CalismaKlasoru {
    yol: PathBuf,
    dosyalar: Vec<PathBuf>,
}

impl CalismaKlasoru {
    pub(crate) fn yeni() -> Result<CalismaKlasoru, OfficeHatasi> {
        static SAYAC: AtomicU64 = AtomicU64::new(0);
        let temel = std::env::temp_dir();
        let mut son_hata = None;
        for _ in 0..16 {
            let sira = SAYAC.fetch_add(1, Ordering::Relaxed);
            let zaman = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |sure| sure.subsec_nanos());
            let yol = temel.join(format!("migez-office-{}-{sira}-{zaman}", std::process::id()));
            // Özyinelemesiz: var olan bir klasör bizim değildir.
            match fs::create_dir(&yol) {
                Ok(()) => return Ok(CalismaKlasoru { yol, dosyalar: Vec::new() }),
                Err(hata) => son_hata = Some(hata),
            }
        }
        let ayrinti = son_hata.map(|hata| hata.to_string()).unwrap_or_default();
        Err(OfficeHatasi::Ic { ayrinti: format!("geçici klasör açılamadı: {ayrinti}") })
    }

    /// Klasörün yolu.
    #[cfg(test)]
    pub(crate) fn yol(&self) -> &Path {
        &self.yol
    }

    /// Klasörde `ad` adlı bir dosyanın yolu; klasör düşünce silinir.
    pub(crate) fn dosya(&mut self, ad: &str) -> PathBuf {
        let yol = self.yol.join(ad);
        self.dosyalar.push(yol.clone());
        yol
    }

    /// `kaynak`'ı klasöre kopyalar ve kopyanın yolunu verir. Kopya kaynağın adını taşır (Office onu
    /// üst ve alt bilgilere yazabilir); ad çok uzunsa ya da Excel'in açamadığı köşeli ayraçlar varsa
    /// kısaltılır ya da düzeltilir. Salt okunur kopya yazılabilir yapılır ki sonra silinebilsin.
    pub(crate) fn kopyala(&mut self, kaynak: &Path) -> Result<PathBuf, OfficeHatasi> {
        let hedef = self.yol.join(kopya_adi(kaynak));
        self.dosyalar.push(hedef.clone());
        fs::copy(kaynak, &hedef).map_err(|hata| OfficeHatasi::Ic { ayrinti: format!("dosya geçici klasöre kopyalanamadı: {hata}") })?;
        let mut izinler = fs::metadata(&hedef).map_err(|hata| OfficeHatasi::Ic { ayrinti: hata.to_string() })?.permissions();
        if izinler.readonly() {
            // Windows'ta yalnız "salt okunur" özniteliğini kaldırır; Unix'teki gibi herkese yazma izni vermez.
            #[allow(clippy::permissions_set_readonly_false)]
            izinler.set_readonly(false);
            fs::set_permissions(&hedef, izinler).map_err(|hata| OfficeHatasi::Ic { ayrinti: hata.to_string() })?;
        }
        Ok(hedef)
    }
}

impl Drop for CalismaKlasoru {
    fn drop(&mut self) {
        for dosya in &self.dosyalar {
            match fs::remove_file(dosya) {
                Ok(()) => {}
                Err(hata) if hata.kind() == io::ErrorKind::NotFound => {}
                Err(hata) => log::warn!("Office dönüştürmesi: geçici dosya silinemedi ({}): {hata}", dosya.display()),
            }
        }
        if let Err(hata) = fs::remove_dir(&self.yol) {
            log::warn!("Office dönüştürmesi: geçici klasör kaldırılamadı ({}): {hata}", self.yol.display());
        }
    }
}

/// Kopyanın adı: kaynağın adı, köşeli ayraçlar normal ayraca çevrilmiş; uzun ya da boşsa `belge.<uzantı>`.
fn kopya_adi(kaynak: &Path) -> String {
    let ad = kaynak.file_name().map(OsStr::to_string_lossy).unwrap_or_default();
    let ad: String = ad
        .chars()
        .map(|harf| match harf {
            '[' => '(',
            ']' => ')',
            _ => harf,
        })
        .collect();
    if ad.is_empty() || ad.starts_with("~$") || ad.chars().count() > AD_SINIRI {
        let uzanti = kaynak.extension().map(OsStr::to_string_lossy).unwrap_or_default().to_lowercase();
        return format!("belge.{uzanti}");
    }
    ad
}

/// Sonuç dosyasının geçici klasördeki adı: girdinin kopyasıyla çakışmayan `sonuc.<uzanti>`.
pub(crate) fn sonuc_adi(kopya: &Path, uzanti: &str) -> String {
    let ad = format!("sonuc.{uzanti}");
    if kopya.file_name().is_some_and(|kopya_adi| kopya_adi.eq_ignore_ascii_case(&ad)) { format!("sonuc-1.{uzanti}") } else { ad }
}

/// Office'in yazdığı dosya beklenen türde mi: var, boş değil ve `imza` ile başlıyor.
pub(crate) fn imzayi_denetle(yol: &Path, imza: &[u8]) -> Result<(), String> {
    let mut bas = Vec::new();
    let dosya = fs::File::open(yol).map_err(|hata| format!("Office sonuç dosyası üretmedi ({hata})"))?;
    dosya.take(u64::try_from(imza.len()).unwrap_or(u64::MAX)).read_to_end(&mut bas).map_err(|hata| hata.to_string())?;
    if bas.as_slice() == imza { Ok(()) } else { Err("Office'in ürettiği dosya beklenen türde değil".to_owned()) }
}

/// Sonucu hedefe taşır: aynı birimdeyse tek adımlık yeniden adlandırma (hedef varsa yerine geçer);
/// başka birimdeyse önce hedefin yanına kopyalar, sonra onu hedefin üzerine adlandırır. Yarım bir
/// dosya hiçbir zaman hedefin adını taşımaz.
pub(crate) fn yerine_koy(kaynak: &Path, hedef: &Path) -> Result<(), OfficeHatasi> {
    let yazilamadi = |hata: io::Error| OfficeHatasi::CiktiYazilamadi { ayrinti: format!("{} ({hata})", hedef.display()) };
    match fs::rename(kaynak, hedef) {
        Ok(()) => return Ok(()),
        Err(hata) if hata.kind() != io::ErrorKind::CrossesDevices => return Err(yazilamadi(hata)),
        Err(_) => {}
    }
    static SAYAC: AtomicU64 = AtomicU64::new(0);
    let ad = hedef.file_name().map(OsStr::to_string_lossy).unwrap_or_default();
    let ara = hedef.with_file_name(format!(".{ad}.migez-office-{}-{}.tmp", std::process::id(), SAYAC.fetch_add(1, Ordering::Relaxed)));
    let sonuc = fs::copy(kaynak, &ara).and_then(|_| fs::rename(&ara, hedef));
    if let Err(hata) = sonuc {
        if let Err(silme) = fs::remove_file(&ara)
            && silme.kind() != io::ErrorKind::NotFound
        {
            log::warn!("Office dönüştürmesi: ara dosya silinemedi ({}): {silme}", ara.display());
        }
        return Err(yazilamadi(hata));
    }
    Ok(())
}

/// Word'e bir `.txt` için verilecek kodlama: BOM'suz ve geçerli UTF-8 olan, ASCII dışı harf taşıyan
/// metin için UTF-8. Öbür durumlarda `None`: Word BOM'u kendisi okur, ötekilerde kendi seçer.
pub(crate) fn metin_kodlamasi(yol: &Path) -> Option<i32> {
    let mut bas = Vec::new();
    fs::File::open(yol).ok()?.take(KODLAMA_OKUMASI).read_to_end(&mut bas).ok()?;
    kodlama_sec(&bas, u64::try_from(bas.len()).is_ok_and(|uzunluk| uzunluk >= KODLAMA_OKUMASI))
}

/// [`metin_kodlamasi`]'nın karar kısmı; `kesik`: dosyanın yalnız başı okundu.
fn kodlama_sec(bas: &[u8], kesik: bool) -> Option<i32> {
    let bom: [&[u8]; 3] = [&[0xEF, 0xBB, 0xBF], &[0xFF, 0xFE], &[0xFE, 0xFF]];
    if bom.iter().any(|imza| bas.starts_with(imza)) || bas.is_ascii() {
        return None;
    }
    match std::str::from_utf8(bas) {
        Ok(_) => Some(UTF8),
        // Okumanın sonunda yarım kalan bir harf geçersizlik değildir.
        Err(hata) if kesik && hata.error_len().is_none() => Some(UTF8),
        Err(_) => None,
    }
}

/// Office açamadığında: dosya parolayla şifrelenmiş yeni bir Office dosyası mı. Bu dosyalar ZIP
/// değil bileşik belge olarak saklanır (MS-OFFCRYPTO).
pub(crate) fn sifreli_office_dosyasi_mi(yol: &Path) -> bool {
    let uzanti = yol.extension().and_then(OsStr::to_str).unwrap_or_default();
    if !OOXML_UZANTILARI.iter().any(|e| uzanti.eq_ignore_ascii_case(e)) {
        return false;
    }
    let mut bas = Vec::new();
    let okundu = fs::File::open(yol).and_then(|dosya| dosya.take(8).read_to_end(&mut bas));
    okundu.is_ok() && bas.as_slice() == OLE_IMZASI
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kopya_adi_kaynagin_adini_tasir() {
        assert_eq!(kopya_adi(Path::new(r"C:\Belgeler\Çığ Şişe.docx")), "Çığ Şişe.docx");
        assert_eq!(kopya_adi(Path::new(r"C:\Belgeler\Tablo [2026].xlsx")), "Tablo (2026).xlsx");
        assert_eq!(kopya_adi(Path::new(r"C:\Belgeler\~$gizli.docx")), "belge.docx");
        let uzun = format!(r"C:\Belgeler\{}.PPTX", "a".repeat(AD_SINIRI));
        assert_eq!(kopya_adi(Path::new(&uzun)), "belge.pptx");
    }

    #[test]
    fn sonuc_adi_kopyayla_cakismaz() {
        assert_eq!(sonuc_adi(Path::new(r"C:\t\rapor.docx"), "pdf"), "sonuc.pdf");
        assert_eq!(sonuc_adi(Path::new(r"C:\t\SONUC.PDF"), "pdf"), "sonuc-1.pdf");
        assert_eq!(sonuc_adi(Path::new(r"C:\t\sonuc.pdf"), "docx"), "sonuc.docx");
    }

    #[test]
    fn metnin_kodlamasi_secilir() {
        assert_eq!(kodlama_sec("Çığ Şişe İğne Güneş Öğrenci".as_bytes(), false), Some(UTF8));
        assert_eq!(kodlama_sec(b"plain ascii", false), None);
        assert_eq!(kodlama_sec(b"", false), None);
        assert_eq!(kodlama_sec(&[0xEF, 0xBB, 0xBF, 0xC3, 0x87], false), None, "BOM'u Word okur");
        assert_eq!(kodlama_sec(&[0xFF, 0xFE, 0xC7, 0x00], false), None, "UTF-16 BOM'u Word okur");
        // Windows-1254 "Çığ": geçerli UTF-8 değil, Word kendi seçer.
        assert_eq!(kodlama_sec(&[0xC7, 0xFD, 0xF0], false), None);
        // Okumanın sonunda yarım kalan "ş" (C5 9F).
        assert_eq!(kodlama_sec(&[b'a', 0xC5], true), Some(UTF8));
        assert_eq!(kodlama_sec(&[b'a', 0xC5], false), None);
    }

    /// Testin kendi geçici klasörü: dosyaları tek tek silinir, sonra boş klasör kaldırılır.
    fn test_klasoru() -> CalismaKlasoru {
        CalismaKlasoru::yeni().unwrap()
    }

    #[test]
    fn sifreli_office_dosyasi_imzasindan_taninir() {
        let mut klasor = test_klasoru();
        let sifreli = klasor.dosya("sifreli.docx");
        fs::write(&sifreli, [OLE_IMZASI, b"rest"].concat()).unwrap();
        let normal = klasor.dosya("normal.docx");
        fs::write(&normal, b"PK\x03\x04rest").unwrap();
        let eski = klasor.dosya("eski.doc");
        fs::write(&eski, [OLE_IMZASI, b"rest"].concat()).unwrap();
        assert!(sifreli_office_dosyasi_mi(&sifreli));
        assert!(!sifreli_office_dosyasi_mi(&normal));
        assert!(!sifreli_office_dosyasi_mi(&eski), "eski biçim zaten bileşik belge");
        assert!(!sifreli_office_dosyasi_mi(&klasor.yol().join("yok.docx")));
    }

    #[test]
    fn yollar_denetlenir() {
        let mut klasor = test_klasoru();
        let girdi = klasor.dosya("girdi.docx");
        fs::write(&girdi, b"PK").unwrap();
        let cikti = klasor.yol().join("cikti.pdf");
        let yollar = yollari_denetle(&girdi, &cikti).unwrap();
        assert!(yollar.girdi.is_absolute() && yollar.cikti.is_absolute());

        let yok = klasor.yol().join("yok.docx");
        assert_eq!(yollari_denetle(&yok, &cikti).err(), Some(OfficeHatasi::DosyaYok(yok.clone())));
        assert_eq!(yollari_denetle(klasor.yol(), &cikti).err(), Some(OfficeHatasi::DosyaYok(klasor.yol().to_path_buf())));
        let buyuk_harfli = klasor.yol().join("GIRDI.DOCX");
        assert_eq!(yollari_denetle(&girdi, &buyuk_harfli).err(), Some(OfficeHatasi::AyniDosya));
        let klasorsuz = klasor.yol().join("olmayan-klasor").join("cikti.pdf");
        assert!(matches!(yollari_denetle(&girdi, &klasorsuz), Err(OfficeHatasi::CiktiYazilamadi { .. })));
        assert!(matches!(yollari_denetle(&girdi, klasor.yol()), Err(OfficeHatasi::CiktiYazilamadi { .. })));
    }

    #[test]
    fn sonuc_hedefe_tasinir_ve_eskisinin_yerine_gecer() {
        let mut klasor = test_klasoru();
        let kaynak = klasor.dosya("sonuc.pdf");
        let hedef = klasor.dosya("hedef.pdf");
        fs::write(&hedef, b"eski").unwrap();
        fs::write(&kaynak, b"%PDF-yeni").unwrap();
        yerine_koy(&kaynak, &hedef).unwrap();
        assert_eq!(fs::read(&hedef).unwrap(), b"%PDF-yeni");
        assert!(!kaynak.exists());
        assert!(imzayi_denetle(&hedef, b"%PDF-").is_ok());
        assert!(imzayi_denetle(&hedef, b"PK\x03\x04").is_err());
        assert!(imzayi_denetle(&kaynak, b"%PDF-").is_err());
    }

    #[test]
    fn salt_okunur_kopya_silinebilir() {
        let mut klasor = test_klasoru();
        let kaynak = klasor.dosya("salt-okunur.docx");
        fs::write(&kaynak, b"PK").unwrap();
        let mut izinler = fs::metadata(&kaynak).unwrap().permissions();
        izinler.set_readonly(true);
        fs::set_permissions(&kaynak, izinler).unwrap();
        let mut ikinci = test_klasoru();
        let kopya = ikinci.kopyala(&kaynak).unwrap();
        assert!(!fs::metadata(&kopya).unwrap().permissions().readonly());
        let ikinci_yol = ikinci.yol().to_path_buf();
        drop(ikinci);
        assert!(!ikinci_yol.exists(), "kopya ve klasörü silindi");
        // Kaynağı silinebilir yap: klasör tek tek siler.
        let mut izinler = fs::metadata(&kaynak).unwrap().permissions();
        #[allow(clippy::permissions_set_readonly_false)]
        izinler.set_readonly(false);
        fs::set_permissions(&kaynak, izinler).unwrap();
    }
}
