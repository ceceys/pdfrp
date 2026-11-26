# migez-office

Word, Excel ve PowerPoint belgelerini bilgisayarda kurulu **Microsoft Office** ile PDF'e, PDF'i de
Word ile `.docx`'e çevirir. Yalnız Windows'ta çalışır (COM otomasyonu, IDispatch ile geç bağlama).
Başka sistemlerde derlenir, dönüştürme işlevleri `OfficeHatasi::Desteklenmiyor` döner.

LibreOffice ile dönüştürmenin (`migez_create::office`) yanında ikinci bir yoldur. Motor
`migez_engine::ms_office` üzerinden kullanır; arayüzde Dosya ▸ Aç, PDF oluştur ▸ Tek dosya ve
PDF'yi dışa aktar ▸ Word buna bağlıdır (`crates/ui-egui/src/workflows/office_ui.rs`), otomasyonda
`office_status`, `office_to_pdf` ve `office_pdf_to_word` araçları.

## API

```rust
pub fn mevcut() -> OfficeDurumu;
pub fn uygulama_sec(girdi: &Path) -> Option<Uygulama>;
pub fn pdfe_donustur(girdi: &Path, cikti: &Path) -> Result<(), OfficeHatasi>;
pub fn pdfe_donustur_sureli(girdi: &Path, cikti: &Path, sure: Duration) -> Result<(), OfficeHatasi>;
pub fn pdften_worde(girdi: &Path, cikti: &Path) -> Result<(), OfficeHatasi>;
pub fn pdften_worde_sureli(girdi: &Path, cikti: &Path, sure: Duration) -> Result<(), OfficeHatasi>;
```

- `mevcut()`: hangi uygulamaların kurulu olduğu (`OfficeDurumu { word, excel, powerpoint }`,
  `var_mi(Uygulama)`, `herhangi()`). Kayıt defterine bakar: ProgID → CLSID → `LocalServer32` →
  kayıtlı program diskte mi. Office'i başlatmaz, COM gerekmez, hızlıdır.
- `pdfe_donustur`: uzantıya göre uygulamayı seçer (büyük küçük harf fark etmez).
  - Word: `doc`, `docx`, `rtf`, `odt`, `txt` (`WORD_UZANTILARI`)
  - Excel: `xls`, `xlsx`, `ods`, `csv` (`EXCEL_UZANTILARI`), bütün sayfalar
  - PowerPoint: `ppt`, `pptx`, `odp` (`POWERPOINT_UZANTILARI`), slaytlar
- `pdften_worde`: Word'ün PDF açma (PDF Reflow) özelliğiyle `.docx` üretir.
- `cikti` varsa yerine yazılır; klasörü var olmalıdır. Girdi ile çıktı aynı dosya olamaz.
- Süreler: `PDF_SURESI` 120 sn, `WORD_SURESI` 300 sn. `_sureli` sürümler süreyi alır; süre, önceki
  bir dönüştürmenin bitmesini beklemeyi de kapsar.
- `Uygulama` (`Word`, `Excel`, `PowerPoint`; `ad()` "Microsoft Word" gibi, `uzantilar()`, `HEPSI`).

Hatalar (`OfficeHatasi`, `#[non_exhaustive]`): `Desteklenmiyor`, `BicimDesteklenmiyor { uzanti }`,
`UygulamaYok(Uygulama)`, `UygulamaBaslatilamadi`, `DosyaYok(PathBuf)`, `AyniDosya`, `CiktiYazilamadi`,
`ParolaKorumali`, `DosyaAcilamadi`, `DonusturmeBasarisiz`, `ZamanAsimi { saniye }`, `Mesgul`,
`BaglantiKoptu`, `Ic`. `Display` kullanıcıya gösterilebilecek Türkçe bir ileti verir; `ayrinti`
alanlarında Office'in kendi iletisi durur ve ayraç içinde eklenir, örneğin:
"Microsoft Word dosyayı açamadı. (Dosya bozuk görünüyor.)".

## Nasıl çalışır

1. Yollar denetlenir (girdi var mı, çıktının klasörü var mı, ikisi aynı mı), uygulamanın kurulu olduğu
   kayıt defterinden okunur. Bunlar Office'e gitmeden hata verir.
