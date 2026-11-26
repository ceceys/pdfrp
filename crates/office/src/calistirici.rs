//! Dönüştürmeyi kendi STA iş parçacığında çalıştırır ve en çok verilen süre kadar bekler.
//!
//! - Dönüştürmeler sırayla yapılır: aynı anda tek bir iş Office'le konuşur (ayarları değiştirip geri
//!   yükleyen iki iş birbirinin eski değerini bozmasın). Sırası gelmeyen iş bekler; süre dolarsa
//!   [`OfficeHatasi::Mesgul`].
//! - Süre dolunca çağırana [`OfficeHatasi::ZamanAsimi`] döner. Bir COM çağrısı yarıda kesilemez:
//!   iş parçacığı Office'in yanıtını bekler, sonra kalan adımları atlayıp yalnız temizlik yapar
//!   (kendi belgesini kapatır, Office'i kendisi başlattıysa kapatır, geçici dosyaları siler).
//! - İşteki bir panik iş parçacığında yakalanır ve hataya çevrilir; uygulamayı düşürmez.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{RecvTimeoutError, sync_channel};
use std::time::{Duration, Instant};

use crate::OfficeHatasi;
use crate::com::Daire;

/// Office'le şu anda bir iş konuşuyor mu.
static MESGUL: AtomicBool = AtomicBool::new(false);

/// Sıra beklerken denemeler arası süre.
const SIRA_ARASI: Duration = Duration::from_millis(50);

/// İşin sırası: alındığında bayrak kalkar, düşünce iner.
struct Sira {
    bayrak: &'static AtomicBool,
}

impl Sira {
    /// `son` anına kadar sırayı almaya çalışır.
    fn al(bayrak: &'static AtomicBool, son: Instant) -> Option<Sira> {
        loop {
            if bayrak.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire).is_ok() {
                return Some(Sira { bayrak });
            }
            let simdi = Instant::now();
            if simdi >= son {
                return None;
            }
            std::thread::sleep(SIRA_ARASI.min(son.saturating_duration_since(simdi)));
        }
    }
}

impl Drop for Sira {
    fn drop(&mut self) {
        self.bayrak.store(false, Ordering::Release);
    }
}

/// Bir işin vazgeçilip vazgeçilmediğini söyler: süre dolunca çağıran bayrağı kaldırır.
pub(crate) struct Iptal(Arc<AtomicBool>);

impl Iptal {
    /// Çağıran beklemekten vazgeçti mi.
    pub(crate) fn istendi(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }

    /// Vazgeçildiyse işin kalan adımlarını atlatan hata.
    pub(crate) fn denetle(&self) -> Result<(), OfficeHatasi> {
        if self.istendi() { Err(OfficeHatasi::Ic { ayrinti: "beklemekten vazgeçildi".into() }) } else { Ok(()) }
    }
}

/// `is`'i kendi STA iş parçacığında çalıştırır ve sonucunu en çok `sure` kadar bekler; süre sıra
/// beklemeyi de kapsar.
pub(crate) fn calistir<T, F>(sure: Duration, is: F) -> Result<T, OfficeHatasi>
where
    T: Send + 'static,
    F: FnOnce(&Daire, &Iptal) -> Result<T, OfficeHatasi> + Send + 'static,
{
    calistir_bayrakla(&MESGUL, sure, is)
}

