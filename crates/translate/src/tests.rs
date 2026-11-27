use std::collections::{HashMap, HashSet};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use migez_update::{FetchError, Transport};
use sha2::{Digest, Sha256};

use crate::languages::{self, ENGLISH, LANGUAGES};
use crate::manifest::{self, Direction, ModelFile};
use crate::store::{self, InstallError, MARKER, ModelStore, Source};
use crate::text::{self, Line};

// ---------------------------------------------------------------------------------------------
// The embedded manifest

#[test]
fn the_manifest_has_114_directions_each_with_a_model_and_its_vocabularies() {
    let m = manifest::embedded().unwrap();
    assert_eq!(m.tag, "models-v3.0");
    assert_eq!(m.title, "Pdfrp dil modülleri 3.0");
    assert_eq!(m.base_url, "https://github.com/ceceys/pdfrp/releases/download/models-v3.0/");
    assert_eq!(m.license, "MPL-2.0");
    assert_eq!(m.directions.len(), 114);
    for d in &m.directions {
        let kinds: Vec<&str> = d.files.iter().map(|f| f.kind.as_str()).collect();
        let shared = kinds.len() == 2 && kinds.contains(&"model") && kinds.contains(&"vocab");
        let split = kinds.len() == 3 && kinds.contains(&"model") && kinds.contains(&"srcvocab") && kinds.contains(&"trgvocab");
        assert!(shared || split, "{}: {kinds:?}", d.id());
        assert!(matches!(d.architecture.as_str(), "tiny" | "base-memory"), "{}", d.id());
        for f in &d.files {
            // `<from>-<to>.` and Mozilla's attachment name, which is not always the unpacked name
            // with `.zst` (en-nb's model is `model.enno…zst`, unpacked `model.ennb…bin`).
            assert!(f.asset.starts_with(&format!("{}.", d.id())) && f.asset.ends_with(".zst"), "{}: {}", d.id(), f.asset);
        }
    }
    let architectures = |a: &str| m.directions.iter().filter(|d| d.architecture == a).count();
    assert_eq!((architectures("base-memory"), architectures("tiny")), (75, 39));
}

#[test]
fn every_language_has_both_directions_and_a_name() {
    let m = manifest::embedded().unwrap();
    let codes = m.language_codes();
    assert_eq!(codes.len(), m.languages);
    assert_eq!(m.languages, 57, "the manifest counts 57 languages besides English");
    assert_eq!(m.directions.len(), 2 * m.languages);
    for code in &codes {
        assert_eq!(m.modules_of(code).len(), 2, "{code}: to and from English");
        assert!(LANGUAGES.iter().any(|l| l.code == *code), "{code} has no name");
    }
    let table: HashSet<&str> = LANGUAGES.iter().map(|l| l.code).collect();
    assert_eq!(table, codes.iter().copied().collect::<HashSet<_>>(), "the names table lists exactly the manifest's languages");
    assert_eq!(LANGUAGES.len(), table.len(), "no language twice");
    assert_eq!(languages::name("tr", true), "Türkçe");
    assert_eq!(languages::name("de", false), "German");
    assert_eq!(languages::name("en", true), "İngilizce");
}

#[test]
fn release_names_are_unique_and_every_download_is_one_this_repository_may_reach() {
    let m = manifest::embedded().unwrap();
    let mut seen = HashSet::new();
    for f in m.directions.iter().flat_map(|d| &d.files) {
        assert!(seen.insert(f.asset.clone()), "{} twice", f.asset);
        let url = m.url(f);
        // The update check's own address check (migez_update::allowed): HTTPS, GitHub, and on
        // github.com only this repository's release downloads.
        migez_update::allowed(&url).unwrap();
        assert!(url.starts_with("https://github.com/ceceys/pdfrp/releases/download/models-v3.0/"), "{url}");
    }
    assert_eq!(seen.len(), 233);
}

