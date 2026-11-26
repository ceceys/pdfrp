//! Pdfrp: the engine's most frequent error messages in the interface language.
//!
//! The engine writes its errors in English. The ones the interface shows most often are matched
//! here against their English wording, with `{name}` where a part varies (a file's reason, a
//! character, an action); the varying part is kept as written, and translated too when it is
//! itself one of these messages. Each wording is a catalog key, so a language without it shows the
//! English. A message that matches none stays in English: [`explained`] puts a sentence in the
//! interface language before it, and a template that already says what failed ("Couldn't open
//! {name}: {e}") gives it one of its own. If the engine's wording changes, the message is shown in
//! English again (`known_messages_match_what_the_engine_writes` checks the wordings that matter).

use crate::i18n;

/// The engine's messages, as it writes them, with `{name}` where a part varies.
pub(crate) const KNOWN: &[&str] = &[
    // Opening.
    "the file is not a readable PDF: {reason}",
    "it is password-protected",
    "the document is protected by a password",
    "the password is wrong",
    "the password is incorrect",
    "no such document",
    // Security and signatures.
    "its security settings don't allow copying pages",
    "the document's security settings don't allow {action}; open it with the owner password to make this change",
    "this document can't be edited: {reason}",
    "{action} isn't possible in a signed document: it would rewrite the file and invalidate the signatures",
    "this document is signed: rewriting it would invalidate its signatures (save it incrementally instead)",
    // Saving and editing.
    "the edited document could not be written: {reason}",
    "the edited document could not be reopened: {reason}",
    "couldn't use {what}",
    "nothing to undo",
    "nothing to redo",
    "cancelled",
    "the page's content changed",
    "there is nothing to find",
    "page {page} has no paragraph {paragraph}",
    "justification of mixed-style existing paragraphs is not supported",
    "the paragraph can't be moved that far",
    // Text the fonts can't draw.
    "the fonts can't draw \"{char}\" ({code}) in {what}; Latin (Turkish included), Greek and Cyrillic letters can be written there",
    "the fonts can't draw \"{char}\" ({code}); only Latin (Turkish included), Greek and Cyrillic characters can be added as text for now",
    "the standard fonts can't draw \"{char}\" ({code}) in {what}; only Western European characters can be written there for now",
    "\"{text}\" has characters neither {font} nor {standard} can show",
    "\"{text}\" can't be written into the PDF yet: Fill & Sign text takes Latin, Greek and Cyrillic letters only ({chars}); pick a numeric format such as dd.mm.yyyy",
];

/// The values of `template`'s `{name}` parts when `message` is that template, else `None`. Each
/// part takes the text up to the first place the literal after it follows; a part is never empty.
fn capture(template: &str, message: &str) -> Option<Vec<(String, String)>> {
    let mut values = Vec::new();
    let mut rest_t = template;
    let mut rest_m = message;
    // The literal before the first part.
    let (lead, after) = rest_t.split_once('{').map_or((rest_t, ""), |(a, b)| (a, b));
    rest_m = rest_m.strip_prefix(lead)?;
    if after.is_empty() && !template.contains('{') {
        return rest_m.is_empty().then_some(values);
    }
    rest_t = after;
    loop {
        let (name, after_name) = rest_t.split_once('}')?;
        let (literal, next) = match after_name.split_once('{') {
            Some((literal, next)) => (literal, Some(next)),
            None => (after_name, None),
        };
        let value = if literal.is_empty() {
            // The last part takes the rest.
            if next.is_some() {
                return None;
            }
            std::mem::take(&mut rest_m)
        } else if next.is_none() {
            // The last literal ends the message.
            let value = rest_m.strip_suffix(literal)?;
            rest_m = "";
            value
        } else {
            let at = rest_m.find(literal)?;
            let value = &rest_m[..at];
            rest_m = &rest_m[at + literal.len()..];
            value
        };
        if value.is_empty() {
            return None;
        }
        values.push((name.to_owned(), value.to_owned()));
        match next {
            Some(next) => rest_t = next,
            None => return rest_m.is_empty().then_some(values),
        }
    }
}

/// `message` in the interface language when it is one of [`KNOWN`] and the language has it, its
/// varying parts kept (and translated when they are such a message too); unchanged otherwise.
pub(crate) fn localized(message: &str) -> String {
    localized_depth(message, 2)
}

