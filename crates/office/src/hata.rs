//! Dönüştürme hataları; her biri kullanıcıya gösterilebilecek Türkçe bir ileti verir.

use std::fmt;
use std::path::PathBuf;

use crate::Uygulama;

/// Office ile dönüştürmenin hataları. `Display` kullanıcıya gösterilebilecek Türkçe bir ileti verir.
/// `ayrinti` alanlarında Office'in ya da Windows'un kendi iletisi durur (çoğunlukla Office'in dilinde);
/// iletiye ayraç içinde eklenir.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OfficeHatasi {
    /// Microsoft Office ile dönüştürme yalnız Windows'ta var.
    Desteklenmiyor,
    /// Dosyanın uzantısı bu dönüştürmenin kabul ettiklerinden değil.
    BicimDesteklenmiyor {
        /// Küçük harfle, noktasız uzantı; dosyanın uzantısı yoksa boş.
        uzanti: String,
    },
    /// Gereken Office uygulaması kurulu değil.
    UygulamaYok(Uygulama),
    /// Office uygulaması başlatılamadı.
    UygulamaBaslatilamadi { uygulama: Uygulama, ayrinti: String },
    /// Girdi dosyası yok ya da bir dosya değil.
    DosyaYok(PathBuf),
    /// Çıktı yolu girdi dosyasının kendisi.
    AyniDosya,
    /// Sonuç dosyası yazılamadı: klasör yok, izin yok ya da dosya başka bir programda açık.
    CiktiYazilamadi { ayrinti: String },
    /// Belge parolayla korunuyor ya da şifreli.
    ParolaKorumali,
    /// Office dosyayı açamadı: bozuk, desteklenmeyen bir sürümde ya da Office ayarlarınca engelli.
    DosyaAcilamadi { uygulama: Uygulama, ayrinti: String },
    /// Office dosyayı açtı ama dönüştüremedi.
    DonusturmeBasarisiz { uygulama: Uygulama, ayrinti: String },
    /// Dönüştürme verilen sürede bitmedi. Office arka planda işini bitirince kendi belgesini kapatır.
    ZamanAsimi { saniye: u64 },
    /// Önceki bir dönüştürme hâlâ sürüyor; dönüştürmeler sırayla yapılır.
    Mesgul,
    /// Office ile bağlantı koptu: uygulama kapandı ya da çöktü.
    BaglantiKoptu { uygulama: Uygulama },
    /// Beklenmeyen bir iç hata.
    Ic { ayrinti: String },
}

/// Ayrıntı iletisinin en çok bu kadar karakteri gösterilir.
const AYRINTI_SINIRI: usize = 300;

/// `ayrinti` boş değilse " (ayrinti)" yazar; satır sonlarını boşluğa çevirir, uzunsa keser.
fn ayrinti_ekle(f: &mut fmt::Formatter<'_>, ayrinti: &str) -> fmt::Result {
    let tek_satir = ayrinti.split_whitespace().collect::<Vec<_>>().join(" ");
    if tek_satir.is_empty() {
        return Ok(());
    }
    match tek_satir.char_indices().nth(AYRINTI_SINIRI) {
        Some((kesim, _)) => write!(f, " ({}…)", tek_satir.get(..kesim).unwrap_or_default()),
        None => write!(f, " ({tek_satir})"),
    }
}

