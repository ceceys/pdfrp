# fxtranslate 0.5.0: Pdfrp'nin değişiklikleri

Bu klasör [fxtranslate 0.5.0](https://crates.io/crates/fxtranslate/0.5.0)'ın kaynağıdır
(crates.io paketi `fxtranslate-0.5.0.crate`, SHA-256
`4dfa6e74911486c9d59495e595163d4965dec37aa92b08545782b696b3eec2eb`; üst kaynak
github.com/gregtatum/translations, `inference-rs/crates/fxtranslate`, işleme
`f2f77cd302bf616367aaf836c034af23dff6266f`, `.cargo_vcs_info.json`).

Lisans: Mozilla Public License 2.0 (`LICENSE`). Klasördeki her dosya, değiştirilenler dahil,
MPL-2.0 kapsamında kalır. İçindeki `vendor/gemmology` (MIT) ve `vendor/xsimd` (BSD-3-Clause)
kendi lisanslarıyla gelir; onlara dokunulmadı.

Pdfrp bu kodu yalnız `pdfrp-ceviri` yardımcı programında kullanır (`apps/pdfrp-ceviri`).
`download`, `net` ve `discovery` özellikleri açılmaz: programda ağ kodu yoktur.

## Değişen dosyalar

Yalnız iki dosya değişti; ikisi de Windows'ta derleme içindir, çeviri hesabını değiştirmez.
Değişen satırlar `Pdfrp:` diye başlayan bir yorumla işaretlidir. İki dosyada da değişiklikten
önce bir MPL başlık yorumu yoktu (dosyalar açıklama yorumuyla başlıyor); lisans bu klasörün
`LICENSE` dosyasıyla bildirilir.

### `src/gemmology_shim.cpp`

- **Neden:** `std::aligned_alloc` Windows'un C kitaplıklarında (MSVCRT, UCRT) yok. mingw g++ ile
  derleme `'aligned_alloc' is not a member of 'std'` hatasıyla duruyordu; fxtranslate'in
  `build.rs`'i bu hatada sessizce skaler çekirdeğe düşer.
- **Değişiklik:**
  - `_WIN32`'de `<malloc.h>` eklenir.
  - `aligned()` Windows'ta `_aligned_malloc(n, 64)` ile ayırır, başka yerde eskisi gibi
    `std::aligned_alloc(64, n)` ile.
  - Yeni `aligned_free()` Windows'ta `_aligned_free`, başka yerde `std::free` çağırır.
  - `aligned()` belleğini bırakan beş `std::free` çağrısı `aligned_free` oldu (`gemmology_prepare_b`,
    `gemmology_free_b`, `gemmology_multiply`). Shim'de başka `std::free` yok.
- Özgün SHA-256: `b5ab774bf1a2c12e1002e8c90deb23c0e909edefe6c1f5ad2096021fe63d3597`
- Değişmiş SHA-256: `627b7e96d48ccab8852792971dd05192b274961a622897d919db4e11a6e169df`

### `build.rs`

- **Neden 1:** MSVC'de (cl.exe, clang-cl) AVX2 bayrağı `/arch:AVX2`'dir; `-mavx2`'yi uyarıyla
  yok sayar, `__AVX2__` tanımsız kalır ve xsimd'in AVX2 çekirdeği derlenmez. Pdfrp'nin sürüm iş
  akışı (`.github/workflows/release.yml`) MSVC ile derler.
- **Neden 2:** `CXXSTDLIB_STATIC=1` C++ standart kitaplığını exe'ye gömmeli (GNU'da
  `libstdc++-6.dll` gerekmesin). cc 1.12 bu değişkeni kendisi okur; Pdfrp'nin Cargo.lock'undaki
  cc 1.5.1 okumaz.
- **Değişiklik:**
  - Derleyici MSVC türündeyse x86_64'te `-mavx2` yerine `/arch:AVX2` verilir.
  - `CXXSTDLIB_STATIC` boş değilse ve `0`, `no`, `false` değilse `cpp_link_stdlib_static(true)`
    çağrılır; değişken değişince derleme betiği yeniden çalışır. MSVC'de ayrı bir C++
    kitaplığı bağlanmadığı için orada etkisi yoktur.
  - `cc::Build`'e `cpp(true)` derleyici sorgusundan önce verilir (zincirdeki ikinci `cpp(true)`
    özgün satırdır, kaldı).
