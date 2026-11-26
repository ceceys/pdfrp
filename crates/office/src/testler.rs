//! Microsoft Office gerektiren testler. Hepsi `#[ignore]`: `cargo test -p migez-office -- --ignored`.
//!
//! Test belgeleri Office'in kendisiyle üretilir (Word'de yeni belge, Excel'de tablo, PowerPoint'te
//! tek slayt). Hepsi testin geçici klasöründe durur ve test bitince tek tek silinir. PDF'teki
//! Türkçe harfler, PDF Word'e geri çevrilip metni Word'le okunarak denetlenir: Word metni PDF'in
//! metin katmanından alır, harfler orada bozuksa geri gelen metinde de bozuk olur.
//!
//! Testler sırayla çalışır: aynı anda iki test Office'i açıp kapatmasın.

use std::path::{Path, PathBuf};
use std::sync::mpsc::sync_channel;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crate::com::{Arg, Daire, Deger, Nesne};
use crate::dosya::CalismaKlasoru;
use crate::islem::{AcikBelge, Oturum};
use crate::{OfficeHatasi, Uygulama, mevcut, pdfe_donustur, pdfe_donustur_sureli, pdften_worde_sureli};

/// Görevdeki deneme metni.
const TURKCE: &str = "Çığ Şişe İğne Güneş Öğrenci";
/// Türkçeye özgü bütün harfler.
const TURKCE_HARFLER: &str = "ÇĞİÖŞÜ çğıöşü";
/// Parolalı deneme belgelerinin parolası: gerçek bir parola değil, yalnız şifreli test dosyası üretmek için.
const DENEME_PAROLASI: &str = "deneme-1234";
/// Testlerde bir dönüştürmenin en çok süresi: takılan bir test bütün çalışmayı bekletmesin.
const SURE: Duration = Duration::from_secs(120);

static SIRA: Mutex<()> = Mutex::new(());

/// Paketin günlük kayıtlarını (Office'e bağlanma kararı, Office hata kodları) standart hataya yazar.
struct TestGunlugu;

impl log::Log for TestGunlugu {
    fn enabled(&self, _: &log::Metadata<'_>) -> bool {
        true
    }

    fn log(&self, kayit: &log::Record<'_>) {
        eprintln!("[{}] {}", kayit.level(), kayit.args());
    }

    fn flush(&self) {}
}

static GUNLUK: TestGunlugu = TestGunlugu;

/// Testleri sıraya sokar (önceki test paniklese de sıra sürer) ve günlüğü açar.
fn sira() -> MutexGuard<'static, ()> {
    // Günlük bir kez kurulabilir; sonraki denemeler yok sayılır.
    if log::set_logger(&GUNLUK).is_ok() {
        log::set_max_level(log::LevelFilter::Debug);
    }
    SIRA.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `is`'i COM'u başlatılmış yeni bir iş parçacığında çalıştırır.
fn sta<T: Send + 'static>(is: impl FnOnce(&Daire) -> T + Send + 'static) -> T {
    std::thread::spawn(move || {
        let daire = Daire::baslat().unwrap();
        is(&daire)
    })
    .join()
    .unwrap()
}

fn nesne<'d>(deger: Deger<'d>, uye: &str) -> Nesne<'d> {
    deger.nesne(uye).unwrap()
}

fn al<'d>(nesne_: &Nesne<'d>, ad: &str) -> Nesne<'d> {
    nesne(nesne_.al(ad).unwrap(), ad)
}

/// `yol`'un uzantısı eski (97-2003) biçim mi: `.doc`, `.xls`, `.ppt`.
fn eski_bicim(yol: &Path) -> bool {
    yol.extension().is_some_and(|uzanti| ["doc", "xls", "ppt"].iter().any(|eski| uzanti.eq_ignore_ascii_case(eski)))
}

