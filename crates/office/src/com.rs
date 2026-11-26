//! COM otomasyonu için küçük, güvenli bir katman: IDispatch ile geç bağlama.
//!
//! Paketin `unsafe` kullanan tek modülü (README, "unsafe"). Her `unsafe` blok tek bir COM çağrısını
//! ya da VARIANT birliğinden tek bir okumayı sarar ve neden güvenli olduğunu yazar. Dışarıya
//! verilen API güvenlidir:
//!
//! - [`Daire`]: iş parçacığında COM'u tek iş parçacıklı daire (STA) olarak başlatır, düşünce kapatır.
//! - [`Nesne`]: bir IDispatch. Ömrü `Daire`'ye bağlıdır (COM kapandıktan sonra kullanılamaz) ve
//!   iş parçacığından çıkamaz (`Send` değildir).
//! - [`Arg`] ve [`Deger`]: çağrıya giden ve çağrıdan dönen değerler. VARIANT'lar bu modülden dışarı
//!   çıkmaz; her biri `VariantClear` ile bırakılır.
//!
//! Office meşgulken çağrıyı reddederse (`RPC_E_CALL_REJECTED`, `RPC_E_SERVERCALL_RETRYLATER`) çağrı
//! kısa aralıklarla [`MESGUL_SURESI`] boyunca yinelenir.

#![allow(unsafe_code)]

use std::fmt;
use std::marker::PhantomData;
use std::mem::ManuallyDrop;
use std::os::windows::ffi::OsStrExt;
use std::path::Path;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{
    DISP_E_EXCEPTION, DISP_E_MEMBERNOTFOUND, DISP_E_PARAMNOTFOUND, DISP_E_UNKNOWNNAME, RPC_E_CALL_REJECTED, RPC_E_SERVERCALL_RETRYLATER,
    VARIANT_FALSE, VARIANT_TRUE,
};
use windows::Win32::System::Com::{
    CLSCTX_LOCAL_SERVER, CLSIDFromProgID, COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoCreateInstance, CoInitializeEx, CoUninitialize,
    DISPATCH_FLAGS, DISPATCH_METHOD, DISPATCH_PROPERTYGET, DISPATCH_PROPERTYPUT, DISPPARAMS, EXCEPINFO, IDispatch,
};
use windows::Win32::System::Ole::DISPID_PROPERTYPUT;
use windows::Win32::System::Variant::{
    VARENUM, VARIANT, VARIANT_0, VARIANT_0_0, VARIANT_0_0_0, VT_BOOL, VT_BSTR, VT_DISPATCH, VT_EMPTY, VT_I2, VT_I4, VT_INT, VT_NULL, VT_UI1,
    VariantClear,
};
use windows::core::{BSTR, GUID, HRESULT, IUnknown, PCWSTR};

/// GetIDsOfNames ve Invoke'a verilen yerel ayar: kullanıcının varsayılanı (`LOCALE_USER_DEFAULT`).
const YEREL_AYAR: u32 = 0x0400;
/// Office meşgul diye reddettiği bir çağrı en çok bu kadar süre yinelenir.
pub(crate) const MESGUL_SURESI: Duration = Duration::from_secs(30);
/// Reddedilen çağrının yinelenmeden önceki beklemesi.
const MESGUL_ARASI: Duration = Duration::from_millis(100);

/// Bir COM çağrısının hatası.
#[derive(Clone, Debug)]
pub(crate) struct ComHatasi {
    /// Çağrının HRESULT'u; Office hata fırlattıysa (`DISP_E_EXCEPTION`) onun kodu.
    pub(crate) kod: i32,
    /// Office'in ya da Windows'un iletisi.
    pub(crate) aciklama: String,
    /// Çağrılan üye ya da COM işlevi; günlük için.
    pub(crate) uye: String,
}

impl ComHatasi {
    fn windows(uye: &str, hata: &windows::core::Error) -> Self {
        ComHatasi { kod: hata.code().0, aciklama: hata.message().trim().to_owned(), uye: uye.to_owned() }
    }

    pub(crate) fn yeni(uye: &str, kod: HRESULT, aciklama: &str) -> Self {
        ComHatasi { kod: kod.0, aciklama: aciklama.to_owned(), uye: uye.to_owned() }
    }