2. İş kendi STA iş parçacığında çalışır (`CoInitializeEx`, `COINIT_APARTMENTTHREADED`). Çağıran en çok
   verilen süre kadar bekler. Aynı anda tek bir dönüştürme Office'le konuşur; sırası gelmeyen bekler.
3. Girdi `%TEMP%\migez-office-…` altındaki kendi klasörüne kopyalanır ve Office kopyayı açar.
   Kullanıcının dosyası kilitlenmez; kullanıcı aynı dosyayı Office'te açık tutuyorsa onunla
   karışmaz. Kopya kaynağın adını taşır (Office onu üst ve alt bilgilere yazabilir). Excel'in
   açamadığı köşeli ayraçlar normal ayraca çevrilir, 120 karakterden uzun ad `belge.<uzantı>` olur.
4. Office uygulaması görünmez çalışır: kendi başlattığımız Word ve Excel'de `Visible = false`.
   PowerPoint'te `Visible`'a dokunulmaz (kodla başlatılan PowerPoint zaten görünmez), sunum
   penceresiz açılır (`WithWindow = msoFalse`).
5. Makrolar kapalıdır: iş süresince `AutomationSecurity = msoAutomationSecurityForceDisable`.
   Kapatılamazsa belge açılmaz. Uygulama kapatılmıyorsa eski değer iş bitince geri yüklenir.
6. Belge salt okunur ve iletişim kutusu açılmadan açılır:
   - Word: `Documents.OpenNoRepairDialog` (yoksa `Open`), `ConfirmConversions = false`,
     `ReadOnly`, `AddToRecentFiles = false`, `Visible = false`, `NoEncodingDialog = true`,
     `Revert = false`. BOM'suz UTF-8 bir `.txt` için kodlama UTF-8 verilir.
   - Excel: `Workbooks.Open`, `UpdateLinks = 0` (dış bağlantılar güncellenmez), `ReadOnly`,
     `IgnoreReadOnlyRecommended`, `AddToMru = false`, `Local = true` (CSV kullanıcının bölge
     ayarlarıyla okunur; Türkçede alan ayracı `;`).
   - PowerPoint: `Presentations.Open(ReadOnly, Untitled = false, WithWindow = false)`.
   - Parola: Office'e bilerek yanlış bir parola verilir (PowerPoint'te `dosya::parola::` biçimi).
     Parolasız belgede yok sayılır; parolalı belgede parola penceresi açılmaz, `ParolaKorumali` döner.
7. Dışa aktarma: Word ve Excel `ExportAsFixedFormat` (PDF, baskı kalitesi, belge özellikleri; Word'de
   başlıklardan yer imleri ve etiketli PDF), PowerPoint `SaveAs(…, ppSaveAsPDF)`. PDF'ten Word'e:
   `SaveAs2(…, wdFormatDocumentDefault)`.
8. Sonuç önce çalışma klasörüne yazılır, türü denetlenir (`%PDF-`, `.docx` için ZIP imzası), sonra
   hedefe tek adımda taşınır (başka birimdeyse hedefin yanına kopyalanıp üzerine adlandırılır). Yarım
   bir dosya hiçbir zaman hedefin adını taşımaz.
9. Belge kaydedilmeden kapatılır, ayarlar geri yüklenir ya da uygulama kapatılır (aşağıda), çalışma
   klasöründeki dosyalar tek tek silinir ve boşalan klasör kaldırılır.

Office meşgulken reddettiği çağrılar (`RPC_E_CALL_REJECTED`, `RPC_E_SERVERCALL_RETRYLATER`) 30 saniye
boyunca yinelenir.

## Kullanıcının belgelerini koruma

PowerPoint tek örneklidir: kullanıcının PowerPoint'i açıksa dönüştürme ona bağlanır. Word ve Excel
de çalışan bir örneğe bağlanabilir. Bu yüzden:

- **Yalnız kendi açtığımız belge kapatılır.** Belge, `Open`'ın döndürdüğü nesne üzerinden kapatılır
  (Word `Close(wdDoNotSaveChanges)`, Excel `Close(false)`, PowerPoint `Saved = msoTrue` ve `Close`).
  Belgeler koleksiyonunda başka hiçbir belgeye dokunulmaz. Açtığımız dosya her zaman çalışma
  klasöründeki kopyadır, kullanıcının açık tuttuğu bir yolla aynı olamaz.