/// Word'de yeni bir belge açar, `metin`'i yazar ve `.docx` (ya da `.doc`) olarak kaydeder (`parola` ile şifreli).
fn word_belgesi_yaz(yol: &Path, metin: &str, parola: Option<&str>) {
    let (yol, metin, parola) = (yol.to_path_buf(), metin.to_owned(), parola.map(str::to_owned));
    sta(move |daire| {
        let oturum = Oturum::ac(daire, Uygulama::Word).unwrap();
        let belgeler = al(oturum.uygulama(), "Documents");
        // Test yarıda kalırsa belge kaydedilmeden kapanır, Word de kapatılabilir.
        let mut acik = AcikBelge::sar(nesne(belgeler.cagir("Add", &[]).unwrap(), "Add"), Uygulama::Word);
        let belge = acik.belge().unwrap();
        al(belge, "Content").ata("Text", Arg::Metin(&metin)).unwrap();
        // wdFormatDocument (97-2003) ya da wdFormatDocumentDefault
        let bicim = if eski_bicim(&yol) { 0 } else { 16 };
        let mut argumanlar = vec![("FileName", Arg::Yol(&yol)), ("FileFormat", Arg::Tam(bicim))];
        if let Some(parola) = &parola {
            argumanlar.push(("Password", Arg::Metin(parola)));
        }
        belge.cagir_adli("SaveAs2", &argumanlar).unwrap();
        acik.kapat().unwrap();
        drop((acik, belgeler));
        oturum.bitir();
    });
}

/// Excel'de yeni bir kitabın ilk sayfasına birkaç hücre yazar ve `.xlsx` (ya da `.xls`) olarak kaydeder.
fn excel_kitabi_yaz(yol: &Path, parola: Option<&str>) {
    let (yol, parola) = (yol.to_path_buf(), parola.map(str::to_owned));
    sta(move |daire| {
        let oturum = Oturum::ac(daire, Uygulama::Excel).unwrap();
        let kitaplar = al(oturum.uygulama(), "Workbooks");
        let mut acik = AcikBelge::sar(nesne(kitaplar.cagir("Add", &[]).unwrap(), "Add"), Uygulama::Excel);
        let kitap = acik.belge().unwrap();
        let sayfa = nesne(al(kitap, "Worksheets").al_arg("Item", &[Arg::Tam(1)]).unwrap(), "Item");
        let hucreler = [
            ("A1", Arg::Metin(TURKCE)),
            ("B1", Arg::Metin(TURKCE_HARFLER)),
            ("A2", Arg::Metin("Adet")),
            ("B2", Arg::Tam(1234)),
            ("A3", Arg::Metin("Onaylı")),
            ("B3", Arg::Mantik(true)),
        ];
        for (adres, deger) in hucreler {
            nesne(sayfa.al_arg("Range", &[Arg::Metin(adres)]).unwrap(), "Range").ata("Value", deger).unwrap();
        }
        nesne(sayfa.al_arg("Columns", &[Arg::Metin("A:B")]).unwrap(), "Columns").cagir("AutoFit", &[]).unwrap();
        // xlExcel8 (97-2003) ya da xlOpenXMLWorkbook
        let bicim = if eski_bicim(&yol) { 56 } else { 51 };
        let mut argumanlar = vec![("Filename", Arg::Yol(&yol)), ("FileFormat", Arg::Tam(bicim))];
        if let Some(parola) = &parola {
            argumanlar.push(("Password", Arg::Metin(parola)));
        }
        kitap.cagir_adli("SaveAs", &argumanlar).unwrap();
        drop(sayfa);
        acik.kapat().unwrap();
        drop((acik, kitaplar));
        oturum.bitir();
    });
}