    /// HRESULT, belgelerde yazıldığı gibi işaretsiz (`0x8001_0108` gibi).
    pub(crate) fn kod_u32(&self) -> u32 {
        self.kod.cast_unsigned()
    }

    /// Nesnede bu adda bir üye ya da bağımsız değişken yok (eski Office sürümü).
    pub(crate) fn uye_yok(&self) -> bool {
        self.kod == DISP_E_UNKNOWNNAME.0 || self.kod == DISP_E_MEMBERNOTFOUND.0
    }

    /// Office süreciyle bağlantı koptu: uygulama kapandı, çöktü ya da yanıt vermiyor.
    pub(crate) fn baglanti_koptu(&self) -> bool {
        matches!(
            self.kod_u32(),
            // RPC_E_DISCONNECTED, RPC_E_SERVERFAULT, RPC_E_SERVER_DIED, RPC_E_SERVER_DIED_DNE,
            // RPC_S_SERVER_UNAVAILABLE, RPC_S_CALL_FAILED, CO_E_OBJNOTCONNECTED
            0x8001_0108 | 0x8001_0105 | 0x8001_0007 | 0x8001_0012 | 0x8007_06BA | 0x8007_06BE | 0x8004_01FD
        )
    }

    /// Office'in Visual Basic hata numarası (`0x800Axxxx` kodlarındaki `xxxx`), varsa.
    pub(crate) fn office_numarasi(&self) -> Option<u16> {
        let kod = self.kod_u32();
        (kod & 0xFFFF_0000 == 0x800A_0000).then(|| u16::try_from(kod & 0xFFFF).unwrap_or_default())
    }
}

impl fmt::Display for ComHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {} (0x{:08X})", self.uye, self.aciklama, self.kod_u32())
    }
}

/// Bu iş parçacığında başlatılmış COM (STA). Düşünce `CoUninitialize` çağrılır; bu daireden alınan
/// her [`Nesne`] ondan önce bırakılmış olur, çünkü nesnelerin ömrü daireye bağlıdır.
pub(crate) struct Daire {
    /// `Send` ve `Sync` değil: COM'u başlatan iş parçacığında kapatılmalı.
    _tek_is_parcacigi: PhantomData<*const ()>,
}

impl Daire {
    pub(crate) fn baslat() -> Result<Daire, ComHatasi> {
        // SAFETY: CoInitializeEx'in ayrılmış bağımsız değişkeni boş; işaretçi verilmiyor. Başarılı her
        // çağrı (S_OK ya da S_FALSE) aynı iş parçacığında Drop'taki tek bir CoUninitialize ile
        // dengelenir: Daire `Send` değil, başka iş parçacığına geçemez.
        let sonuc = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE) };
        if sonuc.is_err() {
            return Err(ComHatasi::yeni("CoInitializeEx", sonuc, &sonuc.message()));
        }
        Ok(Daire { _tek_is_parcacigi: PhantomData })
    }
}

impl Drop for Daire {
    fn drop(&mut self) {
        // SAFETY: `baslat`taki başarılı CoInitializeEx'i aynı iş parçacığında dengeler. Bu daireye
        // bağlı nesneler (`Nesne<'d>`) ödünç denetimi sayesinde daha önce bırakılmıştır.
        unsafe { CoUninitialize() }
    }
}

/// Bir çağrıya giden değer.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Arg<'a> {
    Metin(&'a str),
    /// Dosya yolu; UTF-16'ya kayıpsız çevrilir.
    Yol(&'a Path),
    Tam(i32),
    Mantik(bool),
}

impl Arg<'_> {
    fn varyant(self) -> Varyant {
        match self {
            Arg::Metin(metin) => Varyant::yeni(VT_BSTR, VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(BSTR::from(metin)) }),
            Arg::Yol(yol) => {
                let genis: Vec<u16> = yol.as_os_str().encode_wide().collect();
                Varyant::yeni(VT_BSTR, VARIANT_0_0_0 { bstrVal: ManuallyDrop::new(BSTR::from_wide(&genis)) })
            }
            Arg::Tam(sayi) => Varyant::yeni(VT_I4, VARIANT_0_0_0 { lVal: sayi }),
            Arg::Mantik(deger) => Varyant::yeni(VT_BOOL, VARIANT_0_0_0 { boolVal: if deger { VARIANT_TRUE } else { VARIANT_FALSE } }),
        }
    }
}

