use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, Mutex};

use migez_translate::{Direction, ModelStore, manifest};
use migez_update::{FetchError, Transport};

use super::{TranslateSetup, wording};
use crate::{Dialog, MigezApp, RightPanel};

const BASE: &str = "https://github.com/ceceys/pdfrp/releases/download/models-v3.0/";

/// A release that records every address asked for and answers from `files`.
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

fn copy(input: &mut dyn Read, output: &mut dyn Write, limit: u64) -> Result<u64, String> {
    std::io::copy(&mut input.take(limit.saturating_add(1)), output).map_err(|e| e.to_string())
}

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("migez-ui-translate-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// An app with a document open, translation set up with `release` and `exe`, work inline.
fn app(name: &str, release: Arc<Release>, exe: Option<PathBuf>) -> (MigezApp, PathBuf) {
    let root = temp(name);
    let mut app = MigezApp::new();
    app.set_option("language", "en").unwrap();
    app.run_inline = true;
    let pdf = migez_engine::Session::new().create_from_text("t", "Sample text.").unwrap().to_vec();
    app.open_bytes("doc.pdf", None, pdf).unwrap();
    app.set_translate_setup(TranslateSetup::Fake { exe, store: ModelStore::new(&root), transport: release, base: BASE.into(), unpack: copy });
    (app, root)
}

fn direction(id: &str) -> &'static Direction {
    manifest::embedded().unwrap().directions.iter().find(|d| d.id() == id).unwrap()
}