/// PowerPoint'te tek slaytlık, penceresiz bir sunum açar, bir metin kutusu ekler ve `.pptx` (ya da
/// `.ppt`) kaydeder.
fn powerpoint_sunumu_yaz(yol: &Path, parola: Option<&str>) {
    let (yol, parola) = (yol.to_path_buf(), parola.map(str::to_owned));
    sta(move |daire| {
        let oturum = Oturum::ac(daire, Uygulama::PowerPoint).unwrap();
        let sunumlar = al(oturum.uygulama(), "Presentations");
        // WithWindow = msoFalse
        let mut acik = AcikBelge::sar(nesne(sunumlar.cagir("Add", &[Arg::Tam(0)]).unwrap(), "Add"), Uygulama::PowerPoint);
        let sunum = acik.belge().unwrap();
        // Index 1, ppLayoutBlank
        let slayt = nesne(al(sunum, "Slides").cagir("Add", &[Arg::Tam(1), Arg::Tam(12)]).unwrap(), "Add");
        // msoTextOrientationHorizontal, sol, üst, genişlik, yükseklik
        let kutu = al(&slayt, "Shapes").cagir("AddTextbox", &[Arg::Tam(1), Arg::Tam(40), Arg::Tam(40), Arg::Tam(640), Arg::Tam(200)]).unwrap();
        let kutu = nesne(kutu, "AddTextbox");
        al(&al(&kutu, "TextFrame"), "TextRange").ata("Text", Arg::Metin(&format!("{TURKCE}\r{TURKCE_HARFLER}"))).unwrap();
        if let Some(parola) = &parola {
            sunum.ata("Password", Arg::Metin(parola)).unwrap();
        }
        // ppSaveAsPresentation (97-2003) ya da ppSaveAsOpenXMLPresentation
        let bicim = if eski_bicim(&yol) { 1 } else { 24 };
        sunum.cagir("SaveAs", &[Arg::Yol(&yol), Arg::Tam(bicim)]).unwrap();
        drop((kutu, slayt));
        acik.kapat().unwrap();
        drop((acik, sunumlar));
        oturum.bitir();
    });
}

/// `yol`'daki belgeyi Word'de salt okunur açıp bütün metnini okur.
fn word_metni_oku(yol: &Path) -> String {
    let yol = yol.to_path_buf();
    sta(move |daire| {
        let oturum = Oturum::ac(daire, Uygulama::Word).unwrap();
        let belge = oturum.belge_ac(&yol, None).unwrap();
        let metin = match al(belge.belge().unwrap(), "Content").al("Text").unwrap() {
            Deger::Metin(metin) => metin,
            _ => panic!("Word belgenin metnini vermedi"),
        };
        drop(belge);
        oturum.bitir();
        metin
    })
}

/// Boşlukları ve Word'ün denetim karakterlerini (paragraf, tablo hücresi) tek boşluğa indirir.
fn sade(metin: &str) -> String {
    metin.split(|harf: char| harf.is_whitespace() || harf.is_control()).filter(|parca| !parca.is_empty()).collect::<Vec<_>>().join(" ")
}

/// PDF var, PDF gibi başlıyor ve bitiyor, boş değil.
fn pdf_denetle(yol: &Path) {
    let baytlar = std::fs::read(yol).unwrap();
    assert!(baytlar.starts_with(b"%PDF-"), "PDF başlığı yok: {}", yol.display());
    let son = baytlar.get(baytlar.len().saturating_sub(1024)..).unwrap_or_default();
    assert!(son.windows(5).any(|parca| parca == b"%%EOF"), "PDF sonu yok: {}", yol.display());
    assert!(baytlar.len() > 1000, "PDF çok küçük: {} bayt", baytlar.len());
    eprintln!("{}: {} bayt, başlık {:?}", yol.display(), baytlar.len(), String::from_utf8_lossy(baytlar.get(..8).unwrap_or_default()));
}

/// PDF'i Word'e geri çevirir, belgeyi Word'de açıp metnini okur ve Türkçe harfleri arar.
fn turkce_harfleri_denetle(klasor: &mut CalismaKlasoru, pdf: &Path, ad: &str) {
    let docx = klasor.dosya(&format!("{ad} geri.docx"));
    let baslangic = Instant::now();
    pdften_worde_sureli(pdf, &docx, SURE).unwrap();
    eprintln!("PDF → Word ({ad}): {:?}", baslangic.elapsed());
    assert!(std::fs::read(&docx).unwrap().starts_with(b"PK\x03\x04"), "docx bir ZIP paketi değil");
    let metin = sade(&word_metni_oku(&docx));
    eprintln!("PDF'ten geri okunan metin ({ad}): {metin}");
    assert!(metin.contains(TURKCE), "\"{TURKCE}\" yok: {metin}");
    for harf in TURKCE_HARFLER.chars().filter(|harf| !harf.is_whitespace()) {
        assert!(metin.contains(harf), "'{harf}' yok: {metin}");
    }
}

