//! The update check without the network: versions, addresses, the release JSON, SHA256SUMS.txt,
//! the download and its SHA-256, and the test hook `MIGEZ_UPDATE_URL`.

use std::collections::HashMap;
use std::io::Write;
use std::sync::atomic::AtomicU64;

use crate::*;

/// Answers from a table; an address it doesn't hold is HTTP 404.
struct Fake(HashMap<String, Result<Vec<u8>, FetchError>>);

impl Fake {
    fn new() -> Fake {
        Fake(HashMap::new())
    }
    fn with(mut self, url: &str, answer: Result<Vec<u8>, FetchError>) -> Fake {
        self.0.insert(url.to_owned(), answer);
        self
    }
}

impl Transport for Fake {
    fn get(&self, url: &str, limit: u64, sink: &mut dyn Write) -> Result<u64, FetchError> {
        match self.0.get(url) {
            Some(Ok(body)) if body.len() as u64 > limit => Err(FetchError::TooLarge),
            Some(Ok(body)) => {
                sink.write_all(body).map_err(|e| FetchError::Network(e.to_string()))?;
                Ok(body.len() as u64)
            }
            Some(Err(e)) => Err(e.clone()),
            None => Err(FetchError::Status(404)),
        }
    }
}

const API: &str = "https://api.github.com/repos/ceceys/pdfrp/releases/latest";
const EXE_URL: &str = "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe";
const SUMS_URL: &str = "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/SHA256SUMS.txt";

/// GitHub's answer for a release `tag` with these (name, url, size) assets.
fn release_json(tag: &str, assets: &[(&str, &str, u64)]) -> Vec<u8> {
    let assets: Vec<serde_json::Value> =
        assets.iter().map(|(name, url, size)| serde_json::json!({ "name": name, "browser_download_url": url, "size": size, "content_type": "application/octet-stream" })).collect();
    serde_json::json!({
        "tag_name": tag,
        "name": format!("Pdfrp {}", tag.trim_start_matches('v')),
        "html_url": format!("https://github.com/ceceys/pdfrp/releases/tag/{tag}"),
        "draft": false,
        "prerelease": false,
        "assets": assets,
    })
    .to_string()
    .into_bytes()
}

fn v(text: &str) -> Version {
    Version::parse(text).unwrap()
}

const INSTALLER: &[u8] = b"MZ pretend installer 1.2.0";

fn sums_for(bytes: &[u8], name: &str) -> Vec<u8> {
    format!("{}  {name}\n{}  other.zip\n", sha256_hex(bytes).unwrap(), "0".repeat(64)).into_bytes()
}

/// A release 1.2.0 with the installer (as GitHub names it on upload, with dots) and its sums.
fn github_with(installer: &[u8], sums: Vec<u8>) -> Fake {
    let json =
        release_json("v1.2.0", &[("Pdfrp.Kurulum.1.2.0.exe", EXE_URL, installer.len() as u64), ("SHA256SUMS.txt", SUMS_URL, sums.len() as u64)]);
    Fake::new().with(API, Ok(json)).with(EXE_URL, Ok(installer.to_vec())).with(SUMS_URL, Ok(sums))
}

