//! pdfrp-ceviri: Pdfrp's offline translation helper.
//!
//! Usage: `pdfrp-ceviri <models-folder>`
//!
//! Pdfrp installs each language module in `<models-folder>/<from>-<to>/<version>/`
//! (`migez_translate::install`): a `model.*.bin` and either one `vocab.*.spm` or, for pairs with
//! two vocabularies (Japanese, Korean, Chinese), `srcvocab.*.spm` and `trgvocab.*.spm`.
//!
//! Protocol: one JSON object per line on stdin; one JSON line back on stdout for each. stdout
//! carries nothing else, diagnostics go to stderr. The helper exits when stdin closes, so it ends
//! with the program that started it.
//!
//! ```text
//! {"op":"translate","chain":"tr-en@3.0,en-de@3.0","text":"…"} -> {"translation":"…","ms":12.3,"load_ms":40.1}
//! {"op":"load","chain":"tr-en@3.0"}                            -> {"loaded":["tr-en@3.0"],"load_ms":40.1}
//! {"op":"status"}                                              -> {"version":"1.2.0","kernel":"avx2","loaded":["tr-en@3.0"]}
//! any failure                                                  -> {"error":"…"}
//! ```
//!
//! A chain is one direction, or two that meet in English: Turkish to German is `tr-en`, then
//! `en-de`. Each module is loaded on first use (memory-mapped, so Windows keeps its model file
//! open until the helper exits) and kept for the next request; at most [`MAX_LOADED`] stay loaded.
//!
//! No network: fxtranslate is built without its `download`, `net` and `discovery` features, and
//! models are only read from the folder given on the command line.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::ffi::OsString;
use std::io::{self, BufRead, Read, Write};
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::time::Instant;

use fxtranslate::engine::Engine;
use serde::Deserialize;
use serde_json::{Value, json};

/// The helper's version: Pdfrp's.
const VERSION: &str = env!("CARGO_PKG_VERSION");
/// The longest request line read (bytes). A longer line is skipped to its end and answered with
/// an error, so the next request starts clean.
const MAX_LINE: usize = 4 * 1024 * 1024;
/// The most characters translated in one request.
const MAX_TEXT: usize = 100_000;
/// The most modules in one chain: a direction, or two through English.
const MAX_CHAIN: usize = 2;
/// The most modules kept loaded; the least recently used goes first.
const MAX_LOADED: usize = 4;

fn main() {
    // A panic message goes to stderr, never into the JSON on stdout.
    panic::set_hook(Box::new(|info| eprintln!("pdfrp-ceviri: internal error: {info}")));
    let mut out = io::stdout().lock();
    let root = match models_folder(std::env::args_os().skip(1)) {
        Ok(root) => root,
        Err(e) => {
            let _ = write_line(&mut out, &error(&e));
            std::process::exit(2);
        }
    };
    let mut engines = Engines { root, loaded: Vec::new() };
    let mut input = io::stdin().lock();
    loop {
        let answer = match read_line(&mut input) {
            Ok(Line::End) => break,
            Ok(Line::TooLong) => error(&format!("the request line is too long (at most {} MB)", MAX_LINE >> 20)),
            Ok(Line::Full(line)) => match panic::catch_unwind(AssertUnwindSafe(|| engines.answer(&line))) {
                Ok(answer) => answer,
                Err(_) => {
                    // The engine's state is unknown: drop every loaded module; the next request
                    // loads what it needs again.
                    engines.loaded.clear();
                    error("internal error in the translation engine; the language modules will be loaded again")
                }
            },
            Err(e) => {
                eprintln!("pdfrp-ceviri: can't read stdin: {e}");
                break;
            }
        };
        if write_line(&mut out, &answer).is_err() {
            // Pdfrp has gone; leave quietly.
            break;
        }
    }
}

/// The one argument: the folder the language modules are installed in.
fn models_folder(mut args: impl Iterator<Item = OsString>) -> Result<PathBuf, String> {
    let (Some(dir), None) = (args.next(), args.next()) else {
        return Err("usage: pdfrp-ceviri <models-folder>".into());
    };
    let dir = PathBuf::from(dir);
    if !dir.is_dir() {
        return Err(format!("the models folder doesn't exist: {}", dir.display()));
    }
    Ok(dir)
}

