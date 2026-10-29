//! Paragraph bullets: characters (possibly in a symbol font) and automatic numbering.

use crate::Color;

#[derive(Clone, Debug, PartialEq)]
pub struct Bullet {
    pub kind: BulletKind,
    /// `a:buFont`; `None` uses the text's font (`a:buFontTx`).
    pub font: Option<String>,
    /// `a:buClr`; `None` uses the text color.
    pub color: Option<Color>,
    /// `a:buSzPct` as a fraction of the text size; `None` is 100%.
    pub size: Option<f32>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum BulletKind {
    /// `a:buChar`, as written in the file (a symbol-font code for Wingdings etc.).
    Char(char),
    /// `a:buAutoNum`: a numbering scheme such as `arabicPeriod`, starting at `start`.
    AutoNum { scheme: String, start: u32 },
}

impl Bullet {
    /// A plain bullet character in the text's font.
    pub fn char(c: char) -> Self {
        Bullet { kind: BulletKind::Char(c), font: None, color: None, size: None }
    }

    pub fn is_numbered(&self) -> bool {
        matches!(self.kind, BulletKind::AutoNum { .. })
    }

    /// What to draw for the `n`th item (1-based, only used by numbering), and
    /// the font to draw it in (`None`: the text's font).
    pub fn label(&self, n: u32) -> (String, Option<&str>) {
        match &self.kind {
            BulletKind::Char(c) => match self.font.as_deref().and_then(|f| symbol_char(f, *c)) {
                // Symbol fonts are not installed on macOS: show the Unicode equivalent.
                Some(u) => (u.to_string(), None),
                None => (c.to_string(), self.font.as_deref()),
            },
            BulletKind::AutoNum { scheme, .. } => (number_label(scheme, n), self.font.as_deref().filter(|f| !is_symbol_font(f))),
        }
    }
}

pub fn is_symbol_font(font: &str) -> bool {
    matches!(font.to_lowercase().as_str(), "wingdings" | "wingdings 2" | "wingdings 3" | "symbol" | "webdings")
}

/// The Unicode character a symbol-font bullet stands for (`None` for regular fonts).
pub fn symbol_char(font: &str, c: char) -> Option<char> {
    if !is_symbol_font(font) {
        return None;
    }
    // Symbol fonts are often addressed through the U+F0xx private-use block.
    let code = match c as u32 {
        0xF020..=0xF0FF => c as u32 - 0xF000,
        v => v,
    };
    let font = font.to_lowercase();
    let mapped = match (font.as_str(), code) {
        ("symbol", 0xB7) => '•',
        ("symbol", 0xDE) => '⇒',
        ("symbol", 0xAE) => '→',
        ("symbol", 0xA8) => '♦',
        ("wingdings 2", 0x50) => '✓',
        ("wingdings 2", 0x4F) => '✗',
        ("wingdings 2", 0x97) => '◆',
        ("wingdings 2", 0xF8) => '✱',
        ("wingdings 3", 0x7D) => '▶',
        ("wingdings 3", 0x75) => '▸',
        ("wingdings 3", 0xC6) => '➔',
        (_, 0x6C) => '●',
        (_, 0x6E) => '■',
        (_, 0x6F) => '□',
        (_, 0x71) => '❑',
        (_, 0x75) => '◆',
        (_, 0x76) => '❖',
        (_, 0x77) => '⬥',
        (_, 0x9F) => '•',
        (_, 0xA7) => '▪',
        (_, 0xA8) => '◻',
        (_, 0xD8) => '➢',
        (_, 0xE0) => '➔',
        (_, 0xE8) => '➔',
        (_, 0xF0) => '⇨',
        (_, 0xFB) => '✗',
        (_, 0xFC) => '✔',
        (_, 0xFE) => '☑',
        (_, 0x4A) => '☺',
        _ => '•',
    };
    Some(mapped)
}

fn roman(mut n: u32) -> String {
    const TABLE: [(u32, &str); 13] =
        [(1000, "M"), (900, "CM"), (500, "D"), (400, "CD"), (100, "C"), (90, "XC"), (50, "L"), (40, "XL"), (10, "X"), (9, "IX"), (5, "V"), (4, "IV"), (1, "I")];
    let mut out = String::new();
    for (v, s) in TABLE {
        while n >= v {
            out.push_str(s);
            n -= v;
        }
    }
    out
}

/// a, b, ..., z, aa, bb, ... like PowerPoint.
fn alpha(n: u32) -> String {
    let n = n.max(1) - 1;
    let letter = (b'a' + (n % 26) as u8) as char;
    letter.to_string().repeat((n / 26 + 1) as usize)
}

/// Chinese numerals for 1..=99 (一, 二, ..., 十, 十一, ..., 九十九).
fn chinese(n: u32) -> String {
    const DIGITS: [&str; 10] = ["〇", "一", "二", "三", "四", "五", "六", "七", "八", "九"];
    match n {
        0..=9 => DIGITS[n as usize].to_string(),
        10..=19 => format!("十{}", if n > 10 { DIGITS[(n % 10) as usize] } else { "" }),
        20..=99 => format!("{}十{}", DIGITS[(n / 10) as usize], if !n.is_multiple_of(10) { DIGITS[(n % 10) as usize] } else { "" }),
        _ => n.to_string(),
    }
}

/// The label of item `n` in an `a:buAutoNum` scheme (ST_TextAutonumberScheme).
pub fn number_label(scheme: &str, n: u32) -> String {
    let circled = |base: u32, limit: u32| if (1..=limit).contains(&n) { char::from_u32(base + n - 1).map(String::from) } else { None };
    let (body, style) = match scheme {
        s if s.starts_with("arabic") => (n.to_string(), &s["arabic".len()..]),
        s if s.starts_with("romanUc") => (roman(n), &s["romanUc".len()..]),
        s if s.starts_with("romanLc") => (roman(n).to_lowercase(), &s["romanLc".len()..]),
        s if s.starts_with("alphaUc") => (alpha(n).to_uppercase(), &s["alphaUc".len()..]),
        s if s.starts_with("alphaLc") => (alpha(n), &s["alphaLc".len()..]),
        "circleNumDbPlain" | "circleNumWdWhitePlain" => return circled(0x2460, 20).unwrap_or_else(|| n.to_string()),
        "circleNumWdBlackPlain" => return circled(0x2776, 10).unwrap_or_else(|| n.to_string()),
        "ea1ChsPeriod" | "ea1ChtPeriod" | "ea1JpnChsDbPeriod" => return format!("{}.", chinese(n)),
        "ea1ChsPlain" | "ea1ChtPlain" => return chinese(n),
        "ea1JpnKorPeriod" => return format!("{n}."),
        _ => (n.to_string(), "Period"),
    };
    match style {
        "ParenBoth" => format!("({body})"),
        "ParenR" => format!("{body})"),
        "Plain" => body,
        "Minus" => format!("- {body} -"),
        "DbPeriod" => format!("{body}．"),
        _ => format!("{body}."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_schemes() {
        assert_eq!(number_label("arabicPeriod", 3), "3.");
        assert_eq!(number_label("arabicParenR", 2), "2)");
        assert_eq!(number_label("arabicParenBoth", 12), "(12)");
        assert_eq!(number_label("romanUcPeriod", 4), "IV.");
        assert_eq!(number_label("romanLcParenR", 9), "ix)");
        assert_eq!(number_label("alphaLcPeriod", 28), "bb.");
        assert_eq!(number_label("alphaUcParenBoth", 1), "(A)");
        assert_eq!(number_label("circleNumDbPlain", 3), "③");
        assert_eq!(number_label("ea1ChtPeriod", 12), "十二.");
        assert_eq!(number_label("unknownScheme", 5), "5.");
    }

    #[test]
    fn symbol_fonts_map_to_unicode() {
        let b = Bullet { kind: BulletKind::Char('l'), font: Some("Wingdings".into()), color: None, size: None };
        assert_eq!(b.label(1), ("●".to_string(), None));
        assert_eq!(symbol_char("Wingdings", '\u{F0D8}'), Some('➢'));
        assert_eq!(symbol_char("Arial", 'l'), None);
        let plain = Bullet { kind: BulletKind::Char('✪'), font: Some("系统字体常规体".into()), color: None, size: None };
        assert_eq!(plain.label(1), ("✪".to_string(), Some("系统字体常规体")));
    }
}