/// Bir çağrıdan dönen değer.
pub(crate) enum Deger<'d> {
    /// Değer yok (`VT_EMPTY`, `VT_NULL`).
    Bos,
    Tam(i32),
    Mantik(bool),
    Metin(String),
    Nesne(Nesne<'d>),
    /// Office boş bir nesne döndürdü (`Nothing`).
    BosNesne,
    /// Bu katmanın okumadığı bir VARIANT türü.
    Diger(u16),
}

impl<'d> Deger<'d> {
    /// Değer bir nesneyse onu verir; değilse `uye` adına bir hata.
    pub(crate) fn nesne(self, uye: &str) -> Result<Nesne<'d>, ComHatasi> {
        match self {
            Deger::Nesne(nesne) => Ok(nesne),
            Deger::BosNesne => Err(ComHatasi::yeni(uye, DISP_E_MEMBERNOTFOUND, "Office boş bir nesne döndürdü")),
            Deger::Diger(tur) => Err(ComHatasi::yeni(uye, DISP_E_MEMBERNOTFOUND, &format!("Office nesne yerine {tur} türünde bir değer döndürdü"))),
            _ => Err(ComHatasi::yeni(uye, DISP_E_MEMBERNOTFOUND, "Office bir nesne döndürmedi")),
        }
    }

    /// Tamsayı değeri; Office'in mantıksal değerleri -1 (doğru) ve 0 (yanlış) sayılır.
    pub(crate) fn tam(&self) -> Option<i32> {
        match self {
            Deger::Tam(sayi) => Some(*sayi),
            Deger::Mantik(deger) => Some(if *deger { -1 } else { 0 }),
            _ => None,
        }
    }

    /// Mantıksal değer; Office'in üç durumlu değerlerinde (MsoTriState) sıfır olmayan doğrudur.
    pub(crate) fn dogru_mu(&self) -> Option<bool> {
        match self {
            Deger::Mantik(deger) => Some(*deger),
            Deger::Tam(sayi) => Some(*sayi != 0),
            _ => None,
        }
    }

    /// Değeri aynı türle geri yazmak için bir [`Arg`]; nesne ve okunmayan türler için `None`.
    pub(crate) fn arg(&self) -> Option<Arg<'_>> {
        match self {
            Deger::Tam(sayi) => Some(Arg::Tam(*sayi)),
            Deger::Mantik(deger) => Some(Arg::Mantik(*deger)),
            Deger::Metin(metin) => Some(Arg::Metin(metin)),
            _ => None,
        }
    }
}

/// Sahibi olunan bir VARIANT; düşünce `VariantClear` ile içindekini (BSTR, arayüz) bırakır.
#[repr(transparent)]
struct Varyant(VARIANT);

impl Varyant {
    /// `VT_EMPTY`.
    fn bos() -> Varyant {
        Varyant(VARIANT::default())
    }

    /// `vt` türünde, değeri `deger` olan bir VARIANT. `deger`'in sahipliği (BSTR) VARIANT'a geçer.
    fn yeni(vt: VARENUM, deger: VARIANT_0_0_0) -> Varyant {
        let ic = VARIANT_0_0 { vt, wReserved1: 0, wReserved2: 0, wReserved3: 0, Anonymous: deger };
        Varyant(VARIANT { Anonymous: VARIANT_0 { Anonymous: ManuallyDrop::new(ic) } })
    }

