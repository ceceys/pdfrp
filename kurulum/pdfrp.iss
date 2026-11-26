; Pdfrp kurulumu (Inno Setup 6). Herkese açık depo: github.com/ceceys/pdfrp, MIT lisansı.
; Derleme (deponun kökünden, önce sürüm derlemesi ve lisans listesi):
;   cargo build --release --locked -p migez-pdf            (çeviriler depodaki i18n\ klasöründen)
;   pwsh -NoProfile -File kurulum\lisans-listesi.ps1        (kurulum\cikti\ucuncu-taraf-lisanslar.html)
;   ISCC.exe kurulum\pdfrp.iss                         (çıktı: setup\Pdfrp Kurulum X.Y.Z.exe)
; Yollar bu dosyanın klasörüne göredir; ISCC /D ile değiştirilebilir: /DSurum=<exe klasörü>,
; /DI18n=<çeviri klasörü>, /DCikti=<çıktı klasörü>, /DLisansListesi=<html>, /DAppVersion=X.Y.Z.
; Program dosyası target\release\migez-pdf.exe; kurulumda "Pdfrp.exe" adını alır.
; Kurulum "tüm kullanıcılar / yalnız benim için" diye sorar: tüm kullanıcılar Program Files'a ve
; HKLM'ye, yalnız benim için %LOCALAPPDATA%\Programs'a ve HKCU'ya kurar ({auto...} ve HKA).
; .pdf için yalnız "Birlikte aç" listesine girer; varsayılan PDF uygulamasını değiştirmez.
; Gezgin sağ tık: "Pdfrp ile PDF'ye dönüştür" (--create-file) ve "Pdfrp'de birleştir"
; (--combine); türler [Registry] sonundaki açıklamada. 1.0.1'de eklendi.
; 1.1.0: uygulama içi güncelleme. Program, kullanıcı "Şimdi güncelle" deyince SHA-256'sı doğrulanmış
; kurulumu /NORESTART /MIGEZYENIDENAC=1 ile ve /CURRENTUSER ya da /ALLUSERS ile (kurulu kopyanın
; yerine göre) başlatıp kapanır. Güncelleme sessiz kurulum değildir: sihirbaz görünür. Kurulum
; /MIGEZYENIDENAC=1 ile başlarsa güncellemede gereksiz sayfaları (karşılama, klasör seçimi, ek
; görevler) atlar, "Kurulmaya hazır" sayfasında kullanıcının "Kur" demesini bekler; açık kalan program
; varsa kapatmadan önce sorar (CloseApplications); son sayfada "Pdfrp uygulamasını çalıştır"
; seçili gelir ve "Bitti" ile program yeniden açılır ([Run], [Code]).
; 1.1.0: program 1.0.1'e kadar "Mig-EZ Pdf" adıyla kuruluydu (EskiAd). AppId aynıdır: Pdfrp eski
; kurulumun yerine geçer, Programlar listesinde tek program görünür, kullanıcının önceki seçimleri
; (kurulum türü, masaüstü kısayolu) korunur. ProgId de aynıdır (iç kimlik): PDF'leri Mig-EZ Pdf ile
; açacak biçimde varsayılan uygulama seçmiş kullanıcının seçimi yeni exe'ye geçer. Eski kurulumdan
; kalanlar silinir: eski exe ([InstallDelete], açıksa yeniden başlatma yöneticisi kapatmayı sorar),
; eski Başlat menüsü ve masaüstü kısayolları, eski "Varsayılan uygulamalar" ve "Birlikte aç" kayıtları
; ([Registry] sonu). Eski klasör "...\Mig-EZ Pdf" ise yeni klasör aynı yerde "...\Pdfrp" olur;
; kurulum bitince eski klasördeki kaldırma dosyaları ve lisanslar silinir, boş kalan klasör kalkar
; ([Code]). Kullanıcı verisi (%APPDATA%, %LOCALAPPDATA%) silinmez; program ilk açılışta onu yeni
; klasöre kopyalar (apps\migez-pdf\src\legacy_names.rs).
; Upstream'in WiX dosyalarından (packaging\windows) bağımsızdır, onlara dokunmaz.
; Bu dosya BOM'lu UTF-8 kaydedilmelidir (Türkçe harfler var).

