//! OPC package access: zip parts and relationships.

use std::collections::HashMap;
use std::io::{Cursor, Read};

use zip::ZipArchive;

use crate::Error;

pub const REL_NS: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

pub struct Package<'a> {
    zip: ZipArchive<Cursor<&'a [u8]>>,
}

#[derive(Clone, Debug)]
pub struct Rel {
    pub kind: String,
    /// Absolute part name, or the raw target for external relationships.
    pub target: String,
    pub external: bool,
}

impl<'a> Package<'a> {
    pub fn open(bytes: &'a [u8]) -> Result<Self, Error> {
        let zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| Error::Zip(e.to_string()))?;
        Ok(Self { zip })
    }

    pub fn read_bytes(&mut self, part: &str) -> Result<Vec<u8>, Error> {
        let mut file = self.zip.by_name(part).map_err(|_| Error::Missing(part.to_string()))?;
        let mut out = Vec::with_capacity(file.size() as usize);
        file.read_to_end(&mut out).map_err(|e| Error::Zip(e.to_string()))?;
        Ok(out)
    }

    pub fn read_string(&mut self, part: &str) -> Result<String, Error> {
        String::from_utf8(self.read_bytes(part)?).map_err(|e| Error::Xml(format!("{part}: {e}")))
    }

    /// Relationships of `part`, keyed by `Id`, with targets resolved to absolute part names.
    pub fn rels(&mut self, part: &str) -> HashMap<String, Rel> {
        let (dir, file) = part.rsplit_once('/').unwrap_or(("", part));
        let rels_part = if dir.is_empty() { format!("_rels/{file}.rels") } else { format!("{dir}/_rels/{file}.rels") };
        let Ok(xml) = self.read_string(&rels_part) else { return HashMap::new() };
        let Ok(doc) = roxmltree::Document::parse(&xml) else { return HashMap::new() };
        doc.root_element()
            .children()
            .filter(|n| n.tag_name().name() == "Relationship")
            .filter_map(|n| {
                let id = n.attribute("Id")?;
                let target = n.attribute("Target")?;
                let kind = n.attribute("Type").unwrap_or_default();
                let external = n.attribute("TargetMode") == Some("External");
                let target = if external { target.to_string() } else { resolve(dir, target) };
                Some((id.to_string(), Rel { kind: kind.to_string(), target, external }))
            })
            .collect()
    }

    /// First relationship of `part` whose type ends with `/{suffix}`.
    pub fn rel_of_type(&mut self, part: &str, suffix: &str) -> Option<String> {
        self.rels(part).into_values().find(|r| !r.external && r.kind.ends_with(&format!("/{suffix}"))).map(|r| r.target)
    }
}

/// Resolves a relationship target relative to the source part's directory.
pub fn resolve(base_dir: &str, target: &str) -> String {
    if let Some(abs) = target.strip_prefix('/') {
        return abs.to_string();
    }
    let mut parts: Vec<&str> = if base_dir.is_empty() { Vec::new() } else { base_dir.split('/').collect() };
    for seg in target.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

#[cfg(test)]
mod tests {
    use super::resolve;

    #[test]
    fn resolves_relative_targets() {
        assert_eq!(resolve("ppt/slides", "../slideLayouts/slideLayout1.xml"), "ppt/slideLayouts/slideLayout1.xml");
        assert_eq!(resolve("ppt", "slides/slide1.xml"), "ppt/slides/slide1.xml");
        assert_eq!(resolve("ppt/slides", "/ppt/media/image1.png"), "ppt/media/image1.png");
    }
}
