//! Font service: finds system font files for the families a deck uses.
//!
//! Office decks name Windows fonts (Calibri, 微軟正黑體, ...) that macOS does not
//! ship, so each request goes through a substitution table first.

use std::collections::HashSet;
use std::sync::Mutex;

use fontique::{Collection, CollectionOptions, SourceKind};
use serde::Serialize;

pub struct FontService {
    collection: Mutex<Collection>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedFont {
    /// Family the editor should use for the requested name.
    pub family: String,
    /// Font files of that family (several when bold/italic are separate files).
    pub paths: Vec<String>,
}

/// Windows/Office families → (metric-compatible or look-alike) macOS families, in preference order.
fn substitutes(requested: &str) -> &'static [&'static str] {
    match requested.to_lowercase().as_str() {
        "calibri" | "calibri light" | "aptos" | "aptos display" | "aptos narrow" | "segoe ui" | "segoe ui light"
        | "segoe ui semibold" | "tahoma" | "verdana" | "corbel" | "candara" => &["Carlito", "Helvetica Neue"],
        "cambria" | "cambria math" | "constantia" => &["Caladea", "Times New Roman", "Times"],
        "consolas" | "courier new" => &["Menlo", "Courier"],
        "微軟正黑體" | "microsoft jhenghei" | "microsoft jhenghei ui" | "microsoft jhenghei light" | "新細明體" | "細明體"
        | "pmingliu" | "mingliu" | "標楷體" | "dfkai-sb" => &["PingFang TC"],
        "微软雅黑" | "microsoft yahei" | "microsoft yahei ui" | "等线" | "dengxian" | "宋体" | "simsun" | "黑体" | "simhei" => {
            &["PingFang SC"]
        }
        "meiryo" | "meiryo ui" | "yu gothic" | "yu gothic ui" | "游ゴシック" | "ms gothic" | "ms pgothic" | "ｍｓ ｐゴシック" => {
            &["Hiragino Sans", "Hiragino Kaku Gothic ProN"]
        }
        "malgun gothic" | "맑은 고딕" => &["Apple SD Gothic Neo"],
        "思源黑体" | "思源黑体 cn" | "思源黑體" | "source han sans" | "source han sans cn" | "source han sans sc" | "noto sans cjk sc"
        | "noto sans sc" | "苹方" | "苹方-简" | "pingfang" | "oppo sans" | "opposans" | "harmonyos sans sc" => &["PingFang SC"],
        "source han sans tc" | "source han sans tw" | "noto sans cjk tc" | "noto sans tc" | "思源黑體 tc" | "蘋方" | "蘋方-繁" => {
            &["PingFang TC"]
        }
        _ => &[],
    }
}

/// Weight words that end face names, longest first so `ExtraBold` wins over `Bold`.
/// Only used to find the family; PowerPoint draws a missing `思源黑体 CN Bold`
/// at regular weight, so the weight itself is not applied.
const WEIGHT_SUFFIXES: &[(&str, u16)] = &[
    ("extralight", 200),
    ("ultralight", 200),
    ("extrabold", 800),
    ("ultrabold", 800),
    ("semibold", 600),
    ("demibold", 600),
    ("regular", 400),
    ("medium", 500),
    ("normal", 400),
    ("heavy", 900),
    ("black", 900),
    ("light", 300),
    ("bold", 700),
    ("thin", 100),
    ("特粗", 900),
    ("中粗", 600),
    ("中黑", 500),
    ("粗体", 700),
    ("粗體", 700),
    ("细体", 300),
    ("細體", 300),
    ("极细", 200),
    ("纤细", 100),
    ("常规", 400),
    ("中等", 500),
    // OPPOSans style suffixes: R(egular), M(edium), B(old), H(eavy), L(ight).
    ("r", 400),
    ("m", 500),
    ("b", 700),
    ("h", 900),
    ("l", 300),
];

/// Splits `思源黑体 CN Bold` into (`思源黑体 CN`, 700). Single-letter suffixes need a separator.
fn split_weight(name: &str) -> Option<(&str, u16)> {
    let lower = name.to_lowercase();
    WEIGHT_SUFFIXES.iter().find_map(|&(suffix, weight)| {
        let base = lower.strip_suffix(suffix)?;
        let trimmed = base.trim_end_matches([' ', '-', '_']);
        let separated = trimmed.len() < base.len();
        if trimmed.is_empty() || (suffix.len() == 1 && !separated) {
            return None;
        }
        // `lower` and `name` share byte offsets for the ASCII/CJK names handled here.
        name.get(..trimmed.len()).map(|b| (b, weight))
    })
}