// ---------------------------------------------------------------------------------------------
// Requests

#[derive(Deserialize)]
struct Request {
    #[serde(default)]
    op: Option<String>,
    #[serde(default)]
    chain: Option<String>,
    #[serde(default)]
    text: Option<String>,
}

/// One installed language module: a direction (`tr-en`, `zh-Hans-en`) and its version (`3.0`).
#[derive(Clone, Debug, PartialEq, Eq)]
struct Module {
    direction: String,
    version: String,
}

impl Module {
    fn key(&self) -> String {
        format!("{}@{}", self.direction, self.version)
    }

    fn folder(&self, root: &Path) -> PathBuf {
        root.join(&self.direction).join(&self.version)
    }

    /// The direction's target language: what follows its last `-`.
    fn target(&self) -> &str {
        self.direction.rsplit('-').next().unwrap_or_default()
    }

    /// Whether the source language writes sentences one after another with no space (Chinese,
    /// Japanese) and the target language separates them with one: then the translated sentences
    /// need a space between them, which the source text doesn't have.
    fn spaces_sentences(&self) -> bool {
        let target = self.target();
        let source = self.direction.strip_suffix(target).and_then(|s| s.strip_suffix('-')).unwrap_or_default();
        let unspaced = |language: &str| language == "ja" || language == "zh" || language.starts_with("zh-");
        unspaced(source) && !unspaced(target)
    }
}

/// Translates `text` sentence by sentence (ICU sentence breaks, each within the model's window),
/// keeping the source's whitespace between sentences, as fxtranslate's `translate_long` does. With
/// `spaced`, two sentences the source wrote with nothing between them get one space.
fn translate(engine: &Engine, text: &str, spaced: bool) -> String {
    if !spaced {
        return engine.translate_long(text);
    }
    use fxtranslate::segment::{IcuSegmenter, Segmenter};
    let spans = IcuSegmenter::new().sentences(text);
    let Some(first) = spans.first() else { return text.to_owned() };
    let mut out = text.get(..first.start).unwrap_or_default().to_owned();
    let mut previous_end = None;
    for span in &spans {
        if let Some(end) = previous_end {
            let gap = text.get(end..span.start).unwrap_or_default();
            out.push_str(if gap.is_empty() { " " } else { gap });
        }
        out.push_str(&engine.translate_long(text.get(span.start..span.end).unwrap_or_default()));
        previous_end = Some(span.end);
    }
    if let Some(end) = previous_end {
        out.push_str(text.get(end..).unwrap_or_default());
    }
    out
}

/// `direction@version`. Both become folder names under the models folder, so only letters,
/// digits, `-` and `_` (direction) and digits and dots (version) are accepted: nothing that could
/// leave the folder (`..`, `\`, `/`, `:`), and no empty part.
fn parse_module(text: &str) -> Result<Module, String> {
    let bad = || format!("not a language module: {text:?}");
    let (direction, version) = text.split_once('@').ok_or_else(bad)?;
    let direction_ok = (3..=32).contains(&direction.len())
        && direction.split('-').count() >= 2
        && direction.split('-').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    let version_ok = (1..=16).contains(&version.len()) && version.split('.').all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()));
    if direction_ok && version_ok { Ok(Module { direction: direction.to_owned(), version: version.to_owned() }) } else { Err(bad()) }
}

/// A comma-separated chain of one or two modules; the second must start where the first ends
/// (`tr-en@3.0,en-de@3.0`).
fn parse_chain(text: &str) -> Result<Vec<Module>, String> {
    let modules = text.split(',').map(|m| parse_module(m.trim())).collect::<Result<Vec<_>, _>>()?;
    if modules.is_empty() || modules.len() > MAX_CHAIN {
        return Err(format!("a chain has one or two language modules: {text:?}"));
    }
    for pair in modules.windows(2) {
        if let [first, second] = pair
            && !second.direction.starts_with(&format!("{}-", first.target()))
        {
            return Err(format!("{} doesn't continue from {}", second.key(), first.key()));
        }
    }
    Ok(modules)
}