#[test]
fn a_broken_manifest_is_refused() {
    let good = manifest::EMBEDDED;
    assert!(manifest::parse(good).is_ok());
    for (from, to) in [
        ("\"name\": \"model.afen.intgemm.alphas.bin\"", "\"name\": \"../model.afen.intgemm.alphas.bin\""),
        ("\"version\": \"3.0\"", "\"version\": \"3.0/..\""),
        ("\"languages\": 57", "\"languages\": 58"),
        ("https://github.com/ceceys", "http://github.com/ceceys"),
    ] {
        assert!(good.contains(from), "{from}");
        let broken = good.replacen(from, to, 1);
        assert!(manifest::parse(&broken).is_err(), "{to}");
    }
}

#[test]
fn two_other_languages_go_through_english() {
    let m = manifest::embedded().unwrap();
    let ids = |from: &str, to: &str| m.route(from, to).map(|r| r.iter().map(|d| d.id()).collect::<Vec<_>>());
    assert_eq!(ids("tr", "de").unwrap(), ["tr-en", "en-de"]);
    assert_eq!(ids("zh-Hans", "ja").unwrap(), ["zh-Hans-en", "en-ja"]);
    assert_eq!(ids("tr", "en").unwrap(), ["tr-en"]);
    assert_eq!(ids("en", "de").unwrap(), ["en-de"]);
    assert_eq!(ids("tr", "tr"), Err(manifest::RouteError::Same));
    assert_eq!(ids("tr", "xx"), Err(manifest::RouteError::Unknown("xx".into())));
    assert_eq!(m.route("tr", "de").unwrap().iter().map(|d| d.key()).collect::<Vec<_>>(), ["tr-en@3.0", "en-de@3.0"]);
}

#[test]
fn the_readme_states_the_manifests_language_count() {
    let m = manifest::embedded().unwrap();
    let readme = std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../README.md")).unwrap();
    let n = m.languages;
    assert!(readme.contains(&format!("{n} dil")), "README (TR) names {n} languages");
    assert!(readme.contains(&format!("{n} languages")), "README (EN) names {n} languages");
    for other in (1..=120).filter(|k| *k != n) {
        assert!(!readme.contains(&format!(" {other} dil ")) && !readme.contains(&format!(" {other} languages")), "README names {other} languages");
    }
}

#[test]
fn names_sort_as_their_readers_expect() {
    use std::cmp::Ordering::Less;
    assert_eq!(languages::compare_names("Çekçe", "Danca", true), Less, "Ç before D");
    assert_eq!(languages::compare_names("Bulgarca", "Çekçe", true), Less);
    assert_eq!(languages::compare_names("Hırvatça", "Hintçe", true), Less, "ı before i");
    assert_eq!(languages::compare_names("İngilizce", "Japonca", true), Less);
    assert_eq!(languages::compare_names("Arabic", "bengali", false), Less);
    let mut names: Vec<&str> = LANGUAGES.iter().map(|l| l.turkish).collect();
    names.sort_by(|a, b| languages::compare_names(a, b, true));
    assert_eq!(names.first(), Some(&"Afrikaanca"));
    assert_eq!(names.last(), Some(&"Yunanca"));
    let _ = ENGLISH;
}

// ---------------------------------------------------------------------------------------------
// Installing: download, SHA-256, unpack, SHA-256, into place

/// A release in memory, recording every address asked for.
#[derive(Default)]
struct Release {
    files: HashMap<String, Vec<u8>>,
    asked: Mutex<Vec<String>>,
}

impl Transport for Release {
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
        self.asked.lock().unwrap().push(url.to_owned());
        let body = self.files.get(url).ok_or(FetchError::Status(404))?;
        if body.len() as u64 > limit {
            return Err(FetchError::TooLarge);
        }
        sink.write_all(body).map_err(|e| FetchError::Network(e.to_string()))?;
        Ok(body.len() as u64)
    }
}

/// "Unpacking" for the tests: the stored file is the unpacked one.
fn copy(input: &mut dyn Read, output: &mut dyn Write, limit: u64) -> Result<u64, String> {
    std::io::copy(&mut input.take(limit.saturating_add(1)), output).map_err(|e| e.to_string())
}

fn sha(bytes: &[u8]) -> String {
    store::hex(&Sha256::digest(bytes))
}

