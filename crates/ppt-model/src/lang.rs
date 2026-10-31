//! The interface language, for the few strings the core writes or draws itself
//! (placeholder prompts, the new-deck template). Set once by the host at startup.

use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    ZhHant,
    ZhHans,
    En,
}

static LANG: AtomicU8 = AtomicU8::new(0);

impl Lang {
    /// Parses the host's language code (`zh-Hant`, `zh-Hans`, `en`); anything else is Traditional Chinese.
    pub fn from_code(code: &str) -> Lang {
        match code {
            "zh-Hans" => Lang::ZhHans,
            "en" => Lang::En,
            _ => Lang::ZhHant,
        }
    }

    /// Maps a BCP 47 tag to a supported language. Same rules as the frontend's
    /// `matchLang`: the script decides (zh-Hans-TW is Simplified), then the
    /// region's usual script.
    pub fn from_tag(tag: &str) -> Option<Lang> {
        let t = tag.to_ascii_lowercase();
        if !t.starts_with("zh") {
            return t.starts_with("en").then_some(Lang::En);
        }
        if t.contains("hans") {
            return Some(Lang::ZhHans);
        }
        if t.contains("hant") {
            return Some(Lang::ZhHant);
        }
        let simplified = t.split(['-', '_']).skip(1).any(|p| matches!(p, "cn" | "sg" | "my"));
        Some(if simplified { Lang::ZhHans } else { Lang::ZhHant })
    }

    /// The first supported language among the user's preferred `tags`, else English.
    pub fn detect<'a>(tags: impl IntoIterator<Item = &'a str>) -> Lang {
        tags.into_iter().find_map(Lang::from_tag).unwrap_or(Lang::En)
    }

    /// The `lang` attribute for text runs in this language.
    pub fn tag(self) -> &'static str {
        match self {
            Lang::ZhHant => "zh-TW",
            Lang::ZhHans => "zh-CN",
            Lang::En => "en-US",
        }
    }

    /// Picks the string for this language.
    pub fn pick(
        self,
        zh_hant: &'static str,
        zh_hans: &'static str,
        en: &'static str,
    ) -> &'static str {
        match self {
            Lang::ZhHant => zh_hant,
            Lang::ZhHans => zh_hans,
            Lang::En => en,
        }
    }
}

pub fn set_lang(lang: Lang) {
    LANG.store(lang as u8, Ordering::Relaxed);
}

pub fn lang() -> Lang {
    match LANG.load(Ordering::Relaxed) {
        1 => Lang::ZhHans,
        2 => Lang::En,
        _ => Lang::ZhHant,
    }
}

/// The message for the current language, with each `{}` replaced by the next of `args`.
pub fn tr(zh_hant: &str, zh_hans: &str, en: &str, args: &[&dyn std::fmt::Display]) -> String {
    let template = match lang() {
        Lang::ZhHant => zh_hant,
        Lang::ZhHans => zh_hans,
        Lang::En => en,
    };
    let mut out = String::with_capacity(template.len());
    let mut args = args.iter();
    let mut rest = template;
    while let Some(i) = rest.find("{}") {
        out.push_str(&rest[..i]);
        if let Some(a) = args.next() {
            out.push_str(&a.to_string());
        }
        rest = &rest[i + 2..];
    }
    out.push_str(rest);
    out
}

/// `one` when `n` is 1, else `other` (English plurals; Chinese passes the same text twice).
pub fn plural<'a>(n: usize, one: &'a str, other: &'a str) -> &'a str {
    if n == 1 { one } else { other }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_tags_like_the_frontend() {
        assert_eq!(Lang::from_tag("zh-Hant-TW"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("zh-Hans-TW"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_tag("zh-CN"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_tag("zh_SG"), Some(Lang::ZhHans));
        assert_eq!(Lang::from_tag("zh-HK"), Some(Lang::ZhHant));
        assert_eq!(Lang::from_tag("en-GB"), Some(Lang::En));
        assert_eq!(Lang::from_tag("ja-JP"), None);
        assert_eq!(Lang::detect(["ja-JP", "zh-Hans-CN", "en"]), Lang::ZhHans);
        assert_eq!(Lang::detect(["fr-FR"]), Lang::En);
    }
}