- **`Quit` yalnız şu durumda çağrılır:** bağlanıldığı anda uygulama görünmüyordu ve açık belge,
  çalışma kitabı ya da sunum sayısı (Korumalı Görünüm pencereleri dahil) 0'dı; iş bitince de
  görünmüyor ve sayı yine 0. Bu değerlerden biri okunamazsa uygulama kullanıcınınmış sayılır ve
  açık bırakılır.
- **Kullanıcının açık belgesi varsa ya da uygulama ekrandaysa** uygulama açık bırakılır, `Visible`
  değiştirilmez. Ekrandaki uygulamada `DisplayAlerts` de değiştirilmez: uygulamanın tamamına işler,
  kullanıcının oturumu değiştirilmez. Görünmeyen uygulamada uyarılar kapatılır ve iş bitince eski
  değeri geri yüklenir (uygulama kapatılmıyorsa).
- **Ekranda pencere ya da iletişim kutusu kalmaz:** belgeler görünmez ya da penceresiz açılır,
  parola ve onarım pencereleri yukarıdaki bağımsız değişkenlerle önlenir (ekrandaki bir PowerPoint'e
  bağlanıldığında ne olabileceği "Sınırlar"da).

Testler bunu gerçek Office'le sınar: "kullanıcının" PowerPoint'inde açık, kaydedilmemiş bir sunum
ve Word'de açık, kaydedilmemiş bir belge varken dönüştürme yapılır; ikisi de açık ve değişmemiş
kalır, uygulama kapanmaz.

## unsafe

Çalışma alanı unsafe kodu yasaklar (`unsafe_code = "forbid"`) ve gerektiğinde bakımı
süren güvenli bir sarmalayıcı ister. COM otomasyonu için uygun bir güvenli bağlama yok:

- `windows` crate'inde COM çağrıları (`CoCreateInstance`, `IDispatch::Invoke`) ve VARIANT
  birliğinden okuma `unsafe`tır.
- Ağaçtaki `winsafe` (0.0.29) IDispatch'i güvenli sarar, ama adlı ve eksik bağımsız değişken
  veremez, bilmediği VARIANT türünde panikler ve Office boş bir nesne (`VT_DISPATCH` + boş işaretçi)
  döndürürse boş işaretçide `AddRef` çağırıp süreci çökertir.

Bu yüzden paket kendi `[lints]` tablosunu taşır: çalışma alanınınkilerin aynısı, yalnız
`unsafe_code = "deny"` (yasak değil red) ve `clippy::undocumented_unsafe_blocks = "deny"`. `unsafe`
yalnız `src/com.rs`'te izinlidir. Orada her blok tek bir COM çağrısını ya da tek bir VARIANT
okumasını sarar ve neden güvenli olduğunu yazar. Dışarıya güvenli bir API verir: COM nesneleri
`Daire`'den (COM'un başlatıldığı iş parçacığı) uzun yaşayamaz ve iş parçacığından çıkamaz;
VARIANT'lar `VariantClear` ile bırakılır. **Bu istisna depo sahibinin onayını ister.**

## Sınırlar

- Yalnız Windows; Office kurulu ve etkinleştirilmiş olmalı. İlk açılış, etkinleştirme ya da
  lisans pencereleri dönüştürmeyi süre dolana dek bekletebilir.
- Bir COM çağrısı yarıda kesilemez. Süre dolunca çağırana `ZamanAsimi` döner. İş parçacığı Office'in
  yanıtını bekler, sonra kalan adımları atlayıp temizlik yapar. O sırada yeni dönüştürmeler sıra
  bekler ve süreleri dolarsa `Mesgul` alır. Takılan Office süreci öldürülmez, kullanıcınınki olabilir.
- Sıra süreç içindir. İki ayrı süreç (örneğin uygulama ve `migez-cli`) aynı anda PowerPoint'le
  dönüştürürse aynı PowerPoint'e bağlanırlar: birbirlerinin sunumunu kapatmazlar, ama ikisi de
  uygulamayı kendisinin saymadığı için görünmez bir PowerPoint açık kalabilir.
- **Korumalı Görünüm devre dışı kalır.** Kodla açılan belge, internetten gelmiş olsa bile
  (Zone.Identifier, ZoneId=3) Korumalı Görünüm'e girmeden açılır; test bunu gösterdi. Makrolar
  kapalıdır, ama dış bağlantılar, uzak şablonlar ve gömülü nesneler Office'in Güven Merkezi
  ayarlarına kalır. Excel'de dış bağlantılar güncellenmez; Word için ayrı bir ayar yapılmaz.