fn localized_depth(message: &str, depth: u8) -> String {
    let lang = i18n::current();
    if lang == i18n::Lang::EN || depth == 0 {
        return message.to_owned();
    }
    for template in KNOWN {
        if !i18n::has(lang, template) {
            continue;
        }
        if let Some(values) = capture(template, message) {
            let values: Vec<(String, String)> = values.into_iter().map(|(k, v)| (k, localized_depth(&v, depth - 1))).collect();
            let args: Vec<(&str, &str)> = values.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
            return i18n::fmt(i18n::tr(lang, template), &args);
        }
    }
    message.to_owned()
}

/// [`localized`], and for a message the interface language has no wording for, a sentence in that
/// language before the English: for an error shown on its own.
pub(crate) fn explained(message: &str) -> String {
    let shown = localized(message);
    if shown != message || i18n::current() == i18n::Lang::EN {
        return shown;
    }
    i18n::fmt(tl!("The operation couldn't be completed. Details: {e}"), &[("e", message)])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn templates_capture_their_varying_parts() {
        assert_eq!(
            capture("the file is not a readable PDF: {reason}", "the file is not a readable PDF: no page tree"),
            Some(vec![("reason".into(), "no page tree".into())])
        );
        assert_eq!(capture("nothing to undo", "nothing to undo"), Some(vec![]));
        assert_eq!(capture("nothing to undo", "nothing to undo now"), None);
        let v = capture("page {page} has no paragraph {paragraph}", "page 3 has no paragraph 12").unwrap();
        assert_eq!(v, vec![("page".into(), "3".into()), ("paragraph".into(), "12".into())]);
        let fonts = KNOWN.iter().find(|t| t.starts_with("the fonts can't draw") && t.contains("in {what}")).unwrap();
        let v = capture(
            fonts,
            "the fonts can't draw \"签\" (U+7B7E) in the comment; Latin (Turkish included), Greek and Cyrillic letters can be written there",
        )
        .unwrap();
        assert_eq!(v, vec![("char".into(), "签".into()), ("code".into(), "U+7B7E".into()), ("what".into(), "the comment".into())]);
        assert_eq!(capture("couldn't use {what}", "couldn't use "), None, "a part is never empty");
    }

    /// The wordings that matter are the engine's own: if one changes, this says so before users
    /// see English again.
    #[test]
    fn known_messages_match_what_the_engine_writes() {
        let engine = [
            migez_engine::EditError::NothingToUndo.to_string(),
            migez_engine::EditError::Signed.to_string(),
            migez_engine::EditError::ReadOnly("it is a portfolio".into()).to_string(),
            migez_engine::EditError::SignedRewrite("Optimize".into()).to_string(),
            migez_engine::SourceProblem::Password.to_string(),
            migez_fonts::bundled::undrawable_message("the comment", '签'),
            migez_engine::undrawable_message("the reason", '签'),
        ];
        for message in engine {
            assert!(KNOWN.iter().any(|t| capture(t, &message).is_some()), "{message}");
        }
    }

    /// In English nothing changes; another language uses its catalog, keeping the varying part
    /// (translated too when it is a known message), and a message it has no wording for is
    /// introduced in that language. A fixture catalog, so the test doesn't hang on the wording.
    #[test]
    fn messages_follow_the_interface_language() {
        i18n::set_current(i18n::Lang::EN);
        assert_eq!(localized("nothing to undo"), "nothing to undo");
        assert_eq!(explained("an odd failure"), "an odd failure");
        let tr = i18n::fixture_lang(
            "tr",
            "\tnothing to undo\tgeri alınacak bir şey yok\n\
             \tthe edited document could not be written: {reason}\tdüzenlenen belge yazılamadı: {reason}\n\
             \tthe document is protected by a password\tbelge bir parola ile korunuyor\n\
             \tThe operation couldn't be completed. Details: {e}\tİşlem tamamlanamadı. Ayrıntı: {e}\n",
        );
        i18n::set_current(tr);
        assert_eq!(localized("nothing to undo"), "geri alınacak bir şey yok");
        assert_eq!(localized("the edited document could not be written: disk full"), "düzenlenen belge yazılamadı: disk full");
        assert_eq!(
            localized("the edited document could not be written: the document is protected by a password"),
            "düzenlenen belge yazılamadı: belge bir parola ile korunuyor",
            "a known message inside another is translated too"
        );
        // A template the language has no wording for stays English.
        assert_eq!(localized("nothing to redo"), "nothing to redo");
        assert_eq!(explained("an odd failure"), "İşlem tamamlanamadı. Ayrıntı: an odd failure");
        assert_eq!(explained("nothing to undo"), "geri alınacak bir şey yok", "a known message needs no introduction");
        i18n::set_current(i18n::Lang::EN);
    }
}
