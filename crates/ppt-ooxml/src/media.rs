//! Audio and video objects of slides (`a:audioFile`, `a:videoFile`, ...) and
//! their playback settings, read for video export. They are kept verbatim on
//! save like any other shape; nothing here edits them.

use roxmltree::{Document, Node};

use ppt_model::media::{ClipKind, ClipStart, MediaClip};
use ppt_model::Presentation;

use crate::package::REL_NS;

fn local(n: Node, name: &str) -> bool {
    n.is_element() && n.tag_name().name() == name
}

fn child<'a, 'b>(n: Node<'a, 'b>, name: &str) -> Option<Node<'a, 'b>> {
    n.children().find(|c| local(*c, name))
}

/// The clips of every slide of `pres`, in slide order.
pub fn clips(pres: &Presentation) -> Vec<MediaClip> {
    let mut out = Vec::new();
    for (index, slide) in pres.slides.iter().enumerate() {
        let Some(source) = slide.source.as_ref() else { continue };
        let Ok(doc) = Document::parse(&source.doc) else { continue };
        let timing = timing_of(&doc);
        for shape in &slide.shapes {
            let Some(src) = shape.source.as_ref().filter(|s| s.part == source.part) else { continue };
            let Some(node) = doc.descendants().find(|n| n.range() == src.range) else { continue };
            let Some(info) = frame_info(node, &src.rels) else { continue };
            if !pres.media.contains_key(&info.part) {
                continue;
            }
            // A clip without playback settings in the timing tree is not played by a show.
            let Some(play) = timing.iter().find(|p| p.spid == info.spid) else { continue };
            out.push(MediaClip {
                slide: index,
                kind: info.kind,
                shape: shape.id,
                part: info.part,
                trim_start_ms: info.trim_start_ms,
                trim_end_ms: info.trim_end_ms,
                volume: if play.muted { 0.0 } else { play.volume },
                repeat: play.repeat,
                slides: play.slides,
                start: play.start,
                delay_ms: play.delay_ms,
            });
        }
    }
    out
}

struct FrameInfo {
    spid: u32,
    kind: ClipKind,
    part: String,
    trim_start_ms: f64,
    trim_end_ms: f64,
}

/// The media a picture shape refers to, or `None` if it is not a clip (or is linked, not embedded).
fn frame_info(n: Node, rels: &[(String, String, String)]) -> Option<FrameInfo> {
    let nv = n.children().find(|c| c.is_element() && c.tag_name().name().starts_with("nv"))?;
    let spid: u32 = child(nv, "cNvPr")?.attribute("id")?.parse().ok()?;
    let nv_pr = child(nv, "nvPr")?;
    let (file, kind) = nv_pr.children().find_map(|c| match c.tag_name().name() {
        "videoFile" | "quickTimeFile" if c.is_element() => Some((c, ClipKind::Video)),
        "audioFile" | "wavAudioFile" if c.is_element() => Some((c, ClipKind::Audio)),
        _ => None,
    })?;
    // PowerPoint 2010+ also lists the media in an extension: prefer it, it is the embedded copy.
    let ext_media = nv_pr.descendants().find(|d| local(*d, "media"));
    let rid = ext_media.and_then(|m| m.attribute((REL_NS, "embed"))).or_else(|| file.attribute((REL_NS, "embed"))).or_else(|| file.attribute((REL_NS, "link")))?;
    let target = rels.iter().find(|(id, _, _)| id == rid).map(|(_, _, t)| t.clone())?;
    if target.starts_with("external:") {
        return None;
    }
    // `p14:trim`: milliseconds removed from the start and from the end.
    let trim = ext_media.and_then(|m| m.descendants().find(|d| local(*d, "trim")));
    let ms = |name| trim.and_then(|t| t.attribute(name)?.parse::<f64>().ok()).unwrap_or(0.0).max(0.0);
    Some(FrameInfo { spid, kind, part: target, trim_start_ms: ms("st"), trim_end_ms: ms("end") })
}