- Parolalı dosya şöyle tanınır: yeni biçimler (`docx`, `xlsx`, `pptx`) imzalarından (şifreli olunca
  ZIP değil bileşik belgedir), Word belgeleri 5408 hatasından. Excel'in parola hatası genel 1004
  (`0x800A03EC`), PowerPoint'inki genel `E_FAIL` döner; eski biçimli `.xls` ve `.ppt`'de parola hatası
  Office'in iletisinden tanınır (Türkçe ve İngilizce Office). Başka dillerde `DosyaAcilamadi` döner,
  Office'in iletisi görünür.
- IRM (erişim izni) ile korunan belgeler ve 32 bitlik Office denenmedi.
- Excel: `ExportAsFixedFormat` yazdırma alanlarını uygular (`IgnorePrintAreas = false`). BOM'lu
  UTF-8 CSV'deki Türkçe harfler doğru çıkar (testte denendi); BOM'suz CSV'yi hangi kodlamayla
  okuyacağı Excel'e kalır. Excel 218 karakterden uzun yolları açamaz (kendi iletisi böyle diyor);
  kopya klasörü kısa tutulur, ama `%TEMP%` çok uzunsa yine olabilir.
- Excel'de dolu bir komşu hücre taşan metni basılı sayfada keser; PDF bunu olduğu gibi gösterir.
- Çıktının boyutu sınırlanmaz (LibreOffice yolu 512 MB'ta keser).
- Açtığımız belge bir nedenle kapatılamazsa (hiç görülmedi) uygulama kapatılmaz; görünmez bir Office
  süreci ve çalışma klasöründe kopya kalabilir. Günlüğe yazılır.
- Kullanıcının ekrandaki PowerPoint'ine bağlanıldığında uyarılar açık kalır: bozuk bir sunumun onarım
  sorusu kullanıcının ekranında çıkabilir; yanıtlanana dek dönüştürme bekler.

## Arayüze bağlarken

- Çağrıları kare iş parçacığında değil, arka planda yapın: süre dolana dek bekleyebilirler.
- `mevcut()` sonucunu menü ve seçenekleri göstermek için kullanın; açılışta bir kez okuyup saklayın.
  Office yoksa ya da uygulama eksikse LibreOffice yoluna (`migez_create::office`) düşün.
- Dönüştürmeleri sıraya koyun; aynı anda birden çok çağrı `Mesgul` alabilir. `ZamanAsimi` ve `Mesgul`
  için "yeniden dene" sunun.
- Hata iletisi için `OfficeHatasi`'nın `Display`'ini kullanın. `ParolaKorumali` için parolayı
  kaldırmasını söyleyin; parola sorup Office'e iletme bu pakette yok.
- Çıktı varsa üzerine yazılır: sormak arayüzün işidir. Varsayılan ad aynı klasörde `.pdf` /
  `.docx` olabilir.
- Girdi internetten geldiyse (`migez_platform::zone::read_zone`) kullanıcıyı uyarın: Office
  belgeyi Korumalı Görünüm'süz açacak. Çıktıya girdinin işaretini verin:
  `migez_platform::zone::mark_from(cikti, Some(girdi))`.
- Çalışma alanının kuralları: `xtask/src/layers.rs` `TABLE`'a bu paket eklenmeli (örneğin
  `("office", Class::Layer(4))`; çalışma alanından bağımlılığı yok), yoksa `cargo xtask layers`
  "unknown workspace crate" der. Her özellik için bir otomasyon aracı (`crates/automation`) ve
  uçtan uca testi gerekir.

## Testler

Office gerektirmeyen birim testleri her çalışmada koşar. Office gerektirenler `#[ignore]`dur ve
sırayla çalışır:

```
cargo test -p migez-office -- --ignored
```

Test belgeleri Office'in kendisiyle üretilir (Word'de yeni belge, Excel'de tablo, PowerPoint'te tek
slayt; parolalı ve eski biçimli sürümleri de), testin geçici klasöründe durur ve tek tek silinir.
PDF'teki Türkçe harfler PDF Word'e geri çevrilip metni Word'le okunarak denetlenir.
