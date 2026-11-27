craft-i18n
==========

Pdfrp'nin arayüz çevirileri. Derleme bu klasörü `MIGEZ_I18N_DIR` ortam değişkeniyle bulur
ve katalogları programa gömer; değişken verilmezse arayüz İngilizce olur.

## İçerik

| Yol | Ne |
|---|---|
| `locales/<dil>/common.tsv` | Paylaşılan dizeler. |
| `locales/<dil>/migez-pdf.tsv` | Pdfrp'nin kendi dizeleri ve paylaşılanların üzerine yazdıkları. |
| `language-data/<dil>/` | Yazım sözcük listeleri ve heceleme kalıpları; bkz. [`language-data/README.md`](language-data/README.md). |
| `docs/catalog-format.md` | Katalog biçimi ve denetlenen kurallar. |
| `ATTRIBUTION.md` | Çevirmenler ve dil verisi dosyalarının kaynağı ve lisansı. |
| `crates/craft-i18n-tests` | Katalog denetleyicisi (`cargo test --workspace`). |

`locales/` altındaki öteki `.tsv` dosyaları başka uygulamalara aittir; Pdfrp onları okumaz.

## Çeviri

Kataloglar düz metindir, satır başına bir dize: `bağlam<TAB>İngilizce<TAB>çeviri`. İngilizce sütun
programın gösterdiği metnin aynısıdır; eksik bir satır İngilizce görünür. `{ad}` yer tutucuları ve
sondaki `…` çeviride de bulunmalıdır. Ayrıntılar [`docs/catalog-format.md`](docs/catalog-format.md)
dosyasındadır.

**Temiz oda:** her satır İngilizce metnin düz anlamından yazılır; başka bir ürünün çevirisine,
dize tablosuna ya da çevrilmiş arayüzüne bakılmaz, oradan alınmaz.

## Lisans

Kod ve çeviriler "MIT OR Apache-2.0" lisanslıdır (`LICENSE-MIT`, `LICENSE-APACHE`). `language-data/`
altındaki dil verisi kendi lisansını taşır; her dosyanın yanında (`LICENSE-<kaynak>.txt`) ve
`ATTRIBUTION.md` dosyasında yazılıdır.