/// Dönüştürmenin geçici klasörü silinmiş mi: bu süreçte testin kendi klasöründen başka kalmamalı.
fn gecici_klasor_kalmadi(klasor: &CalismaKlasoru) {
    let onek = format!("migez-office-{}-", std::process::id());
    let kalanlar: Vec<PathBuf> = std::fs::read_dir(std::env::temp_dir())
        .unwrap()
        .flatten()
        .map(|girdi| girdi.path())
        .filter(|yol| yol.file_name().is_some_and(|ad| ad.to_string_lossy().starts_with(&onek)) && yol != klasor.yol())
        .collect();
    assert!(kalanlar.is_empty(), "geçici klasör kaldı: {kalanlar:?}");
}

#[test]
#[ignore = "Microsoft Office gerektirir"]
fn office_kurulu_gorunur() {
    let durum = mevcut();
    eprintln!("{durum:?}");
    assert!(durum.word && durum.excel && durum.powerpoint, "{durum:?}");
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn word_belgesi_pdfe_doner_ve_turkce_harfler_korunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("Türkçe belge.docx");
    let pdf = klasor.dosya("Türkçe belge.pdf");
    word_belgesi_yaz(&docx, &format!("{TURKCE}\r{TURKCE_HARFLER}"), None);
    let baslangic = Instant::now();
    pdfe_donustur(&docx, &pdf).unwrap();
    eprintln!("Word → PDF: {:?}", baslangic.elapsed());
    pdf_denetle(&pdf);
    turkce_harfleri_denetle(&mut klasor, &pdf, "word");
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Excel ve Word gerektirir"]
fn excel_tablosu_pdfe_doner_ve_turkce_harfler_korunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    // Excel köşeli ayraçlı adla kaydedemez ve açamaz; bu ad dönüştürmenin kopyasında düzeltilir.
    let kaydedilen = klasor.dosya("Tablo.xlsx");
    let xlsx = klasor.dosya("Tablo [2026].xlsx");
    let pdf = klasor.dosya("Tablo.pdf");
    excel_kitabi_yaz(&kaydedilen, None);
    std::fs::rename(&kaydedilen, &xlsx).unwrap();
    let baslangic = Instant::now();
    pdfe_donustur(&xlsx, &pdf).unwrap();
    eprintln!("Excel → PDF: {:?}", baslangic.elapsed());
    pdf_denetle(&pdf);
    turkce_harfleri_denetle(&mut klasor, &pdf, "excel");
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft PowerPoint ve Word gerektirir"]
fn powerpoint_slayti_pdfe_doner_ve_turkce_harfler_korunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let pptx = klasor.dosya("Sunum.pptx");
    let pdf = klasor.dosya("Sunum.pdf");
    powerpoint_sunumu_yaz(&pptx, None);
    let baslangic = Instant::now();
    pdfe_donustur(&pptx, &pdf).unwrap();
    eprintln!("PowerPoint → PDF: {:?}", baslangic.elapsed());
    pdf_denetle(&pdf);
    turkce_harfleri_denetle(&mut klasor, &pdf, "powerpoint");
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn bomsuz_utf8_metin_dosyasi_turkce_okunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let txt = klasor.dosya("metin.txt");
    let pdf = klasor.dosya("metin.pdf");
    std::fs::write(&txt, format!("{TURKCE}\r\n{TURKCE_HARFLER}\r\n")).unwrap();
    pdfe_donustur(&txt, &pdf).unwrap();
    pdf_denetle(&pdf);
    turkce_harfleri_denetle(&mut klasor, &pdf, "txt");
}