/// Playback settings found in a slide's timing tree.
struct Play {
    spid: u32,
    /// 0–1.
    volume: f64,
    muted: bool,
    repeat: bool,
    slides: usize,
    start: ClipStart,
    delay_ms: f64,
}

fn timing_of(doc: &Document) -> Vec<Play> {
    let mut out = Vec::new();
    let Some(timing) = doc.root_element().children().find(|c| local(*c, "timing")) else { return out };
    let target = |n: Node| n.descendants().find(|d| local(*d, "spTgt")).and_then(|t| t.attribute("spid")?.parse::<u32>().ok());
    for media in timing.descendants().filter(|n| local(*n, "audio") || local(*n, "video")) {
        let Some(node) = child(media, "cMediaNode") else { continue };
        let Some(spid) = target(node) else { continue };
        // PowerPoint keeps the settings beside the main sequence and starts the clip with a
        // `playFrom` command inside it; older files put the settings in the step itself.
        let start = timing
            .descendants()
            .filter(|c| local(*c, "cmd") && c.attribute("type") == Some("call") && c.attribute("cmd").is_some_and(|v| v.starts_with("play")))
            .find(|c| target(*c) == Some(spid))
            .and_then(start_of)
            .or_else(|| start_of(media));
        let Some((start, delay_ms)) = start else { continue };
        let ctn = child(node, "cTn");
        out.push(Play {
            spid,
            // The schema's default volume is 50%.
            volume: node.attribute("vol").and_then(|v| v.parse::<f64>().ok()).map_or(0.5, |v| (v / 100_000.0).clamp(0.0, 1.0)),
            muted: matches!(node.attribute("mute"), Some("1" | "true")),
            repeat: ctn.is_some_and(|c| c.attribute("repeatCount") == Some("indefinite")),
            slides: node.attribute("numSld").and_then(|v| v.parse::<usize>().ok()).unwrap_or(1).max(1),
            start,
            delay_ms,
        });
    }
    out
}

/// When the click step holding `media` (a command or a media node) starts: with the slide or with a click.
/// `None` for media triggered some other way (an interactive sequence).
fn start_of(media: Node) -> Option<(ClipStart, f64)> {
    // The step is the `p:par` directly under the main sequence's child list.
    let step = media.ancestors().find(|a| {
        local(*a, "par")
            && a.parent().is_some_and(|list| local(list, "childTnLst") && list.parent().and_then(|ctn| ctn.parent()).is_some_and(|seq| local(seq, "seq") && child(*a, "cTn").is_some()))
            && a.parent().and_then(|l| l.parent()).and_then(|ctn| ctn.attribute("nodeType")) == Some("mainSeq")
    })?;
    let list = step.parent()?;
    let mut clicks = 0;
    for p in list.children().filter(|c| local(*c, "par")) {
        let conds: Vec<Node> = child(p, "cTn").and_then(|c| child(c, "stCondLst")).map(|l| l.children().filter(|c| local(*c, "cond")).collect()).unwrap_or_default();
        let on_click = conds.iter().any(|c| c.attribute("delay") == Some("indefinite"));
        let auto = conds.iter().any(|c| c.attribute("evt") == Some("onBegin"));
        let is_click = on_click && !auto;
        if is_click {
            clicks += 1;
        }
        if p == step {
            // The effect's own delay, from the nodes between the step and the command.
            let delay: f64 = media
                .ancestors()
                .take_while(|a| *a != step)
                .filter(|a| local(*a, "cTn"))
                .filter_map(|c| child(c, "stCondLst"))
                .flat_map(|l| l.children().filter(|c| local(*c, "cond")))
                .filter_map(|c| c.attribute("delay")?.parse::<f64>().ok())
                .filter(|d| *d > 0.0)
                .sum();
            return Some((if is_click { ClipStart::Click(clicks) } else { ClipStart::Auto }, delay));
        }
    }
    None
}