impl fmt::Display for OfficeHatasi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OfficeHatasi::Desteklenmiyor => f.write_str("Microsoft Office ile dönüştürme yalnız Windows'ta yapılabilir."),
            OfficeHatasi::BicimDesteklenmiyor { uzanti } if uzanti.is_empty() => {
                f.write_str("Dosyanın uzantısı yok; hangi Office uygulamasıyla dönüştürüleceği anlaşılamadı.")
            }
            OfficeHatasi::BicimDesteklenmiyor { uzanti } => write!(f, "\".{uzanti}\" uzantılı dosyalar bu yolla dönüştürülemiyor."),
            OfficeHatasi::UygulamaYok(uygulama) => {
                write!(f, "Bu dönüştürme için {} gerekiyor, ancak bu bilgisayarda kurulu değil.", uygulama.ad())
            }
            OfficeHatasi::UygulamaBaslatilamadi { uygulama, ayrinti } => {
                write!(f, "{} başlatılamadı.", uygulama.ad())?;
                ayrinti_ekle(f, ayrinti)
            }
            OfficeHatasi::DosyaYok(yol) => write!(f, "Dosya bulunamadı: {}", yol.display()),
            OfficeHatasi::AyniDosya => f.write_str("Sonuç, dönüştürülen dosyanın üzerine yazılamaz; başka bir ad seçin."),
            OfficeHatasi::CiktiYazilamadi { ayrinti } => {
                f.write_str("Sonuç dosyası kaydedilemedi.")?;
                ayrinti_ekle(f, ayrinti)
            }
            OfficeHatasi::ParolaKorumali => {
                f.write_str("Dosya parolayla korunuyor. Office ile dönüştürmek için önce parolasını kaldırın.")
            }
            // Office nedenini söylediyse o yazılır; söylemediyse en olası nedenler.
            OfficeHatasi::DosyaAcilamadi { uygulama, ayrinti } if ayrinti.trim().is_empty() => {
                write!(f, "{} dosyayı açamadı; dosya bozuk ya da desteklenmeyen bir sürümde olabilir.", uygulama.ad())
            }
            OfficeHatasi::DosyaAcilamadi { uygulama, ayrinti } => {
                write!(f, "{} dosyayı açamadı.", uygulama.ad())?;
                ayrinti_ekle(f, ayrinti)
            }
            OfficeHatasi::DonusturmeBasarisiz { uygulama, ayrinti } => {
                write!(f, "{} dosyayı dönüştüremedi.", uygulama.ad())?;
                ayrinti_ekle(f, ayrinti)
            }
            OfficeHatasi::ZamanAsimi { saniye } => {
                write!(f, "Dönüştürme {saniye} saniyede bitmedi. Office işini arka planda bitirince kendiliğinden kapanır.")
            }
            OfficeHatasi::Mesgul => f.write_str("Önceki Office dönüştürmesi henüz bitmedi. Biraz sonra yeniden deneyin."),
            OfficeHatasi::BaglantiKoptu { uygulama } => {
                write!(f, "{} ile bağlantı koptu; uygulama kapanmış ya da yanıt vermiyor olabilir.", uygulama.ad())
            }
            OfficeHatasi::Ic { ayrinti } => {
                f.write_str("Office ile çalışırken beklenmeyen bir hata oluştu.")?;
                ayrinti_ekle(f, ayrinti)
            }
        }
    }
}

impl std::error::Error for OfficeHatasi {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iletiler_turkce_ve_dolu() {
        let hatalar = [
            OfficeHatasi::Desteklenmiyor,
            OfficeHatasi::BicimDesteklenmiyor { uzanti: "png".into() },
            OfficeHatasi::BicimDesteklenmiyor { uzanti: String::new() },
            OfficeHatasi::UygulamaYok(Uygulama::Excel),
            OfficeHatasi::UygulamaBaslatilamadi { uygulama: Uygulama::Word, ayrinti: String::new() },
            OfficeHatasi::DosyaYok(PathBuf::from(r"C:\yok\belge.docx")),
            OfficeHatasi::AyniDosya,
            OfficeHatasi::CiktiYazilamadi { ayrinti: "Erişim engellendi.".into() },
            OfficeHatasi::ParolaKorumali,
            OfficeHatasi::DosyaAcilamadi { uygulama: Uygulama::PowerPoint, ayrinti: String::new() },
            OfficeHatasi::DonusturmeBasarisiz { uygulama: Uygulama::Excel, ayrinti: "Yazdırılacak bir şey yok.".into() },
            OfficeHatasi::ZamanAsimi { saniye: 120 },
            OfficeHatasi::Mesgul,
            OfficeHatasi::BaglantiKoptu { uygulama: Uygulama::Word },
            OfficeHatasi::Ic { ayrinti: "x".into() },
        ];
        for hata in hatalar {
            let ileti = hata.to_string();
            assert!(ileti.len() > 20, "{ileti}");
            assert!(!ileti.contains("()"), "boş ayrıntı ayraçsız kalır: {ileti}");
        }
        assert_eq!(
            OfficeHatasi::UygulamaYok(Uygulama::Excel).to_string(),
            "Bu dönüştürme için Microsoft Excel gerekiyor, ancak bu bilgisayarda kurulu değil."
        );
        assert_eq!(
            OfficeHatasi::CiktiYazilamadi { ayrinti: "Erişim\r\n engellendi.".into() }.to_string(),
            "Sonuç dosyası kaydedilemedi. (Erişim engellendi.)"
        );
        assert_eq!(
            OfficeHatasi::DosyaAcilamadi { uygulama: Uygulama::Word, ayrinti: "Dosya bozuk görünüyor.".into() }.to_string(),
            "Microsoft Word dosyayı açamadı. (Dosya bozuk görünüyor.)"
        );
        assert_eq!(
            OfficeHatasi::DosyaAcilamadi { uygulama: Uygulama::Word, ayrinti: " ".into() }.to_string(),
            "Microsoft Word dosyayı açamadı; dosya bozuk ya da desteklenmeyen bir sürümde olabilir."
        );
    }

    #[test]
    fn uzun_ayrinti_harf_sinirinda_kesilir() {
        let uzun = "ğ".repeat(AYRINTI_SINIRI + 50);
        let ileti = OfficeHatasi::Ic { ayrinti: uzun }.to_string();
        assert!(ileti.ends_with("…)"), "{ileti}");
        assert_eq!(ileti.matches('ğ').count(), AYRINTI_SINIRI);
    }
}
