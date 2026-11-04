//! 設計: slide backgrounds, and swapping in a restyled deck (theme variants,
//! slide size) produced from the package by the host.

use std::sync::Arc;

use ppt_model::{Background, Presentation};
use ppt_text::TextEngine;

use crate::Editor;

impl Editor {
    /// The current slide's background as shown, and whether it is its own (not the layout's).
    pub fn background(&self) -> Option<(&Background, bool)> {
        let slide = self.pres.slides.get(self.slide)?;
        Some((self.pres.slide_background(slide), slide.background.is_some()))
    }

    /// Gives the current slide its own background; `None` goes back to the layout's.
    pub fn set_background(&mut self, engine: &mut TextEngine, bg: Option<Background>) -> bool {
        self.end_text_edit(engine);
        let Some(slide) = self.pres.slides.get(self.slide) else { return false };
        if slide.background == bg {
            return false;
        }
        let before = self.snapshot();
        self.pres.slides[self.slide].background = bg;
        self.push_undo(before);
        true
    }

    /// Sets the current slide's background to a new picture (encoded `bytes` stored under `key`).
    pub fn set_background_picture(&mut self, engine: &mut TextEngine, key: String, bytes: Arc<Vec<u8>>) -> bool {
        if self.pres.slides.is_empty() {
            return false;
        }
        self.pres.media.insert(key.clone(), bytes);
        self.set_background(engine, Some(Background::Picture { media: key }))
    }

    pub fn set_hide_background_graphics(&mut self, engine: &mut TextEngine, hide: bool) -> bool {
        self.end_text_edit(engine);
        let Some(slide) = self.pres.slides.get(self.slide) else { return false };
        if slide.hide_background_graphics == hide {
            return false;
        }
        let before = self.snapshot();
        self.pres.slides[self.slide].hide_background_graphics = hide;
        self.push_undo(before);
        true
    }

    /// Hides slide `index` from the slide show, or shows it again (`p:sld/@show`).
    pub fn set_slide_hidden(&mut self, engine: &mut TextEngine, index: usize, hidden: bool) -> bool {
        self.end_text_edit(engine);
        let Some(slide) = self.pres.slides.get(index) else { return false };
        if slide.hidden == hidden {
            return false;
        }
        let before = self.snapshot();
        self.pres.slides[index].hidden = hidden;
        self.push_undo(before);
        true
    }

    /// Gives every slide the current slide's background settings (套用到全部).
    pub fn apply_background_to_all(&mut self, engine: &mut TextEngine) -> bool {
        self.end_text_edit(engine);
        let Some(cur) = self.pres.slides.get(self.slide) else { return false };
        let (bg, hide) = (cur.background.clone(), cur.hide_background_graphics);
        if self.pres.slides.iter().all(|s| s.background == bg && s.hide_background_graphics == hide) {
            return false;
        }
        let before = self.snapshot();
        for s in &mut self.pres.slides {
            s.background = bg.clone();
            s.hide_background_graphics = hide;
        }
        self.push_undo(before);
        true
    }

    /// Replaces the whole deck with `pres` (a restyled copy of it) as one undo step.
    pub fn replace_document(&mut self, engine: &mut TextEngine, pres: Presentation) {
        self.end_text_edit(engine);
        let before = self.snapshot();
        self.pres = pres;
        self.slide = self.slide.min(self.pres.slides.len().saturating_sub(1));
        self.selection.clear();
        self.entered_group = None;
        self.cell_range = None;
        self.crop = None;
        self.push_undo(before);
        self.refresh_tables(engine);
    }
}
