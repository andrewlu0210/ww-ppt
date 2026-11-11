//! What screen readers get: the current slide as a list of described objects
//! in reading order, and short sentences to announce.

use ppt_model::{Shape, ShapeId, ShapeKind, Slide, Xfrm, lang, plural, tr};

use crate::Editor;

/// How an object is presented to assistive technology.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum A11yRole {
    /// Title placeholders: headings of the slide.
    Heading,
    Text,
    Image,
    Table,
    Chart,
    Shape,
}

#[derive(Clone, Debug, PartialEq)]
pub struct A11yParagraph {
    pub text: String,
    pub level: u8,
    pub bullet: bool,
}

/// One object of the slide, in reading order.
#[derive(Clone, Debug, PartialEq)]
pub struct A11yItem {
    pub id: ShapeId,
    pub role: A11yRole,
    /// What it is and what it is called, e.g. "圖片「標誌」".
    pub label: String,
    pub alt_text: String,
    pub paragraphs: Vec<A11yParagraph>,
    /// Table cell texts by row.
    pub cells: Vec<Vec<String>>,
    pub xfrm: Xfrm,
    pub selected: bool,
}

/// What kind of object `s` is, in words.
fn kind_word(s: &Shape) -> &'static str {
    let l = lang();
    match &s.kind {
        ShapeKind::Picture { .. } => l.pick("圖片", "图片", "Picture"),
        ShapeKind::Table(_) => l.pick("表格", "表格", "Table"),
        ShapeKind::Chart(_) => l.pick("圖表", "图表", "Chart"),
        ShapeKind::Diagram { .. } => "SmartArt",
        ShapeKind::Opaque { .. } => l.pick("物件", "对象", "Object"),
        ShapeKind::Shape if s.geometry.is_line() => l.pick("線條", "线条", "Line"),
        ShapeKind::Shape if s.placeholder.as_ref().is_some_and(|p| matches!(p.kind.as_str(), "title" | "ctrTitle")) => l.pick("標題", "标题", "Title"),
        ShapeKind::Shape if s.placeholder.is_some() => l.pick("文字版面配置區", "文本占位符", "Text Placeholder"),
        ShapeKind::Shape if s.fill.is_none() && s.line.is_none() && s.text.is_some() => l.pick("文字方塊", "文本框", "Text Box"),
        ShapeKind::Shape => l.pick("圖案", "形状", "Shape"),
    }
}

/// Kind and name, e.g. "圖片「標誌」" / "Picture “Logo”".
fn labeled(s: &Shape) -> String {
    tr("{}「{}」", "{}“{}”", "{} “{}”", &[&kind_word(s), &s.name])
}

fn plain(s: &Shape) -> String {
    s.text_bodies().map(|t| t.plain_text()).filter(|t| !t.trim().is_empty()).collect::<Vec<_>>().join("\n")
}

/// The slide's title: its title placeholder's text.
pub fn slide_title(slide: &Slide) -> String {
    slide
        .shapes
        .iter()
        .find(|s| s.placeholder.as_ref().is_some_and(|p| matches!(p.kind.as_str(), "title" | "ctrTitle")))
        .and_then(|s| s.text.as_ref())
        .map(|t| t.plain_text().replace('\n', " ").trim().to_string())
        .unwrap_or_default()
}

/// One line about a shape, e.g. "圖案「Oval 3」，替代文字：…，文字：Ellipse".
pub fn describe(s: &Shape) -> String {
    let mut out = labeled(s);
    if s.access.decorative {
        out.push_str(&tr("，裝飾用", "，装饰性", ", decorative", &[]));
    } else if !s.access.alt_text.is_empty() {
        out.push_str(&tr("，替代文字：{}", "，替换文字：{}", ", alt text: {}", &[&s.access.alt_text]));
    }
    let text = plain(s).replace('\n', " ");
    if !text.is_empty() {
        let short: String = text.chars().take(80).collect();
        let more = if text.chars().count() > 80 { "…" } else { "" };
        out.push_str(&tr("，文字：{}{}", "，文本：{}{}", ", text: {}{}", &[&short, &more]));
    }
    out
}