fn temp_dir(test: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("migez-update-test-{test}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn versions_read_with_and_without_v_and_order_as_semver_says() {
    assert_eq!(v("v1.2.3"), Version::new(1, 2, 3));
    assert_eq!(v("1.2.3"), Version::new(1, 2, 3));
    assert_eq!(v(" V1.2.3 ").to_string(), "1.2.3");
    assert_eq!(v("1.2.3+build.7"), Version::new(1, 2, 3), "build metadata is ignored");
    assert_eq!(v("1.2.3-rc.1").to_string(), "1.2.3-rc.1");
    assert!(v("1.1.0") < v("1.2.0"));
    assert!(v("1.1.0") < v("v1.1.1"));
    assert!(v("1.9.0") < v("1.10.0"), "numbers, not text");
    assert!(v("1.99.99") < v("2.0.0"));
    assert!(v("1.2.0-rc.1") < v("1.2.0"), "a pre-release comes before its release");
    assert!(v("1.2.0-alpha") < v("1.2.0-alpha.1"));
    assert!(v("1.2.0-alpha.1") < v("1.2.0-alpha.beta"));
    assert!(v("1.2.0-beta.2") < v("1.2.0-beta.11"));
    assert!(v("1.2.0-beta.11") < v("1.2.0-rc.1"));
    assert_eq!(v("1.1.0").cmp(&v("v1.1.0")), std::cmp::Ordering::Equal);
    assert!(v("1.2.0-rc.1").is_pre_release() && !v("1.2.0").is_pre_release());
    for bad in
        ["", "v", "1", "1.2", "1.2.3.4", "01.2.3", "1.02.3", "1.2.3-", "1.2.3-01", "1.2.3-a..b", "1.2.x", "1.2.3+", "a1.2.3", "1.2.3 beta", "١.٢.٣"]
    {
        assert_eq!(Version::parse(bad), None, "{bad:?}");
    }
    assert_eq!(Version::parse(&format!("1.2.3-{}", "a".repeat(80))), None, "too long");
}

/// The running version is Pdfrp's, and it reads as a release.
#[test]
fn the_workspace_version_reads_as_a_release() {
    let current = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    assert!(!current.is_pre_release());
    assert_eq!(user_agent(env!("CARGO_PKG_VERSION")), format!("Pdfrp/{current}"));
}

#[test]
fn only_https_to_githubs_hosts_is_allowed() {
    for ok in [
        API,
        EXE_URL,
        SUMS_URL,
        "https://objects.githubusercontent.com/github-production-release-asset/1?x=y",
        "https://release-assets.githubusercontent.com/github-production-release-asset/1",
        "HTTPS://API.GitHub.com/repos",
        "https://GitHub.com/Ceceys/Pdfrp/releases/download/v1.2.0/SHA256SUMS.txt?x=y#z",
    ] {
        assert!(allowed(ok).is_ok(), "{ok}");
    }
    for no in [
        "http://api.github.com/repos/ceceys/pdfrp/releases/latest",
        "https://example.com/",
        "https://api.github.com.example.com/",
        "https://evil-github.com/",
        "https://github.com.evil/",
        "https://user@github.com/",
        "https://github.com:8443/",
        "https://github.com:443/",
        "https://evil.com\\@github.com/",
        "https://raw.githubusercontent.com/x",
        "https://gist.github.com/x",
        "ftp://github.com/",
        "file:///C:/x.json",
        "",
        "https://",
    ] {
        assert!(allowed(no).is_err(), "{no}");
    }
    // The check itself refuses an address outside them before any request.
}

/// On github.com only this repository's release downloads: no other repository, no other page of
/// this one, and nothing a URL parser could lead out of the download folder.
#[test]
fn on_github_com_only_this_repositorys_release_downloads_are_allowed() {
    for no in [
        "https://github.com",
        "https://github.com/",
        "https://github.com/ceceys",
        "https://github.com/ceceys/pdfrp",
        "https://github.com/ceceys/pdfrp/releases/tag/v1.2.0",
        "https://github.com/ceceys/pdfrp/releases/download/",
        "https://github.com/ceceys/pdfrp/releases/downloads/v1.2.0/x.exe",
        "https://github.com/ceceys/pdfrp-fork/releases/download/v1.2.0/x.exe",
        "https://github.com/someone/pdfrp/releases/download/v1.2.0/Pdfrp.Kurulum.1.2.0.exe",
        "https://github.com/someone/else/releases/download/v1.2.0/SHA256SUMS.txt",
        "https://github.com/x/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        // Dot segments, escapes, backslashes and control characters a URL parser would resolve.
        "https://github.com/ceceys/pdfrp/releases/download/../../../someone/else/x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/../../../../x/y/releases/download/v1/x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/./x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/%2e%2e/%2e%2e/x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/..\\..\\x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/.\t./x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/v1.2.0//x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/a b.exe",
        // The path hidden in a query or a fragment doesn't count.
        "https://github.com?/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        "https://github.com#/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        "https://github.com/?/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
    ] {
        assert!(allowed(no).is_err(), "{no}");
    }
}

/// The same rule for a redirect, on the address as the HTTP client parsed it: GitHub's own
/// redirect to its file hosts goes through; a redirect elsewhere on github.com doesn't.
#[cfg(windows)]
#[test]
fn redirects_stay_under_the_release_downloads_or_on_the_file_hosts() {
    let at = |url: &str| crate::https::url_allowed(&reqwest::Url::parse(url).unwrap());
    for ok in [
        EXE_URL,
        SUMS_URL,
        "https://objects.githubusercontent.com/github-production-release-asset-2e65be/1/2?X-Amz-Algorithm=AWS4-HMAC-SHA256&response-content-disposition=attachment",
        "https://release-assets.githubusercontent.com/github-production-release-asset/1/2?sp=r&sig=abc",
        "https://github.com:443/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
    ] {
        assert!(at(ok).is_ok(), "{ok}");
    }
    for no in [
        "https://github.com/",
        "https://github.com/ceceys/pdfrp/releases/tag/v1.2.0",
        "https://github.com/login?return_to=/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        // Resolved by the parser to /ceceys/pdfrp/x.exe and /someone/else/x.exe.
        "https://github.com/ceceys/pdfrp/releases/download/v1.2.0/../../../x.exe",
        "https://github.com/ceceys/pdfrp/releases/download/%2e%2e/%2e%2e/%2e%2e/%2e%2e/someone/else/x.exe",
        "https://github.com:8443/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        "https://user@github.com/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        "https://example.com/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
        "http://github.com/ceceys/pdfrp/releases/download/v1.2.0/x.exe",
    ] {
        assert!(at(no).is_err(), "{no}");
    }
}

#[test]
fn sha256sums_lines_are_read_in_text_and_binary_mode() {
    let hash = "a".repeat(64);
    let text =
        format!("# list\n\n{hash}  Pdfrp Kurulum 1.2.0.exe\n{}  *SHA256SUMS.sig\nnot a line\n{}  short\n", "B".repeat(64), "c".repeat(63));
    let entries = parse_sums(&text);
    assert_eq!(entries, vec![(hash.clone(), "Pdfrp Kurulum 1.2.0.exe".to_owned()), ("b".repeat(64), "SHA256SUMS.sig".to_owned())]);
    // GitHub's dotted asset name finds the line written with spaces, and the other way round.
    assert_eq!(expected_sha256(&text, "Pdfrp.Kurulum.1.2.0.exe"), Some(hash.clone()));
    assert_eq!(expected_sha256(&text, &installer_name(&v("1.2.0"))), Some(hash.clone()));
    assert_eq!(expected_sha256(&text, "Pdfrp Kurulum 1.2.1.exe"), None);
    // Two different hashes for one file: neither is trusted.
    let twice = format!("{hash}  a.exe\n{}  a.exe\n", "d".repeat(64));
    assert_eq!(expected_sha256(&twice, "a.exe"), None);
    assert!(same_name("Pdfrp Kurulum 1.2.0.exe", "pdfrp.kurulum.1.2.0.EXE"));
}

#[test]
fn a_newer_release_with_its_installer_and_sums_is_offered() {
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe"));
    let Ok(Outcome::Newer(release)) = check(&fake, API, &v("1.1.0")) else { panic!("1.2.0 is newer than 1.1.0") };
    assert_eq!(release.version, v("1.2.0"));
    assert_eq!(release.tag, "v1.2.0");
    assert_eq!(release.installer.url, EXE_URL);
    assert_eq!(release.sums.url, SUMS_URL);
    assert_eq!(release.page, "https://github.com/ceceys/pdfrp/releases/tag/v1.2.0");
}

#[test]
fn the_same_or_an_older_release_is_up_to_date() {
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "x"));
    assert_eq!(check(&fake, API, &v("1.2.0")), Ok(Outcome::UpToDate(v("1.2.0"))));
    assert_eq!(check(&fake, API, &v("1.3.0")), Ok(Outcome::UpToDate(v("1.2.0"))));
    // A pre-release or a draft is never offered, whatever its number.
    for (key, value) in [("prerelease", true), ("draft", true)] {
        let mut json: serde_json::Value = serde_json::from_slice(&release_json("v9.0.0", &[])).unwrap();
        json[key] = value.into();
        let fake = Fake::new().with(API, Ok(json.to_string().into_bytes()));
        assert_eq!(check(&fake, API, &v("1.1.0")), Ok(Outcome::UpToDate(v("9.0.0"))), "{key}");
    }
    let fake = Fake::new().with(API, Ok(release_json("v9.0.0-rc.1", &[])));
    assert!(matches!(check(&fake, API, &v("1.1.0")), Ok(Outcome::UpToDate(_))));
}