struct Engines {
    root: PathBuf,
    /// Loaded modules, the most recently used last.
    loaded: Vec<(Module, Engine)>,
}

impl Engines {
    fn answer(&mut self, line: &[u8]) -> Value {
        self.answer_or_error(line).unwrap_or_else(|e| error(&e))
    }

    fn answer_or_error(&mut self, line: &[u8]) -> Result<Value, String> {
        let line = line.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(line);
        let text = std::str::from_utf8(line).map_err(|_| "the request isn't UTF-8".to_string())?;
        let text = text.trim_end_matches(['\n', '\r']);
        if text.trim().is_empty() {
            return Err("empty request".into());
        }
        let request: Request = serde_json::from_str(text).map_err(|e| format!("the request isn't a JSON object of strings: {e}"))?;
        match request.op.as_deref().unwrap_or("translate") {
            "translate" => {
                let chain = parse_chain(request.chain.as_deref().ok_or("\"chain\" is missing")?)?;
                let source = request.text.ok_or("\"text\" is missing")?;
                if source.chars().count() > MAX_TEXT {
                    return Err(format!("the text is too long (at most {MAX_TEXT} characters)"));
                }
                let start = Instant::now();
                let mut load_ms = 0.0;
                let mut current = source;
                for module in &chain {
                    let (engine, ms) = self.engine(module)?;
                    load_ms += ms;
                    if !current.trim().is_empty() {
                        current = translate(engine, &current, module.spaces_sentences());
                    }
                }
                let ms = (millis(start) - load_ms).max(0.0);
                Ok(json!({ "translation": current, "ms": round(ms), "load_ms": round(load_ms) }))
            }
            "load" => {
                let chain = parse_chain(request.chain.as_deref().ok_or("\"chain\" is missing")?)?;
                let mut load_ms = 0.0;
                for module in &chain {
                    load_ms += self.engine(module)?.1;
                }
                Ok(json!({ "loaded": chain.iter().map(Module::key).collect::<Vec<_>>(), "load_ms": round(load_ms) }))
            }
            "status" => Ok(json!({
                "version": VERSION,
                "kernel": kernel(),
                "loaded": self.loaded.iter().map(|(m, _)| m.key()).collect::<Vec<_>>(),
            })),
            other => Err(format!("unknown op: {other}")),
        }
    }

    /// The module's engine, loading it if needed, and how long loading took (0 when it was
    /// loaded already).
    fn engine(&mut self, module: &Module) -> Result<(&Engine, f64), String> {
        let mut load_ms = 0.0;
        match self.loaded.iter().position(|(m, _)| m == module) {
            Some(i) => {
                let entry = self.loaded.remove(i);
                self.loaded.push(entry);
            }
            None => {
                let start = Instant::now();
                let engine = load(&module.folder(&self.root)).map_err(|e| format!("{}: {e}", module.key()))?;
                load_ms = millis(start);
                if self.loaded.len() >= MAX_LOADED {
                    // Dropping an engine unmaps its model file.
                    self.loaded.remove(0);
                }
                self.loaded.push((module.clone(), engine));
            }
        }
        self.loaded.last().map(|(_, engine)| (engine, load_ms)).ok_or_else(|| "no engine".to_string())
    }
}

/// The model and vocabulary files of a module folder.
struct ModelFiles {
    model: PathBuf,
    source_vocab: PathBuf,
    target_vocab: PathBuf,
}

