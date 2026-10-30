//! Slide show navigation: which slide is on screen, how many of its
//! animation clicks have played, hidden slides, the end screen and blanking.
//! Rendering, timing and input mapping belong to the host.

use ppt_model::Presentation;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blank {
    Black,
    White,
}

/// What the show displays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Slide(usize),
    /// The black "end of slide show" screen after the last slide.
    End,
}

#[derive(Clone, Debug)]
pub struct SlideShow {
    /// Hidden flags per slide, captured when the show starts.
    hidden: Vec<bool>,
    /// Animation clicks per slide.
    clicks: Vec<usize>,
    screen: Screen,
    /// Clicks played on the current slide.
    step: usize,
    blank: Option<Blank>,
}

impl SlideShow {
    /// Starts at slide `start`. A hidden start slide is still shown (as
    /// PowerPoint does when starting from it); `None` when there are no slides.
    pub fn new(pres: &Presentation, start: usize) -> Option<Self> {
        let hidden: Vec<bool> = pres.slides.iter().map(|s| s.hidden).collect();
        if hidden.is_empty() {
            return None;
        }
        let clicks = pres.slides.iter().map(|s| s.timing.as_deref().map_or(0, |t| ppt_model::anim::Player::new(t).clicks())).collect();
        Some(Self { screen: Screen::Slide(start.min(hidden.len() - 1)), hidden, clicks, step: 0, blank: None })
    }

    /// Starts at the first slide that is not hidden.
    pub fn from_beginning(pres: &Presentation) -> Option<Self> {
        let mut show = Self::new(pres, 0)?;
        if show.hidden[0] {
            show.screen = show.visible_after(0).map_or(Screen::End, Screen::Slide);
        }
        Some(show)
    }

    pub fn screen(&self) -> Screen {
        self.screen
    }

    pub fn blank(&self) -> Option<Blank> {
        self.blank
    }

    pub fn slide_count(&self) -> usize {
        self.hidden.len()
    }

    /// Animation clicks played on the current slide.
    pub fn step(&self) -> usize {
        self.step
    }

    /// Animation clicks the current slide has.
    pub fn steps(&self) -> usize {
        match self.screen {
            Screen::Slide(i) => self.clicks.get(i).copied().unwrap_or(0),
            Screen::End => 0,
        }
    }

    fn show_slide(&mut self, screen: Screen, step: usize) {
        self.screen = screen;
        self.step = step;
    }

    fn visible_after(&self, i: usize) -> Option<usize> {
        (i + 1..self.hidden.len()).find(|&j| !self.hidden[j])
    }

    fn visible_before(&self, i: usize) -> Option<usize> {
        (0..i).rev().find(|&j| !self.hidden[j])
    }

    /// Plays the next animation click, or advances to the next slide when the
    /// current one has played them all. Returns false when already on the end
    /// screen (the host ends the show).
    pub fn advance(&mut self) -> bool {
        if self.blank.take().is_some() {
            return true;
        }
        if self.step < self.steps() {
            self.step += 1;
            return true;
        }
        match self.screen {
            Screen::Slide(i) => {
                self.show_slide(self.visible_after(i).map_or(Screen::End, Screen::Slide), 0);
                true
            }
            Screen::End => false,
        }
    }

    /// Undoes the last animation click, or goes back to the previous visible
    /// slide with all its animations played; stays on the first slide.
    pub fn back(&mut self) {
        if self.blank.take().is_some() {
            return;
        }
        if self.step > 0 {
            self.step -= 1;
            return;
        }
        let from = match self.screen {
            Screen::Slide(i) => i,
            Screen::End => self.hidden.len(),
        };
        if let Some(j) = self.visible_before(from) {
            self.show_slide(Screen::Slide(j), self.clicks[j]);
        }
    }

    /// Jumps to slide `index` (0-based), hidden or not.
    pub fn goto(&mut self, index: usize) {
        if index < self.hidden.len() {
            self.blank = None;
            self.show_slide(Screen::Slide(index), 0);
        }
    }

    pub fn first(&mut self) {
        self.blank = None;
        if let Some(i) = (0..self.hidden.len()).find(|&j| !self.hidden[j]) {
            self.show_slide(Screen::Slide(i), 0);
        }
    }

    pub fn last(&mut self) {
        self.blank = None;
        if let Some(i) = self.visible_before(self.hidden.len()) {
            self.show_slide(Screen::Slide(i), 0);
        }
    }

    /// What [`SlideShow::advance`] would show next (ignoring blanking): the presenter's preview.
    pub fn upcoming(&self) -> Screen {
        match self.screen {
            Screen::Slide(i) => self.visible_after(i).map_or(Screen::End, Screen::Slide),
            Screen::End => Screen::End,
        }
    }

    /// Sets the exact state, e.g. to mirror another window's show.
    pub fn set(&mut self, screen: Screen, blank: Option<Blank>) {
        if let Screen::Slide(i) = screen
            && i >= self.hidden.len()
        {
            return;
        }
        self.screen = screen;
        self.blank = blank;
        self.step = self.step.min(self.steps());
    }

    /// Sets the clicks played on the current slide, e.g. to mirror another window.
    pub fn set_step(&mut self, step: usize) {
        self.step = step.min(self.steps());
    }