    /// İçindeki değeri okur. Nesneler için kendi başvurumuzu alır; VARIANT'ınki Drop'ta bırakılır.
    fn deger<'d>(&self) -> Deger<'d> {
        // SAFETY: Bu modüldeki her VARIANT `bos`/`yeni` ile kurulur ya da Invoke tarafından yazılır;
        // ikisi de `vt`'yi ve ona uyan birlik üyesini doldurur. VARIANT_0_0 görünümü, V_VT gibi
        // makroların okuduğu görünümdür (DECIMAL yalnız `vt`'nin ardındaki alanları paylaşır).
        let ic: &VARIANT_0_0 = unsafe { &self.0.Anonymous.Anonymous };
        let deger = &ic.Anonymous;
        match ic.vt {
            VT_EMPTY | VT_NULL => Deger::Bos,
            VT_I4 | VT_INT => {
                // SAFETY: VT_I4 ve VT_INT 32 bitlik tamsayıyı `lVal`'da tutar.
                let sayi = unsafe { deger.lVal };
                Deger::Tam(sayi)
            }
            VT_I2 => {
                // SAFETY: VT_I2 16 bitlik tamsayıyı `iVal`'da tutar.
                let sayi = unsafe { deger.iVal };
                Deger::Tam(i32::from(sayi))
            }
            VT_UI1 => {
                // SAFETY: VT_UI1 bir baytı `bVal`'da tutar.
                let sayi = unsafe { deger.bVal };
                Deger::Tam(i32::from(sayi))
            }
            VT_BOOL => {
                // SAFETY: VT_BOOL bir VARIANT_BOOL'u `boolVal`'da tutar.
                let mantik = unsafe { deger.boolVal };
                Deger::Mantik(mantik.0 != 0)
            }
            VT_BSTR => {
                // SAFETY: VT_BSTR bir BSTR'yi (boş işaretçi boş metin demektir) `bstrVal`'da tutar.
                // Yalnız ödünç alınıp kopyalanır; BSTR'yi VariantClear bırakır.
                let metin: &BSTR = unsafe { &deger.bstrVal };
                Deger::Metin(metin.to_string())
            }
            VT_DISPATCH => {
                // SAFETY: VT_DISPATCH bir IDispatch işaretçisini (boş olabilir) `pdispVal`'da tutar.
                // `Option<IDispatch>` boş işaretçiyi `None` olarak gösterir; `clone` kendi başvurumuzu
                // ekler (AddRef), VARIANT'ınkini VariantClear bırakır.
                let nesne: &Option<IDispatch> = unsafe { &deger.pdispVal };
                match nesne {
                    Some(disp) => Deger::Nesne(Nesne { disp: disp.clone(), _daire: PhantomData }),
                    None => Deger::BosNesne,
                }
            }
            diger => Deger::Diger(diger.0),
        }
    }
}

impl Drop for Varyant {
    fn drop(&mut self) {
        // SAFETY: `self.0` bu değerin sahibi olduğu, kurulmuş bir VARIANT. VariantClear içindekini
        // (BSTR, arayüz başvurusu) bırakır ve VARIANT'ı VT_EMPTY yapar; iki kez bırakılmaz.
        let _ = unsafe { VariantClear(&mut self.0) };
    }
}

/// Office'in bir hatayı anlatan EXCEPINFO'sundan okunanlar.
struct Istisna {
    kod: i32,
    aciklama: String,
}

/// `istisna`'daki BSTR'leri alıp bırakır (her durumda: başarıda boştur); `doldur` doğruysa önce
/// Office'in ertelenmiş doldurma işlevini çağırır.
fn istisna_al(istisna: &mut EXCEPINFO, doldur: bool) -> Istisna {
    if doldur && let Some(ertelenmis) = istisna.pfnDeferredFillIn.take() {
        // SAFETY: Office bu işlevi tam bu EXCEPINFO'yu doldurmak için verdi (IDispatch::Invoke
        // sözleşmesi); `istisna` geçerli ve yazılabilir.
        let _ = unsafe { ertelenmis(istisna) };
    }
    let _kaynak = ManuallyDrop::into_inner(std::mem::take(&mut istisna.bstrSource));
    let _yardim = ManuallyDrop::into_inner(std::mem::take(&mut istisna.bstrHelpFile));
    let aciklama = ManuallyDrop::into_inner(std::mem::take(&mut istisna.bstrDescription));
    // `scode` boşsa `wCode` bir Visual Basic hata numarasıdır (0x800A0000 + numara).
    let kod = if istisna.scode != 0 {
        istisna.scode
    } else if istisna.wCode != 0 {
        (0x800A_0000_u32 | u32::from(istisna.wCode)).cast_signed()
    } else {
        DISP_E_EXCEPTION.0
    };
    Istisna { kod, aciklama: aciklama.to_string().trim().to_owned() }
}

fn mesgul_mu(kod: HRESULT) -> bool {
    kod == RPC_E_CALL_REJECTED || kod == RPC_E_SERVERCALL_RETRYLATER
}

/// Bir otomasyon nesnesi (IDispatch). Ömrü onu veren [`Daire`]'ye bağlı; iş parçacığından çıkamaz.
pub(crate) struct Nesne<'d> {
    disp: IDispatch,
    _daire: PhantomData<&'d Daire>,
}