fn looks_cjk(name: &str) -> bool {
    name.chars().any(|c| ('\u{2E80}'..='\u{9FFF}').contains(&c) || ('\u{AC00}'..='\u{D7AF}').contains(&c))
}

impl Default for FontService {
    fn default() -> Self {
        Self::new()
    }
}

impl FontService {
    pub fn new() -> Self {
        Self { collection: Mutex::new(Collection::new(CollectionOptions { shared: false, system_fonts: true })) }
    }

    fn files_of(collection: &mut Collection, family: &str) -> Option<(String, Vec<String>)> {
        let info = collection.family_by_name(family)?;
        let mut seen = HashSet::new();
        let paths: Vec<String> = info
            .fonts()
            .iter()
            .filter_map(|f| match f.source().kind() {
                SourceKind::Path(p) => Some(p.to_string_lossy().into_owned()),
                SourceKind::Memory(_) => None,
            })
            .filter(|p| seen.insert(p.clone()))
            .collect();
        (!paths.is_empty()).then(|| (info.name().to_string(), paths))
    }

    /// Installed font families for the font picker, sorted, without hidden system faces.
    pub fn families(&self) -> Vec<String> {
        let mut collection = self.collection.lock().expect("font collection lock");
        let mut names: Vec<String> = collection.family_names().filter(|n| !n.starts_with('.')).map(str::to_string).collect();
        names.sort_by_key(|n| n.to_lowercase());
        names.dedup();
        names
    }

    /// Resolves `requested` to installed font files, applying substitutions and
    /// falling back to a default sans-serif (CJK-aware) family.
    pub fn resolve(&self, requested: &str) -> Option<ResolvedFont> {
        let mut collection = self.collection.lock().expect("font collection lock");
        if let Some((family, paths)) = Self::files_of(&mut collection, requested) {
            return Some(ResolvedFont { family, paths });
        }
        // A face name (family + weight) that is not installed: substitute by its family.
        let base = split_weight(requested).map_or(requested, |(b, _)| b);
        let fallback: &[&str] = if looks_cjk(requested) { &["PingFang TC"] } else { &["Helvetica Neue"] };
        std::iter::once(base)
            .filter(|b| *b != requested)
            .chain(substitutes(base).iter().copied())
            .chain(fallback.iter().copied())
            .find_map(|name| Self::files_of(&mut collection, name))
            .map(|(family, paths)| ResolvedFont { family, paths })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_installed_and_substituted_families() {
        let fonts = FontService::new();
        let helvetica = fonts.resolve("Helvetica").unwrap();
        assert_eq!(helvetica.family, "Helvetica");
        assert!(helvetica.paths[0].ends_with(".ttc"));

        let jhenghei = fonts.resolve("微軟正黑體").unwrap();
        assert_eq!(jhenghei.family, "PingFang TC");

        let calibri = fonts.resolve("Calibri").unwrap();
        assert!(calibri.family == "Carlito" || calibri.family == "Helvetica Neue", "{}", calibri.family);

        let unknown = fonts.resolve("No Such Font 123").unwrap();
        assert_eq!(unknown.family, "Helvetica Neue");

        assert_eq!(fonts.resolve("思源黑体 CN Bold").unwrap().family, "PingFang SC");
        assert_eq!(fonts.resolve("苹方 特粗").unwrap().family, "PingFang SC");
    }

    #[test]
    fn splits_weight_suffixes() {
        assert_eq!(split_weight("Segoe UI Semibold"), Some(("Segoe UI", 600)));
        assert_eq!(split_weight("思源黑体 CN Normal"), Some(("思源黑体 CN", 400)));
        assert_eq!(split_weight("等线 Light"), Some(("等线", 300)));
        assert_eq!(split_weight("OPPOSans R"), Some(("OPPOSans", 400)));
        assert_eq!(split_weight("苹方特粗"), Some(("苹方", 900)));
        assert_eq!(split_weight("Arial"), None);
        // A trailing letter that is part of the name is not a weight.
        assert_eq!(split_weight("Corbel"), None);
    }
}
