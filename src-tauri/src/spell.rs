//! Spelling through macOS's system spell checker (NSSpellChecker), with the
//! user's languages and dictionary. Ranges are in UTF-16 units, as the
//! frontend counts them. Commands are synchronous so they run on the main
//! thread, where AppKit expects them.

#[cfg(target_os = "macos")]
mod mac {
    use std::sync::OnceLock;

    use objc2_app_kit::NSSpellChecker;
    use objc2_foundation::{NSRange, NSString};

    /// One spell document for the app: words ignored with "忽略" stay ignored this session.
    fn tag() -> isize {
        static TAG: OnceLock<isize> = OnceLock::new();
        *TAG.get_or_init(NSSpellChecker::uniqueSpellDocumentTag)
    }

    pub fn check(text: &str) -> Vec<[u32; 2]> {
        let checker = NSSpellChecker::sharedSpellChecker();
        checker.setAutomaticallyIdentifiesLanguages(true);
        let s = NSString::from_str(text);
        let len = s.length() as isize;
        let mut out = Vec::new();
        let mut start = 0isize;
        while start < len {
            // SAFETY: a null word count pointer is allowed.
            let r = unsafe { checker.checkSpellingOfString_startingAt_language_wrap_inSpellDocumentWithTag_wordCount(&s, start, None, false, tag(), std::ptr::null_mut()) };
            if r.length == 0 || r.location == usize::MAX || (r.location as isize) < start {
                break;
            }
            out.push([r.location as u32, r.length as u32]);
            start = (r.location + r.length) as isize;
        }
        out
    }

    pub fn guesses(text: &str, start: u32, len: u32) -> Vec<String> {
        let checker = NSSpellChecker::sharedSpellChecker();
        let s = NSString::from_str(text);
        let range = NSRange::new(start as usize, len as usize);
        checker
            .guessesForWordRange_inString_language_inSpellDocumentWithTag(range, &s, None, tag())
            .map(|a| a.iter().map(|w| w.to_string()).collect())
            .unwrap_or_default()
    }

    pub fn ignore(word: &str) {
        NSSpellChecker::sharedSpellChecker().ignoreWord_inSpellDocumentWithTag(&NSString::from_str(word), tag());
    }

    pub fn learn(word: &str) {
        NSSpellChecker::sharedSpellChecker().learnWord(&NSString::from_str(word));
    }
}

#[cfg(not(target_os = "macos"))]
mod mac {
    pub fn check(_: &str) -> Vec<[u32; 2]> {
        Vec::new()
    }
    pub fn guesses(_: &str, _: u32, _: u32) -> Vec<String> {
        Vec::new()
    }
    pub fn ignore(_: &str) {}
    pub fn learn(_: &str) {}
}

/// Misspelled ranges `[start, length]` of each text.
#[tauri::command]
pub fn spell_check(texts: Vec<String>) -> Vec<Vec<[u32; 2]>> {
    texts.iter().map(|t| mac::check(t)).collect()
}

/// Suggested corrections for the word at `start..start + len` of `text`.
#[tauri::command]
pub fn spell_guesses(text: String, start: u32, len: u32) -> Vec<String> {
    mac::guesses(&text, start, len)
}

/// Stops flagging `word` for the rest of the session.
#[tauri::command]
pub fn spell_ignore(word: String) {
    mac::ignore(&word);
}

/// Adds `word` to the user's dictionary (shared with other apps).
#[tauri::command]
pub fn spell_learn(word: String) {
    mac::learn(&word);
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    #[test]
    fn flags_misspellings_in_utf16_units() {
        let text = "中文 This sentense has a mispelled word";
        let found = super::mac::check(text);
        let words: Vec<String> = found.iter().map(|[s, l]| String::from_utf16(&text.encode_utf16().collect::<Vec<_>>()[*s as usize..(*s + *l) as usize]).unwrap()).collect();
        assert!(words.contains(&"mispelled".to_string()), "{words:?}");
        assert!(!super::mac::guesses(text, found.last().unwrap()[0], found.last().unwrap()[1]).is_empty());
    }
}