impl<'d> Nesne<'d> {
    /// `progid` ile kayıtlı uygulamayı ayrı bir süreç olarak başlatır ya da çalışan örneğine bağlanır
    /// (`CLSCTX_LOCAL_SERVER`). Hangisinin olacağına uygulama karar verir: PowerPoint tek örneklidir.
    pub(crate) fn olustur(_daire: &'d Daire, progid: &str) -> Result<Nesne<'d>, ComHatasi> {
        let ad: Vec<u16> = progid.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `ad` NUL ile biten bir UTF-16 dizisi ve çağrı boyunca yaşıyor; bu iş parçacığında
        // COM başlatılmış (`_daire`).
        let clsid = unsafe { CLSIDFromProgID(PCWSTR(ad.as_ptr())) }.map_err(|e| ComHatasi::windows("CLSIDFromProgID", &e))?;
        // SAFETY: `clsid` az önce okunan geçerli bir CLSID; dış nesne yok; bu iş parçacığında COM
        // STA olarak başlatılmış (`_daire`). Dönen arayüzün sahipliği `disp`'e geçer.
        let disp = unsafe { CoCreateInstance::<_, IDispatch>(&clsid, None::<&IUnknown>, CLSCTX_LOCAL_SERVER) }
            .map_err(|e| ComHatasi::windows("CoCreateInstance", &e))?;
        Ok(Nesne { disp, _daire: PhantomData })
    }

    /// Özelliği okur.
    pub(crate) fn al(&self, ad: &str) -> Result<Deger<'d>, ComHatasi> {
        self.al_arg(ad, &[])
    }