#[test]
#[ignore = "Microsoft Excel ve Word gerektirir"]
fn csv_bolge_ayarlariyla_hucrelere_bolunur() {
    let _sira = sira();
    // Windows'un liste ayracı (Türkçede ";"): Excel CSV'yi bununla bölmeli (Local = True).
    let ayrac = windows_registry::CURRENT_USER
        .open(r"Control Panel\International")
        .and_then(|anahtar| anahtar.get_string("sList"))
        .ok()
        .and_then(|liste| liste.chars().next())
        .unwrap_or(',');
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let csv = klasor.dosya("veri.csv");
    let pdf = klasor.dosya("veri.pdf");
    // BOM'lu UTF-8: Excel CSV'yi BOM'suz olunca ANSI sayar. Uzun metinler sağındaki hücre boş
    // kalacak yerde: dolu bir komşu hücre taşan metni basılı sayfada keser.
    std::fs::write(&csv, format!("\u{FEFF}Ad{ayrac}Kod\r\n{TURKCE}\r\n{ayrac}{TURKCE_HARFLER}\r\nBir{ayrac}2\r\n")).unwrap();
    pdfe_donustur(&csv, &pdf).unwrap();
    pdf_denetle(&pdf);
    let docx = klasor.dosya("veri geri.docx");
    pdften_worde_sureli(&pdf, &docx, SURE).unwrap();
    let metin = sade(&word_metni_oku(&docx));
    eprintln!("CSV (ayraç {ayrac:?}) PDF'ten geri: {metin}");
    assert!(metin.contains(TURKCE), "{metin}");
    for harf in TURKCE_HARFLER.chars().filter(|harf| !harf.is_whitespace()) {
        assert!(metin.contains(harf), "'{harf}' yok: {metin}");
    }
    assert!(!metin.contains(ayrac), "satırlar hücrelere bölünmedi: {metin}");
}

