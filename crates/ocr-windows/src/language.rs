//! Which installed Windows language reads the language a job asks for, and what to tell the user
//! when none does.

use migez_ocr::OcrError;

/// The longest language code taken as one (BCP 47 tags in use are far shorter).
const MAX_CODE: usize = 35;

/// The longest display name echoed in a message.
const MAX_NAME: usize = 64;

/// Whether `code` looks like a language tag: 1 to 35 ASCII letters, digits, hyphens and
/// underscores. Only such a code is passed to Windows or echoed back in a message.
pub(crate) fn plausible(code: &str) -> bool {
    (1..=MAX_CODE).contains(&code.len()) && code.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// `tag` compared case-insensitively, with `_` read as `-` (`en_US` is `en-us`).
fn normalise(tag: &str) -> String {
    tag.trim().replace('_', "-").to_ascii_lowercase()
}

/// Whether a normalised tag names a script (`zh-hans`, `sr-latn`): a four-letter subtag right
/// after the language.
fn names_script(tag: &str) -> bool {
    tag.split('-').nth(1).is_some_and(|s| s.len() == 4 && s.bytes().all(|b| b.is_ascii_alphabetic()))
}

/// The index of the installed tag that reads `wanted`: the same tag first; then one that only
/// narrows it (`tr` → `tr-TR`, `zh-Hans` → `zh-Hans-CN`); then, when neither names a script, one
/// of the same language in another region (`en-GB` → `en-US`, but never `zh-Hant` for
/// `zh-Hans`). `None` when nothing installed reads it.
pub(crate) fn pick(wanted: &str, installed: &[&str]) -> Option<usize> {
    let wanted = normalise(wanted);
    if !plausible(&wanted) {
        return None;
    }
    let installed: Vec<String> = installed.iter().map(|t| normalise(t)).collect();
    if let Some(i) = installed.iter().position(|t| *t == wanted) {
        return Some(i);
    }
    let narrower = format!("{wanted}-");
    if let Some(i) = installed.iter().position(|t| t.starts_with(&narrower)) {
        return Some(i);
    }
    let base = |t: &str| t.split('-').next().unwrap_or_default().to_string();
    if names_script(&wanted) {
        return None;
    }
    installed.iter().position(|t| !names_script(t) && base(t) == base(&wanted))
}

/// The error for a language Windows has no recognition installed for: which language (`code`,
/// with the name Windows gives it when there is one) and where Windows installs it, in the
/// English and the Turkish settings. A `code` that is not [`plausible`] is not echoed.
pub(crate) fn missing(code: &str, name: Option<&str>) -> OcrError {
    let code = code.trim();
    let code = if plausible(code) { code } else { "?" };
    let name: Option<String> = name.map(|n| n.trim().chars().take(MAX_NAME).collect()).filter(|n: &String| !n.is_empty() && n != code);
    let (label, setting) = match &name {
        Some(name) => (format!("{name} ({code})"), name.as_str()),
        None => (format!("\"{code}\""), code),
    };
    OcrError::Recognize(format!(
        "Windows has no text recognition (OCR) installed for {label}. Install it in Windows Settings > Time & language > \
         Language & region > {setting} > Language options > Optical character recognition (on Turkish Windows: Ayarlar > \
         Saat ve dil > Dil ve bölge > {setting} > Dil seçenekleri > Optik karakter tanıma); add the language there first if \
         it is not listed"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_tag_wins() {
        assert_eq!(pick("tr", &["en-US", "tr"]), Some(1));
        assert_eq!(pick(" TR ", &["tr"]), Some(0), "trimmed, any case");
        assert_eq!(pick("en_US", &["en-US"]), Some(0), "an underscore is a hyphen");
        assert_eq!(pick("en-us", &["en-GB", "en-US"]), Some(1), "the exact region before another one");
    }

    #[test]
    fn a_narrower_tag_reads_a_wider_one() {
        assert_eq!(pick("en", &["tr", "en-US"]), Some(1));
        assert_eq!(pick("zh-Hans", &["zh-Hant-TW", "zh-Hans-CN"]), Some(1));
        assert_eq!(pick("zh", &["zh-Hans-CN"]), Some(0), "a bare language takes any of its tags");
    }

    #[test]
    fn another_region_reads_the_same_language_but_another_script_does_not() {
        assert_eq!(pick("en-GB", &["en-US"]), Some(0));
        assert_eq!(pick("pt-PT", &["pt-BR"]), Some(0));
        assert_eq!(pick("zh-Hans", &["zh-Hant-TW"]), None, "Traditional never reads Simplified");
        assert_eq!(pick("sr-Latn", &["sr-Cyrl-RS"]), None);
        assert_eq!(pick("zh-CN", &["zh-Hant-TW"]), None, "a script-bearing tag is no region variant");
    }

    #[test]
    fn nothing_installed_reads_other_languages_or_junk() {
        assert_eq!(pick("en", &["tr"]), None);
        assert_eq!(pick("tlh", &["tr", "en-US"]), None);
        assert_eq!(pick("", &["tr"]), None);
        assert_eq!(pick("t", &["tr"]), None, "a prefix of a language is not that language");
        assert_eq!(pick("tr;rm -rf", &["tr"]), None, "junk is no tag");
        assert_eq!(pick(&"x".repeat(36), &["tr"]), None);
        assert_eq!(pick("tr", &[]), None);
    }

    #[test]
    fn plausible_codes_are_short_tag_characters() {
        for ok in ["tr", "en-US", "zh-Hans-CN", "en_US", "tlh"] {
            assert!(plausible(ok), "{ok}");
        }
        for bad in ["", "tr ", "tr\n", "tr/../x", "ç", &"a".repeat(36)] {
            assert!(!plausible(bad), "{bad:?}");
        }
    }

    /// The message names the language (its Windows name and its code) and the setting that
    /// installs it, in both languages of the settings; junk is not echoed.
    #[test]
    fn the_missing_language_message_says_what_to_install_and_where() {
        let message = missing("tr", Some("Türkçe")).to_string();
        for part in [
            "Türkçe (tr)",
            "Language & region > Türkçe > Language options > Optical character recognition",
            "Ayarlar > Saat ve dil > Dil ve bölge > Türkçe > Dil seçenekleri > Optik karakter tanıma",
        ] {
            assert!(message.contains(part), "{part:?} missing from {message}");
        }
        let unnamed = missing("tlh", None).to_string();
        assert!(unnamed.contains("\"tlh\"") && unnamed.contains("Dil ve bölge > tlh >"), "{unnamed}");
        let same = missing("tlh", Some("tlh")).to_string();
        assert!(same.contains("\"tlh\""), "a name that is only the code is not repeated: {same}");
        let junk = missing("tr\"; echo", None).to_string();
        assert!(!junk.contains("echo") && junk.contains("\"?\""), "{junk}");
        assert!(matches!(missing("tr", None), OcrError::Recognize(_)));
    }
}