impl Editor {
    /// The current slide's readable objects, back to front (PowerPoint's reading
    /// order). Hidden and decorative objects are left out.
    pub fn a11y_items(&self) -> Vec<A11yItem> {
        self.current_shapes()
            .iter()
            .filter(|s| !s.access.hidden && !s.access.decorative && !matches!(s.kind, ShapeKind::Opaque { .. }))
            .map(|s| {
                let heading = s.placeholder.as_ref().is_some_and(|p| matches!(p.kind.as_str(), "title" | "ctrTitle"));
                let role = match &s.kind {
                    _ if heading => A11yRole::Heading,
                    ShapeKind::Picture { .. } => A11yRole::Image,
                    ShapeKind::Table(_) => A11yRole::Table,
                    ShapeKind::Chart(_) => A11yRole::Chart,
                    _ if s.text.as_ref().is_some_and(|t| !t.is_empty()) => A11yRole::Text,
                    ShapeKind::Diagram { .. } => A11yRole::Text,
                    _ => A11yRole::Shape,
                };
                let paragraphs = s
                    .text_bodies()
                    .filter(|_| !matches!(s.kind, ShapeKind::Table(_)))
                    .flat_map(|t| t.paragraphs.iter())
                    .map(|p| A11yParagraph { text: p.text(), level: p.level, bullet: p.bullet.is_some() })
                    .filter(|p| !p.text.trim().is_empty())
                    .collect();
                let cells = match &s.kind {
                    ShapeKind::Table(t) => t.rows.iter().map(|r| r.cells.iter().map(|c| c.text.plain_text()).collect()).collect(),
                    _ => Vec::new(),
                };
                let mut alt_text = s.access.alt_text.clone();
                if alt_text.is_empty()
                    && let ShapeKind::Chart(c) = &s.kind
                {
                    alt_text = c.title.as_ref().map(|t| t.text.clone()).unwrap_or_default();
                }
                A11yItem {
                    id: s.id,
                    role,
                    label: labeled(s),
                    alt_text,
                    paragraphs,
                    cells,
                    xfrm: s.xfrm,
                    selected: self.selection.contains(&s.id),
                }
            })
            .collect()
    }

    /// "投影片 3／4：標題" (plus whether it is hidden), for slide changes.
    pub fn describe_slide(&self) -> String {
        let Some(slide) = self.pres.slides.get(self.slide) else { return tr("沒有投影片", "没有幻灯片", "No slides", &[]) };
        let mut out = tr("投影片 {}／{}", "幻灯片 {}/{}", "Slide {} of {}", &[&(self.slide + 1), &self.pres.slides.len()]);
        let title = slide_title(slide);
        if !title.is_empty() {
            out.push_str(&tr("：{}", "：{}", ": {}", &[&title]));
        }
        if slide.hidden {
            out.push_str(&tr("，已隱藏", "，已隐藏", ", hidden", &[]));
        }
        let n = slide.shapes.iter().filter(|s| !s.access.hidden && !matches!(s.kind, ShapeKind::Opaque { .. })).count();
        out.push_str(&tr("，{} 個物件", "，{} 个对象", plural(n, ", {} object", ", {} objects"), &[&n]));
        out
    }

    /// What is selected or being edited, in one sentence ("" when nothing is).
    pub fn describe_selection(&self) -> String {
        if let Some(edit) = &self.text
            && let Some(s) = self.pres.shape(self.slide, edit.shape)
        {
            return tr("編輯文字：{}", "编辑文本：{}", "Editing text: {}", &[&labeled(s)]);
        }
        if let Some(g) = self.selection_group() {
            let n = self.members(g).len();
            return tr("已選取 群組，{} 個物件", "已选择 组合，{} 个对象", plural(n, "Selected group, {} object", "Selected group, {} objects"), &[&n]);
        }
        match self.selected_shapes().as_slice() {
            [] => String::new(),
            [s] => tr("已選取 {}", "已选择 {}", "Selected {}", &[&describe(s)]),
            many => tr("已選取 {} 個物件", "已选择 {} 个对象", "Selected {} objects", &[&many.len()]),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::{Placeholder, Presentation, TextBody};

    #[test]
    fn titles_are_headings_and_decorative_objects_are_skipped() {
        let mut pres = Presentation::default();
        let mut title = Shape::new(pres.alloc_id(), "Title 1", Xfrm::new(0, 0, 100, 100));
        title.placeholder = Some(Placeholder { kind: "title".into(), idx: 0 });
        let mut body = TextBody::default();
        body.set_plain_text("核心目標");
        title.text = Some(body);
        let mut logo = Shape::new(pres.alloc_id(), "Logo", Xfrm::new(0, 0, 10, 10));
        logo.kind = ShapeKind::Picture { media: None, crop: Default::default() };
        logo.access.alt_text = "公司標誌".into();
        let mut frame = Shape::new(pres.alloc_id(), "Frame", Xfrm::new(0, 0, 10, 10));
        frame.access.decorative = true;
        pres.slides.push(std::sync::Arc::new(Slide { shapes: vec![title, logo, frame], ..Default::default() }));
        let mut e = Editor::new(pres);
        let items = e.a11y_items();
        assert_eq!(items.iter().map(|i| i.role).collect::<Vec<_>>(), vec![A11yRole::Heading, A11yRole::Image]);
        assert_eq!(items[0].paragraphs[0].text, "核心目標");
        assert_eq!(e.describe_slide(), "投影片 1／1：核心目標，3 個物件");
        e.selection = vec![items[1].id];
        assert_eq!(e.describe_selection(), "已選取 圖片「Logo」，替代文字：公司標誌");
    }
}