#[test]
fn nothing_downloads_before_the_question_is_answered_and_it_names_the_size() {
    let release = Arc::new(Release::default());
    let (mut app, root) = app("question", release.clone(), None);
    app.set_option("translate_from", "tr").unwrap();
    app.set_option("translate_to", "de").unwrap();
    app.translation.text = "Merhaba.".into();
    assert!(app.execute("edit.translate"));
    assert_eq!(app.right, Some(RightPanel::Translate));
    assert_eq!(app.dialog, Some(Dialog::TranslateDownload));
    let question = app.translation_download_question().cloned().unwrap();
    assert_eq!(question.ids, ["tr-en", "en-de"], "both modules of Turkish to German through English");
    assert!(release.asked.lock().unwrap().is_empty(), "nothing asked of the network before Download");
    // The size the question names: both modules' downloads.
    let size = direction("tr-en").download_size() + direction("en-de").download_size();
    assert_eq!(super::megabytes(size), format!("{} MB", (size as f64 / 1_048_576.0).round()));
    // Cancel: nothing downloaded, no translation.
    app.answer_translate_download(false);
    assert!(app.translation_download_question().is_none());
    assert!(release.asked.lock().unwrap().is_empty());
    assert!(!root.join("tr-en").exists());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_download_whose_sha256_differs_is_refused_and_nothing_is_installed() {
    let mut release = Release::default();
    // tr → en needs one module; its files on the "release" are not Mozilla's: the right size,
    // other bytes.
    for f in &direction("tr-en").files {
        release.files.insert(format!("{BASE}{}", f.asset), vec![0u8; usize::try_from(f.size).unwrap()]);
    }
    let release = Arc::new(release);
    let (mut app, root) = app("mismatch", release.clone(), None);
    app.set_translation_languages(Some("tr"), "en");
    app.translation.text = "Merhaba.".into();
    app.start_translation();
    assert_eq!(app.translation_download_question().map(|q| q.ids.clone()), Some(vec!["tr-en".to_owned()]));
    app.answer_translate_download(true);
    app.poll_translate();
    let asked = release.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "stops at the first file: {asked:?}");
    for url in &asked {
        migez_update::allowed(url).unwrap();
    }
    let message = match app.translation_result() {
        Some(Err(e)) => e.clone(),
        other => panic!("{other:?}"),
    };
    assert!(message.contains("SHA-256"), "{message}");
    assert!(!root.join("tr-en").join("3.0").exists(), "nothing installed");
    assert!(!app.translation_busy());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn the_same_language_twice_and_no_text_are_said_not_downloaded() {
    let release = Arc::new(Release::default());
    let (mut app, root) = app("same", release.clone(), None);
    app.set_translation_languages(Some("tr"), "tr");
    app.translation.text = "Merhaba.".into();
    app.start_translation();
    assert_eq!(app.translation_result(), Some(&Err("The two languages are the same.".to_owned())));
    app.set_translation_languages(None, "tr");
    app.start_translation();
    assert_eq!(app.translation_result(), Some(&Err("Choose the language of the text.".to_owned())));
    app.set_translation_languages(Some("de"), "tr");
    app.translation.text = "   ".into();
    app.start_translation();
    assert!(matches!(app.translation_result(), Some(Err(e)) if e.contains("Select text")));
    assert!(app.dialog.is_none() && release.asked.lock().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_copy_set_up_without_translation_says_so() {
    let mut app = MigezApp::new();
    app.set_option("language", "en").unwrap();
    app.set_translation_languages(Some("tr"), "en");
    app.translation.text = "Merhaba.".into();
    app.start_translation();
    assert_eq!(app.translation_result(), Some(&Err("This copy of Pdfrp can't translate.".to_owned())));
}

#[test]
fn the_languages_are_kept_in_the_settings_and_odd_values_refused() {
    let mut app = MigezApp::new();
    app.set_option("translate_from", "zh-Hans").unwrap();
    app.set_option("translate_to", "tr").unwrap();
    assert!(app.set_option("translate_to", "xx").is_err());
    assert!(app.set_option("translate_from", "../tr").is_err());
    let saved = app.persist();
    let mut again = MigezApp::new();
    again.restore(&saved);
    assert_eq!(again.translation_languages(), (Some("zh-Hans"), "tr"));
    let mut odd = MigezApp::new();
    let before = odd.translation_languages().1.to_owned();
    odd.restore(r#"{"translation":{"from":"klingon","to":5}}"#);
    assert_eq!(odd.translation_languages(), (None, before.as_str()));
}

#[test]
fn removing_a_language_stops_the_helper_and_takes_both_modules() {
    let release = Arc::new(Release::default());
    let (mut app, root) = app("remove", release, None);
    for d in manifest::embedded().unwrap().modules_of("tr") {
        std::fs::create_dir_all(root.join(d.id()).join(&d.version)).unwrap();
    }
    app.remove_language("tr");
    app.poll_translate();
    assert!(!root.join("tr-en").exists() && !root.join("en-tr").exists());
    assert!(!app.translation_busy());
    assert!(app.translation.notice.is_none());
    let _ = std::fs::remove_dir_all(root);
}

/// Legal review (26.11.2026), condition (b): module downloads carry the update check's
/// User-Agent, through the same client constructor (`migez_update::github_transport`, via
/// `migez_translate::Source`).
#[test]
fn module_downloads_use_the_update_checks_user_agent() {
    assert_eq!(crate::updates::user_agent(), format!("Pdfrp/{}", migez_engine::build_info::PRODUCT_VERSION));
    assert_eq!(crate::updates::user_agent(), migez_update::user_agent(migez_engine::build_info::PRODUCT_VERSION));
}

/// The Turkish the interface shows is the legal review's, word for word (27.11.2026), and the
/// English is the code's.
#[test]
fn the_wordings_are_the_legal_reviews_in_turkish_and_english() {
    use crate::i18n::{Lang, tr};
    let Some(turkish) = Lang::from_code("tr") else {
        println!("SKIP: no Turkish catalog embedded (MIGEZ_I18N_DIR)");
        return;
    };
    let tr = |s: &str| tr(turkish, s).to_owned();
    assert_eq!(tr(wording::TITLE_ONE), "Dil modülü indirilsin mi?");
    assert_eq!(tr(wording::TITLE_TWO), "Dil modülleri indirilsin mi?");
    assert_eq!(
        tr(wording::BODY_ONE),
        "{diller} dil modülü ({boyut}) Pdfrp'nin GitHub sayfasından indirilecek; modül, Mozilla'nın açık lisanslı (MPL-2.0) çeviri modelini içerir ve çeviri tamamen bilgisayarınızda yapılır."
    );
    assert_eq!(
        tr(wording::BODY_TWO),
        "{diller} çevirisi İngilizce üzerinden yapıldığı için iki dil modülü ({boyut}) Pdfrp'nin GitHub sayfasından indirilecek; modüller, Mozilla'nın açık lisanslı (MPL-2.0) çeviri modellerini içerir ve çeviri tamamen bilgisayarınızda yapılır."
    );
    assert_eq!(
        tr(wording::BODY_LANGUAGE),
        "{dil} için iki dil modülü ({boyut}) Pdfrp'nin GitHub sayfasından indirilecek: {dil} → İngilizce ve İngilizce → {dil}. Modüller, Mozilla'nın açık lisanslı (MPL-2.0) çeviri modellerini içerir ve çeviri tamamen bilgisayarınızda yapılır."
    );
    assert_eq!(tr(wording::NOT_AFFILIATED), "Pdfrp'nin Mozilla ya da Mozilla ürünleriyle bir bağlantısı yoktur.");
    assert_eq!(tr(wording::MACHINE_NOTE), "Makine çevirisidir, hata içerebilir; hukuki ve resmî işlerde belgenin aslını esas alın.");
    assert_eq!(tr(wording::FXTRANSLATE_LINE), "Mozilla Public License 2.0 · Pdfrp için değiştirildi · Kaynak kodu: {source}");
    assert_eq!(
        tr(wording::MODULES_LINE),
        "Mozilla'nın Firefox Translations çeviri modelleri, değiştirilmeden · Mozilla Public License 2.0 · Kaynak: github.com/mozilla/translations · Pdfrp'nin Mozilla ya da Mozilla ürünleriyle bir bağlantısı yoktur."
    );
    assert_eq!(
        tr(wording::NOTICE),
        "pdfrp-ceviri.exe, fxtranslate 0.5.0'ın Pdfrp için değiştirilmiş bir sürümünü içerir (Mozilla Public License 2.0). Değişiklik dahil kaynak kodu: {source}. Dil modülleri Mozilla'nın Firefox Translations modelleridir (MPL-2.0) ve değiştirilmeden dağıtılır. Kaynak: https://github.com/mozilla/translations (önceki model deposu: https://github.com/mozilla/firefox-translations-models). Pdfrp'nin Mozilla ya da Mozilla ürünleriyle bir bağlantısı yoktur."
    );
    assert_eq!(crate::i18n::tr_ctx(turkish, "module", "Cancel"), "Vazgeç");
    assert_eq!(tr("Download"), "İndir");
}

/// Every wording that names Mozilla says, in the same place, that Pdfrp isn't associated with it,
/// in the words of Mozilla's trademark policy: the download question has its own line under the
/// body, the modules' licence line and NOTICE carry it themselves; the panel's note doesn't name
/// Mozilla. NOTICE (the file) says the same, with this version's source address.
#[test]
fn wordings_that_name_mozilla_say_pdfrp_is_not_associated() {
    const STATEMENT: &str = "Pdfrp is not officially associated with Mozilla or its products.";
    assert_eq!(wording::NOT_AFFILIATED, STATEMENT);
    for s in [wording::MODULES_LINE, wording::NOTICE] {
        assert!(s.contains("Mozilla") && s.ends_with(STATEMENT), "{s}");
    }
    assert!(wording::BODY_ONE.contains("Mozilla") && wording::BODY_TWO.contains("Mozilla") && wording::BODY_LANGUAGE.contains("Mozilla"));
    assert!(!wording::MACHINE_NOTE.contains("Mozilla") && !wording::MACHINE_NOTE.contains("Firefox"));
    let source = super::fxtranslate_source();
    assert!(source.ends_with(&format!("/tree/v{}/vendor/fxtranslate", migez_engine::build_info::PRODUCT_VERSION)));
    let notice = include_str!("../../../../../NOTICE").replace("\r\n", "\n");
    let paragraph = crate::i18n::fmt(wording::NOTICE, &[("source", &source)]);
    assert!(notice.contains(&paragraph), "NOTICE carries the About box's paragraph, with v{}'s source", migez_engine::build_info::PRODUCT_VERSION);
}

/// Preferences ▸ Language modules ▸ Download asks the translation's question before anything is
/// downloaded: both modules of a language missing, the language's own body (C) naming it and
/// both directions; one missing, the one-module body (A) naming its direction. The size is the
/// row's. Cancel goes back to Preferences and downloads nothing; Download starts the download and
/// no translation.
#[test]
fn preferences_download_asks_first_with_the_languages_own_question() {
    let release = Arc::new(Release::default());
    let (mut app, root) = app("preferences", release.clone(), None);
    app.dialog = Some(Dialog::Preferences);
    app.download_language("de");
    assert_eq!(app.dialog, Some(Dialog::TranslateDownload));
    let both = app.translation_download_question().cloned().unwrap();
    assert_eq!(both.ids, ["de-en", "en-de"]);
    assert_eq!(both.language.as_deref(), Some("de"));
    assert!(release.asked.lock().unwrap().is_empty(), "nothing asked of the network before Download");
    let size = super::megabytes(direction("de-en").download_size() + direction("en-de").download_size());
    let (title, body) = super::question(&both).unwrap();
    assert_eq!(title, wording::TITLE_TWO);
    assert_eq!(
        body,
        format!(
            "Two German language modules ({size}) will be downloaded from Pdfrp's GitHub page: German → English and English → German. They contain Mozilla's openly licensed (MPL-2.0) translation models, and translation runs entirely on your computer."
        )
    );
    // Cancel: back to Preferences, nothing downloaded.
    app.answer_translate_download(false);
    assert_eq!(app.dialog, Some(Dialog::Preferences));
    assert!(app.translation_download_question().is_none() && release.asked.lock().unwrap().is_empty());
    // Partly installed (German → English is): only English → German, with the one-module body.
    app.translation.installed.insert("de-en".into(), true);
    app.download_language("de");
    let one = app.translation_download_question().cloned().unwrap();
    assert_eq!(one.ids, ["en-de"]);
    let (title, body) = super::question(&one).unwrap();
    assert_eq!(title, wording::TITLE_ONE);
    let size = super::megabytes(direction("en-de").download_size());
    assert!(body.starts_with(&format!("The English → German language module ({size}) will be downloaded")), "{body}");
    // Download: the download starts (this release has no files, so it fails and says so in
    // Preferences); nothing is translated, and the question goes back to Preferences.
    app.answer_translate_download(true);
    app.poll_translate();
    assert_eq!(app.dialog, Some(Dialog::Preferences));
    let asked = release.asked.lock().unwrap().clone();
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(asked[0].starts_with(&format!("{BASE}en-de.")), "{asked:?}");
    assert!(app.translation.notice.is_some());
    assert!(app.translation_result().is_none(), "no translation");
    // A translation's own question keeps its wordings: one module (A), a pair through English (B).
    let pair = super::Confirm { ids: vec!["tr-en".into(), "en-de".into()], from: "tr".into(), to: "de".into(), language: None };
    let (title, body) = super::question(&pair).unwrap();
    assert_eq!(title, wording::TITLE_TWO);
    assert!(body.starts_with("Because Turkish → German is translated via English, two language modules ("), "{body}");
    let single = super::Confirm { ids: vec!["tr-en".into()], from: "tr".into(), to: "en".into(), language: None };
    assert_eq!(super::question(&single).unwrap().0, wording::TITLE_ONE);
    let _ = std::fs::remove_dir_all(root);
}

/// Turkish to German through English with real models (`PDFRP_CEVIRI_MODELLER`, unpacked as
/// `<from>-<to>\<version>\<name>`) and the real helper (`PDFRP_CEVIRI_EXE`): installed through the
/// store from a local stand-in of the release, then translated from the panel's text, the PDF's
/// three lines of one sentence joined first.
#[test]
#[ignore = "needs PDFRP_CEVIRI_MODELLER and PDFRP_CEVIRI_EXE"]
fn turkish_to_german_from_the_panel_with_real_models() {
    let (Some(unpacked), Some(exe)) =
        (std::env::var_os("PDFRP_CEVIRI_MODELLER").map(PathBuf::from), std::env::var_os("PDFRP_CEVIRI_EXE").map(PathBuf::from))
    else {
        println!("SKIP: PDFRP_CEVIRI_MODELLER or PDFRP_CEVIRI_EXE is not set");
        return;
    };
    let m = manifest::embedded().unwrap();
    // The stand-in serves the unpacked files; each module is installed with its download size
    // and hash made the unpacked ones (no zstd here), which leaves the installed module exactly
    // what the real one is.
    let mut release = Release::default();
    let mut route: Vec<Direction> = m.route("tr", "de").unwrap().into_iter().cloned().collect();
    for d in &mut route {
        let folder = unpacked.join(d.id()).join(&d.version);
        for f in &mut d.files {
            let body = std::fs::read(folder.join(&f.name)).unwrap();
            f.size = f.unpacked_size;
            f.sha256 = f.unpacked_sha256.clone();
            release.files.insert(m.url(f), body);
        }
    }
    let release = Arc::new(release);
    let (mut app, root) = app("real", release.clone(), Some(exe));
    let store = ModelStore::new(&root);
    for d in &route {
        store.install(d, (release.as_ref(), BASE), copy, &AtomicU64::new(0), &AtomicBool::new(false)).unwrap();
    }
    app.set_translation_languages(Some("tr"), "de");
    app.translation.text = migez_translate::text::prepare(
        "Bu sözleşme, imzalandığı tarihte yürürlüğe girer ve\ntaraflardan biri yazılı olarak feshedinceye kadar\ngeçerliliğini korur.",
    );
    app.start_translation();
    assert!(app.dialog.is_none(), "both modules are installed: no question");
    app.poll_translate();
    let german = match app.translation_result() {
        Some(Ok(t)) => t.clone(),
        other => panic!("{other:?}"),
    };
    println!("tr → de: {german}");
    assert!(!german.contains('\n') && (german.contains("Vertrag") || german.contains("Vereinbarung")), "{german}");
    // Removing Turkish stops the helper first, so its mapped model files can go.
    app.remove_language("tr");
    app.poll_translate();
    assert!(app.translation.notice.is_none(), "{:?}", app.translation.notice);
    assert!(!root.join("tr-en").exists());
    app.translate_exiting();
    let _ = std::fs::remove_dir_all(root);
}
