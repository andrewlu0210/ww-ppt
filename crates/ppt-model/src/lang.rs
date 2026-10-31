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