    /// Bağımsız değişkenli bir özelliği okur (`Item(1)`, `Range("A1")` gibi).
    pub(crate) fn al_arg(&self, ad: &str, argumanlar: &[Arg<'_>]) -> Result<Deger<'d>, ComHatasi> {
        let kimlik = self.kimlik(ad)?;
        let mut degerler = sirali(argumanlar);
        self.cagir_ic(ad, kimlik, DISPATCH_PROPERTYGET, &mut degerler, &mut [])
    }

    /// Özelliğe yazar.
    pub(crate) fn ata(&self, ad: &str, deger: Arg<'_>) -> Result<(), ComHatasi> {
        let kimlik = self.kimlik(ad)?;
        let mut degerler = [deger.varyant()];
        let mut adlar = [DISPID_PROPERTYPUT];
        self.cagir_ic(ad, kimlik, DISPATCH_PROPERTYPUT, &mut degerler, &mut adlar).map(drop)
    }

    /// Yöntemi sıralı bağımsız değişkenlerle çağırır.
    pub(crate) fn cagir(&self, ad: &str, argumanlar: &[Arg<'_>]) -> Result<Deger<'d>, ComHatasi> {
        let kimlik = self.kimlik(ad)?;
        let mut degerler = sirali(argumanlar);
        self.cagir_ic(ad, kimlik, DISPATCH_METHOD, &mut degerler, &mut [])
    }

    /// Yöntemi adlı bağımsız değişkenlerle çağırır; verilmeyenler Office'in varsayılanını alır.
    pub(crate) fn cagir_adli(&self, ad: &str, argumanlar: &[(&str, Arg<'_>)]) -> Result<Deger<'d>, ComHatasi> {
        let mut adlar = Vec::with_capacity(argumanlar.len().saturating_add(1));
        adlar.push(ad);
        adlar.extend(argumanlar.iter().map(|(ad, _)| *ad));
        let kimlikler = self.kimlikler(&adlar)?;
        let Some((kimlik, parametreler)) = kimlikler.split_first() else {
            return Err(ComHatasi::yeni(ad, DISP_E_UNKNOWNNAME, "üye kimliği alınamadı"));
        };
        let mut parametreler = parametreler.to_vec();
        // Adlı bağımsız değişkenlerde `rgvarg[i]`, `rgdispidNamedArgs[i]`'nin değeridir.
        let mut degerler: Vec<Varyant> = argumanlar.iter().map(|(_, arg)| arg.varyant()).collect();
        self.cagir_ic(ad, *kimlik, DISPATCH_METHOD, &mut degerler, &mut parametreler)
    }

    fn kimlik(&self, ad: &str) -> Result<i32, ComHatasi> {
        let kimlikler = self.kimlikler(&[ad])?;
        kimlikler.first().copied().ok_or_else(|| ComHatasi::yeni(ad, DISP_E_UNKNOWNNAME, "üye kimliği alınamadı"))
    }

    /// `adlar`'ın kimlikleri (DISPID): ilki üyenin, kalanlar adlı bağımsız değişkenlerinin.
    fn kimlikler(&self, adlar: &[&str]) -> Result<Vec<i32>, ComHatasi> {
        let uye = adlar.first().copied().unwrap_or_default();
        let sayi = u32::try_from(adlar.len()).map_err(|_| ComHatasi::yeni(uye, DISP_E_UNKNOWNNAME, "çok fazla ad"))?;
        let genis: Vec<Vec<u16>> = adlar.iter().map(|ad| ad.encode_utf16().chain(std::iter::once(0)).collect()).collect();
        let isaretler: Vec<PCWSTR> = genis.iter().map(|ad| PCWSTR(ad.as_ptr())).collect();
        let mut kimlikler = vec![0_i32; adlar.len()];
        let baslangic = Instant::now();
        loop {
            // SAFETY: `isaretler` `genis`'teki `sayi` tane NUL ile biten UTF-16 dizisini gösteriyor ve
            // ikisi de çağrı boyunca yaşıyor; `kimlikler`'de `sayi` DISPID'lik yer var; IID boş
            // (IID_NULL) olmalı.
            let sonuc = unsafe { self.disp.GetIDsOfNames(&GUID::zeroed(), isaretler.as_ptr(), sayi, YEREL_AYAR, kimlikler.as_mut_ptr()) };
            match sonuc {
                Ok(()) => return Ok(kimlikler),
                Err(hata) if mesgul_mu(hata.code()) && baslangic.elapsed() < MESGUL_SURESI => std::thread::sleep(MESGUL_ARASI),
                Err(hata) => return Err(ComHatasi::windows(&adlar.join(", "), &hata)),
            }
        }
    }

    /// `IDispatch::Invoke`. `degerler` ters sırayla (sıralılar) ya da `adlar` ile eşleşmiş (adlılar).
    fn cagir_ic(&self, uye: &str, kimlik: i32, bayrak: DISPATCH_FLAGS, degerler: &mut [Varyant], adlar: &mut [i32]) -> Result<Deger<'d>, ComHatasi> {
        let deger_sayisi = u32::try_from(degerler.len()).map_err(|_| ComHatasi::yeni(uye, DISP_E_PARAMNOTFOUND, "çok fazla bağımsız değişken"))?;
        let ad_sayisi = u32::try_from(adlar.len()).map_err(|_| ComHatasi::yeni(uye, DISP_E_PARAMNOTFOUND, "çok fazla bağımsız değişken"))?;
        let parametreler = DISPPARAMS {
            // Varyant, VARIANT'ın `repr(transparent)` sarmalayıcısı: dizi olduğu gibi VARIANT dizisidir.
            rgvarg: if degerler.is_empty() { std::ptr::null_mut() } else { degerler.as_mut_ptr().cast::<VARIANT>() },
            rgdispidNamedArgs: if adlar.is_empty() { std::ptr::null_mut() } else { adlar.as_mut_ptr() },
            cArgs: deger_sayisi,
            cNamedArgs: ad_sayisi,
        };
        let sonuc_ister = bayrak != DISPATCH_PROPERTYPUT;
        let baslangic = Instant::now();
        loop {
            let mut sonuc = Varyant::bos();
            let mut istisna = EXCEPINFO::default();
            let mut hatali_arguman = 0_u32;
            let sonuc_yeri = sonuc_ister.then_some(std::ptr::from_mut(&mut sonuc.0));
            // SAFETY: `parametreler`, çağrı boyunca ödünç alınmış `degerler`'deki `deger_sayisi` tane
            // kurulmuş VARIANT'ı ve `adlar`'daki `ad_sayisi` tane DISPID'i gösteriyor; `sonuc`,
            // `istisna` ve `hatali_arguman` yazılabilir; IID boş (IID_NULL) olmalı. Office yalnız
            // `sonuc`'u ve `istisna`'yı doldurur; ikisinin de sahibi biziz ve aşağıda bırakılır.
            let cagri = unsafe {
                self.disp.Invoke(
                    kimlik,
                    &GUID::zeroed(),
                    YEREL_AYAR,
                    bayrak,
                    &parametreler,
                    sonuc_yeri,
                    Some(std::ptr::from_mut(&mut istisna)),
                    Some(std::ptr::from_mut(&mut hatali_arguman)),
                )
            };
            let firlatilan = cagri.as_ref().err().is_some_and(|hata| hata.code() == DISP_E_EXCEPTION);
            let istisna = istisna_al(&mut istisna, firlatilan);
            match cagri {
                Ok(()) => return Ok(sonuc.deger()),
                Err(hata) if mesgul_mu(hata.code()) && baslangic.elapsed() < MESGUL_SURESI => std::thread::sleep(MESGUL_ARASI),
                Err(_) if firlatilan => {
                    let aciklama = if istisna.aciklama.is_empty() { "Office bir hata bildirdi".to_owned() } else { istisna.aciklama };
                    return Err(ComHatasi { kod: istisna.kod, aciklama, uye: uye.to_owned() });
                }
                Err(hata) => return Err(ComHatasi::windows(uye, &hata)),
            }
        }
    }
}

/// Sıralı bağımsız değişkenler: IDispatch onları ters sırayla ister.
fn sirali(argumanlar: &[Arg<'_>]) -> Vec<Varyant> {
    argumanlar.iter().rev().map(|arg| arg.varyant()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::Variant::VT_ERROR;

    #[test]
    fn degerler_variant_olarak_gidip_ayni_doner() {
        assert!(matches!(Arg::Tam(-7).varyant().deger(), Deger::Tam(-7)));
        assert!(matches!(Arg::Mantik(true).varyant().deger(), Deger::Mantik(true)));
        assert!(matches!(Arg::Mantik(false).varyant().deger(), Deger::Mantik(false)));
        assert!(matches!(Varyant::bos().deger(), Deger::Bos));
        match Arg::Metin("Çığ Şişe İğne Güneş Öğrenci").varyant().deger() {
            Deger::Metin(metin) => assert_eq!(metin, "Çığ Şişe İğne Güneş Öğrenci"),
            _ => panic!("metin bekleniyordu"),
        }
        match Arg::Yol(Path::new(r"C:\Kullanıcılar\Ömer\belge.docx")).varyant().deger() {
            Deger::Metin(metin) => assert_eq!(metin, r"C:\Kullanıcılar\Ömer\belge.docx"),
            _ => panic!("metin bekleniyordu"),
        }
        // Okunmayan bir tür (VT_ERROR) nesne değildir ve türüyle bildirilir.
        let hata = Varyant::yeni(VT_ERROR, VARIANT_0_0_0 { scode: DISP_E_PARAMNOTFOUND.0 }).deger().nesne("Open").err().unwrap();
        assert!(hata.aciklama.contains("10"), "{hata}");
    }

    #[test]
    fn office_hata_numarasi_okunur() {
        // Word 5408 (0x1520): "Parola yanlış."
        let hata = ComHatasi::yeni("Open", HRESULT(0x800A_1520_u32.cast_signed()), "parola");
        assert_eq!(hata.office_numarasi(), Some(5408));
        assert_eq!(hata.to_string(), "Open: parola (0x800A1520)");
        assert_eq!(ComHatasi::yeni("Open", DISP_E_EXCEPTION, "").office_numarasi(), None);
        assert!(ComHatasi::yeni("Open", DISP_E_UNKNOWNNAME, "").uye_yok());
        assert!(ComHatasi::yeni("Open", HRESULT(0x8001_0108_u32.cast_signed()), "").baglanti_koptu());
        assert!(!ComHatasi::yeni("Open", DISP_E_EXCEPTION, "").baglanti_koptu());
    }

    #[test]
    fn com_baslar_ve_kapanir() {
        let daire = Daire::baslat().unwrap();
        drop(daire);
        // Aynı iş parçacığında yeniden başlatılabilir: CoUninitialize dengelendi.
        let _daire = Daire::baslat().unwrap();
    }

    #[test]
    fn kayitsiz_progid_hata_verir() {
        let daire = Daire::baslat().unwrap();
        let Err(hata) = Nesne::olustur(&daire, "MigEZPdf.YokBoyleBirUygulama") else { panic!("kayıtsız ProgID başlatıldı") };
        assert_eq!(hata.uye, "CLSIDFromProgID");
    }
}
