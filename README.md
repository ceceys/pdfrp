<p align="center">
  <img src="assets/pdfrp/yazi-acik-800.png" alt="Pdfrp" width="400">
</p>

<p align="center"><b>Windows için Türkçe, açık kaynak PDF görüntüleyici ve düzenleyici</b></p>

<p align="center">
  <a href="https://github.com/ceceys/pdfrp/releases/latest"><img src="https://img.shields.io/github/v/release/ceceys/pdfrp?label=s%C3%BCr%C3%BCm&color=0b8a7f" alt="Son sürüm"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011%20(64%20bit)-0078D4" alt="Windows 10 ve 11, 64 bit">
  <a href="LICENSE"><img src="https://img.shields.io/badge/lisans-MIT-1f6feb" alt="MIT lisansı"></a>
</p>

<h3 align="center">
  <a href="https://github.com/ceceys/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe">⬇ Pdfrp'yi indir</a>
</h3>

<p align="center">
  Pdfrp 1.2.0 · Windows 10 ve 11, 64 bit · yaklaşık 25 MB<br>
  macOS ve Linux sürümleri de eklenecektir.<br>
  <a href="https://github.com/ceceys/pdfrp/releases">Tüm sürümler</a> · <a href="#kurulum">Kurulum</a> · <a href="#english">English</a>
</p>

# Pdfrp

Pdfrp, Windows için Türkçe arayüzlü, açık kaynak bir PDF görüntüleyici ve düzenleyicidir.
PDF okuma, yorumlama, form doldurma ve imzalama, sayfa düzenleme, birleştirme ve bölme, metin ve
resim düzenleme, karartma, parola koruması, Türkçe metin tanıma ve çevrimdışı çeviri yapar.
Belgeler bilgisayarda kalır: program hesap istemez, kullanım verisi göndermez. Pdfrp internete
yalnız iki iş için bağlanır: yeni sürüm denetimi ve sizin onayladığınız dil modüllerinin
indirilmesi. İkisi de Pdfrp'nin GitHub sayfasından yapılır. İstekte, istenen adres dışında yalnız
program adı ve sürümü bulunur; dil modülü indirilirken bu adres hangi modülün indirildiğini
gösterir. GitHub, her web isteğinde olduğu gibi IP adresinizi görür. Çeviri tamamen
bilgisayarınızda yapılır; belgeleriniz ve çevirdiğiniz metin hiçbir yere gönderilmez. Programın
adı “Pdf For Friday” ifadesinden gelir.

Araç düzeni, masaüstü PDF programlarına alışık kullanıcıların yadırgamayacağı biçimde kuruldu: solda araçlar, üstte sekmeler, sağda panel.

<p align="center">
  <img src="docs/ekran/03-duzenleme.png" alt="Belge düzenleme" width="720">
</p>

## Öne çıkan özellikler

- **Türkçe arayüz:** menüler, araçlar ve iletiler Türkçe.
- **Türkçe metin tanıma (OCR):** taranmış sayfalar Windows'un kendi metin tanıma motoruyla
  aranabilir ve kopyalanabilir metne dönüşür (Windows'ta Türkçe metin tanıma özelliği yüklüyse).
- **Word, Excel ve PowerPoint dosyalarından PDF'ye dönüştürme (bilgisayarda Microsoft Office kuruluysa).**
- **UDF dosyaları:** UYAP Doküman Editörü'nün kullandığı UDF dosyalarını başka bir program
  gerekmeden açın, okuyun ve PDF'ye dönüştürün. Dosyadaki elektronik imza PDF'ye taşınmaz;
  dönüştürülen PDF imzalı belgenin yerine geçmez. Pdfrp'nin Adalet Bakanlığı ya da UYAP ile bir
  bağlantısı yoktur; bu özellik Bakanlıkça onaylanmış ya da desteklenmiş değildir.
- **Çevrimdışı çeviri:** Seçtiğiniz metni internete göndermeden, doğrudan bilgisayarınızda çevirin.
  İngilizce ile 57 dil arasında iki yönde çalışır; öteki dil çiftleri İngilizce üzerinden çevrilir.
  Yalnız ihtiyaç duyduğunuz dilin modülünü indirirsiniz. Makine çevirisidir; hukuki ve resmî
  işlerde belgenin aslını esas alın.
- **Gezgin'den:** dosyaya sağ tıklayıp "Pdfrp ile PDF'ye dönüştür" (Office belgeleri,
  UDF dosyaları, resimler, metin) ya da birkaç dosyayı seçip "Pdfrp'de birleştir". Windows 11'de bu
  komutlar sağ tık menüsünde "Diğer seçenekleri göster" altındadır.
