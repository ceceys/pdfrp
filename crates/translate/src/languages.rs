//! The languages' names, in English and in Turkish: our own table, not a product's string list.
//! The interface shows the Turkish names when it is in Turkish and the English names otherwise.
//! `tests.rs` checks that every language of the embedded manifest has a row here, and no other.

use std::cmp::Ordering;

/// One language: its code as the manifest writes it, and its names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Language {
    pub code: &'static str,
    pub english: &'static str,
    pub turkish: &'static str,
}

impl Language {
    /// The name in Turkish (`turkish`) or English.
    pub fn name(&self, turkish: bool) -> &'static str {
        if turkish { self.turkish } else { self.english }
    }
}

/// English: every translation starts or ends in it, or passes through it.
pub const ENGLISH: Language = Language { code: "en", english: "English", turkish: "İngilizce" };

/// The other languages, by code.
pub const LANGUAGES: &[Language] = &[
    Language { code: "af", english: "Afrikaans", turkish: "Afrikaanca" },
    Language { code: "ar", english: "Arabic", turkish: "Arapça" },
    Language { code: "az", english: "Azerbaijani", turkish: "Azerbaycan Türkçesi" },
    Language { code: "be", english: "Belarusian", turkish: "Belarusça" },
    Language { code: "bg", english: "Bulgarian", turkish: "Bulgarca" },
    Language { code: "bn", english: "Bengali", turkish: "Bengalce" },
    Language { code: "bs", english: "Bosnian", turkish: "Boşnakça" },
    Language { code: "ca", english: "Catalan", turkish: "Katalanca" },
    Language { code: "cs", english: "Czech", turkish: "Çekçe" },
    Language { code: "da", english: "Danish", turkish: "Danca" },
    Language { code: "de", english: "German", turkish: "Almanca" },
    Language { code: "el", english: "Greek", turkish: "Yunanca" },
    Language { code: "es", english: "Spanish", turkish: "İspanyolca" },
    Language { code: "et", english: "Estonian", turkish: "Estonca" },
    Language { code: "eu", english: "Basque", turkish: "Baskça" },
    Language { code: "fa", english: "Persian", turkish: "Farsça" },
    Language { code: "fi", english: "Finnish", turkish: "Fince" },
    Language { code: "fr", english: "French", turkish: "Fransızca" },
    Language { code: "gl", english: "Galician", turkish: "Galiçyaca" },
    Language { code: "gu", english: "Gujarati", turkish: "Güceratça" },
    Language { code: "he", english: "Hebrew", turkish: "İbranice" },
    Language { code: "hi", english: "Hindi", turkish: "Hintçe" },
    Language { code: "hr", english: "Croatian", turkish: "Hırvatça" },
    Language { code: "hu", english: "Hungarian", turkish: "Macarca" },
    Language { code: "id", english: "Indonesian", turkish: "Endonezce" },
    Language { code: "is", english: "Icelandic", turkish: "İzlandaca" },
    Language { code: "it", english: "Italian", turkish: "İtalyanca" },
    Language { code: "ja", english: "Japanese", turkish: "Japonca" },
    Language { code: "kn", english: "Kannada", turkish: "Kannada dili" },
    Language { code: "ko", english: "Korean", turkish: "Korece" },
    Language { code: "lt", english: "Lithuanian", turkish: "Litvanca" },
    Language { code: "lv", english: "Latvian", turkish: "Letonca" },
    Language { code: "ml", english: "Malayalam", turkish: "Malayalam dili" },
    Language { code: "mr", english: "Marathi", turkish: "Marathi dili" },
    Language { code: "ms", english: "Malay", turkish: "Malayca" },
    Language { code: "nb", english: "Norwegian Bokmål", turkish: "Norveççe (Bokmål)" },
    Language { code: "nl", english: "Dutch", turkish: "Felemenkçe" },
    Language { code: "pl", english: "Polish", turkish: "Lehçe" },
    Language { code: "pt", english: "Portuguese", turkish: "Portekizce" },
    Language { code: "ro", english: "Romanian", turkish: "Rumence" },
    Language { code: "ru", english: "Russian", turkish: "Rusça" },
    Language { code: "sk", english: "Slovak", turkish: "Slovakça" },
    Language { code: "sl", english: "Slovenian", turkish: "Slovence" },
    Language { code: "sq", english: "Albanian", turkish: "Arnavutça" },
    Language { code: "sr", english: "Serbian", turkish: "Sırpça" },
    Language { code: "sv", english: "Swedish", turkish: "İsveççe" },
    Language { code: "ta", english: "Tamil", turkish: "Tamilce" },
    Language { code: "te", english: "Telugu", turkish: "Telugu dili" },
    Language { code: "th", english: "Thai", turkish: "Tayca" },
    Language { code: "tl", english: "Tagalog", turkish: "Tagalogca" },
    Language { code: "tr", english: "Turkish", turkish: "Türkçe" },
    Language { code: "ug", english: "Uyghur", turkish: "Uygurca" },
    Language { code: "uk", english: "Ukrainian", turkish: "Ukraynaca" },
    Language { code: "ur", english: "Urdu", turkish: "Urduca" },
    Language { code: "vi", english: "Vietnamese", turkish: "Vietnamca" },
    Language { code: "zh-Hans", english: "Chinese (Simplified)", turkish: "Çince (Basitleştirilmiş)" },
    Language { code: "zh-Hant", english: "Chinese (Traditional)", turkish: "Çince (Geleneksel)" },
];

/// The language with this code, English included.
pub fn language(code: &str) -> Option<&'static Language> {
    if code == ENGLISH.code { Some(&ENGLISH) } else { LANGUAGES.iter().find(|l| l.code == code) }
}

/// The name of the language with this code, or the code itself for one not in the table.
pub fn name(code: &str, turkish: bool) -> &str {
    language(code).map_or(code, |l| l.name(turkish))
}

/// The letters of the Turkish alphabet in order; any other character sorts after them by its code
/// point.
const TURKISH_ORDER: &str = "aâbcçdefgğhıiîjklmnoöprsştuûüvyz";

/// One character's place: Turkish letters (either case) in alphabet order, then everything else.
fn place(c: char, turkish: bool) -> (u32, u32) {
    let lower = match c {
        'I' if turkish => 'ı',
        'İ' => 'i',
        c => c.to_lowercase().next().unwrap_or(c),
    };
    match TURKISH_ORDER.chars().position(|l| l == lower) {
        Some(i) if turkish => (0, i as u32),
        _ => (1, lower as u32),
    }
}

/// Orders names the way a reader of that language expects: in Turkish `Ç` after `C`, `İ` after
/// `I`, `Ş` after `S`; otherwise by lower case.
pub fn compare_names(a: &str, b: &str, turkish: bool) -> Ordering {
    let key = |s: &str| s.chars().map(|c| place(c, turkish)).collect::<Vec<_>>();
    key(a).cmp(&key(b)).then_with(|| a.cmp(b))
}