    /// Blanks the screen with `kind`, or shows the slide again if it is already blanked that way.
    pub fn toggle_blank(&mut self, kind: Blank) {
        self.blank = if self.blank == Some(kind) { None } else { Some(kind) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ppt_model::Slide;

    fn deck(hidden: &[bool]) -> Presentation {
        let mut pres = Presentation::default();
        pres.slides = hidden.iter().map(|&h| Slide { hidden: h, ..Default::default() }).collect();
        pres
    }

    #[test]
    fn walks_to_the_end_screen_and_stops() {
        let pres = deck(&[false, false]);
        let mut s = SlideShow::from_beginning(&pres).unwrap();
        assert_eq!(s.screen(), Screen::Slide(0));
        assert!(s.advance());
        assert_eq!(s.screen(), Screen::Slide(1));
        assert!(s.advance());
        assert_eq!(s.screen(), Screen::End);
        assert!(!s.advance());
        s.back();
        assert_eq!(s.screen(), Screen::Slide(1));
        s.back();
        s.back();
        assert_eq!(s.screen(), Screen::Slide(0));
    }

    #[test]
    fn hidden_slides_are_skipped_unless_jumped_to() {
        let pres = deck(&[true, false, true, false, true]);
        let mut s = SlideShow::from_beginning(&pres).unwrap();
        assert_eq!(s.screen(), Screen::Slide(1));
        s.advance();
        assert_eq!(s.screen(), Screen::Slide(3));
        s.advance();
        assert_eq!(s.screen(), Screen::End);
        s.back();
        assert_eq!(s.screen(), Screen::Slide(3));
        s.goto(2);
        assert_eq!(s.screen(), Screen::Slide(2));
        s.advance();
        assert_eq!(s.screen(), Screen::Slide(3));
        s.first();
        assert_eq!(s.screen(), Screen::Slide(1));
        s.last();
        assert_eq!(s.screen(), Screen::Slide(3));
        // Starting from a hidden slide shows it.
        assert_eq!(SlideShow::new(&pres, 2).unwrap().screen(), Screen::Slide(2));
    }

    #[test]
    fn upcoming_skips_hidden_and_mirroring_sets_state() {
        let pres = deck(&[false, true, false]);
        let mut s = SlideShow::new(&pres, 0).unwrap();
        assert_eq!(s.upcoming(), Screen::Slide(2));
        s.goto(2);
        assert_eq!(s.upcoming(), Screen::End);
        s.set(Screen::Slide(1), Some(Blank::White));
        assert_eq!((s.screen(), s.blank()), (Screen::Slide(1), Some(Blank::White)));
        s.set(Screen::Slide(9), None);
        assert_eq!(s.screen(), Screen::Slide(1));
        s.set(Screen::End, None);
        assert_eq!(s.screen(), Screen::End);
    }

    #[test]
    fn blanking_pauses_navigation() {
        let pres = deck(&[false, false]);
        let mut s = SlideShow::new(&pres, 0).unwrap();
        s.toggle_blank(Blank::Black);
        assert_eq!(s.blank(), Some(Blank::Black));
        s.toggle_blank(Blank::White);
        assert_eq!(s.blank(), Some(Blank::White));
        // The first key after blanking only returns to the slide.
        assert!(s.advance());
        assert_eq!((s.blank(), s.screen()), (None, Screen::Slide(0)));
        s.toggle_blank(Blank::Black);
        s.toggle_blank(Blank::Black);
        assert_eq!(s.blank(), None);
    }

    #[test]
    fn animation_clicks_come_before_the_next_slide() {
        use ppt_model::anim::{Behavior, NodeKind, Target, TimeNode, Timing, Value};
        let appear = |id| TimeNode {
            kind: NodeKind::Behavior(Target { element: ppt_model::ShapeId(id), shapes: vec![ppt_model::ShapeId(id)], paragraphs: None }, Behavior::Set { attr: "style.visibility".into(), to: Value::Str("visible".into()) }),
            dur_ms: Some(1.0),
            ..Default::default()
        };
        let click = |id| TimeNode { click: true, children: vec![TimeNode { children: vec![appear(id)], ..Default::default() }], ..Default::default() };
        let main = TimeNode { kind: NodeKind::Seq, node_type: "mainSeq".into(), children: vec![click(1), click(2)], ..Default::default() };
        let mut pres = deck(&[false, false]);
        pres.slides[0].timing = Some(std::sync::Arc::new(Timing { root: TimeNode { children: vec![main], ..Default::default() }, ..Default::default() }));
        let mut s = SlideShow::new(&pres, 0).unwrap();
        assert_eq!((s.step(), s.steps()), (0, 2));
        s.advance();
        s.advance();
        assert_eq!((s.screen(), s.step()), (Screen::Slide(0), 2));
        s.advance();
        assert_eq!((s.screen(), s.step()), (Screen::Slide(1), 0));
        // Back to the previous slide shows it with everything played; then clicks are undone.
        s.back();
        assert_eq!((s.screen(), s.step()), (Screen::Slide(0), 2));
        s.back();
        assert_eq!(s.step(), 1);
        s.goto(0);
        assert_eq!(s.step(), 0);
    }

    #[test]
    fn empty_deck_has_no_show() {
        assert!(SlideShow::from_beginning(&deck(&[])).is_none());
        assert!(SlideShow::from_beginning(&deck(&[true])).is_some_and(|s| s.screen() == Screen::End));
    }
}