#[test]
#[ignore = "Microsoft Office gerektirir"]
fn parolali_belgeler_pencere_acmadan_parola_hatasi_verir() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("parolalı.docx");
    let xlsx = klasor.dosya("parolalı.xlsx");
    let pptx = klasor.dosya("parolalı.pptx");
    word_belgesi_yaz(&docx, TURKCE, Some(DENEME_PAROLASI));
    excel_kitabi_yaz(&xlsx, Some(DENEME_PAROLASI));
    powerpoint_sunumu_yaz(&pptx, Some(DENEME_PAROLASI));
    for girdi in [&docx, &xlsx, &pptx] {
        let pdf = klasor.dosya("parolalı.pdf");
        let baslangic = Instant::now();
        let sonuc = pdfe_donustur_sureli(girdi, &pdf, Duration::from_secs(60));
        eprintln!("{}: {sonuc:?} ({:?})", girdi.display(), baslangic.elapsed());
        assert_eq!(sonuc, Err(OfficeHatasi::ParolaKorumali), "{}", girdi.display());
        assert!(!pdf.exists());
    }
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Office gerektirir"]
fn eski_bicimler_pdfe_doner() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let doc = klasor.dosya("eski.doc");
    let xls = klasor.dosya("eski.xls");
    let ppt = klasor.dosya("eski.ppt");
    word_belgesi_yaz(&doc, TURKCE, None);
    excel_kitabi_yaz(&xls, None);
    powerpoint_sunumu_yaz(&ppt, None);
    for girdi in [&doc, &xls, &ppt] {
        let pdf = klasor.dosya("eski.pdf");
        let baslangic = Instant::now();
        pdfe_donustur(girdi, &pdf).unwrap();
        eprintln!("{} → PDF: {:?}", girdi.display(), baslangic.elapsed());
        pdf_denetle(&pdf);
        std::fs::remove_file(&pdf).unwrap();
    }
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Office gerektirir"]
fn eski_bicimli_parolali_belgeler_takilmadan_hata_verir() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let doc = klasor.dosya("parolalı.doc");
    let xls = klasor.dosya("parolalı.xls");
    let ppt = klasor.dosya("parolalı.ppt");
    word_belgesi_yaz(&doc, TURKCE, Some(DENEME_PAROLASI));
    excel_kitabi_yaz(&xls, Some(DENEME_PAROLASI));
    powerpoint_sunumu_yaz(&ppt, Some(DENEME_PAROLASI));
    for girdi in [&doc, &xls, &ppt] {
        let pdf = klasor.dosya("parolalı.pdf");
        let baslangic = Instant::now();
        let sonuc = pdfe_donustur_sureli(girdi, &pdf, Duration::from_secs(60));
        eprintln!("{}: {sonuc:?} ({:?})", girdi.display(), baslangic.elapsed());
        if let Err(hata) = &sonuc {
            eprintln!("kullanıcıya: {hata}");
        }
        assert_eq!(sonuc, Err(OfficeHatasi::ParolaKorumali), "{}", girdi.display());
        assert!(!pdf.exists());
    }
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn zaman_asimindan_sonra_office_kapanir_ve_sira_birakilir() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("belge.docx");
    let pdf = klasor.dosya("belge.pdf");
    word_belgesi_yaz(&docx, TURKCE, None);
    // Çağıran hemen vazgeçer; iş parçacığı Word'ü açar, vazgeçildiğini görüp belgeyi açmadan kapatır.
    let sonuc = pdfe_donustur_sureli(&docx, &pdf, Duration::from_millis(1));
    assert_eq!(sonuc, Err(OfficeHatasi::ZamanAsimi { saniye: 1 }));
    assert!(!pdf.exists());
    // Sonraki dönüştürme önceki işin bitmesini bekler ve olağan biçimde tamamlanır.
    let baslangic = Instant::now();
    pdfe_donustur(&docx, &pdf).unwrap();
    eprintln!("zaman aşımından sonraki dönüştürme: {:?}", baslangic.elapsed());
    pdf_denetle(&pdf);
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft PowerPoint gerektirir"]
fn kullanicinin_acik_sunumu_ve_powerpointi_korunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let pptx = klasor.dosya("Sunum.pptx");
    let pdf = klasor.dosya("Sunum.pdf");
    powerpoint_sunumu_yaz(&pptx, None);
    // "Kullanıcı": PowerPoint'te açık, kaydedilmemiş bir sunum. PowerPoint tek örneklidir;
    // dönüştürme aynı PowerPoint'e bağlanır.
    let (hazir_gonder, hazir_al) = sync_channel(0);
    let (bitti_gonder, bitti_al) = sync_channel::<()>(0);
    let kullanici = std::thread::spawn(move || {
        let daire = Daire::baslat().unwrap();
        let powerpoint = Nesne::olustur(&daire, "PowerPoint.Application").unwrap();
        let sunumlar = al(&powerpoint, "Presentations");
        let sunum = nesne(sunumlar.cagir("Add", &[Arg::Tam(0)]).unwrap(), "Add");
        let ad = match sunum.al("Name").unwrap() {
            Deger::Metin(ad) => ad,
            _ => panic!("sunumun adı yok"),
        };
        hazir_gonder.send(()).unwrap();
        bitti_al.recv().unwrap();
        // Dönüştürmeden sonra sunum hâlâ açık ve PowerPoint çalışıyor.
        let sonra_ad = sunum.al("Name").map(|deger| matches!(deger, Deger::Metin(sonra) if sonra == ad));
        let sayi = sunumlar.al("Count").ok().and_then(|deger| deger.tam());
        // Temizlik: "kullanıcının" sunumunu kapat, PowerPoint'i yalnız boşsa kapat.
        let _ = sunum.ata("Saved", Arg::Tam(-1));
        let _ = sunum.cagir("Close", &[]);
        if sunumlar.al("Count").ok().and_then(|deger| deger.tam()) == Some(0) {
            let _ = powerpoint.cagir("Quit", &[]);
        }
        (sonra_ad.ok(), sayi)
    });
    hazir_al.recv_timeout(Duration::from_secs(60)).unwrap();
    let sonuc = pdfe_donustur(&pptx, &pdf);
    bitti_gonder.send(()).unwrap();
    let (ayni_sunum, sayi) = kullanici.join().unwrap();
    sonuc.unwrap();
    pdf_denetle(&pdf);
    assert_eq!(ayni_sunum, Some(true), "kullanıcının sunumu kapandı");
    assert_eq!(sayi, Some(1), "dönüştürme yalnız kendi sunumunu kapattı");
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn kullanicinin_acik_word_belgesi_korunur() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("belge.docx");
    let pdf = klasor.dosya("belge.pdf");
    word_belgesi_yaz(&docx, TURKCE, None);
    let (hazir_gonder, hazir_al) = sync_channel(0);
    let (bitti_gonder, bitti_al) = sync_channel::<()>(0);
    let kullanici = std::thread::spawn(move || {
        let daire = Daire::baslat().unwrap();
        let word = Nesne::olustur(&daire, "Word.Application").unwrap();
        let belgeler = al(&word, "Documents");
        let belge = nesne(belgeler.cagir("Add", &[]).unwrap(), "Add");
        al(&belge, "Content").ata("Text", Arg::Metin("kaydedilmemiş iş")).unwrap();
        hazir_gonder.send(()).unwrap();
        bitti_al.recv().unwrap();
        let metin = al(&belge, "Content").al("Text").ok().and_then(|deger| match deger {
            Deger::Metin(metin) => Some(metin),
            _ => None,
        });
        let sayi = belgeler.al("Count").ok().and_then(|deger| deger.tam());
        let _ = belge.cagir("Close", &[Arg::Tam(0)]);
        if belgeler.al("Count").ok().and_then(|deger| deger.tam()) == Some(0) {
            let _ = word.cagir("Quit", &[Arg::Tam(0)]);
        }
        (metin, sayi)
    });
    hazir_al.recv_timeout(Duration::from_secs(60)).unwrap();
    let sonuc = pdfe_donustur(&docx, &pdf);
    bitti_gonder.send(()).unwrap();
    let (metin, sayi) = kullanici.join().unwrap();
    sonuc.unwrap();
    pdf_denetle(&pdf);
    assert_eq!(metin.as_deref().map(sade), Some("kaydedilmemiş iş".to_owned()), "kullanıcının belgesi kapandı ya da değişti");
    assert_eq!(sayi, Some(1));
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn internetten_gelmis_belge_takilmadan_sonuclanir() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("indirilen.docx");
    let pdf = klasor.dosya("indirilen.pdf");
    word_belgesi_yaz(&docx, TURKCE, None);
    // Windows'un internetten gelen dosyaya koyduğu işaret (Mark of the Web): Office bunu görünce
    // dosyayı Korumalı Görünüm'de açar.
    let mut isaret = docx.as_os_str().to_owned();
    isaret.push(":Zone.Identifier");
    std::fs::write(&isaret, "[ZoneTransfer]\r\nZoneId=3\r\n").unwrap();
    let sonuc = pdfe_donustur_sureli(&docx, &pdf, Duration::from_secs(60));
    eprintln!("internetten gelmiş belge: {sonuc:?}");
    assert!(!matches!(sonuc, Err(OfficeHatasi::ZamanAsimi { .. })), "{sonuc:?}");
    if sonuc.is_ok() {
        pdf_denetle(&pdf);
    }
    gecici_klasor_kalmadi(&klasor);
}

#[test]
#[ignore = "Microsoft Word gerektirir"]
fn bozuk_belge_pencere_acmadan_hata_verir() {
    let _sira = sira();
    let mut klasor = CalismaKlasoru::yeni().unwrap();
    let docx = klasor.dosya("bozuk.docx");
    let pdf = klasor.dosya("bozuk.pdf");
    std::fs::write(&docx, b"PK\x03\x04 bu bir Word belgesi degil").unwrap();
    let sonuc = pdfe_donustur_sureli(&docx, &pdf, Duration::from_secs(60));
    eprintln!("bozuk belge: {sonuc:?}");
    if let Err(hata) = &sonuc {
        eprintln!("kullanıcıya: {hata}");
    }
    assert!(matches!(sonuc, Err(OfficeHatasi::DosyaAcilamadi { uygulama: Uygulama::Word, .. })), "{sonuc:?}");
    assert!(!pdf.exists());
    gecici_klasor_kalmadi(&klasor);
}
