//! The language modules Pdfrp offers: `models-v3.0.json`, embedded at build time.
//!
//! `kurulum/ceviri-modelleri.ps1` makes the file from the models pulled once from Mozilla's Remote
//! Settings. Each direction (`tr-en`, `en-tr` …) lists its files: the model and its vocabulary (or
//! a source and a target vocabulary), each as a `.zst` file in the `models-v3.0` release of this
//! repository with its size and Mozilla's SHA-256, and unpacked with its size and SHA-256. The
//! release names its files `<from>-<to>.<Mozilla's file name>`: Mozilla gives en-zh-Hans and
//! en-zh-Hant the same file names, and a release holds a name only once.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::languages::ENGLISH;

/// The embedded manifest's text.
pub const EMBEDDED: &str = include_str!("../models-v3.0.json");

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// The release's tag (`models-v3.0`).
    pub tag: String,
    /// The release's title ("Pdfrp dil modülleri 3.0").
    pub title: String,
    /// Where the release's files are: `https://github.com/ceceys/pdfrp/releases/download/<tag>/`.
    pub base_url: String,
    /// Where the files came from (Mozilla's Remote Settings collection).
    pub source: String,
    /// When they were pulled (YYYY-MM-DD).
    pub fetched: String,
    pub license: String,
    /// What the two SHA-256 values are.
    pub hashes: String,
    /// How the release names files.
    pub asset_names: String,
    /// How many languages besides English: the manifest counts them, nobody types the number.
    pub languages: usize,
    pub directions: Vec<Direction>,
}

/// One direction of translation (`tr-en`): one language module.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Direction {
    pub from: String,
    pub to: String,
    /// The model's version (`3.0`, `3.1`): the install folder's name.
    pub version: String,
    /// `tiny` or `base-memory`.
    pub architecture: String,
    pub files: Vec<ModelFile>,
}

/// One file of a module.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ModelFile {
    /// `model`, `vocab`, `srcvocab` or `trgvocab`.
    pub kind: String,
    /// The unpacked file's name in the install folder (`model.tren.intgemm.alphas.bin`).
    pub name: String,
    /// The `.zst` file's name in the release (`tr-en.model.tren.intgemm.alphas.bin.zst`).
    pub asset: String,
    /// The `.zst` file's size.
    pub size: u64,
    /// The `.zst` file's SHA-256 (Mozilla's `attachment.hash`).
    pub sha256: String,
    pub unpacked_size: u64,
    /// The unpacked file's SHA-256 (Mozilla's `decompressedHash`).
    pub unpacked_sha256: String,
}

/// Why two languages can't be translated.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RouteError {
    /// Source and target are the same language.
    Same,
    /// No module for this language.
    Unknown(String),
}

impl std::fmt::Display for RouteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RouteError::Same => f.write_str("the source and target languages are the same"),
            RouteError::Unknown(code) => write!(f, "no language module for {code}"),
        }
    }
}

static PARSED: LazyLock<Result<Manifest, String>> = LazyLock::new(|| parse(EMBEDDED));

/// The embedded manifest. An error only if the build embedded a broken file (`tests.rs` reads it
/// on every test run).
pub fn embedded() -> Result<&'static Manifest, String> {
    PARSED.as_ref().map_err(Clone::clone)
}

/// Parses and checks a manifest: every name is a plain file name (it becomes a path), every hash
/// 64 hex digits, every version digits and dots, and the language count matches the directions.
pub fn parse(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = serde_json::from_str(text).map_err(|e| format!("language module manifest: {e}"))?;
    let plain = |s: &str| {
        !s.is_empty() && !s.starts_with('.') && s.len() <= 128 && s.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
    };
    let hash = |s: &str| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit());
    let version = |s: &str| (1..=16).contains(&s.len()) && s.split('.').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()));
    let code = |s: &str| (2..=16).contains(&s.len()) && s.split('-').all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_alphanumeric()));
    if !manifest.base_url.starts_with("https://") || !manifest.base_url.ends_with('/') {
        return Err(format!("language module manifest: base_url {} isn't an https:// folder", manifest.base_url));
    }
    for d in &manifest.directions {
        let id = d.id();
        if !code(&d.from) || !code(&d.to) || !version(&d.version) || (d.from != ENGLISH.code && d.to != ENGLISH.code) || d.from == d.to {
            return Err(format!("language module manifest: {id} {} isn't a valid direction", d.version));
        }
        if d.files.is_empty() {
            return Err(format!("language module manifest: {id} has no files"));
        }
        for f in &d.files {
            if !plain(&f.name) || !plain(&f.asset) || !hash(&f.sha256) || !hash(&f.unpacked_sha256) || f.size == 0 || f.unpacked_size == 0 {
                return Err(format!("language module manifest: {id}: {} isn't a valid file entry", f.name));
            }
        }
    }
    if manifest.language_codes().len() != manifest.languages {
        return Err("language module manifest: its language count doesn't match its directions".into());
    }
    Ok(manifest)
}

impl Manifest {
    /// The module from `from` to `to`, if there is one.
    pub fn direction(&self, from: &str, to: &str) -> Option<&Direction> {
        self.directions.iter().find(|d| d.from == from && d.to == to)
    }

    /// Every language besides English that has a module, sorted by code.
    pub fn language_codes(&self) -> Vec<&str> {
        let mut codes: Vec<&str> = self.directions.iter().map(|d| if d.from == ENGLISH.code { d.to.as_str() } else { d.from.as_str() }).collect();
        codes.sort_unstable();
        codes.dedup();
        codes
    }

    /// Both modules of a language (to English, from English), as far as they exist.
    pub fn modules_of(&self, language: &str) -> Vec<&Direction> {
        [self.direction(language, ENGLISH.code), self.direction(ENGLISH.code, language)].into_iter().flatten().collect()
    }

    /// The modules that translate `from` into `to`, in order: one when either is English, two
    /// through English otherwise (Turkish to German is `tr-en`, then `en-de`).
    pub fn route(&self, from: &str, to: &str) -> Result<Vec<&Direction>, RouteError> {
        if from == to {
            return Err(RouteError::Same);
        }
        let module =
            |a: &str, b: &str| self.direction(a, b).ok_or_else(|| RouteError::Unknown(if a == ENGLISH.code { b.to_owned() } else { a.to_owned() }));
        if from == ENGLISH.code || to == ENGLISH.code {
            return module(from, to).map(|d| vec![d]);
        }
        Ok(vec![module(from, ENGLISH.code)?, module(ENGLISH.code, to)?])
    }

    /// Where a release file is.
    pub fn url(&self, file: &ModelFile) -> String {
        format!("{}{}", self.base_url, file.asset)
    }
}

impl Direction {
    /// `tr-en`.
    pub fn id(&self) -> String {
        format!("{}-{}", self.from, self.to)
    }

    /// `tr-en@3.0`, as the helper takes it.
    pub fn key(&self) -> String {
        format!("{}-{}@{}", self.from, self.to, self.version)
    }

    /// What downloading the module takes (bytes of its `.zst` files).
    pub fn download_size(&self) -> u64 {
        self.files.iter().map(|f| f.size).fold(0, u64::saturating_add)
    }

    /// What the installed module takes on disk.
    pub fn installed_size(&self) -> u64 {
        self.files.iter().map(|f| f.unpacked_size).fold(0, u64::saturating_add)
    }
}