fn model_files(folder: &Path) -> Result<ModelFiles, String> {
    let entries = std::fs::read_dir(folder).map_err(|e| format!("the language module isn't installed ({}): {e}", folder.display()))?;
    let (mut model, mut shared, mut source, mut target) = (None, None, None, None);
    for entry in entries {
        let entry = entry.map_err(|e| format!("can't read the language module's folder: {e}"))?;
        if !entry.file_type().is_ok_and(|t| t.is_file()) {
            continue;
        }
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let slot = if name.starts_with("model.") && name.ends_with(".bin") {
            &mut model
        } else if name.starts_with("vocab.") && name.ends_with(".spm") {
            &mut shared
        } else if name.starts_with("srcvocab.") && name.ends_with(".spm") {
            &mut source
        } else if name.starts_with("trgvocab.") && name.ends_with(".spm") {
            &mut target
        } else {
            continue;
        };
        if slot.is_some() {
            return Err(format!("the language module has two files of one kind: {name}"));
        }
        *slot = Some(entry.path());
    }
    let model = model.ok_or("the language module has no model file")?;
    let (source_vocab, target_vocab) = match (shared, source, target) {
        // One vocabulary for both sides: the engine tells that from the two paths being equal.
        (Some(v), None, None) => (v.clone(), v),
        (None, Some(s), Some(t)) => (s, t),
        _ => return Err("the language module's vocabulary files are missing or don't match".into()),
    };
    Ok(ModelFiles { model, source_vocab, target_vocab })
}

fn load(folder: &Path) -> Result<Engine, String> {
    let files = model_files(folder)?;
    #[cfg(any(feature = "fast", feature = "portable"))]
    let engine = Engine::load_mmapped(&files.model, &files.source_vocab, &files.target_vocab);
    #[cfg(not(any(feature = "fast", feature = "portable")))]
    let engine = Engine::load(&files.model, &files.source_vocab, &files.target_vocab);
    engine.map_err(|e| format!("the model can't be loaded: {e}"))
}

/// Which GEMM kernel this build runs: `avx2` for the AVX2 build. If its C++ kernel hadn't been
/// compiled, fxtranslate would fall back to `scalar` silently; release builds set
/// FXTRANSLATE_REQUIRE_SIMD so that fails the build instead, and this shows it at run time.
fn kernel() -> &'static str {
    #[cfg(feature = "fast")]
    {
        fxtranslate::gemm::backend()
    }
    #[cfg(not(feature = "fast"))]
    {
        "portable"
    }
}