fn calistir_bayrakla<T, F>(bayrak: &'static AtomicBool, sure: Duration, is: F) -> Result<T, OfficeHatasi>
where
    T: Send + 'static,
    F: FnOnce(&Daire, &Iptal) -> Result<T, OfficeHatasi> + Send + 'static,
{
    let baslangic = Instant::now();
    let son = baslangic.checked_add(sure).unwrap_or(baslangic);
    let sira = Sira::al(bayrak, son).ok_or(OfficeHatasi::Mesgul)?;
    let iptal = Arc::new(AtomicBool::new(false));
    let is_iptali = Iptal(Arc::clone(&iptal));
    let (gonder, al) = sync_channel(1);
    let is_parcacigi = std::thread::Builder::new()
        .name("migez-office".into())
        .spawn(move || {
            let sonuc = catch_unwind(AssertUnwindSafe(|| {
                let daire = Daire::baslat().map_err(|hata| OfficeHatasi::Ic { ayrinti: hata.to_string() })?;
                // `daire`'ye bağlı bütün COM nesneleri `is` dönmeden bırakılır; daire en son düşer.
                is(&daire, &is_iptali)
            }))
            .unwrap_or_else(|_| Err(OfficeHatasi::Ic { ayrinti: "Office iş parçacığı beklenmedik biçimde durdu".into() }));
            // Sıra sonuç gönderilmeden bırakılır: sonucu alan çağıran hemen yeni bir iş başlatabilir.
            drop(sira);
            let _ = gonder.send(sonuc);
        })
        .map_err(|hata| OfficeHatasi::Ic { ayrinti: format!("iş parçacığı başlatılamadı: {hata}") })?;
    let kalan = son.saturating_duration_since(Instant::now());
    match al.recv_timeout(kalan) {
        Ok(sonuc) => {
            let _ = is_parcacigi.join();
            sonuc
        }
        Err(RecvTimeoutError::Timeout) => {
            // İş parçacığı bırakılır; Office yanıt verince kalan adımları atlar ve temizlik yapar.
            iptal.store(true, Ordering::Release);
            // Saniyeye yukarı yuvarlanır: "0 saniyede bitmedi" denmez.
            Err(OfficeHatasi::ZamanAsimi { saniye: sure.as_secs().saturating_add(u64::from(sure.subsec_nanos() > 0)) })
        }
        Err(RecvTimeoutError::Disconnected) => {
            let _ = is_parcacigi.join();
            Err(OfficeHatasi::Ic { ayrinti: "Office iş parçacığı sonuç vermeden bitti".into() })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sonuc_is_parcacigindan_gelir() {
        static BAYRAK: AtomicBool = AtomicBool::new(false);
        let sonuc = calistir_bayrakla(&BAYRAK, Duration::from_secs(10), |_, iptal| Ok(iptal.istendi()));
        assert_eq!(sonuc, Ok(false));
        assert!(!BAYRAK.load(Ordering::Acquire), "iş bitince sıra bırakılır");
    }

    #[test]
    fn sure_dolunca_beklemekten_vazgecilir_ve_is_haber_alir() {
        static BAYRAK: AtomicBool = AtomicBool::new(false);
        let (haber_gonder, haber_al) = sync_channel(1);
        let baslangic = Instant::now();
        let sonuc = calistir_bayrakla(&BAYRAK, Duration::from_millis(200), move |_, iptal| {
            std::thread::sleep(Duration::from_millis(800));
            let _ = haber_gonder.send(iptal.istendi());
            Ok(())
        });
        assert_eq!(sonuc, Err(OfficeHatasi::ZamanAsimi { saniye: 1 }));
        assert!(baslangic.elapsed() < Duration::from_millis(700), "çağıran işi beklemedi");
        assert_eq!(haber_al.recv_timeout(Duration::from_secs(10)), Ok(true), "iş vazgeçildiğini gördü");
        // İş bitince sıra bırakılır.
        let son = Instant::now() + Duration::from_secs(10);
        assert!(Sira::al(&BAYRAK, son).is_some());
    }

    #[test]
    fn suren_is_varken_ikincisi_mesgul_doner() {
        static BAYRAK: AtomicBool = AtomicBool::new(false);
        let (basladi_gonder, basladi_al) = sync_channel(1);
        let ilk = std::thread::spawn(move || {
            calistir_bayrakla(&BAYRAK, Duration::from_secs(10), move |_, _| {
                let _ = basladi_gonder.send(());
                std::thread::sleep(Duration::from_millis(600));
                Ok(1)
            })
        });
        basladi_al.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(calistir_bayrakla(&BAYRAK, Duration::from_millis(100), |_, _| Ok(2)), Err(OfficeHatasi::Mesgul));
        // Sırasını bekleyen iş, süresi yetince ilk işten sonra çalışır.
        assert_eq!(calistir_bayrakla(&BAYRAK, Duration::from_secs(10), |_, _| Ok(3)), Ok(3));
        assert_eq!(ilk.join().unwrap(), Ok(1));
    }

    #[test]
    fn isteki_panik_hataya_donusur() {
        static BAYRAK: AtomicBool = AtomicBool::new(false);
        let sonuc: Result<(), _> = calistir_bayrakla(&BAYRAK, Duration::from_secs(10), |_, _| panic!("deneme paniği"));
        assert!(matches!(sonuc, Err(OfficeHatasi::Ic { .. })), "{sonuc:?}");
        assert!(!BAYRAK.load(Ordering::Acquire), "panikten sonra da sıra bırakılır");
    }
}
