# i18n: arayüz çevirileri

Bu klasör, programa derleme sırasında gömülen arayüz kataloglarını taşır:
`locales/<dil>/common.tsv` ve `locales/<dil>/migez-pdf.tsv`. `MIGEZ_I18N_DIR` ortam değişkeni
verilmediğinde `crates/ui-egui/build.rs` bu klasörü kendiliğinden kullanır.

Kataloglar craft-i18n çeviri projesinden alınmıştır. craft-i18n "MIT OR Apache-2.0" lisanslıdır ve
burada MIT ile kullanılır; telif bildirimi `LICENSE-MIT`, Apache metni `LICENSE-APACHE`,
çevirmenlerin listesi `ATTRIBUTION.md` dosyasındadır. Türkçe katalog (`locales/tr/`) Pdfrp
için değiştirilmiştir.

## English

The interface catalogs compiled into the program (`locales/<code>/common.tsv` and
`migez-pdf.tsv`), used by `crates/ui-egui/build.rs` when `MIGEZ_I18N_DIR` is not set. They come
from the craft-i18n translation project (MIT OR Apache-2.0, used here under MIT; see
`LICENSE-MIT`, `LICENSE-APACHE` and the translators in `ATTRIBUTION.md`). The Turkish catalog is
modified for Pdfrp.