fn millis(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn round(ms: f64) -> f64 {
    (ms * 10.0).round() / 10.0
}

fn error(message: &str) -> Value {
    json!({ "error": message })
}

// ---------------------------------------------------------------------------------------------
// Lines

enum Line {
    Full(Vec<u8>),
    TooLong,
    End,
}

fn read_line(input: &mut impl BufRead) -> io::Result<Line> {
    let mut buffer = Vec::new();
    let read = input.by_ref().take(MAX_LINE as u64 + 1).read_until(b'\n', &mut buffer)?;
    if read == 0 {
        return Ok(Line::End);
    }
    if buffer.len() > MAX_LINE && buffer.last() != Some(&b'\n') {
        // Skip the rest of the line, so the next request starts clean.
        loop {
            let chunk = input.fill_buf()?;
            if chunk.is_empty() {
                break;
            }
            if let Some(i) = chunk.iter().position(|&b| b == b'\n') {
                input.consume(i + 1);
                break;
            }
            let n = chunk.len();
            input.consume(n);
        }
        return Ok(Line::TooLong);
    }
    Ok(Line::Full(buffer))
}

fn write_line(out: &mut impl Write, value: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *out, value)?;
    out.write_all(b"\n")?;
    out.flush()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modules_stay_inside_the_models_folder() {
        assert_eq!(parse_module("tr-en@3.0").unwrap(), Module { direction: "tr-en".into(), version: "3.0".into() });
        assert!(parse_module("zh-Hans-en@3.1").is_ok());
        assert!(parse_module("en-zh_hant@3").is_ok());
        for bad in
            ["tr-en", "tr-en@", "@3.0", "..@3.0", "../tr-en@3.0", "tr-en\\..@3.0", "c:tr-en@3.0", "tr en@3.0", "-tr@3.0", "tr-@3.0", "tren@3.0"]
        {
            assert!(parse_module(bad).is_err(), "{bad}");
        }
        for bad in ["tr-en@..", "tr-en@3..0", "tr-en@.3", "tr-en@3.", "tr-en@3/0", "tr-en@3.0a", "tr-en@12345678901234567"] {
            assert!(parse_module(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn sentences_from_chinese_and_japanese_get_spaces_in_other_languages() {
        let module = |text: &str| parse_module(text).unwrap();
        assert!(module("zh-Hans-en@3.1").spaces_sentences());
        assert!(module("zh-Hant-en@3.0").spaces_sentences());
        assert!(module("ja-en@3.0").spaces_sentences());
        for direction in ["en-zh-Hans@3.0", "en-ja@3.0", "ko-en@3.0", "tr-en@3.0", "en-de@3.0"] {
            assert!(!module(direction).spaces_sentences(), "{direction}");
        }
    }

    #[test]
    fn a_chain_meets_in_its_middle_language() {
        assert_eq!(parse_chain("tr-en@3.0, en-de@3.0").unwrap().len(), 2);
        assert_eq!(parse_chain("zh-Hans-en@3.1,en-ja@3.0").unwrap().len(), 2);
        assert!(parse_chain("tr-en@3.0,de-en@3.0").is_err(), "German doesn't continue from English");
        assert!(parse_chain("tr-en@3.0,en-de@3.0,de-en@3.0").is_err(), "at most two");
        assert!(parse_chain("").is_err());
    }

    #[test]
    fn requests_and_failures_answer_in_one_json_line() {
        let dir = std::env::temp_dir().join(format!("pdfrp-ceviri-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut engines = Engines { root: dir.clone(), loaded: Vec::new() };
        let status = engines.answer(b"\xEF\xBB\xBF{\"op\":\"status\"}\r\n");
        assert_eq!(status["version"], VERSION);
        assert!(status["kernel"].is_string());
        for (request, want) in [
            (&b"{\"op\":5}"[..], "isn't a JSON object of strings"),
            (b"[1]", "isn't a JSON object of strings"),
            (b"{\"op\":\"x\"}", "unknown op"),
            (b"{\"chain\":\"tr-en@3.0\"}", "\"text\" is missing"),
            (b"{\"text\":\"a\"}", "\"chain\" is missing"),
            (b"{\"chain\":\"tr-en@3.0\",\"text\":\"Merhaba\"}", "isn't installed"),
            (b"\xFF\xFE", "isn't UTF-8"),
            (b"  \r\n", "empty request"),
        ] {
            let answer = engines.answer(request);
            let message = answer["error"].as_str().unwrap_or_default();
            assert!(message.contains(want), "{}: {message}", String::from_utf8_lossy(request));
        }
        let long = "a".repeat(MAX_TEXT + 1);
        let answer = engines.answer(json!({ "chain": "tr-en@3.0", "text": long }).to_string().as_bytes());
        assert!(answer["error"].as_str().unwrap().contains("too long"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_long_line_is_skipped_and_the_next_one_read() {
        let mut data = vec![b'a'; MAX_LINE + 10];
        data.extend_from_slice(b"\n{\"op\":\"status\"}\n");
        let mut input = io::Cursor::new(data);
        assert!(matches!(read_line(&mut input).unwrap(), Line::TooLong));
        match read_line(&mut input).unwrap() {
            Line::Full(line) => assert_eq!(line, b"{\"op\":\"status\"}\n"),
            _ => panic!("the second line wasn't read"),
        }
        assert!(matches!(read_line(&mut input).unwrap(), Line::End));
    }

    #[test]
    fn a_module_folder_needs_a_model_and_matching_vocabularies() {
        let dir = std::env::temp_dir().join(format!("pdfrp-ceviri-files-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(model_files(&dir).is_err(), "empty");
        std::fs::write(dir.join("model.tren.intgemm.alphas.bin"), b"").unwrap();
        assert!(model_files(&dir).is_err(), "no vocabulary");
        std::fs::write(dir.join("srcvocab.tren.spm"), b"").unwrap();
        assert!(model_files(&dir).is_err(), "half a pair");
        std::fs::write(dir.join("trgvocab.tren.spm"), b"").unwrap();
        let files = model_files(&dir).unwrap();
        assert_ne!(files.source_vocab, files.target_vocab);
        std::fs::write(dir.join("vocab.tren.spm"), b"").unwrap();
        assert!(model_files(&dir).is_err(), "shared and split together");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Real models, from a folder of installed modules named by `PDFRP_CEVIRI_MODELLER`
    /// (`<folder>/<from>-<to>/<version>/`, unpacked): Turkish to German through English, a
    /// base-memory pair (de-en), Chinese both ways (en-zh-Hans has two vocabularies) and English to
    /// Japanese. `cargo test -p pdfrp-ceviri -- --ignored --nocapture real_models`.
    #[test]
    #[ignore = "needs installed language modules (PDFRP_CEVIRI_MODELLER)"]
    fn real_models_translate_through_english_and_with_two_vocabularies() {
        let Some(root) = std::env::var_os("PDFRP_CEVIRI_MODELLER").map(PathBuf::from) else {
            println!("SKIP: PDFRP_CEVIRI_MODELLER is not set");
            return;
        };
        let mut engines = Engines { root, loaded: Vec::new() };
        let mut ask = |chain: &str, text: &str| {
            let answer = engines.answer(json!({ "chain": chain, "text": text }).to_string().as_bytes());
            let translation = answer["translation"].as_str().unwrap_or_else(|| panic!("{chain}: {answer}")).to_owned();
            println!("{chain}: {text} -> {translation}");
            translation
        };
        let de = ask("tr-en@3.0,en-de@3.0", "Toplantı saat kaçta başlıyor? Ofisimiz Çarşamba günü kapalı olacaktır.");
        assert!(de.contains("Mittwoch"), "{de}");
        let en = ask("de-en@3.0", "Der Mieter zahlt die Miete bis zum fünften Tag jedes Monats.");
        assert!(en.to_lowercase().contains("rent"), "{en}");
        let zh = ask("en-zh-Hans@3.0", "The meeting will take place on Wednesday afternoon.");
        assert!(zh.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c)), "{zh}");
        let back = ask("zh-Hans-en@3.1", "会议将于星期三下午举行。我们的办公室在伊斯坦布尔。");
        assert!(back.contains(". "), "a space between the two sentences: {back}");
        let ja = ask("en-ja@3.0", "Thank you very much.");
        assert!(ja.chars().any(|c| ('\u{3040}'..='\u{30FF}').contains(&c)), "{ja}");
        let paragraphs = ask("tr-en@3.0", "Birinci paragraf burada biter.\n\nİkinci paragraf burada başlar.");
        assert_eq!(paragraphs.matches("\n\n").count(), 1, "the paragraph break stays: {paragraphs}");
    }

    /// The AVX2 build really runs the AVX2 kernel (FXTRANSLATE_REQUIRE_SIMD makes a build without
    /// it fail; this catches a build that skipped that variable), and on this machine its results
    /// match the scalar kernel's where the 16-bit intermediate sums can't saturate (weights within
    /// ±63, the same bound as fxtranslate's own gemm_parity test).
    #[cfg(all(feature = "fast", target_arch = "x86_64"))]
    #[test]
    fn the_avx2_kernel_is_linked_and_agrees_with_the_scalar_one() {
        use fxtranslate::{gemm, ops};
        assert_eq!(gemm::backend(), "avx2", "the AVX2 kernel isn't linked");
        if !std::arch::is_x86_feature_detected!("avx2") {
            return;
        }
        let mut state = 0x9E37_79B9_7F4A_7C15u64;
        let mut random = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };
        for &(m, k, n) in &[(1, 256, 8), (3, 256, 13), (7, 512, 1000), (16, 256, 37)] {
            let a: Vec<u8> = (0..m * k).map(|_| random() as u8).collect();
            let b: Vec<i8> = (0..n * k).map(|_| ((random() % 127) as i16 - 63) as i8).collect();
            let bias: Vec<f32> = (0..n).map(|_| (random() % 2001) as f32 / 100.0 - 10.0).collect();
            let scalar = ops::intgemm_affine(&a, m, k, &b, n, 0.0007, &bias);
            let simd = gemm::PreparedB::new(&b, n, k).expect("prepared").matmul(&a, m, 0.0007, &bias);
            assert_eq!(scalar.len(), simd.len());
            for (i, (s, g)) in scalar.iter().zip(&simd).enumerate() {
                assert!((s - g).abs() <= 1e-2 + 1e-4 * s.abs(), "m={m} k={k} n={n} i={i}: {s} / {g}");
            }
        }
    }
}
