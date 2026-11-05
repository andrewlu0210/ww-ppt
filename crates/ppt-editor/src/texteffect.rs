//! Text effects of the Format pane's "Text Options": the outline and the shadow of the selected
//! text (the selection while editing, else all text of the selected shapes).

use ppt_model::{Color, RunStyle, Shadow, TextOutline};

use crate::formatinfo::{ShadowSpec, default_shadow};
use crate::{Editor, TextEngine};

/// Changes to the text outline; what is `None` stays as it is.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextOutlineSpec {
    /// `Some(false)`: no outline. `Some(true)`: an outline (a default one if there was none).
    pub visible: Option<bool>,
    pub rgb: Option<(u8, u8, u8)>,
    /// New opacity, 0 (clear) to 255.
    pub alpha: Option<u8>,
    /// Line width in EMU.
    pub width: Option<i64>,
}

/// Changes to the outline and shadow of text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TextEffectSpec {
    pub outline: TextOutlineSpec,
    pub shadow: ShadowSpec,
}

/// What the pane shows: `None` when the selected text differs, `Some(None)` when it has none.
#[derive(Clone, Debug, PartialEq)]
pub struct TextEffectInfo {
    pub outline: Option<Option<TextOutline>>,
    pub shadow: Option<Option<Shadow>>,
}

impl TextEffectSpec {
    /// Applies the changes to one run's style.
    fn apply(&self, s: &mut RunStyle) {
        let o = &self.outline;
        if o.visible == Some(false) {
            s.outline = None;
        } else {
            let wants = o.visible == Some(true) || o.rgb.is_some() || o.alpha.is_some() || o.width.is_some();
            // A new outline is the theme blue at 1 pt: in the text's own color it would not show.
            if let Some(mut outline) = s.outline.or_else(|| wants.then_some(TextOutline { color: Color::rgb(0x44, 0x72, 0xC4), width: 12_700 })) {
                if let Some((r, g, b)) = o.rgb {
                    outline.color = Color { r, g, b, a: outline.color.a };
                }
                if let Some(a) = o.alpha {
                    outline.color.a = a;
                }
                if let Some(w) = o.width {
                    outline.width = w.max(0);
                }
                s.outline = Some(outline);
            }
        }
        let sh = &self.shadow;
        if sh.visible == Some(false) {
            s.shadow = None;
        } else {
            let wants = sh.visible == Some(true) || sh.rgb.is_some() || sh.alpha.is_some() || sh.blur.is_some() || sh.dist.is_some() || sh.dir.is_some();
            if let Some(mut shadow) = s.shadow.or_else(|| wants.then(default_shadow)) {
                if let Some((r, g, b)) = sh.rgb {
                    shadow.color = Color { r, g, b, a: shadow.color.a };
                }
                if let Some(a) = sh.alpha {
                    shadow.color.a = a;
                }
                if let Some(v) = sh.blur {
                    shadow.blur = v.max(0);
                }
                if let Some(v) = sh.dist {
                    shadow.dist = v.max(0);
                }
                if let Some(v) = sh.dir {
                    shadow.dir = v.rem_euclid(360.0);
                }
                s.shadow = Some(shadow);
            }
        }
    }
}

impl Editor {
    /// The run styles the text effects apply to.
    fn effect_target_styles(&self) -> Vec<RunStyle> {
        if let Some(edit) = &self.text {
            let r = edit.rich.range();
            if r.is_empty() {
                return vec![edit.rich.typing_style()];
            }
            return edit.rich.body.styles_in(r);
        }
        if self.cell_range.is_some() {
            return self.range_text_body().map(|b| vec![b.style_at(0)]).unwrap_or_default();
        }
        let mut out: Vec<RunStyle> = Vec::new();
        for body in self.selected_shapes().into_iter().flat_map(|s| s.text_bodies()) {
            let styles = body.styles_in(0..body.len());
            for s in if styles.is_empty() { vec![body.style_at(0)] } else { styles } {
                if !out.contains(&s) {
                    out.push(s);
                }
            }
        }
        out
    }

    /// The outline and shadow of the target text; `None` when there is no text to format.
    pub fn text_effect_info(&self) -> Option<TextEffectInfo> {
        let styles = self.effect_target_styles();
        let first = styles.first()?;
        Some(TextEffectInfo {
            outline: styles.iter().all(|s| s.outline == first.outline).then_some(first.outline),
            shadow: styles.iter().all(|s| s.shadow == first.shadow).then_some(first.shadow),
        })
    }

    /// Changes the text outline and shadow, as one undo step; nothing happens when nothing would change.
    pub fn set_text_effect_spec(&mut self, engine: &mut TextEngine, spec: TextEffectSpec) -> bool {
        let changes = self.effect_target_styles().iter().any(|s| {
            let mut copy = s.clone();
            spec.apply(&mut copy);
            copy != *s
        });
        changes && self.apply_text_style(engine, |s| spec.apply(s))
    }
}