#define AppName "Pdfrp"
#ifndef AppVersion
  #define AppVersion "1.1.0"
#endif
#define AppPublisher "Friday"
#define AppExeName "Pdfrp.exe"
#define ProgId "MigEZPdf.PDF"
; Programın 1.0.1'e kadarki adı ve exe'si: yalnız eski kurulumun kalıntılarını silmek için.
#define EskiAd "Mig-EZ Pdf"
#define EskiExe "Mig-EZ Pdf.exe"
#ifndef Kok
  #define Kok AddBackslash(SourcePath) + ".."
#endif
#ifndef Surum
  #define Surum Kok + "\target\release"
#endif
#ifndef I18n
  #define I18n Kok + "\i18n"
#endif
#ifndef Cikti
  #define Cikti Kok + "\setup"
#endif
#ifndef LisansListesi
  #define LisansListesi Kok + "\kurulum\cikti\ucuncu-taraf-lisanslar.html"
#endif

[Setup]
AppId={{D59E66E4-1B3B-4506-BC78-70C42D700181}
AppName={#AppName}
AppVersion={#AppVersion}
AppVerName={#AppName} {#AppVersion}
AppPublisher={#AppPublisher}
AppPublisherURL=https://github.com/ceceys/pdfrp
AppSupportURL=https://github.com/ceceys/pdfrp
AppUpdatesURL=https://github.com/ceceys/pdfrp/releases
DefaultDirName={autopf}\{#AppName}
DefaultGroupName={#AppName}
DisableProgramGroupPage=yes
; 1.1.0: 1.0.x'in kaydettiği "Mig-EZ Pdf" grup adı yeniden kullanılmaz (kısayollar {autoprograms}
; kökünde, grup klasörü yok; ad yalnız kaldırma kaydına yazılır).
UsePreviousGroup=no
PrivilegesRequired=admin
PrivilegesRequiredOverridesAllowed=dialog commandline
ArchitecturesAllowed=x64compatible
ArchitecturesInstallIn64BitMode=x64compatible
MinVersion=10.0
OutputDir={#Cikti}
OutputBaseFilename={#AppName} Kurulum {#AppVersion}
SetupIconFile={#Kok}\assets\pdfrp\pdfrp.ico
UninstallDisplayIcon={app}\{#AppExeName}
UninstallDisplayName={#AppName}
VersionInfoVersion={#AppVersion}.0
VersionInfoCompany={#AppPublisher}
VersionInfoProductName={#AppName}
VersionInfoProductVersion={#AppVersion}.0
VersionInfoDescription={#AppName} Kurulum
Compression=lzma2/max
SolidCompression=yes
WizardStyle=modern
ChangesAssociations=yes
; Güncellemede ve elle kurulumda Pdfrp açık kalmışsa kurulum "Kuruluma hazırlanılıyor" sayfasında
; durur, açık programları gösterir ve kapatılsın mı diye kullanıcıya sorar (Windows'un yeniden
; başlatma yöneticisi). Program güncellemede zaten kendisi kapanır. Kapatılan programı yeniden
; başlatma yöneticisi açmaz (RestartApplications=no); güncellemede programı son sayfa açar ([Run]).
CloseApplications=yes
RestartApplications=no

[Languages]
Name: "turkish"; MessagesFile: "compiler:Languages\Turkish.isl"

[Tasks]
Name: "masaustu"; Description: "{cm:CreateDesktopIcon}"; GroupDescription: "{cm:AdditionalIcons}"; Flags: unchecked

[InstallDelete]
; 1.1.0: Mig-EZ Pdf'ten kalanlar. Eski exe, kurulum klasörü değişmediyse {app} içinde, değiştiyse eski
; klasörde (EskiKlasor). [InstallDelete]'teki exe'yi açık tutan program varsa kurulum "Kuruluma
; hazırlanılıyor" sayfasında kapatılsın mı diye sorar (CloseApplications).
Type: files; Name: "{app}\{#EskiExe}"
Type: files; Name: "{app}\lisanslar\LICENSE-mig-ez-pdf-logo.txt"
Type: files; Name: "{code:EskiKlasor}\{#EskiExe}"; Check: KlasorTasindi
Type: files; Name: "{autoprograms}\{#EskiAd}.lnk"
Type: files; Name: "{autodesktop}\{#EskiAd}.lnk"

[Files]
Source: "{#Surum}\migez-pdf.exe"; DestDir: "{app}"; DestName: "{#AppExeName}"; Flags: ignoreversion
; Açık kaynak lisansları (Hakkında penceresi bu klasörü gösterir).
Source: "{#Kok}\LICENSE-MIT"; DestDir: "{app}\lisanslar"; Flags: ignoreversion
; Üst kaynağın Apache-2.0 metni: depoda licenses\LICENSE-APACHE-upstream.txt (kökte ikinci lisans
; gibi görünmesin diye), kurulumda yine LICENSE-APACHE adıyla. Kendi lisans dosyasını taşımayan
; Apache lisanslı bileşenler (ucuncu-taraf-lisanslar.html) bu dosyayı gösterir.
Source: "{#Kok}\licenses\LICENSE-APACHE-upstream.txt"; DestDir: "{app}\lisanslar"; DestName: "LICENSE-APACHE"; Flags: ignoreversion
Source: "{#Kok}\NOTICE"; DestDir: "{app}\lisanslar"; DestName: "NOTICE.txt"; Flags: ignoreversion
Source: "{#Kok}\ATTRIBUTION.md"; DestDir: "{app}\lisanslar"; Flags: ignoreversion
Source: "{#Kok}\assets\fonts\OFL-*.txt"; DestDir: "{app}\lisanslar"; Flags: ignoreversion
Source: "{#Kok}\assets\icons\LICENSE-lucide.txt"; DestDir: "{app}\lisanslar"; Flags: ignoreversion
; Pdfrp adı ve logosu: MIT kapsamında değil; koşullar LICENSE-pdfrp-logo.txt'de.
Source: "{#Kok}\assets\pdfrp\LICENSE.txt"; DestDir: "{app}\lisanslar"; DestName: "LICENSE-pdfrp-logo.txt"; Flags: ignoreversion
Source: "{#Kok}\licenses\*.txt"; Excludes: "LICENSE-APACHE-upstream.txt"; DestDir: "{app}\lisanslar"; Flags: ignoreversion
Source: "{#Kok}\vendor\edit-session\LICENSE-MIT"; DestDir: "{app}\lisanslar\edit-session"; Flags: ignoreversion
Source: "{#Kok}\vendor\edit-session\LICENSE-APACHE"; DestDir: "{app}\lisanslar\edit-session"; Flags: ignoreversion
; 1.1.0: exe'ye derlenen bütün üçüncü taraf Rust bileşenlerinin lisansları (kurulum\lisans-listesi.ps1
; Cargo.lock'tan üretir) ve exe'ye gömülen craft-i18n çevirilerinin telif bildirimi.
Source: "{#LisansListesi}"; DestDir: "{app}\lisanslar"; DestName: "ucuncu-taraf-lisanslar.html"; Flags: ignoreversion
Source: "{#I18n}\LICENSE-MIT"; DestDir: "{app}\lisanslar\craft-i18n"; Flags: ignoreversion

[Icons]
Name: "{autoprograms}\{#AppName}"; Filename: "{app}\{#AppExeName}"
Name: "{autodesktop}\{#AppName}"; Filename: "{app}\{#AppExeName}"; Tasks: masaustu

[Registry]
; ProgID: "Birlikte aç" listesindeki giriş, simgesi programın kendi simgesi.
Root: HKA; Subkey: "Software\Classes\{#ProgId}"; ValueType: string; ValueName: ""; ValueData: "PDF Belgesi"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\{#ProgId}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\{#ProgId}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; .pdf'ye yalnız "Birlikte aç" adayı olarak eklenir; varsayılan (UserChoice) değişmez.
Root: HKA; Subkey: "Software\Classes\.pdf\OpenWithProgids"; ValueType: string; ValueName: "{#ProgId}"; ValueData: ""; Flags: uninsdeletevalue
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}"; ValueType: string; ValueName: "FriendlyAppName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\SupportedTypes"; ValueType: string; ValueName: ".pdf"; ValueData: ""
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\DefaultIcon"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\Applications\{#AppExeName}\shell\open\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" ""%1"""
; Ayarlar > Varsayılan uygulamalar listesinde görünmesi için (seçim kullanıcıda kalır).
Root: HKA; Subkey: "Software\{#AppName}"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationName"; ValueData: "{#AppName}"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\{#AppName}\Capabilities"; ValueType: string; ValueName: "ApplicationDescription"; ValueData: "PDF görüntüleme ve düzenleme"
Root: HKA; Subkey: "Software\{#AppName}\Capabilities\FileAssociations"; ValueType: string; ValueName: ".pdf"; ValueData: "{#ProgId}"
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueType: string; ValueName: "{#AppName}"; ValueData: "Software\{#AppName}\Capabilities"; Flags: uninsdeletevalue

; Gezgin sag tik (klasik menu; Windows 11'de "Diğer seçenekleri göster" altinda). Her secili dosya
; icin ayri surec acilir (MultiSelectModel=Document; Windows bu tur girisi en fazla 15 secili
; dosyada gosterir), tek kopya hepsini calisan pencerede toplar. Komut anahtari kaldirmada silinir;
; .uzanti ve shell anahtarlari bos kalirsa onlar da silinir.
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.doc\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.docx\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.rtf\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.txt\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xls\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.xlsx\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.ppt\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pptx\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpg\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.jpeg\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.png\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.bmp\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.gif\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tif\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: ""; ValueData: "Pdfrp ile PDF'ye dönüştür"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.CreatePdf"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.CreatePdf\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --create-file ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.tiff\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell"; Flags: uninsdeletekeyifempty
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\MigEZPdf.Combine"; ValueType: string; ValueName: ""; ValueData: "Pdfrp'de birleştir"; Flags: uninsdeletekey
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "Icon"; ValueData: """{app}\{#AppExeName}"",0"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\MigEZPdf.Combine"; ValueType: string; ValueName: "MultiSelectModel"; ValueData: "Document"
Root: HKA; Subkey: "Software\Classes\SystemFileAssociations\.pdf\shell\MigEZPdf.Combine\command"; ValueType: string; ValueName: ""; ValueData: """{app}\{#AppExeName}"" --combine ""%1"""

; 1.1.0: Mig-EZ Pdf'in eski adlı kayıtları (ProgId ve sağ tık komutlarının anahtarları aynı kalır,
; yukarıda yeni exe'yi gösterir). ValueType none ile deletekey yalnız siler, yeniden kurmaz;
; deletevalue yalnız "Mig-EZ Pdf" değerini siler, RegisteredApplications anahtarı yerinde kalır.
Root: HKA; Subkey: "Software\{#EskiAd}"; Flags: deletekey
Root: HKA; Subkey: "Software\Classes\Applications\{#EskiExe}"; Flags: deletekey
Root: HKA; Subkey: "Software\RegisteredApplications"; ValueName: "{#EskiAd}"; Flags: deletevalue

[Run]
; Elle kurulumda son sayfadaki "Pdfrp uygulamasını çalıştır" kutusu boş gelir.
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall skipifsilent unchecked; Check: not Guncelleme
; 1.1.0: programın başlattığı güncellemede (/MIGEZYENIDENAC=1) kutu seçili gelir; "Bitti" ile Pdfrp
; yeniden açılır, güncellemeyi başlatan kullanıcının yetkisiyle (yönetici yetkisiyle değil).
Filename: "{app}\{#AppExeName}"; Description: "{cm:LaunchProgram,{#AppName}}"; Flags: nowait postinstall runasoriginaluser; Check: Guncelleme

[Code]
// Kurulumu Pdfrp'nin kendi güncellemesi mi başlattı: /MIGEZYENIDENAC=1.
function Guncelleme: Boolean;
begin
  Result := ExpandConstant('{param:MIGEZYENIDENAC|0}') = '1';
end;

// Güncellemede sihirbaz, kullanıcının daha önce seçtiklerini yeniden soran sayfaları atlar:
// karşılama, kurulum klasörü (kurulu kopyanınki kullanılır), Başlat menüsü klasörü ve ek görevler
// (önceki seçim korunur). "Kurulmaya hazır" sayfası atlanmaz: kurulumu kullanıcı "Kur" ile başlatır.
// Elle kurulumda sayfalar her zamanki gibidir.
function ShouldSkipPage(PageID: Integer): Boolean;
begin
  Result := Guncelleme and ((PageID = wpWelcome) or (PageID = wpSelectDir) or
    (PageID = wpSelectProgramGroup) or (PageID = wpSelectTasks));
end;

// 1.1.0: önceki kurulumun klasörü, adı "Mig-EZ Pdf" ise (1.0.x'in varsayılanı, sondaki \ yok);
// değilse boş.
function EskiKlasor(Param: String): String;
begin
  Result := RemoveBackslashUnlessRoot(WizardForm.PrevAppDir);
  if (Result = '') or (CompareText(ExtractFileName(Result), '{#EskiAd}') <> 0) then
    Result := '';
end;

// Eski "Mig-EZ Pdf" klasörü var ve kurulum başka bir klasöre yapılıyor.
function KlasorTasindi: Boolean;
var
  Eski: String;
begin
  Eski := EskiKlasor('');
  Result := (Eski <> '') and (CompareText(Eski, RemoveBackslashUnlessRoot(ExpandConstant('{app}'))) <> 0);
end;

// Önceki kurulum "...\Mig-EZ Pdf" klasöründeyse yeni klasör aynı yerde "...\Pdfrp" olur
// (Program Files ya da %LOCALAPPDATA%\Programs, kullanıcının seçtiği başka bir üst klasör de olabilir).
// /DIR= ile verilen klasöre dokunulmaz; elle kurulumda kullanıcı klasör sayfasında değiştirebilir.
procedure InitializeWizard;
var
  Eski: String;
begin
  Eski := EskiKlasor('');
  if (Eski <> '') and (ExpandConstant('{param:DIR|}') = '') then
    WizardForm.DirEdit.Text := AddBackslash(ExtractFileDir(Eski)) + '{#AppName}';
end;

// Kurulum bitince eski klasörden 1.0.x'in kendi dosyaları silinir: kaldırma programı ve kaydı
// (Programlar listesindeki kayıt artık yeni klasördeki kaldırma programını gösterir) ve lisanslar
// klasörü. Eski exe [InstallDelete]'te silindi. Klasör boş kalırsa kaldırılır; kullanıcının oraya
// koyduğu bir dosya varsa klasör ve o dosya kalır. Kurulum yarıda kalırsa bu adım çalışmaz.
procedure CurStepChanged(CurStep: TSetupStep);
var
  Eski: String;
begin
  if (CurStep = ssPostInstall) and KlasorTasindi then begin
    Eski := EskiKlasor('');
    DeleteFile(Eski + '\{#EskiExe}');
    DeleteFile(Eski + '\unins000.exe');
    DeleteFile(Eski + '\unins000.dat');
    DelTree(Eski + '\lisanslar', True, True, True);
    if RemoveDir(Eski) then
      Log('Eski kurulum klasörü kaldırıldı: ' + Eski)
    else
      Log('Eski kurulum klasörü boş değil ya da kaldırılamadı, yerinde kaldı: ' + Eski);
  end;
end;