- **Görüntü olarak dışa aktarma:** sayfaları JPEG, PNG ya da TIFF olarak kaydetme.
- **Birleştirme ve sayfa düzenleme:** dosyaları birleştirme, sayfa ekleme, silme, döndürme,
  sıralama, ayırma ve bölme.
- **Formlar:** form doldurma ve form alanı hazırlama.
- **Damga ve yorum:** hazır ve özel damgalar, notlar, vurgulama, çizim.
- **Metin düzeltme:** PDF'deki var olan metni yerinde düzeltme (sınırlarıyla) ve yeni metin
  ekleme; Türkçe harfler korunur.

<p align="center">
  <img src="docs/ekran/02-araclar.png" alt="Araçlar paneli" width="360">
  <img src="docs/ekran/01-ana-sayfa.png" alt="Pdfrp ana sayfası" width="360">
</p>
<p align="center">
  <img src="docs/ekran/04-ocr.png" alt="Metin tanıma (OCR) penceresi" width="360">
  <img src="docs/ekran/05-disa-aktarma.png" alt="Dışa aktarma" width="360">
</p>

## Kurulum

1. Kurulum dosyasını indirin (Windows 10 ya da 11, 64 bit):
   [Pdfrp 1.2.0](https://github.com/ceceys/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe).
   Bütün sürümler [Releases](https://github.com/ceceys/pdfrp/releases) sayfasındadır. Sürüm
   sayfasında `Pdfrp Kurulum X.Y.Z.exe` görünür; indirdiğiniz dosyanın adı
   `Pdfrp.Kurulum.X.Y.Z.exe` olabilir. Kurulu Pdfrp sonraki sürümleri kendisi bulur (aşağıda).
2. İsterseniz dosyayı aynı sürümdeki `SHA256SUMS.txt` ile doğrulayın: dosyanın bulunduğu klasörde
   PowerShell'de `Get-FileHash -Algorithm SHA256 '.\Pdfrp*Kurulum*X.Y.Z.exe'` komutunu çalıştırın
   (iki adı da bulur). Verdiği değer, listede `Pdfrp Kurulum X.Y.Z.exe` satırındaki değerle
   aynı olmalıdır (büyük-küçük harf farkı önemsizdir).
3. Kurulum dosyasını çalıştırın. Kurulum "tüm kullanıcılar" ya da "yalnız benim için" seçeneğini
   sorar; PDF dosyalarının varsayılan programını değiştirmez, Pdfrp'yi "Birlikte aç"
   listesine ekler.

Kurulum dosyası kod imzalı değildir; Windows SmartScreen ilk çalıştırmada "Windows bilgisayarınızı
korudu" uyarısı gösterebilir. Dosyayı yukarıdaki gibi doğruladıysanız "Ek bilgi" ve ardından
"Yine de çalıştır" ile devam edebilirsiniz.

## Güncelleme denetimi

Pdfrp, açılışta (günde en çok bir kez) ve Yardım menüsünden ("Güncellemeleri denetle…") istendiğinde yalnız GitHub'a bağlanıp bu deponun son sürüm numarasına bakar; bu istekte belge, dosya adı ya da kullanıcı bilgisi gönderilmez, yalnız program adı ve sürümü iletilir (GitHub, her web isteğinde olduğu gibi IP adresini görür); yeni sürüm siz onaylamadan indirilmez ve kurulmaz. Açılıştaki denetimi Tercihler > Genel ve Arayüz'den kapatabilirsiniz; Yardım menüsündeki denetim yalnız siz seçtiğinizde çalışır.

“Şimdi güncelle” dediğinizde Pdfrp kapanır ve yeni sürümün kurulum sihirbazı açılır; kurulumu oradan sürdürür ya da iptal edersiniz. İndirilen kurulum dosyası, çalıştırılmadan önce sürümdeki `SHA256SUMS.txt` ile doğrulanır; uymazsa silinir. Kurulum bitince Pdfrp sihirbazın son sayfasından yeniden açılır.

<p align="center">
  <img src="docs/ekran/06-guncelleme.png" alt="Güncelleme bildirimi (örnek)" width="480">
</p>

## Çevrimdışı çeviri

Bir dili ilk kez seçtiğinizde Pdfrp, gereken dil modülünü onayınızı alarak indirir; boyutu indirme
penceresinde gösterilir. İngilizce ile 57 dil arasında doğrudan çeviri yapılır. İki dil arasında
İngilizce yoksa, örneğin Türkçeden Almancaya, metin önce İngilizceye, sonra hedef dile çevrilir ve
iki modül gerekir. Aynı metin, bilgisayarın işlemcisine bağlı olarak küçük farklarla çevrilebilir.
Makine çevirisi hata içerebilir; hukuki, resmî ya da sözleşmeye dayalı işlerde belgenin aslını esas
alın.

Çeviri modelleri Mozilla'nın Firefox Translations projesinin açık modelleridir ve Mozilla Public
License 2.0 ile dağıtılır. Modeller değiştirilmeden Pdfrp'nin GitHub Release sayfasında yeniden
yayımlanır; kaynakları github.com/mozilla/translations adresindedir (önceki model deposu:
github.com/mozilla/firefox-translations-models). Çeviri motoru, fxtranslate kitaplığının (MPL-2.0)
Pdfrp için değiştirilmiş bir sürümünü kullanır; değişiklik dahil kaynak kodu bu depoda
`vendor/fxtranslate` klasöründedir. Pdfrp'nin Mozilla ya da Mozilla ürünleriyle bir bağlantısı
yoktur ve Mozilla tarafından desteklenmez.

## Kaynaktan derleme

Gerekenler:

- [Rust](https://www.rust-lang.org/) 1.95 ya da daha yenisi.
- Windows'ta bir C araç zinciri: MSVC (Visual Studio Build Tools, `x86_64-pc-windows-msvc`) ya da
  GNU (`x86_64-pc-windows-gnu` ve MinGW-w64 GCC).
- Kurulum dosyası için [Inno Setup 6](https://jrsoftware.org/isinfo.php).

```sh
cargo build --release --locked -p migez-pdf      # program: target/release/migez-pdf.exe
cargo run -p migez-pdf -- belge.pdf              # geliştirme derlemesiyle bir belge açar
cargo test -p migez-update                       # bir paketin sınamaları
```

Çeviri yardımcısı `pdfrp-ceviri` iki kez derlenir; kurulum ikisini de kurar, Pdfrp işlemcinin
çalıştırabildiğini başlatır. AVX2 derlemesi bir C++ derleyicisi ister (MSVC ya da MinGW-w64 g++);
`FXTRANSLATE_REQUIRE_SIMD=1` AVX2 çekirdeği derlenemezse derlemeyi durdurur, `CXXSTDLIB_STATIC=1`
C++ kitaplığını exe'ye gömer:

```sh
FXTRANSLATE_REQUIRE_SIMD=1 CXXSTDLIB_STATIC=1 cargo build --release --locked -p pdfrp-ceviri
cargo build --release --locked -p pdfrp-ceviri --no-default-features --features portable
```

Arayüz çevirileri depodaki `i18n/` klasöründen derlemeye gömülür; başka bir kopya
`MIGEZ_I18N_DIR` ortam değişkeniyle verilebilir. Sürüm derlemesinde `MIGEZ_I18N_REQUIRED=1`
çevirileri, `MIGEZ_REQUIRE_WINRES=1` exe simgesini ve sürüm bilgisini zorunlu kılar.

Kurulum dosyası (çıktı `setup\`):

```powershell
pwsh -NoProfile -File kurulum\lisans-listesi.ps1   # üçüncü taraf bileşenlerin lisans listesi
& "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe" kurulum\pdfrp.iss
```

`v*` biçiminde bir etiket gönderildiğinde `.github/workflows/release.yml` aynı adımları GitHub
Actions'ta çalıştırır ve sürümü kurulum dosyası ile `SHA256SUMS.txt` dosyasıyla yayımlar.
Geliştirme notları `docs/development.md` dosyasındadır.

## Lisans

Pdfrp'nin kaynak kodu MIT Lisansı ile sunulur (`LICENSE-MIT`). Pdfrp, MIT veya Apache-2.0 seçimli lisansla yayımlanmış açık kaynak bir PDF programının işlenmesidir; üst kaynağın adı, telif bildirimi ve yapılan değişikliklere ilişkin not `NOTICE` dosyasındadır. `licenses/LICENSE-APACHE-upstream.txt`, üst kaynağın ve bazı bileşenlerin seçimli lisans metni olarak depoda durur. Yazı tipleri, simgeler ve öteki varlıklar kendi lisanslarıyla dağıtılır; kaynakları ve lisansları `ATTRIBUTION.md` dosyasındadır.

`vendor/fxtranslate` klasöründeki çeviri motoru (fxtranslate 0.5.0, Pdfrp'nin değişiklikleriyle) MIT değil, Mozilla Public License 2.0 ile dağıtılır (`vendor/fxtranslate/LICENSE`; değişiklikler `vendor/fxtranslate/PDFRP-DEGISIKLIKLER.md`); içindeki gemmology (MIT) ve xsimd (BSD-3-Clause) kendi lisanslarıyla gelir. İndirilen dil modülleri de MPL-2.0'dır (bkz. Çevrimdışı çeviri).

Pdfrp adı ve logosu MIT lisansının kapsamında değildir; değiştirilmiş sürümlerde çıkarılır ya da değiştirilir (`assets/pdfrp/LICENSE.txt`).

Programa derlenen üçüncü taraf Rust bileşenlerinin lisansları kurulumdaki `lisanslar` klasöründe,
`ucuncu-taraf-lisanslar.html` dosyasındadır.

## English

Pdfrp is an open-source PDF viewer and editor for Windows with a Turkish interface. It reads,
annotates, fills and signs PDFs, organizes, combines and splits pages, edits text and images,
redacts, protects with passwords, turns scanned pages into searchable, copyable text with Windows'
own text recognition (when Windows' Turkish OCR feature is installed), converts Word, Excel and
PowerPoint files to PDF when Microsoft Office is installed, exports pages as JPEG, PNG or TIFF, and
translates selected text offline. The name comes from “Pdf For Friday”.

Pdfrp connects to the internet for two things only: checking for a new version and downloading
language modules you have approved. Both come from Pdfrp's GitHub page. Besides the address it
asks for, the request carries only the program's name and version; when a language module is
downloaded, that address shows which module it is. GitHub sees your IP address, as with any web
request. Translation runs entirely on your computer; your documents and the text you translate are
not sent anywhere.

Opens UDF files used by the UYAP Document Editor and converts them to PDF. The electronic
signature in the file is not carried over; the converted PDF does not replace the signed document.
Pdfrp is not affiliated with the Turkish Ministry of Justice or UYAP, and this feature is not
endorsed or supported by the Ministry.

- **Offline translation:** Translate selected text right on your computer, without sending it to
  the internet. Works in both directions between English and 57 languages; other language pairs are
  translated via English. You download modules only for the languages you need. This is machine
  translation; for legal or official matters, rely on the original document.
- **How translation works:** The first time you choose a language, Pdfrp asks you and then
  downloads the language module it needs; the size is shown in the download window. Translation
  between English and 57 languages is direct. When neither language is English, for example
  Turkish to German, the text is translated into English first and then into the target language,
  which needs two modules. The same text may be translated slightly differently depending on the
  computer's processor. Machine translation may contain errors; for legal, official or contractual
  matters, rely on the original document.
- **Translation models:** The translation models are open models from Mozilla's Firefox
  Translations project and are distributed under the Mozilla Public License 2.0. They are
  republished unmodified on Pdfrp's GitHub Releases page; their sources are at
  github.com/mozilla/translations (earlier model repository:
  github.com/mozilla/firefox-translations-models). The translation engine uses a version of the
  fxtranslate library (MPL-2.0) modified for Pdfrp; its source code, including the changes, is in
  this repository's `vendor/fxtranslate` folder. Pdfrp is not officially associated with Mozilla
  or its products and is not endorsed by Mozilla.
- **Download:** [Pdfrp 1.2.0 installer](https://github.com/ceceys/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe)
  (Windows 10 or 11, 64-bit, about 25 MB). The installed app finds later versions by itself.
  macOS and Linux versions will be added.
- **Install:** all versions are on [Releases](https://github.com/ceceys/pdfrp/releases). The release page shows
  `Pdfrp Kurulum X.Y.Z.exe`; the file you download may be named
  `Pdfrp.Kurulum.X.Y.Z.exe`. To check it, run
  `Get-FileHash -Algorithm SHA256 '.\Pdfrp*Kurulum*X.Y.Z.exe'` in its folder (it finds either
  name); the value must match the line for `Pdfrp Kurulum X.Y.Z.exe` in the release's
  `SHA256SUMS.txt` (letter case aside). The installer is not code-signed.
- **Update check:** at start (at most once a day) and from Help ▸ Check for updates, the app asks
  GitHub only for this repository's latest release. The request carries the program's name and
  version only (GitHub sees the IP address, as with any web request). Nothing is downloaded or
  installed until you choose "Update now"; the download is checked against the release's
  SHA-256 before it runs. Choosing “Update now” closes Pdfrp and opens the new version's
  setup wizard, where you can go on or cancel. Preferences ▸ General & Interface turns the check at
  start off; Help ▸ Check for updates asks only when you choose it. The update notice pictured in
  the Turkish section is an example.
- **Build:** `cargo build --release --locked -p migez-pdf` (Rust 1.95+); the translation helper
  twice, `FXTRANSLATE_REQUIRE_SIMD=1 CXXSTDLIB_STATIC=1 cargo build --release --locked -p pdfrp-ceviri`
  (AVX2, needs a C++ compiler) and with `--no-default-features --features portable`; the
  installer with Inno Setup 6 (`kurulum\pdfrp.iss`).
- **License:** the source code is offered under the MIT License (`LICENSE-MIT`). Pdfrp is a
  derivative of an open-source PDF application released under MIT or Apache-2.0; its name,
  copyright notice and a note on the changes are in `NOTICE`. The translation engine in
  `vendor/fxtranslate` (fxtranslate 0.5.0 with Pdfrp's changes) is under the Mozilla Public
  License 2.0, not MIT (`vendor/fxtranslate/LICENSE`; the changes are in
  `vendor/fxtranslate/PDFRP-DEGISIKLIKLER.md`); the gemmology (MIT) and xsimd (BSD-3-Clause)
  headers it bundles keep their own licences. The downloaded language modules are MPL-2.0 too
  (see Translation models above). The Pdfrp name and logo are not covered by the MIT License;
  modified versions must remove or replace them (`assets/pdfrp/LICENSE.txt`).
- **Trademarks:** product and company names mentioned may be trademarks of their owners. Pdfrp
  is an independent project; it is not affiliated with, sponsored or endorsed by those companies.

## Markalar

Anılan ürün ve şirket adları sahiplerinin markaları olabilir. Pdfrp bağımsız bir projedir; bu şirketlerle bağlantılı değildir, onlarca desteklenmez ya da onaylanmaz.
