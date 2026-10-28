//! Built-in blank presentation: Office theme, one master and six layouts, 16:9.
//!
//! New decks are created by parsing this package, so they save through the
//! same path as opened files.

use std::io::{Cursor, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

const NS: &str = r#"xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:p="http://schemas.openxmlformats.org/presentationml/2006/main""#;
const XML_HEAD: &str = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>"#;
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";

fn rels(entries: &[(&str, &str, &str)]) -> String {
    let body: String = entries.iter().map(|(id, kind, target)| format!(r#"<Relationship Id="{id}" Type="{REL}/{kind}" Target="{target}"/>"#)).collect();
    format!(r#"{XML_HEAD}<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">{body}</Relationships>"#)
}

fn xfrm(x: i64, y: i64, w: i64, h: i64) -> String {
    format!(r#"<a:xfrm><a:off x="{x}" y="{y}"/><a:ext cx="{w}" cy="{h}"/></a:xfrm>"#)
}

/// A placeholder shape. `geom` is `Some((x, y, w, h))` to override the inherited position.
fn ph(id: u32, name: &str, ph: &str, geom: Option<(i64, i64, i64, i64)>, body_pr: &str, lst: &str, text: &str) -> String {
    let sp_pr = match geom {
        Some((x, y, w, h)) => format!(r#"<p:spPr>{}<a:prstGeom prst="rect"><a:avLst/></a:prstGeom></p:spPr>"#, xfrm(x, y, w, h)),
        None => "<p:spPr/>".to_string(),
    };
    let paragraph = if text.is_empty() {
        r#"<a:p><a:endParaRPr lang="zh-TW" altLang="en-US"/></a:p>"#.to_string()
    } else {
        format!(r#"<a:p><a:r><a:rPr lang="zh-TW" altLang="en-US"/><a:t>{text}</a:t></a:r></a:p>"#)
    };
    format!(
        r#"<p:sp><p:nvSpPr><p:cNvPr id="{id}" name="{name}"/><p:cNvSpPr><a:spLocks noGrp="1"/></p:cNvSpPr><p:nvPr>{ph}</p:nvPr></p:nvSpPr>{sp_pr}<p:txBody>{body_pr}<a:lstStyle>{lst}</a:lstStyle>{paragraph}</p:txBody></p:sp>"#
    )
}

fn sp_tree(shapes: &str) -> String {
    format!(
        r#"<p:spTree><p:nvGrpSpPr><p:cNvPr id="1" name=""/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x="0" y="0"/><a:ext cx="0" cy="0"/><a:chOff x="0" y="0"/><a:chExt cx="0" cy="0"/></a:xfrm></p:grpSpPr>{shapes}</p:spTree>"#
    )
}

fn footers() -> String {
    [
        ph(4, "日期版面配置區 3", r#"<p:ph type="dt" sz="half" idx="10"/>"#, None, "<a:bodyPr/>", "", ""),
        ph(5, "頁尾版面配置區 4", r#"<p:ph type="ftr" sz="quarter" idx="11"/>"#, None, "<a:bodyPr/>", "", ""),
        ph(6, "投影片編號版面配置區 5", r#"<p:ph type="sldNum" sz="quarter" idx="12"/>"#, None, "<a:bodyPr/>", "", ""),
    ]
    .concat()
}

fn theme() -> String {
    let scheme = |name: &str, val: &str| format!(r#"<a:{name}><a:srgbClr val="{val}"/></a:{name}>"#);
    let colors = [
        r#"<a:dk1><a:sysClr val="windowText" lastClr="000000"/></a:dk1><a:lt1><a:sysClr val="window" lastClr="FFFFFF"/></a:lt1>"#.to_string(),
        scheme("dk2", "44546A"),
        scheme("lt2", "E7E6E6"),
        scheme("accent1", "4472C4"),
        scheme("accent2", "ED7D31"),
        scheme("accent3", "A5A5A5"),
        scheme("accent4", "FFC000"),
        scheme("accent5", "5B9BD5"),
        scheme("accent6", "70AD47"),
        scheme("hlink", "0563C1"),
        scheme("folHlink", "954F72"),
    ]
    .concat();
    let font = |latin: &str| {
        format!(r#"<a:latin typeface="{latin}" panose="020F0302020204030204"/><a:ea typeface=""/><a:cs typeface=""/><a:font script="Hant" typeface="新細明體"/><a:font script="Hans" typeface="等线"/><a:font script="Jpan" typeface="游ゴシック"/>"#)
    };
    let solid = |inner: &str| format!("<a:solidFill>{inner}</a:solidFill>");
    let ph_clr = r#"<a:schemeClr val="phClr"/>"#;
    format!(
        r#"{XML_HEAD}<a:theme xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" name="Office 佈景主題"><a:themeElements><a:clrScheme name="Office">{colors}</a:clrScheme><a:fontScheme name="Office"><a:majorFont>{}</a:majorFont><a:minorFont>{}</a:minorFont></a:fontScheme><a:fmtScheme name="Office"><a:fillStyleLst>{}{}{}</a:fillStyleLst><a:lnStyleLst><a:ln w="6350" cap="flat" cmpd="sng" algn="ctr">{}<a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="12700" cap="flat" cmpd="sng" algn="ctr">{}<a:prstDash val="solid"/><a:miter lim="800000"/></a:ln><a:ln w="19050" cap="flat" cmpd="sng" algn="ctr">{}<a:prstDash val="solid"/><a:miter lim="800000"/></a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{}{}{}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"#,
        font("Calibri Light"),
        font("Calibri"),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
        solid(ph_clr),
    )
}

fn master() -> String {
    let title = ph(
        2,
        "標題版面配置區 1",
        r#"<p:ph type="title"/>"#,
        Some((838_200, 365_125, 10_515_600, 1_325_563)),
        r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"><a:normAutofit/></a:bodyPr>"#,
        "",
        "按一下以編輯母片標題樣式",
    );
    let body = ph(
        3,
        "文字版面配置區 2",
        r#"<p:ph type="body" idx="1"/>"#,
        Some((838_200, 1_825_625, 10_515_600, 4_351_338)),
        r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0"><a:normAutofit/></a:bodyPr>"#,
        "",
        "按一下以編輯母片文字樣式",
    );
    let footer = |id: u32, name: &str, ph_xml: &str, x: i64, w: i64, algn: &str| {
        ph(
            id,
            name,
            ph_xml,
            Some((x, 6_356_350, w, 365_125)),
            r#"<a:bodyPr vert="horz" lIns="91440" tIns="45720" rIns="91440" bIns="45720" rtlCol="0" anchor="ctr"/>"#,
            &format!(r#"<a:lvl1pPr algn="{algn}"><a:defRPr sz="1200"><a:solidFill><a:schemeClr val="tx1"><a:tint val="75000"/></a:schemeClr></a:solidFill></a:defRPr></a:lvl1pPr>"#),
            "",
        )
    };
    let shapes = [
        title,
        body,
        footer(4, "日期版面配置區 3", r#"<p:ph type="dt" sz="half" idx="2"/>"#, 838_200, 2_743_200, "l"),
        footer(5, "頁尾版面配置區 4", r#"<p:ph type="ftr" sz="quarter" idx="3"/>"#, 4_038_600, 4_114_800, "ctr"),
        footer(6, "投影片編號版面配置區 5", r#"<p:ph type="sldNum" sz="quarter" idx="4"/>"#, 8_610_600, 2_743_200, "r"),
    ]
    .concat();
    let body_lvl = |lvl: u32, mar: i64, sz: u32| {
        format!(
            r#"<a:lvl{lvl}pPr marL="{mar}" indent="-228600" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPts val="{}"/></a:spcBef><a:buFont typeface="Arial"/><a:buChar char="•"/><a:defRPr sz="{sz}" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{lvl}pPr>"#,
            if lvl == 1 { 1000 } else { 500 }
        )
    };
    let body_style = [body_lvl(1, 228_600, 2800), body_lvl(2, 685_800, 2400), body_lvl(3, 1_143_000, 2000), body_lvl(4, 1_600_200, 1800), body_lvl(5, 2_057_400, 1800)].concat();
    let ids: String = (1..=6).map(|i| format!(r#"<p:sldLayoutId id="{}" r:id="rId{i}"/>"#, 2_147_483_648u64 + i as u64)).collect();
    format!(
        r#"{XML_HEAD}<p:sldMaster {NS}><p:cSld><p:bg><p:bgRef idx="1001"><a:schemeClr val="bg1"/></p:bgRef></p:bg>{}</p:cSld><p:clrMap bg1="lt1" tx1="dk1" bg2="lt2" tx2="dk2" accent1="accent1" accent2="accent2" accent3="accent3" accent4="accent4" accent5="accent5" accent6="accent6" hlink="hlink" folHlink="folHlink"/><p:sldLayoutIdLst>{ids}</p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:lnSpc><a:spcPct val="90000"/></a:lnSpc><a:spcBef><a:spcPct val="0"/></a:spcBef><a:buNone/><a:defRPr sz="4400" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mj-lt"/><a:ea typeface="+mj-ea"/><a:cs typeface="+mj-cs"/></a:defRPr></a:lvl1pPr></p:titleStyle><p:bodyStyle>{body_style}</p:bodyStyle><p:otherStyle><a:defPPr><a:defRPr lang="zh-TW"/></a:defPPr><a:lvl1pPr marL="0" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>"#,
        sp_tree(&shapes)
    )
}

fn layout(name: &str, kind: &str, shapes: &str) -> String {
    format!(
        r#"{XML_HEAD}<p:sldLayout {NS} type="{kind}" preserve="1"><p:cSld name="{name}">{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>"#,
        sp_tree(shapes)
    )
}

fn layouts() -> Vec<String> {
    let title = || ph(2, "標題 1", r#"<p:ph type="title"/>"#, None, "<a:bodyPr/>", "", "按一下以編輯母片標題樣式");
    let no_bullet = |sz: u32| format!(r#"<a:lvl1pPr marL="0" indent="0" algn="ctr"><a:buNone/><a:defRPr sz="{sz}"/></a:lvl1pPr>"#);
    vec![
        layout(
            "標題投影片",
            "title",
            &[
                ph(2, "標題 1", r#"<p:ph type="ctrTitle"/>"#, Some((1_524_000, 1_122_363, 9_144_000, 2_387_600)), r#"<a:bodyPr anchor="b"/>"#, r#"<a:lvl1pPr algn="ctr"><a:defRPr sz="6000"/></a:lvl1pPr>"#, "按一下以編輯母片標題樣式"),
                ph(3, "副標題 2", r#"<p:ph type="subTitle" idx="1"/>"#, Some((1_524_000, 3_602_038, 9_144_000, 1_655_762)), "<a:bodyPr/>", &no_bullet(2400), "按一下以編輯母片子標題樣式"),
                footers(),
            ]
            .concat(),
        ),
        layout(
            "標題及內容",
            "obj",
            &[title(), ph(3, "內容版面配置區 2", r#"<p:ph idx="1"/>"#, None, "<a:bodyPr/>", "", "按一下以編輯母片文字樣式"), footers()].concat(),
        ),
        layout(
            "區段標題",
            "secHead",
            &[
                ph(2, "標題 1", r#"<p:ph type="title"/>"#, Some((831_850, 1_709_738, 10_515_600, 2_852_737)), r#"<a:bodyPr anchor="b"/>"#, r#"<a:lvl1pPr><a:defRPr sz="6000"/></a:lvl1pPr>"#, "按一下以編輯母片標題樣式"),
                ph(
                    3,
                    "文字版面配置區 2",
                    r#"<p:ph type="body" idx="1"/>"#,
                    Some((831_850, 4_589_463, 10_515_600, 1_500_187)),
                    "<a:bodyPr/>",
                    r#"<a:lvl1pPr marL="0" indent="0"><a:buNone/><a:defRPr sz="2400"><a:solidFill><a:schemeClr val="tx1"><a:tint val="75000"/></a:schemeClr></a:solidFill></a:defRPr></a:lvl1pPr>"#,
                    "按一下以編輯母片文字樣式",
                ),
                footers(),
            ]
            .concat(),
        ),
        layout(
            "兩項物件",
            "twoObj",
            &[
                title(),
                ph(3, "內容版面配置區 2", r#"<p:ph sz="half" idx="1"/>"#, Some((838_200, 1_825_625, 5_181_600, 4_351_338)), "<a:bodyPr/>", "", "按一下以編輯母片文字樣式"),
                ph(4, "內容版面配置區 3", r#"<p:ph sz="half" idx="2"/>"#, Some((6_172_800, 1_825_625, 5_181_600, 4_351_338)), "<a:bodyPr/>", "", "按一下以編輯母片文字樣式"),
                footers(),
            ]
            .concat(),
        ),
        layout("只有標題", "titleOnly", &[title(), footers()].concat()),
        layout("空白", "blank", &footers()),
    ]
}

fn first_slide() -> String {
    let shapes = [
        ph(2, "標題 1", r#"<p:ph type="ctrTitle"/>"#, None, "<a:bodyPr/>", "", ""),
        ph(3, "副標題 2", r#"<p:ph type="subTitle" idx="1"/>"#, None, "<a:bodyPr/>", "", ""),
    ]
    .concat();
    format!(r#"{XML_HEAD}<p:sld {NS}><p:cSld>{}</p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>"#, sp_tree(&shapes))
}

fn presentation() -> String {
    let lvl = |i: u32| {
        format!(
            r#"<a:lvl{i}pPr marL="{}" algn="l" defTabSz="914400" rtl="0" eaLnBrk="1" latinLnBrk="0" hangingPunct="1"><a:defRPr sz="1800" kern="1200"><a:solidFill><a:schemeClr val="tx1"/></a:solidFill><a:latin typeface="+mn-lt"/><a:ea typeface="+mn-ea"/><a:cs typeface="+mn-cs"/></a:defRPr></a:lvl{i}pPr>"#,
            (i - 1) * 457_200
        )
    };
    let levels: String = (1..=9).map(lvl).collect();
    format!(
        r#"{XML_HEAD}<p:presentation {NS} saveSubsetFonts="1"><p:sldMasterIdLst><p:sldMasterId id="2147483648" r:id="rId1"/></p:sldMasterIdLst><p:sldIdLst><p:sldId id="256" r:id="rId2"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/><p:notesSz cx="6858000" cy="9144000"/><p:defaultTextStyle><a:defPPr><a:defRPr lang="zh-TW"/></a:defPPr>{levels}</p:defaultTextStyle></p:presentation>"#
    )
}

/// The built-in blank presentation as `.pptx` bytes.
pub fn template_package() -> Vec<u8> {
    let layouts = layouts();
    let ct_overrides: String = [
        ("/ppt/presentation.xml", "application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml"),
        ("/ppt/slideMasters/slideMaster1.xml", "application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml"),
        ("/ppt/slides/slide1.xml", "application/vnd.openxmlformats-officedocument.presentationml.slide+xml"),
        ("/ppt/theme/theme1.xml", "application/vnd.openxmlformats-officedocument.theme+xml"),
        ("/ppt/presProps.xml", "application/vnd.openxmlformats-officedocument.presentationml.presProps+xml"),
        ("/ppt/viewProps.xml", "application/vnd.openxmlformats-officedocument.presentationml.viewProps+xml"),
        ("/docProps/core.xml", "application/vnd.openxmlformats-package.core-properties+xml"),
        ("/docProps/app.xml", "application/vnd.openxmlformats-officedocument.extended-properties+xml"),
    ]
    .iter()
    .map(|(p, t)| format!(r#"<Override PartName="{p}" ContentType="{t}"/>"#))
    .chain((1..=layouts.len()).map(|i| format!(r#"<Override PartName="/ppt/slideLayouts/slideLayout{i}.xml" ContentType="application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml"/>"#)))
    .collect();
    let content_types = format!(
        r#"{XML_HEAD}<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Default Extension="png" ContentType="image/png"/><Default Extension="jpeg" ContentType="image/jpeg"/>{ct_overrides}</Types>"#
    );
    let master_rels: Vec<(String, String, String)> = (1..=layouts.len())
        .map(|i| (format!("rId{i}"), "slideLayout".to_string(), format!("../slideLayouts/slideLayout{i}.xml")))
        .chain(std::iter::once((format!("rId{}", layouts.len() + 1), "theme".to_string(), "../theme/theme1.xml".to_string())))
        .collect();
    let master_rels: Vec<(&str, &str, &str)> = master_rels.iter().map(|(a, b, c)| (a.as_str(), b.as_str(), c.as_str())).collect();

    let mut parts: Vec<(String, String)> = vec![
        ("[Content_Types].xml".into(), content_types),
        (
            "_rels/.rels".into(),
            rels(&[
                ("rId1", "officeDocument", "ppt/presentation.xml"),
                ("rId2", "../package/2006/relationships/metadata/core-properties", "docProps/core.xml"),
                ("rId3", "extended-properties", "docProps/app.xml"),
            ])
            .replace(&format!("{REL}/../package/2006/relationships/metadata/core-properties"), "http://schemas.openxmlformats.org/package/2006/relationships/metadata/core-properties"),
        ),
        (
            "docProps/core.xml".into(),
            format!(r#"{XML_HEAD}<cp:coreProperties xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:dcterms="http://purl.org/dc/terms/" xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"><dc:title>簡報</dc:title><dc:creator>ww-ppt</dc:creator></cp:coreProperties>"#),
        ),
        (
            "docProps/app.xml".into(),
            format!(r#"{XML_HEAD}<Properties xmlns="http://schemas.openxmlformats.org/officeDocument/2006/extended-properties"><Application>ww-ppt</Application><PresentationFormat>寬螢幕</PresentationFormat></Properties>"#),
        ),
        ("ppt/presentation.xml".into(), presentation()),
        (
            "ppt/_rels/presentation.xml.rels".into(),
            rels(&[
                ("rId1", "slideMaster", "slideMasters/slideMaster1.xml"),
                ("rId2", "slide", "slides/slide1.xml"),
                ("rId3", "presProps", "presProps.xml"),
                ("rId4", "viewProps", "viewProps.xml"),
                ("rId5", "theme", "theme/theme1.xml"),
            ]),
        ),
        ("ppt/presProps.xml".into(), format!("{XML_HEAD}<p:presentationPr {NS}/>")),
        (
            "ppt/viewProps.xml".into(),
            format!(r#"{XML_HEAD}<p:viewPr {NS}><p:normalViewPr horzBarState="maximized"><p:restoredLeft sz="15000"/><p:restoredTop sz="94660"/></p:normalViewPr><p:gridSpacing cx="72008" cy="72008"/></p:viewPr>"#),
        ),
        ("ppt/theme/theme1.xml".into(), theme()),
        ("ppt/slideMasters/slideMaster1.xml".into(), master()),
        ("ppt/slideMasters/_rels/slideMaster1.xml.rels".into(), rels(&master_rels)),
        ("ppt/slides/slide1.xml".into(), first_slide()),
        ("ppt/slides/_rels/slide1.xml.rels".into(), rels(&[("rId1", "slideLayout", "../slideLayouts/slideLayout1.xml")])),
    ];
    for (i, l) in layouts.into_iter().enumerate() {
        parts.push((format!("ppt/slideLayouts/slideLayout{}.xml", i + 1), l));
        parts.push((format!("ppt/slideLayouts/_rels/slideLayout{}.xml.rels", i + 1), rels(&[("rId1", "slideMaster", "../slideMasters/slideMaster1.xml")])));
    }

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    for (name, body) in parts {
        zip.start_file(name, opts).expect("in-memory zip");
        zip.write_all(body.as_bytes()).expect("in-memory zip");
    }
    zip.finish().expect("in-memory zip").into_inner()
}
