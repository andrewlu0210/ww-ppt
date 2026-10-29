use std::io::{Cursor, Write};

use ppt_model::{Align, Anchor, Color, ShapeKind};
use zip::write::{SimpleFileOptions, ZipWriter};

use super::parse_pptx;

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;

fn rels(entries: &[(&str, &str, &str)]) -> String {
    let body: String = entries
        .iter()
        .map(|(id, kind, target)| {
            format!(r#"<Relationship Id="{id}" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/{kind}" Target="{target}"/>"#)
        })
        .collect();
    format!(r#"<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#)
}

/// A minimal deck exercising theme colors, placeholder inheritance, shape
/// styles and groups.
fn sample_pptx() -> Vec<u8> {
    let theme = format!(
        r#"<a:theme {NS}><a:themeElements>
            <a:clrScheme name="t"><a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>
              <a:dk2><a:srgbClr val="44546A"/></a:dk2><a:lt2><a:srgbClr val="E7E6E6"/></a:lt2><a:accent1><a:srgbClr val="4472C4"/></a:accent1></a:clrScheme>
            <a:fontScheme name="f"><a:majorFont><a:latin typeface="Calibri Light"/><a:ea typeface=""/><a:font script="Hant" typeface="微軟正黑體"/></a:majorFont>
              <a:minorFont><a:latin typeface="Calibri"/><a:ea typeface=""/><a:font script="Hant" typeface="微軟正黑體"/></a:minorFont></a:fontScheme>
            <a:fmtScheme name="s"><a:lnStyleLst><a:ln w="6350"/><a:ln w="12700"/><a:ln w="19050"/></a:lnStyleLst></a:fmtScheme>
        </a:themeElements></a:theme>"#
    );
    let master = format!(
        r#"<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg><p:spTree>
            <p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="100" y="200"/><a:ext cx="1000" cy="500"/></a:xfrm></p:spPr>
              <p:txBody><a:bodyPr anchor="ctr"/><a:p/></p:txBody></p:sp>
            <p:sp><p:nvSpPr><p:cNvPr id="3" name="Logo"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="50" cy="50"/></a:xfrm><a:prstGeom prst="rect"/><a:solidFill><a:schemeClr val="accent1"/></a:solidFill></p:spPr></p:sp>
          </p:spTree></p:cSld>
          <p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1"/>
          <p:txStyles>
            <p:titleStyle><a:lvl1pPr algn="l"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:defRPr sz="4400"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/></a:defRPr></a:lvl1pPr></p:titleStyle>
            <p:bodyStyle><a:lvl1pPr><a:buChar char="•"/><a:defRPr sz="2800"/></a:lvl1pPr></p:bodyStyle>
            <p:otherStyle/>
          </p:txStyles></p:sldMaster>"#
    );
    let layout = format!(
        r#"<p:sldLayout {NS}><p:cSld><p:spTree>
            <p:sp><p:nvSpPr><p:cNvPr id="2" name="Title"/><p:cNvSpPr/><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr>
              <p:spPr><a:xfrm><a:off x="300" y="400"/><a:ext cx="2000" cy="600"/></a:xfrm></p:spPr>
              <p:txBody><a:bodyPr/><a:lstStyle><a:lvl1pPr algn="ctr"/></a:lstStyle><a:p/></p:txBody></p:sp>
          </p:spTree></p:cSld></p:sldLayout>"#
    );
    let slide = format!(
        r#"<p:sld {NS}><p:cSld><p:spTree>
            <p:sp><p:nvSpPr><p:cNvPr id="2" name="標題 1"/><p:cNvSpPr/><p:nvPr><p:ph type="ctrTitle"/></p:nvPr></p:nvSpPr><p:spPr/>
              <p:txBody><a:bodyPr/><a:p><a:r><a:rPr lang="zh-TW"/><a:t>你好 PowerPoint</a:t></a:r></a:p></p:txBody></p:sp>
            <p:sp><p:nvSpPr><p:cNvPr id="3" name="Rect"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
              <p:spPr><a:xfrm rot="5400000"><a:off x="10" y="20"/><a:ext cx="30" cy="40"/></a:xfrm><a:prstGeom prst="roundRect"><a:avLst><a:gd name="adj" fmla="val 10000"/></a:avLst></a:prstGeom></p:spPr>
              <p:style><a:lnRef idx="2"><a:schemeClr val="accent1"><a:shade val="50000"/></a:schemeClr></a:lnRef><a:fillRef idx="1"><a:schemeClr val="accent1"/></a:fillRef>
                <a:effectRef idx="0"><a:schemeClr val="accent1"/></a:effectRef><a:fontRef idx="minor"><a:schemeClr val="lt1"/></a:fontRef></p:style>
              <p:txBody><a:bodyPr anchor="ctr"/><a:p><a:pPr algn="ctr"/><a:r><a:t>A</a:t></a:r><a:br/><a:r><a:rPr b="1" sz="1200"><a:solidFill><a:srgbClr val="FF0000"/></a:solidFill></a:rPr><a:t>B</a:t></a:r></a:p></p:txBody></p:sp>
            <p:grpSp><p:nvGrpSpPr><p:cNvPr id="4" name="Group"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr>
              <p:grpSpPr><a:xfrm><a:off x="1000" y="1000"/><a:ext cx="200" cy="200"/><a:chOff x="0" y="0"/><a:chExt cx="100" cy="100"/></a:xfrm></p:grpSpPr>
              <p:sp><p:nvSpPr><p:cNvPr id="5" name="Child"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr>
                <p:spPr><a:xfrm><a:off x="50" y="0"/><a:ext cx="50" cy="50"/></a:xfrm><a:prstGeom prst="ellipse"/><a:noFill/><a:ln w="25400"><a:solidFill><a:srgbClr val="00FF00"/></a:solidFill></a:ln></p:spPr></p:sp>
            </p:grpSp>
            <p:pic><p:nvPicPr><p:cNvPr id="6" name="Pic"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr>
              <p:blipFill><a:blip r:embed="rId2"/></p:blipFill><p:spPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="10" cy="10"/></a:xfrm></p:spPr></p:pic>
          </p:spTree></p:cSld></p:sld>"#
    );
    let presentation = format!(
        r#"<p:presentation {NS}><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="9144000" cy="6858000"/>
          <p:defaultTextStyle><a:lvl1pPr><a:defRPr sz="1800"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill></a:defRPr></a:lvl1pPr></p:defaultTextStyle></p:presentation>"#
    );

    let parts: Vec<(&str, String)> = vec![
        ("_rels/.rels", rels(&[("rId1", "officeDocument", "ppt/presentation.xml")])),
        ("ppt/presentation.xml", presentation),
        ("ppt/_rels/presentation.xml.rels", rels(&[("rId2", "slide", "slides/slide1.xml")])),
        ("ppt/slides/slide1.xml", slide),
        ("ppt/slides/_rels/slide1.xml.rels", rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml"), ("rId2", "image", "../media/image1.png")])),
        ("ppt/slideLayouts/slideLayout1.xml", layout),
        ("ppt/slideLayouts/_rels/slideLayout1.xml.rels", rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])),
        ("ppt/slideMasters/slideMaster1.xml", master),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels", rels(&[("rId1", "theme", "../theme/theme1.xml")])),
        ("ppt/theme/theme1.xml", theme),
        ("ppt/media/image1.png", "not really a png".to_string()),
    ];
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    for (name, body) in parts {
        zip.start_file(name, opts).unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

#[test]
fn parses_slide_size_and_shapes() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    assert_eq!((pres.slide_width, pres.slide_height), (9_144_000, 6_858_000));
    assert_eq!(pres.slides.len(), 1);
    let slide = &pres.slides[0];
    assert_eq!(slide.shapes.len(), 4);
    assert_eq!(slide.background, Some(Color::WHITE));
    // The master's non-placeholder logo is inherited; its title placeholder is not.
    assert_eq!(slide.background_shapes.len(), 1);
    assert_eq!(slide.background_shapes[0].fill, Color::from_hex("4472C4").map(ppt_model::Paint::Solid));
    assert_eq!(pres.minor_font, ("Calibri".to_string(), "微軟正黑體".to_string()));
}

#[test]
fn placeholder_inherits_layout_and_master() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    let title = &pres.slides[0].shapes[0];
    // Position comes from the layout, anchor from the master.
    assert_eq!((title.xfrm.x, title.xfrm.y, title.xfrm.w, title.xfrm.h), (300, 400, 2000, 600));
    let body = title.text.as_ref().unwrap();
    assert_eq!(body.anchor, Anchor::Middle);
    let p = &body.paragraphs[0];
    // Layout lstStyle overrides the master's left alignment.
    assert_eq!(p.align, Align::Center);
    assert!((p.line_spacing - 0.9).abs() < 1e-6);
    let style = &p.runs[0].style;
    assert_eq!(style.size_pt, 44.0);
    assert_eq!(style.latin_font, "Calibri Light");
    assert_eq!(style.ea_font, "微軟正黑體");
    assert_eq!(style.color, Color::BLACK);
    assert_eq!(p.runs[0].text, "你好 PowerPoint");
}

#[test]
fn shape_style_references_and_runs() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    let rect = &pres.slides[0].shapes[1];
    assert_eq!(rect.xfrm.rot, 90.0);
    assert_eq!(rect.fill, Color::from_hex("4472C4").map(ppt_model::Paint::Solid));
    let line = rect.line.unwrap();
    assert_eq!(line.width, 12_700);
    assert_eq!(line.color, Color::rgb(0x22, 0x39, 0x62));
    match &rect.geometry {
        ppt_model::Geometry::Preset { name, adjust } => {
            assert_eq!(name, "roundRect");
            assert_eq!(adjust, &vec![("adj".to_string(), 10_000.0)]);
        }
        g => panic!("unexpected geometry {g:?}"),
    }
    let p = &rect.text.as_ref().unwrap().paragraphs[0];
    assert_eq!(p.runs.len(), 3);
    assert_eq!(p.runs[0].style.color, Color::WHITE, "fontRef color applies to plain runs");
    assert_eq!(p.runs[1].text, "\u{2028}");
    assert!(p.runs[2].style.bold);
    assert_eq!(p.runs[2].style.size_pt, 12.0);
    assert_eq!(p.runs[2].style.color, Color::rgb(255, 0, 0));
    assert_eq!(p.bullet, None);
}

#[test]
fn groups_are_flattened_with_child_transform() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    let child = &pres.slides[0].shapes[2];
    assert_eq!((child.xfrm.x, child.xfrm.y, child.xfrm.w, child.xfrm.h), (1100, 1000, 100, 100));
    assert_eq!(child.fill, None);
    assert_eq!(child.line.unwrap().color, Color::rgb(0, 255, 0));
}

#[test]
fn pictures_reference_media() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    let pic = &pres.slides[0].shapes[3];
    assert_eq!(pic.kind, ShapeKind::Picture { media: Some("ppt/media/image1.png".to_string()), crop: Default::default() });
    assert!(pres.media.contains_key("ppt/media/image1.png"));
}

#[test]
fn shape_ids_are_unique() {
    let pres = parse_pptx(&sample_pptx()).unwrap();
    let slide = &pres.slides[0];
    let mut ids: Vec<_> = slide.shapes.iter().chain(&slide.background_shapes).map(|s| s.id).collect();
    let n = ids.len();
    ids.sort();
    ids.dedup();
    assert_eq!(ids.len(), n);
}

#[test]
fn rejects_non_zip_input() {
    assert!(parse_pptx(b"hello").is_err());
}