- Özgün SHA-256: `c14a1528f486a17eccf20995262db34d2c309c88a239857c871c43125d5b63f2`
- Değişmiş SHA-256: `489c93b9b4894f45d9d61c43c5d1e9770758ce7c2d753d912c9b1d371ffa9292`

## Çıkarılan dosyalar

`tests/fixtures/rs-models-v2.json` ve `tests/fixtures/tiny.bin.zst` bu kopyaya alınmadı. Bunları yalnız fxtranslate'in kendi `tests/packaging.rs` sınaması okur; o sınama, Pdfrp'de kapalı tutulan `discovery` ve `download` kodunu sınar. Pdfrp'nin derlemesi, programları ve sınamaları bu dosyaları kullanmaz. Pdfrp deposundaki MPL-2.0 istisnası fxtranslate'in kodu içindir (`vendor/README.md`); sınama verisi bu istisnanın dışında kalır. Özgün dosyalar crates.io paketinde (`fxtranslate-0.5.0.crate`) durur.

- `tests/fixtures/rs-models-v2.json` özgün SHA-256: `6f3bd3883edeaceccdd3ef3939b42e8cf26c00e06362fabba2a49db821b721a5`
- `tests/fixtures/tiny.bin.zst` özgün SHA-256: `cb7f98c7ec6f844d1d212cfa1f63aa554ae184e3e3d806f9b2476216352bc1d9`

## Sınama

- GNU (rustc 1.100.0 `x86_64-pc-windows-gnu`, mingw g++ 16.2.0): `FXTRANSLATE_REQUIRE_SIMD=1` ile
  AVX2 çekirdeği derlendi; `apps/pdfrp-ceviri`'nin
  `the_avx2_kernel_is_linked_and_agrees_with_the_scalar_one` sınaması çekirdeğin `avx2` olduğunu ve
  skaler çekirdekle aynı sonucu verdiğini gösterir. Exe yalnız Windows sistem DLL'lerini içe
  aktarır (libstdc++ gömülü).
- MSVC: bu bilgisayarda MSVC yok; `/arch:AVX2` yolu yerelde derlenmedi. İlk sınama GitHub
  Actions sürüm iş akışıdır (`FXTRANSLATE_REQUIRE_SIMD=1` derlenemezse durdurur).

## Üst kaynağa bildirim

Yamaları fxtranslate geliştiricisine bildirmek ağ işidir; karar Pdfrp'nin sahibinindir.

---

# fxtranslate 0.5.0: Pdfrp's changes (English)

This folder is fxtranslate 0.5.0's source as published on crates.io (see above). Two files are
changed, both for building on Windows; neither changes how anything is computed. Changed lines
carry a `Pdfrp:` comment. Every file of fxtranslate here, the changed ones included, stays under
the Mozilla Public License 2.0 (`LICENSE`). The C++ header libraries it bundles keep their own
licences and are not changed: `vendor/gemmology` (MIT) and `vendor/xsimd` (BSD-3-Clause).

- `src/gemmology_shim.cpp`: Windows' C libraries have no `std::aligned_alloc`; on `_WIN32` the
  shim allocates with `_aligned_malloc` and frees with `_aligned_free` (a new `aligned_free()`
  replaces the five `std::free` calls on `aligned()` memory).
- `build.rs`: MSVC gets `/arch:AVX2` instead of `-mavx2`; `CXXSTDLIB_STATIC` links the C++
  standard library statically with the cc 1.5 in Pdfrp's lockfile too.

Two test data files are left out: `tests/fixtures/rs-models-v2.json` and `tests/fixtures/tiny.bin.zst`. Only fxtranslate's own `tests/packaging.rs` reads them, and it tests the `discovery` and `download` code that Pdfrp keeps off; Pdfrp's build, programs and tests do not use them. Pdfrp's MPL-2.0 exception covers fxtranslate's code (`vendor/README.md`), not test data. The original files are in the crates.io package; their SHA-256 values are listed in the Turkish section above.
