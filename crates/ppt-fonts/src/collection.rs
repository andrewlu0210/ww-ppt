//! Trimming large font collections (`.ttc`) to the weights slides use.
//!
//! macOS ships CJK families as collections holding every weight (PingFang:
//! six weights, 78 MB). Slides only use regular and bold text, so the editor
//! loads just the faces nearest to those two weights. Faces sharing tables
//! (PingFang's regional variants of one weight share their outlines) keep
//! sharing them in the trimmed collection.

use std::collections::HashMap;

/// Collections smaller than this are loaded whole.
const TRIM_ABOVE: usize = 20 << 20;

fn u16_at(d: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(d.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(d: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(d.get(at..at + 4)?.try_into().ok()?))
}

struct Face {
    /// Offset of the face's table directory.
    dir: usize,
    /// (tag, checksum, offset, length) of each table.
    tables: Vec<([u8; 4], u32, u32, u32)>,
    family: String,
    weight: u16,
}

fn face(d: &[u8], dir: usize) -> Option<Face> {
    let n = u16_at(d, dir + 4)? as usize;
    let mut tables = Vec::with_capacity(n);
    for i in 0..n {
        let r = dir + 12 + 16 * i;
        tables.push((d.get(r..r + 4)?.try_into().ok()?, u32_at(d, r + 4)?, u32_at(d, r + 8)?, u32_at(d, r + 12)?));
    }
    let table = |tag: &[u8; 4]| tables.iter().find(|t| &t.0 == tag).map(|t| t.2 as usize);
    let weight = table(b"OS/2").and_then(|o| u16_at(d, o + 4)).unwrap_or(400);
    // Typographic family (16), else family (1), from the Windows English names.
    let mut family = String::new();
    if let Some(name) = table(b"name") {
        let (count, strings) = (u16_at(d, name + 2)? as usize, u16_at(d, name + 4)? as usize);
        let mut found: HashMap<u16, String> = HashMap::new();
        for i in 0..count {
            let r = name + 6 + 12 * i;
            let (platform, lang, id, len, off) = (u16_at(d, r)?, u16_at(d, r + 4)?, u16_at(d, r + 6)?, u16_at(d, r + 8)? as usize, u16_at(d, r + 10)? as usize);
            if platform == 3 && lang == 0x409 && matches!(id, 1 | 2 | 16 | 17) {
                let at = name + strings + off;
                let units: Vec<u16> = d.get(at..at + len)?.as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
                found.insert(id, String::from_utf16_lossy(&units));
            }
        }
        family = found.remove(&16).or_else(|| found.remove(&1)).unwrap_or_default();
        // Some collections (Kaiti) give every face weight 400: the style name says better.
        if let Some(w) = found.remove(&17).or_else(|| found.remove(&2)).and_then(|style| style_weight(&style)) {
            return Some(Face { dir, tables, family, weight: w });
        }
    }
    Some(Face { dir, tables, family, weight })
}

/// Color emoji keep their bitmaps at many sizes (Apple Color Emoji: nine, 191 MB).
/// Kept: the strikes nearest to 32 and 64 pixels and the largest one; the
/// renderer scales the nearest larger strike down.
fn trim_sbix(table: &[u8], num_glyphs: usize) -> Option<Vec<u8>> {
    let count = u32_at(table, 4)? as usize;
    if count <= 3 {
        return None;
    }
    // (ppem, start, end) of each strike block.
    let mut strikes = Vec::with_capacity(count);
    for i in 0..count {
        let start = u32_at(table, 8 + 4 * i)? as usize;
        let ppem = u16_at(table, start)?;
        let end = start + u32_at(table, start + 4 + 4 * num_glyphs)? as usize;
        strikes.push((ppem, start, end.min(table.len())));
    }
    let nearest = |target: i32| strikes.iter().min_by_key(|s| (s.0 as i32 - target).abs()).map(|s| s.0);
    let largest = strikes.iter().map(|s| s.0).max();
    let keep: Vec<u16> = [nearest(32), nearest(64), largest].into_iter().flatten().collect();
    let kept: Vec<&(u16, usize, usize)> = strikes.iter().filter(|s| keep.contains(&s.0)).collect();
    // Header, then each strike block verbatim (its offsets are relative to itself).
    let mut out = table.get(0..4)?.to_vec();
    out.extend_from_slice(&(kept.len() as u32).to_be_bytes());
    let mut at = 8 + 4 * kept.len();
    let mut offsets = Vec::new();
    for s in &kept {
        offsets.push(at as u32);
        at += s.2 - s.1;
    }
    for o in offsets {
        out.extend_from_slice(&o.to_be_bytes());
    }
    for s in &kept {
        out.extend_from_slice(table.get(s.1..s.2)?);
    }
    Some(out)
}

/// The OpenType table checksum (sum of big-endian words).
fn checksum(table: &[u8]) -> u32 {
    table.chunks(4).fold(0u32, |sum, c| {
        let mut w = [0u8; 4];
        w[..c.len()].copy_from_slice(c);
        sum.wrapping_add(u32::from_be_bytes(w))
    })
}

/// The weight a style name stands for ("Bold", "Semibold", ...).
fn style_weight(style: &str) -> Option<u16> {
    let s = style.to_lowercase().replace([' ', '-'], "");
    let s = s.trim_end_matches("italic").trim_end_matches("oblique");
    Some(match s {
        "thin" | "hairline" => 100,
        "ultralight" | "extralight" => 200,
        "light" => 300,
        "" | "regular" | "normal" | "book" | "roman" => 400,
        "medium" => 500,
        "semibold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "ultrabold" | "heavy" => 800,
        "black" => 900,
        _ => return None,
    })
}

/// `data` with only the faces nearest to regular (400) and bold (700) weight of
/// each family, when it is a large collection; `None` to use the file as is.
pub fn trim_collection(data: &[u8]) -> Option<Vec<u8>> {
    if data.len() < TRIM_ABOVE || data.get(0..4)? != b"ttcf" {
        return None;
    }
    let count = u32_at(data, 8)? as usize;
    let faces: Vec<Face> = (0..count).filter_map(|i| face(data, u32_at(data, 12 + 4 * i)? as usize)).collect();
    if faces.len() != count {
        return None;
    }
    // Per family, the weights nearest to 400 and 700.
    let mut keep_weights: HashMap<&str, Vec<u16>> = HashMap::new();
    for f in &faces {
        keep_weights.entry(f.family.as_str()).or_default().push(f.weight);
    }
    for weights in keep_weights.values_mut() {
        let nearest = |target: i32| *weights.iter().min_by_key(|w| ((**w as i32) - target).abs()).expect("a family has faces");
        *weights = vec![nearest(400), nearest(700)];
    }
    let kept: Vec<&Face> = faces.iter().filter(|f| keep_weights[f.family.as_str()].contains(&f.weight)).collect();
    // Emoji bitmaps trimmed to fewer sizes, by original table position.
    let mut replaced: HashMap<(u32, u32), Vec<u8>> = HashMap::new();
    for f in &kept {
        let maxp = f.tables.iter().find(|t| &t.0 == b"maxp").and_then(|t| u16_at(data, t.2 as usize + 4));
        if let (Some(sbix), Some(glyphs)) = (f.tables.iter().find(|t| &t.0 == b"sbix"), maxp)
            && !replaced.contains_key(&(sbix.2, sbix.3))
            && let Some(t) = data.get(sbix.2 as usize..(sbix.2 + sbix.3) as usize).and_then(|t| trim_sbix(t, glyphs as usize))
        {
            replaced.insert((sbix.2, sbix.3), t);
        }
    }
    if kept.len() == faces.len() && replaced.is_empty() {
        return None;
    }

    // Layout: header, table directories, then each distinct table once.
    let header = 12 + 4 * kept.len();
    let dirs: usize = kept.iter().map(|f| 12 + 16 * f.tables.len()).sum();
    let mut out = vec![0u8; header + dirs];
    let mut placed: HashMap<(u32, u32), u32> = HashMap::new();
    let pad = |out: &mut Vec<u8>| out.resize(out.len().div_ceil(4) * 4, 0);
    let mut dir_at = header;
    out[0..4].copy_from_slice(b"ttcf");
    out[4..8].copy_from_slice(&0x0001_0000u32.to_be_bytes());
    out[8..12].copy_from_slice(&(kept.len() as u32).to_be_bytes());
    for (i, f) in kept.iter().enumerate() {
        out[12 + 4 * i..16 + 4 * i].copy_from_slice(&(dir_at as u32).to_be_bytes());
        // The directory header (sfnt version, counts, search fields) as it was.
        let head = data.get(f.dir..f.dir + 12)?.to_vec();
        out[dir_at..dir_at + 12].copy_from_slice(&head);
        for (k, (tag, sum, offset, length)) in f.tables.iter().enumerate() {
            let swap = replaced.get(&(*offset, *length));
            let (sum, length) = match swap {
                Some(t) => (checksum(t), t.len() as u32),
                None => (*sum, *length),
            };
            let new_offset = match placed.get(&(*offset, f.tables[k].3)) {
                Some(o) => *o,
                None => {
                    pad(&mut out);
                    let o = out.len() as u32;
                    match swap {
                        Some(t) => out.extend_from_slice(t),
                        None => out.extend_from_slice(data.get(*offset as usize..(*offset + length) as usize)?),
                    }
                    placed.insert((*offset, f.tables[k].3), o);
                    o
                }
            };
            let r = dir_at + 12 + 16 * k;
            out[r..r + 4].copy_from_slice(tag);
            out[r + 4..r + 8].copy_from_slice(&sum.to_be_bytes());
            out[r + 8..r + 12].copy_from_slice(&new_offset.to_be_bytes());
            out[r + 12..r + 16].copy_from_slice(&length.to_be_bytes());
        }
        dir_at += 12 + 16 * f.tables.len();
    }
    pad(&mut out);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pingfang_keeps_regular_and_semibold_of_every_region() {
        let Some(path) = crate::FontService::new().resolve("PingFang TC").and_then(|f| f.paths.into_iter().next()) else { return };
        let data = std::fs::read(&path).unwrap();
        let Some(trimmed) = trim_collection(&data) else { return };
        assert!(trimmed.len() < data.len() / 2, "{} → {}", data.len(), trimmed.len());
        println!("PingFang {} MB → {} MB", data.len() >> 20, trimmed.len() >> 20);
        let count = u32_at(&trimmed, 8).unwrap() as usize;
        let faces: Vec<Face> = (0..count).map(|i| face(&trimmed, u32_at(&trimmed, 12 + 4 * i).unwrap() as usize).unwrap()).collect();
        let mut weights: Vec<u16> = faces.iter().map(|f| f.weight).collect();
        weights.sort();
        weights.dedup();
        assert_eq!(weights.len(), 2, "{weights:?}");
        assert!(faces.iter().any(|f| f.family == "PingFang TC") && faces.iter().any(|f| f.family == "PingFang SC"));
        // The trimmed collection still parses as fonts.
        let mut collection = fontique::Collection::new(fontique::CollectionOptions { shared: false, system_fonts: false });
        let families = collection.register_fonts(fontique::Blob::new(std::sync::Arc::new(trimmed)), None);
        assert!(!families.is_empty());
    }

    #[test]
    fn style_names_override_uniform_weights() {
        // Kaiti marks all seven faces as weight 400; its Black faces still go.
        assert_eq!(style_weight("Black"), Some(900));
        assert_eq!(style_weight("Semibold Italic"), Some(600));
        assert_eq!(style_weight("W3"), None);
        let Some(path) = crate::FontService::new().resolve("Kaiti TC").and_then(|f| f.paths.into_iter().next()) else { return };
        let data = std::fs::read(&path).unwrap();
        if let Some(trimmed) = trim_collection(&data) {
            println!("Kaiti {} MB → {} MB", data.len() >> 20, trimmed.len() >> 20);
            assert!(trimmed.len() < data.len());
        }
    }

    #[test]
    fn emoji_keep_three_bitmap_sizes() {
        let Ok(data) = std::fs::read("/System/Library/Fonts/Apple Color Emoji.ttc") else { return };
        let trimmed = trim_collection(&data).expect("emoji bitmaps are trimmed");
        println!("Apple Color Emoji {} MB → {} MB", data.len() >> 20, trimmed.len() >> 20);
        assert!(trimmed.len() < data.len() * 2 / 3);
        let mut collection = fontique::Collection::new(fontique::CollectionOptions { shared: false, system_fonts: false });
        assert!(!collection.register_fonts(fontique::Blob::new(std::sync::Arc::new(trimmed)), None).is_empty());
    }
}