#[test]
fn http_errors_rate_limits_and_a_missing_release_are_told_apart() {
    let at = |answer: Result<Vec<u8>, FetchError>| check(&Fake::new().with(API, answer), API, &v("1.1.0"));
    assert_eq!(check(&Fake::new(), API, &v("1.1.0")), Err(CheckError::NotFound), "no release published: 404");
    assert_eq!(at(Err(FetchError::Status(404))), Err(CheckError::NotFound));
    assert_eq!(at(Err(FetchError::Status(403))), Err(CheckError::RateLimited));
    assert_eq!(at(Err(FetchError::Status(429))), Err(CheckError::RateLimited));
    assert_eq!(at(Err(FetchError::Status(502))), Err(CheckError::Status(502)));
    assert_eq!(at(Err(FetchError::Network("dns".into()))), Err(CheckError::Offline("dns".into())));
    assert!(matches!(at(Err(FetchError::Refused("x".into()))), Err(CheckError::Refused(_))));
    assert!(matches!(at(Ok(b"<html>rate limited</html>".to_vec())), Err(CheckError::Invalid(_))));
    assert!(matches!(at(Ok(br#"{"tag_name":"latest"}"#.to_vec())), Err(CheckError::Invalid(_))));
    assert!(matches!(at(Ok(br#"{"message":"Not Found"}"#.to_vec())), Err(CheckError::Invalid(_))));
    assert!(matches!(at(Ok(vec![b' '; 2 << 20])), Err(CheckError::Invalid(_))), "a body past 1 MB");
}

#[test]
fn a_newer_release_without_its_installer_or_sums_is_not_offered() {
    let no_exe = Fake::new().with(API, Ok(release_json("v1.2.0", &[("SHA256SUMS.txt", SUMS_URL, 10)])));
    assert!(matches!(check(&no_exe, API, &v("1.1.0")), Err(CheckError::Invalid(e)) if e.contains("Pdfrp Kurulum 1.2.0.exe")));
    let no_sums = Fake::new().with(API, Ok(release_json("v1.2.0", &[("Pdfrp Kurulum 1.2.0.exe", EXE_URL, 10)])));
    assert!(matches!(check(&no_sums, API, &v("1.1.0")), Err(CheckError::Invalid(e)) if e.contains("SHA256SUMS.txt")));
    // Another version's installer doesn't count.
    let wrong = Fake::new().with(API, Ok(release_json("v1.2.0", &[("Pdfrp Kurulum 1.1.9.exe", EXE_URL, 10), ("SHA256SUMS.txt", SUMS_URL, 10)])));
    assert!(matches!(check(&wrong, API, &v("1.1.0")), Err(CheckError::Invalid(_))));
}

fn newer(fake: &Fake) -> Release {
    match check(fake, API, &v("1.1.0")) {
        Ok(Outcome::Newer(r)) => r,
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_download_whose_sha256_matches_is_kept_open_in_a_new_temp_folder() {
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe"));
    let release = newer(&fake);
    let parent = temp_dir("ok");
    let done = AtomicU64::new(0);
    let verified = download(&fake, &release, &parent, &done).unwrap();
    assert_eq!(std::fs::read(&verified.path).unwrap(), INSTALLER);
    assert_eq!(verified.path.file_name().unwrap(), "Pdfrp Kurulum 1.2.0.exe");
    assert_eq!(verified.path.parent().unwrap().parent().unwrap(), parent, "in a folder of its own");
    assert!(verified.path.parent().unwrap().file_name().unwrap().to_string_lossy().starts_with("pdfrp-guncelleme-"));
    assert_eq!(verified.sha256, sha256_hex(INSTALLER).unwrap());
    assert_eq!(done.load(std::sync::atomic::Ordering::Relaxed), INSTALLER.len() as u64);
    // While it waits to start, nothing can change or delete it.
    #[cfg(windows)]
    {
        assert!(std::fs::OpenOptions::new().write(true).open(&verified.path).is_err(), "no writer while it is held");
        assert!(std::fs::remove_file(&verified.path).is_err(), "no delete while it is held");
    }
    let path = verified.path.clone();
    drop(verified);
    std::fs::remove_file(&path).unwrap();
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn a_download_whose_sha256_differs_is_deleted_and_never_started() {
    let fake = github_with(b"MZ something else", sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe"));
    let mut release = newer(&fake);
    release.installer.size = 0; // the size alone would not catch it
    let parent = temp_dir("mismatch");
    let result = download(&fake, &release, &parent, &AtomicU64::new(0));
    let Err(InstallError::Mismatch { expected, actual }) = result else { panic!("{result:?}") };
    assert_eq!(expected, sha256_hex(INSTALLER).unwrap());
    assert_eq!(actual, sha256_hex(&b"MZ something else"[..]).unwrap());
    let left: Vec<_> = std::fs::read_dir(&parent).unwrap().collect();
    assert!(left.is_empty(), "nothing is left behind: {left:?}");
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn a_download_fails_without_its_line_in_the_sums_or_with_the_wrong_size_or_digest() {
    let parent = temp_dir("other");
    // SHA256SUMS.txt doesn't list the installer.
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "Pdfrp Kurulum 1.1.0.exe"));
    assert!(matches!(download(&fake, &newer(&fake), &parent, &AtomicU64::new(0)), Err(InstallError::NotListed(_))));
    // SHA256SUMS.txt is missing.
    let fake = github_with(INSTALLER, Vec::new()).with(SUMS_URL, Err(FetchError::Status(404)));
    assert!(matches!(download(&fake, &newer(&fake), &parent, &AtomicU64::new(0)), Err(InstallError::Fetch(FetchError::Status(404)))));
    // The installer is shorter than the release says.
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe"));
    let mut release = newer(&fake);
    release.installer.size += 1;
    assert!(matches!(download(&fake, &release, &parent, &AtomicU64::new(0)), Err(InstallError::Size { .. })));
    // GitHub's own digest of the asset disagrees with SHA256SUMS.txt: nothing is downloaded.
    let mut release = newer(&fake);
    release.installer.sha256 = Some("e".repeat(64));
    let done = AtomicU64::new(0);
    assert!(matches!(download(&fake, &release, &parent, &done), Err(InstallError::Mismatch { .. })));
    assert_eq!(done.load(std::sync::atomic::Ordering::Relaxed), 0);
    // The network breaks off.
    let fake = github_with(INSTALLER, sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe")).with(EXE_URL, Err(FetchError::Network("reset".into())));
    assert!(matches!(download(&fake, &newer(&fake), &parent, &AtomicU64::new(0)), Err(InstallError::Fetch(FetchError::Network(_)))));
    let left: Vec<_> = std::fs::read_dir(&parent).unwrap().collect();
    assert!(left.is_empty(), "nothing is left behind: {left:?}");
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn githubs_asset_digest_is_read_when_given() {
    let mut json: serde_json::Value = serde_json::from_slice(&release_json("v1.2.0", &[("SHA256SUMS.txt", SUMS_URL, 1)])).unwrap();
    json["assets"][0]["digest"] = format!("sha256:{}", "A".repeat(64)).into();
    let latest = parse_latest(json.to_string().as_bytes()).unwrap();
    assert_eq!(latest.assets[0].sha256.as_deref(), Some("a".repeat(64).as_str()));
    json["assets"][0]["digest"] = "sha512:00".into();
    assert_eq!(parse_latest(json.to_string().as_bytes()).unwrap().assets[0].sha256, None);
}

#[test]
fn the_installer_is_told_which_copy_to_replace() {
    use std::path::Path;
    let local = Path::new(r"C:\Users\x\AppData\Local");
    let programs = Path::new(r"C:\Program Files");
    let args = |exe: &str| installer_args(Path::new(exe), Some(local), Some(programs));
    let base = ["/NORESTART", "/MIGEZYENIDENAC=1"];
    assert_eq!(INSTALLER_ARGS, base);
    assert_eq!(args(r"C:\Users\x\AppData\Local\Programs\Pdfrp\Pdfrp.exe"), [&base[..], &["/CURRENTUSER"]].concat());
    assert_eq!(args(r"c:\program files\Pdfrp\Pdfrp.exe"), [&base[..], &["/ALLUSERS"]].concat());
    assert_eq!(args(r"C:\Program Files (x86)\Pdfrp\Pdfrp.exe"), base, "another folder that only starts the same");
    assert_eq!(args(r"D:\deneme\Pdfrp.exe"), base);
    assert_eq!(installer_args(Path::new(r"C:\Program Files\M\M.exe"), None, None), base);
}

/// No silent install (README: nothing is installed until you choose it): the wizard shows, the user starts the
/// installation, and message boxes (a program still running) are not suppressed, wherever the
/// copy is installed.
#[test]
fn the_update_never_installs_silently() {
    use std::path::Path;
    let local = Path::new(r"C:\Users\x\AppData\Local");
    let programs = Path::new(r"C:\Program Files");
    for exe in [r"C:\Users\x\AppData\Local\Programs\Pdfrp\Pdfrp.exe", r"C:\Program Files\Pdfrp\Pdfrp.exe", r"D:\x\Pdfrp.exe"]
    {
        let args = installer_args(Path::new(exe), Some(local), Some(programs));
        for quiet in ["/SILENT", "/VERYSILENT", "/SUPPRESSMSGBOXES"] {
            assert!(!args.iter().any(|a| a.eq_ignore_ascii_case(quiet)), "{exe}: {quiet} in {args:?}");
        }
        assert!(args.iter().any(|a| a == "/NORESTART"), "{exe}: {args:?}");
        assert!(args.iter().any(|a| a == "/MIGEZYENIDENAC=1"), "{exe}: the installer starts Pdfrp again: {args:?}");
    }
}

#[test]
fn only_our_own_old_download_folders_are_cleaned_up() {
    use std::time::Duration;
    let parent = temp_dir("clean");
    let make = |dir: &str, files: &[&str]| {
        let d = parent.join(dir);
        std::fs::create_dir_all(&d).unwrap();
        for f in files {
            std::fs::write(d.join(f), b"MZ").unwrap();
        }
        d
    };
    let ours = make("pdfrp-guncelleme-4242-0a1b2c3d-0", &["Pdfrp Kurulum 1.1.0.exe"]);
    let empty = make("pdfrp-guncelleme-77-ffffffff-3", &[]);
    let mixed = make("pdfrp-guncelleme-4242-0a1b2c3d-1", &["Pdfrp Kurulum 1.1.0.exe", "notlar.txt"]);
    let other_name = make("pdfrp-guncelleme-eski", &["Pdfrp Kurulum 1.1.0.exe"]);
    let other_app = make("baska-program-12-0a1b2c3d-0", &["setup.exe"]);
    let odd_installer = make("pdfrp-guncelleme-1-00000000-0", &["Pdfrp Kurulum yeni.exe"]);
    std::fs::write(parent.join("pdfrp-guncelleme-5-00000000-0"), b"a file, not a folder").unwrap();
    assert!(is_download_folder("pdfrp-guncelleme-4242-0a1b2c3d-0"));
    assert!(!is_download_folder("pdfrp-guncelleme-4242-0a1b2c3-0"), "seven hex digits");
    assert!(!is_download_folder("pdfrp-guncelleme-4242-0a1b2c3d-0-x"));
    // An antivirus may hold a just-written .exe open for a moment; the clean-up skips a file it
    // can't remove (its folder waits for the next start), so the test asks again for a while.
    let clean = |expected: usize| {
        let mut removed = 0;
        for _ in 0..50 {
            removed += clean_old_downloads(&parent, Duration::ZERO);
            if removed >= expected {
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        removed
    };
    // Too recent: an hour must have passed, so nothing goes yet.
    assert_eq!(clean_old_downloads(&parent, Duration::from_secs(3600)), 0);
    assert!(ours.exists() && empty.exists());
    // Old enough: only our own folders holding nothing but installers go.
    assert_eq!(clean(2), 2);
    assert!(!ours.exists() && !empty.exists());
    for kept in [&mixed, &other_name, &other_app, &odd_installer] {
        assert!(kept.exists(), "{} is not ours to remove", kept.display());
    }
    assert!(parent.join("pdfrp-guncelleme-5-00000000-0").is_file());
    // A checked installer still held open (about to start, or starting) keeps its folder.
    #[cfg(windows)]
    {
        let held = make("pdfrp-guncelleme-9-12345678-0", &["Pdfrp Kurulum 1.2.0.exe"]);
        let file = crate::install::open_checked(&held.join("Pdfrp Kurulum 1.2.0.exe")).unwrap();
        assert_eq!(clean_old_downloads(&parent, Duration::ZERO), 0);
        assert!(held.exists());
        drop(file);
        assert_eq!(clean(1), 1);
    }
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn a_check_is_due_once_a_day() {
    let day = 86_400;
    assert!(due(None, 1_000_000));
    assert!(!due(Some(1_000_000), 1_000_000));
    assert!(!due(Some(1_000_000), 1_000_000 + day - 1));
    assert!(due(Some(1_000_000), 1_000_000 + day));
    assert!(due(Some(1_000_000), 999_999), "the clock was set back");
}

#[test]
fn the_test_hook_reads_local_files_and_never_the_network() {
    assert_eq!(Source::parse("status:403"), Ok(Source::Status(403)));
    assert!(Source::parse("status:x").is_err());
    assert!(matches!(Source::parse("https://api.github.com/repos/someone/else/releases/latest"), Err(FetchError::Refused(_))));
    assert!(matches!(Source::parse("http://localhost:8080/latest.json"), Err(FetchError::Refused(_))));
    assert!(matches!(Source::parse("relative/latest.json"), Err(FetchError::Refused(_))));
    assert!(Source::GitHub.is_override() == false && Source::Status(404).is_override());

    let dir = temp_dir("hook");
    let exe = dir.join("Pdfrp Kurulum 1.2.0.exe");
    std::fs::write(&exe, INSTALLER).unwrap();
    let sums = dir.join("SHA256SUMS.txt");
    std::fs::write(&sums, sums_for(INSTALLER, "Pdfrp Kurulum 1.2.0.exe")).unwrap();
    let file_url = |p: &std::path::Path| format!("file:///{}", p.display().to_string().replace('\\', "/").replace(' ', "%20"));
    let json = dir.join("latest.json");
    std::fs::write(
        &json,
        release_json(
            "v1.2.0",
            &[("Pdfrp Kurulum 1.2.0.exe", &file_url(&exe), INSTALLER.len() as u64), ("SHA256SUMS.txt", &sums.display().to_string(), 1)],
        ),
    )
    .unwrap();
    for value in [json.display().to_string(), file_url(&json)] {
        let source = Source::parse(&value).unwrap();
        let (transport, url) = source.open("Pdfrp/1.1.0").unwrap();
        let Ok(Outcome::Newer(mut release)) = check(transport.as_ref(), &url, &v("1.1.0")) else { panic!("{value}") };
        release.sums.size = 0;
        let verified = download(transport.as_ref(), &release, &dir, &AtomicU64::new(0)).unwrap();
        assert_eq!(std::fs::read(&verified.path).unwrap(), INSTALLER);
    }
    // A missing file reads as 404, a status as itself, an https asset is refused.
    let missing = Source::parse(&dir.join("none.json").display().to_string()).unwrap();
    let (t, url) = missing.open("ua").unwrap();
    assert_eq!(check(t.as_ref(), &url, &v("1.1.0")), Err(CheckError::NotFound));
    for code in [403, 429] {
        let (t, url) = Source::Status(code).open("ua").unwrap();
        assert_eq!(check(t.as_ref(), &url, &v("1.1.0")), Err(CheckError::RateLimited));
    }
    assert!(matches!(FileTransport.get(EXE_URL, 10, &mut Vec::new()), Err(FetchError::Refused(_))));
    let _ = std::fs::remove_dir_all(&dir);
}

/// On Windows the installer starts while the app holds its file open for reading only (so it
/// can't change between the SHA-256 check and the start): the loader shares that handle. A copy
/// of cmd.exe stands in for the installer.
#[cfg(windows)]
#[test]
fn a_checked_file_held_open_still_starts() {
    let dir = temp_dir("start");
    let system = std::env::var_os("SystemRoot").map(std::path::PathBuf::from).unwrap_or_else(|| r"C:\Windows".into());
    let exe = dir.join("Pdfrp Kurulum 9.9.9.exe");
    std::fs::copy(system.join("System32").join("cmd.exe"), &exe).unwrap();
    let held = crate::install::open_checked(&exe).unwrap();
    let status = std::process::Command::new(&exe).args(["/d", "/c", "exit 7"]).status().unwrap();
    assert_eq!(status.code(), Some(7));
    assert!(std::fs::remove_file(&exe).is_err(), "still held");
    drop(held);
    let _ = std::fs::remove_dir_all(&dir);
}

/// What an update request carries, recorded once by a server of the test's own on 127.0.0.1
/// (nothing leaves the computer): the HTTPS transport's own client settings (`https::base`), over
/// plain HTTP to the loopback address and with no proxy. Only the User-Agent is the app's; the
/// rest are HTTP's standard headers, the same for everyone. A new header fails the test, so it is
/// looked at before the README's "only the program's name and version" can go stale. Over HTTPS
/// to GitHub the same headers go inside TLS (with HTTP/2, `Host` is the `:authority` field).
/// `cargo test -p migez-update -- --nocapture the_request_carries` prints the request.
#[cfg(windows)]
#[test]
fn the_request_carries_only_the_user_agent_besides_standard_headers() {
    use std::io::{BufRead, BufReader};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let server = std::thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut lines = Vec::new();
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line).unwrap() == 0 {
                break;
            }
            let line = line.trim_end().to_owned();
            if line.is_empty() {
                break;
            }
            lines.push(line);
        }
        let mut stream = stream;
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        lines
    });
    let client = crate::https::base(&user_agent("1.1.0")).no_proxy().build().unwrap();
    let response = client.get(format!("http://127.0.0.1:{port}/repos/ceceys/pdfrp/releases/latest")).send().unwrap();
    assert_eq!(response.status().as_u16(), 200);
    let lines = server.join().unwrap();
    println!("--- the request as the local server received it ---\n{}\n---", lines.join("\n"));
    assert_eq!(lines.first().map(String::as_str), Some("GET /repos/ceceys/pdfrp/releases/latest HTTP/1.1"), "{lines:#?}");
    let mut headers: Vec<(String, String)> = lines
        .iter()
        .skip(1)
        .filter_map(|l| l.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_owned()))
        .collect();
    headers.sort();
    let expected = [
        ("accept".to_owned(), "*/*".to_owned()),
        ("host".to_owned(), format!("127.0.0.1:{port}")),
        ("user-agent".to_owned(), "Pdfrp/1.1.0".to_owned()),
    ];
    assert_eq!(headers, expected, "{lines:#?}");
}

/// `MIGEZ_UPDATE_URL` end to end, set from outside (`cargo test -p migez-update -- --ignored
/// --nocapture from_the_environment`): prints what the check and the download find. Without the
/// variable it does nothing, so it never reaches the network.
#[test]
#[ignore = "reads MIGEZ_UPDATE_URL from the caller's environment"]
fn from_the_environment() {
    let source = match Source::from_env() {
        Ok(s) if s.is_override() => s,
        other => {
            println!("SKIP: {OVERRIDE_ENV} is not a local test source: {other:?}");
            return;
        }
    };
    let current = Version::parse(env!("CARGO_PKG_VERSION")).unwrap();
    let (transport, url) = source.open(&user_agent(env!("CARGO_PKG_VERSION"))).unwrap();
    match check(transport.as_ref(), &url, &current) {
        Ok(Outcome::Newer(release)) => {
            println!("NEWER {} ({})", release.version, release.installer.name);
            match download(transport.as_ref(), &release, &std::env::temp_dir(), &AtomicU64::new(0)) {
                Ok(verified) => {
                    println!("VERIFIED {} {}", verified.sha256, verified.path.display());
                    let path = verified.path.clone();
                    drop(verified);
                    let _ = std::fs::remove_file(&path);
                    if let Some(dir) = path.parent() {
                        let _ = std::fs::remove_dir(dir);
                    }
                }
                Err(e) => println!("DOWNLOAD FAILED: {e:?}"),
            }
        }
        other => println!("CHECK: {other:?}"),
    }
}
