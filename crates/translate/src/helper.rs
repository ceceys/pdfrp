//! The `pdfrp-ceviri` helper: a separate process that holds the translation engine
//! (`apps/pdfrp-ceviri`). The installer puts its two builds in folders beside Pdfrp.exe, both named
//! `pdfrp-ceviri.exe` (`ceviri\avx2\`, `ceviri\portable\`), and Pdfrp starts the one the processor
//! can run ([`helper_exe`]). It starts on the first translation, stays open for the next ones and
//! ends with Pdfrp (its stdin closes); Pdfrp stops it before a module is installed or removed,
//! because the helper keeps its model files mapped and Windows won't delete those.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

use crate::manifest::Direction;

/// The helper's file name.
pub const EXE: &str = if cfg!(windows) { "pdfrp-ceviri.exe" } else { "pdfrp-ceviri" };
/// Where the AVX2 build is, under Pdfrp's folder (about five times faster).
pub const AVX2_DIR: [&str; 2] = ["ceviri", "avx2"];
/// Where the build that runs on any x86-64 processor is.
pub const PORTABLE_DIR: [&str; 2] = ["ceviri", "portable"];

/// The longest answer line read from the helper (a translation of the longest text it takes).
const MAX_ANSWER: u64 = 16 << 20;

/// Which build this processor runs: AVX2 when it has AVX2 (the macro also checks that the system
/// saves the AVX registers), the portable build otherwise.
pub fn use_avx2() -> bool {
    #[cfg(target_arch = "x86_64")]
    {
        std::arch::is_x86_feature_detected!("avx2")
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        false
    }
}

/// The helper to start from the folder Pdfrp is installed in: the AVX2 build on a processor with
/// AVX2, else (or when only it is there) the portable build. `None` when neither is there.
pub fn helper_exe(dir: &Path, avx2: bool) -> Option<PathBuf> {
    let builds: &[[&str; 2]] = if avx2 { &[AVX2_DIR, PORTABLE_DIR] } else { &[PORTABLE_DIR] };
    builds.iter().map(|[a, b]| dir.join(a).join(b).join(EXE)).find(|p| p.is_file())
}

/// Why the helper gave no translation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelperError {
    /// It couldn't be started, or it stopped (its exit or a broken pipe).
    Process(String),
    /// It answered with an error.
    Answer(String),
}

impl std::fmt::Display for HelperError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HelperError::Process(e) => write!(f, "the translation helper isn't running: {e}"),
            HelperError::Answer(e) => f.write_str(e),
        }
    }
}

/// A running helper.
pub struct Helper {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
}

impl Helper {
    /// Starts `exe` on the models folder, with no console window.
    pub fn start(exe: &Path, models: &Path) -> Result<Helper, HelperError> {
        let mut command = Command::new(exe);
        command.arg(models).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::null());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            /// CREATE_NO_WINDOW: the helper is a console program; Pdfrp shows no console for it.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| HelperError::Process(format!("{}: {e}", exe.display())))?;
        let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
            let _ = child.kill();
            return Err(HelperError::Process("no pipes".into()));
        };
        Ok(Helper { child, stdin: Some(stdin), stdout: BufReader::new(stdout) })
    }

    /// One request, one answer. An answer with `error` is [`HelperError::Answer`].
    pub fn request(&mut self, request: &Value) -> Result<Value, HelperError> {
        let gone = |e: std::io::Error| HelperError::Process(e.to_string());
        let stdin = self.stdin.as_mut().ok_or_else(|| HelperError::Process("stopped".into()))?;
        let mut line = serde_json::to_vec(request).map_err(|e| HelperError::Process(e.to_string()))?;
        line.push(b'\n');
        stdin.write_all(&line).map_err(gone)?;
        stdin.flush().map_err(gone)?;
        let mut answer = Vec::new();
        let read = (&mut self.stdout).take(MAX_ANSWER).read_until(b'\n', &mut answer).map_err(gone)?;
        if read == 0 {
            return Err(HelperError::Process("it exited".into()));
        }
        let value: Value = serde_json::from_slice(&answer).map_err(|e| HelperError::Process(format!("unreadable answer: {e}")))?;
        match value.get("error").and_then(Value::as_str) {
            Some(e) => Err(HelperError::Answer(e.to_owned())),
            None => Ok(value),
        }
    }

    /// Translates `text` through the modules in order (one, or two through English).
    pub fn translate(&mut self, chain: &[&Direction], text: &str) -> Result<String, HelperError> {
        let chain = chain.iter().map(|d| d.key()).collect::<Vec<_>>().join(",");
        let answer = self.request(&json!({ "op": "translate", "chain": chain, "text": text }))?;
        answer
            .get("translation")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| HelperError::Process("an answer without a translation".into()))
    }

    /// The helper's version, kernel (`avx2` or `portable`) and loaded modules.
    pub fn status(&mut self) -> Result<Value, HelperError> {
        self.request(&json!({ "op": "status" }))
    }

    /// Whether it is still running.
    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Stops the helper: closes its stdin (it exits on its own) and, if it hasn't within two
    /// seconds, ends it. Its model files are free afterwards.
    pub fn stop(mut self) {
        self.finish();
    }

    fn finish(&mut self) {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(2);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(Duration::from_millis(20)),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Helper {
    fn drop(&mut self) {
        if self.stdin.is_some() {
            self.finish();
        }
    }
}