const BASE: &str = "https://github.com/ceceys/pdfrp/releases/download/models-v3.0/";

/// A two-file module whose release files are their own unpacked form (for [`copy`]).
fn module(from: &str, to: &str, version: &str) -> (Direction, Release) {
    let mut release = Release::default();
    let mut files = Vec::new();
    for (kind, name) in [("model", format!("model.{from}{to}.bin")), ("vocab", format!("vocab.{from}{to}.spm"))] {
        let body: Vec<u8> = format!("{kind} {from}-{to} {version}").into_bytes().repeat(1000);
        let asset = format!("{from}-{to}.{name}.zst");
        release.files.insert(format!("{BASE}{asset}"), body.clone());
        files.push(ModelFile {
            kind: kind.into(),
            name,
            asset,
            size: body.len() as u64,
            sha256: sha(&body),
            unpacked_size: body.len() as u64,
            unpacked_sha256: sha(&body),
        });
    }
    (Direction { from: from.into(), to: to.into(), version: version.into(), architecture: "tiny".into(), files }, release)
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("migez-translate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn install(store: &ModelStore, d: &Direction, release: &Release) -> Result<u64, InstallError> {
    let progress = AtomicU64::new(0);
    store.install(d, (release, BASE), copy, &progress, &AtomicBool::new(false)).map(|()| progress.load(Ordering::Relaxed))
}

#[test]
fn a_module_installs_only_after_both_checks_and_leaves_no_part_files() {
    let root = temp("install");
    let store = ModelStore::new(&root);
    let (d, release) = module("tr", "en", "3.0");
    assert!(!store.is_installed(&d));
    let downloaded = install(&store, &d, &release).unwrap();
    assert_eq!(downloaded, d.download_size(), "progress counts the downloaded bytes");
    assert!(store.is_installed(&d));
    let folder = store.folder(&d);
    assert_eq!(folder, root.join("tr-en").join("3.0"));
    let mut names: Vec<String> = std::fs::read_dir(&folder).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert_eq!(names, ["model.tren.bin", MARKER, "vocab.tren.spm"]);
    assert!(!root.join("tr-en").join("3.0.part").exists());
    assert_eq!(release.asked.lock().unwrap().len(), 2);
    // A file changed on disk afterwards no longer counts as installed.
    std::fs::write(folder.join("vocab.tren.spm"), b"short").unwrap();
    assert!(!store.is_installed(&d));
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_download_with_a_wrong_sha256_is_refused_and_nothing_is_installed() {
    let root = temp("mismatch");
    let store = ModelStore::new(&root);
    let (d, mut release) = module("tr", "en", "3.0");
    // The release file is altered: same size, other bytes.
    let url = format!("{BASE}{}", d.files[1].asset);
    let body = release.files.get_mut(&url).unwrap();
    body[0] ^= 1;
    assert_eq!(install(&store, &d, &release), Err(InstallError::Mismatch { file: d.files[1].asset.clone() }));
    assert!(!store.is_installed(&d));
    assert!(!root.join("tr-en").join("3.0").exists());
    assert!(!root.join("tr-en").join("3.0.part").exists(), "the .part folder is removed");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn an_unpacked_file_with_a_wrong_sha256_or_size_is_refused() {
    let root = temp("unpacked");
    let store = ModelStore::new(&root);
    let (mut d, release) = module("tr", "en", "3.0");
    d.files[0].unpacked_sha256 = sha(b"something else");
    assert_eq!(install(&store, &d, &release), Err(InstallError::Mismatch { file: d.files[0].name.clone() }));
    let (mut d, release) = module("tr", "en", "3.0");
    d.files[0].unpacked_size -= 1;
    assert_eq!(install(&store, &d, &release), Err(InstallError::Size { file: d.files[0].name.clone() }));
    let (mut d, release) = module("tr", "en", "3.0");
    d.files[0].size += 1;
    assert_eq!(install(&store, &d, &release), Err(InstallError::Size { file: d.files[0].asset.clone() }));
    assert!(!store.is_installed(&d));
    assert!(!root.join("tr-en").join("3.0.part").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_cancelled_or_failed_download_leaves_nothing() {
    let root = temp("cancel");
    let store = ModelStore::new(&root);
    let (d, release) = module("tr", "en", "3.0");
    let cancel = AtomicBool::new(true);
    assert_eq!(store.install(&d, (&release, BASE), copy, &AtomicU64::new(0), &cancel), Err(InstallError::Cancelled));
    let empty = Release::default();
    assert_eq!(install(&store, &d, &empty), Err(InstallError::Fetch(FetchError::Status(404))));
    assert!(!root.join("tr-en").join("3.0").exists());
    assert!(!root.join("tr-en").join("3.0.part").exists());
    // A release file that checks out but isn't zstd fails at unpacking, after the download and its
    // SHA-256 check, and leaves nothing installed.
    let failed = store.install(&d, (&release, BASE), store::unpack_zstd, &AtomicU64::new(0), &AtomicBool::new(false));
    assert!(matches!(failed, Err(InstallError::Unpack { .. })), "{failed:?}");
    assert!(!root.join("tr-en").join("3.0").exists());
    let _ = std::fs::remove_dir_all(root);
}

/// A small zstd frame built here, byte by byte (RFC 8878, section 3.1.1), so the test needs no
/// file from elsewhere: one segment with its content size (105) in the header, then "Hello" as a
/// raw block and a hundred "!" as the last, RLE block.
fn small_zstd() -> Vec<u8> {
    // A block header: three bytes, little-endian: the size, the type (0 raw, 1 RLE), last or not.
    let block = |size: u32, kind: u32, last: bool| {
        let b = ((size << 3) | (kind << 1) | u32::from(last)).to_le_bytes();
        [b[0], b[1], b[2]]
    };
    let mut frame = vec![0x28, 0xB5, 0x2F, 0xFD, 0x20, 105];
    frame.extend(block(5, 0, false));
    frame.extend(b"Hello");
    frame.extend(block(100, 1, true));
    frame.push(b'!');
    frame
}

/// zstd: a small frame unpacks to what it holds; a cap below its size stops it; a broken or cut
/// file is an error, not a panic. (The real modules unpack in the ignored tests below.)
#[test]
fn zstd_unpacks_within_its_limit_and_refuses_broken_files() {
    let frame = small_zstd();
    let mut out = Vec::new();
    let n = store::unpack_zstd(&mut &frame[..], &mut out, 1 << 20).unwrap();
    let mut expected = b"Hello".to_vec();
    expected.extend([b'!'; 100]);
    assert_eq!(out, expected);
    assert_eq!(n, 105);
    let mut capped = Vec::new();
    assert!(store::unpack_zstd(&mut &frame[..], &mut capped, n - 1).is_err(), "more than the limit");
    assert!(store::unpack_zstd(&mut &b"not zstd at all"[..], &mut Vec::new(), 1 << 20).is_err());
    let cut = &frame[..frame.len() / 2];
    assert!(store::unpack_zstd(&mut &cut[..], &mut Vec::new(), 1 << 20).is_err(), "cut short");
}

#[test]
fn a_new_version_replaces_the_old_and_remove_takes_every_version() {
    let root = temp("versions");
    let store = ModelStore::new(&root);
    let (old, release_old) = module("tr", "en", "3.0");
    let (new, release_new) = module("tr", "en", "3.1");
    install(&store, &old, &release_old).unwrap();
    std::fs::create_dir_all(root.join("tr-en").join("2.9.part")).unwrap();
    install(&store, &new, &release_new).unwrap();
    assert!(store.is_installed(&new));
    assert!(!root.join("tr-en").join("3.0").exists(), "the older version is removed");
    assert!(!root.join("tr-en").join("2.9.part").exists(), "and a left-over part folder");
    // Reinstalling the same version over itself works too.
    install(&store, &new, &release_new).unwrap();
    assert!(store.is_installed(&new));
    store.remove(&new).unwrap();
    assert!(!root.join("tr-en").exists());
    store.remove(&new).unwrap();
    let _ = std::fs::remove_dir_all(root);
}

/// Legal review (27.11.2026), condition (b): a module download asks only for addresses, through
/// the transport the update check uses; the transport is given nothing but the address (and a
/// size limit), so the request can carry nothing of ours but what the transport adds itself — the
/// User-Agent (`migez_update`'s header test records exactly what that is).
#[test]
fn module_downloads_ask_the_update_checks_client_for_release_addresses_only() {
    let root = temp("addresses");
    let store = ModelStore::new(&root);
    let m = manifest::embedded().unwrap();
    let (d, release) = module("tr", "en", "3.0");
    install(&store, &d, &release).unwrap();
    for url in release.asked.lock().unwrap().iter() {
        migez_update::allowed(url).unwrap();
        assert!(url.starts_with(&m.base_url), "{url}");
    }
    // Source::GitHub hands out migez_update's HTTPS client and the manifest's address.
    if cfg!(windows) {
        let (_transport, base) = Source::GitHub.open(m, &migez_update::user_agent("1.2.0")).unwrap();
        assert_eq!(base, m.base_url);
    }
    let _ = std::fs::remove_dir_all(root);
}

// ---------------------------------------------------------------------------------------------
// Text

#[test]
fn lines_of_a_sentence_are_joined_and_paragraphs_kept() {
    // The sentence that came out in three pieces in the trial (27.11, sentence 15).
    let three = "Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve\ntaraflardan biri yazılı olarak feshedinceye kadar\ngeçerliliğini korur.";
    assert_eq!(
        text::prepare(three),
        "Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve taraflardan biri yazılı olarak feshedinceye kadar geçerliliğini korur."
    );
    assert_eq!(text::prepare("Birinci paragraf\nsürer.\n\nİkinci paragraf.\r\n"), "Birinci paragraf sürer.\n\nİkinci paragraf.");
    assert_eq!(text::prepare("  \n\n"), "");
}

#[test]
fn words_divided_at_a_line_end_are_mended() {
    assert_eq!(text::prepare("Bu sözleş-\nmenin hükümleri"), "Bu sözleşmenin hükümleri");
    assert_eq!(text::prepare("Ankara-\nİstanbul hattı"), "Ankara-İstanbul hattı", "a hyphen before a capital stays");
    assert_eq!(text::prepare("sözleş\u{AD}\nmesi"), "sözleşmesi", "a soft hyphen goes");
    assert_eq!(text::prepare("12-\n15 Ocak"), "12-15 Ocak");
    assert_eq!(text::prepare("会议将于\n星期三举行。"), "会议将于星期三举行。", "no space inside Chinese");
}

#[test]
fn a_short_line_that_ends_a_sentence_and_a_list_item_start_paragraphs() {
    let letter = "The first paragraph is a long line of text that runs to the edge.\nIt ends here.\nThe second paragraph starts on this line and runs on to\nthe next one.";
    assert_eq!(
        text::prepare(letter),
        "The first paragraph is a long line of text that runs to the edge. It ends here.\n\nThe second paragraph starts on this line and runs on to the next one."
    );
    assert_eq!(text::prepare("Şartlar:\n1. Birinci madde\n2. İkinci madde\nsürer."), "Şartlar:\n\n1. Birinci madde\n\n2. İkinci madde sürer.");
    assert_eq!(text::prepare("• bir\n• iki"), "• bir\n\n• iki");
    assert_eq!(text::prepare("a) bir\n(b) iki\nve sonu"), "a) bir\n\n(b) iki ve sonu");
    assert_eq!(text::prepare("2026 yılında\nbaşladı"), "2026 yılında başladı", "a number isn't a list item");
}

fn line(text: &str, left: f32, right: f32, top: f32) -> Line {
    Line { text: text.into(), left, right, top, bottom: top + 10.0 }
}

#[test]
fn the_selections_geometry_finds_paragraph_gaps() {
    let lines = [
        line("Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve", 50.0, 500.0, 100.0),
        line("taraflardan biri yazılı olarak feshedinceye kadar", 50.0, 490.0, 112.0),
        line("geçerliliğini korur.", 50.0, 200.0, 124.0),
        line("Taraflar bu sözleşmeyi okuyup", 50.0, 480.0, 136.0),
        line("imzalamıştır.", 50.0, 150.0, 148.0),
        // A wider gap: a new paragraph, though the line above doesn't end a sentence.
        line("Ek-1 belge listesi", 50.0, 300.0, 175.0),
    ];
    assert_eq!(
        text::from_lines(&lines),
        "Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve taraflardan biri yazılı olarak feshedinceye kadar geçerliliğini korur.\n\nTaraflar bu sözleşmeyi okuyup imzalamıştır.\n\nEk-1 belge listesi"
    );
    // A line that ends a sentence but reaches the right edge is not a paragraph end.
    let full = [line("Birinci cümle burada biter.", 50.0, 500.0, 100.0), line("İkinci cümle sürer.", 50.0, 300.0, 112.0)];
    assert_eq!(text::from_lines(&full), "Birinci cümle burada biter. İkinci cümle sürer.");
    // The next line above this one: another column.
    let columns = [line("Sol sütunun sonu", 50.0, 250.0, 700.0), line("sağ sütunun başı", 300.0, 500.0, 100.0)];
    assert_eq!(text::from_lines(&columns), "Sol sütunun sonu\n\nsağ sütunun başı");
    assert_eq!(text::from_lines(&[]), "");
}

#[test]
fn the_processor_decides_which_helper_build_starts() {
    use crate::helper::{AVX2_DIR, EXE, PORTABLE_DIR, helper_exe};
    let dir = temp("helper-exe");
    let at = |[a, b]: [&str; 2]| dir.join(a).join(b).join(EXE);
    assert_eq!(helper_exe(&dir, true), None, "neither installed");
    for build in [AVX2_DIR, PORTABLE_DIR] {
        std::fs::create_dir_all(at(build).parent().unwrap()).unwrap();
        std::fs::write(at(build), b"").unwrap();
    }
    assert_eq!(helper_exe(&dir, true), Some(at(AVX2_DIR)), "AVX2 where the processor has it");
    assert_eq!(helper_exe(&dir, false), Some(at(PORTABLE_DIR)), "portable where it hasn't");
    std::fs::remove_file(at(PORTABLE_DIR)).unwrap();
    assert_eq!(helper_exe(&dir, false), None, "never the AVX2 build without AVX2");
    std::fs::write(at(PORTABLE_DIR), b"").unwrap();
    std::fs::remove_file(at(AVX2_DIR)).unwrap();
    assert_eq!(helper_exe(&dir, true), Some(at(PORTABLE_DIR)), "portable when only it is there");
    let _ = std::fs::remove_dir_all(dir);
}

// ---------------------------------------------------------------------------------------------
// End to end with real models: download from a local release, install, translate through English

/// The helper build to run, from `PDFRP_CEVIRI_EXE`.
fn helper_from_env() -> Option<PathBuf> {
    std::env::var_os("PDFRP_CEVIRI_EXE").map(PathBuf::from).filter(|p| p.is_file())
}

/// Turkish to German with Mozilla's real model files (`MIGEZ_TEST_MODELS_V3`: the folder they
/// were pulled into, `<from>-<to>\<Mozilla's .zst name>`), downloaded from a local stand-in of the
/// release (the same addresses, read from disk), unpacked with zstd, checked and installed, then
/// translated by the helper. `cargo test -p migez-translate -- --ignored --nocapture pivot`.
#[test]
#[ignore = "needs MIGEZ_TEST_MODELS_V3 and PDFRP_CEVIRI_EXE"]
fn pivot_turkish_to_german_from_a_local_release() {
    let (Some(models), Some(exe)) = (std::env::var_os("MIGEZ_TEST_MODELS_V3").map(PathBuf::from), helper_from_env()) else {
        println!("SKIP: MIGEZ_TEST_MODELS_V3 or PDFRP_CEVIRI_EXE is not set");
        return;
    };
    let m = manifest::embedded().unwrap();
    let route = m.route("tr", "de").unwrap();
    /// Answers the release's addresses from the pulled files.
    struct Local<'a>(&'a Path, &'a manifest::Manifest);
    impl Transport for Local<'_> {
        fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
            migez_update::allowed(url).map_err(FetchError::Refused)?;
            let f = self.1.directions.iter().flat_map(|d| d.files.iter().map(move |f| (d, f))).find(|(_, f)| self.1.url(f) == url);
            let (d, f) = f.ok_or(FetchError::Status(404))?;
            let pulled = f.asset.strip_prefix(&format!("{}.", d.id())).ok_or(FetchError::Status(404))?;
            let path = self.0.join(d.id()).join(pulled);
            let file = std::fs::File::open(path).map_err(|_| FetchError::Status(404))?;
            std::io::copy(&mut file.take(limit + 1), sink).map_err(|e| FetchError::Network(e.to_string()))
        }
    }
    let root = temp("pivot");
    let store = ModelStore::new(&root);
    let release = Local(&models, m);
    for d in &route {
        store.install(d, (&release, &m.base_url), store::unpack_zstd, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap();
        assert!(store.is_installed(d));
    }
    let mut helper = crate::Helper::start(&exe, store.root()).unwrap();
    let german = helper.translate(&route, &text::prepare("Toplantı saat kaçta\nbaşlıyor?")).unwrap();
    println!("tr → de: {german}");
    assert!(german.contains("Meeting") || german.contains("Sitzung") || german.contains("Besprechung"), "{german}");
    helper.stop();
    let _ = std::fs::remove_dir_all(root);
}

/// The same chain without zstd: the release stand-in serves the files already unpacked
/// (`PDFRP_CEVIRI_MODELLER`, `<from>-<to>\<version>\<name>`), and the manifest entries are made to
/// match them, so download, both SHA-256 checks, the install and the helper's translation through
/// English all run on real model files.
#[test]
#[ignore = "needs PDFRP_CEVIRI_MODELLER and PDFRP_CEVIRI_EXE"]
fn pivot_turkish_to_german_through_install_and_helper_with_unpacked_files() {
    let (Some(unpacked), Some(exe)) = (std::env::var_os("PDFRP_CEVIRI_MODELLER").map(PathBuf::from), helper_from_env()) else {
        println!("SKIP: PDFRP_CEVIRI_MODELLER or PDFRP_CEVIRI_EXE is not set");
        return;
    };
    let m = manifest::embedded().unwrap();
    let mut release = Release::default();
    let mut route: Vec<Direction> = m.route("tr", "de").unwrap().into_iter().cloned().collect();
    for d in &mut route {
        let folder = unpacked.join(d.id()).join(&d.version);
        for f in &mut d.files {
            let body = std::fs::read(folder.join(&f.name)).unwrap();
            assert_eq!(sha(&body), f.unpacked_sha256, "the unpacked file is Mozilla's");
            f.size = f.unpacked_size;
            f.sha256 = f.unpacked_sha256.clone();
            release.files.insert(m.url(f), body);
        }
    }
    let root = temp("pivot-unpacked");
    let store = ModelStore::new(&root);
    for d in &route {
        install(&store, d, &release).unwrap();
        assert!(store.is_installed(d));
    }
    let mut helper = crate::Helper::start(&exe, store.root()).unwrap();
    let source = "Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve\ntaraflardan biri yazılı olarak feshedinceye kadar\ngeçerliliğini korur.";
    let chain: Vec<&Direction> = route.iter().collect();
    let german = helper.translate(&chain, &text::prepare(source)).unwrap();
    println!("tr → de: {german}");
    assert!(!german.contains('\n'), "one sentence, not three: {german}");
    assert!(german.contains("Vertrag") || german.contains("Vereinbarung"), "{german}");
    let status = helper.status().unwrap();
    println!("helper: {status}");
    assert_eq!(status["loaded"].as_array().map(Vec::len), Some(2));
    helper.stop();
    // Stopped, the helper holds no model file: the modules can be removed.
    for d in &route {
        store.remove(d).unwrap();
    }
    let _ = std::fs::remove_dir_all(root);
}
