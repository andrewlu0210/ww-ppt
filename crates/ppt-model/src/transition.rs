//! Slide transitions (`p:transition`): how a slide appears in a slide show.

/// Direction of movement: `Left` means the slide moves to the left (it comes in
/// from the right), as in the OOXML `dir` attribute.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dir {
    Left,
    Right,
    Up,
    Down,
    LeftUp,
    RightUp,
    LeftDown,
    RightDown,
}

impl Dir {
    /// Unit movement vector (x right, y down).
    pub fn vector(self) -> (f64, f64) {
        match self {
            Dir::Left => (-1.0, 0.0),
            Dir::Right => (1.0, 0.0),
            Dir::Up => (0.0, -1.0),
            Dir::Down => (0.0, 1.0),
            Dir::LeftUp => (-1.0, -1.0),
            Dir::RightUp => (1.0, -1.0),
            Dir::LeftDown => (-1.0, 1.0),
            Dir::RightDown => (1.0, 1.0),
        }
    }

    /// The OOXML value (`l`, `r`, `u`, `d`, `lu`, ...).
    pub fn code(self) -> &'static str {
        match self {
            Dir::Left => "l",
            Dir::Right => "r",
            Dir::Up => "u",
            Dir::Down => "d",
            Dir::LeftUp => "lu",
            Dir::RightUp => "ru",
            Dir::LeftDown => "ld",
            Dir::RightDown => "rd",
        }
    }

    pub fn from_code(s: &str) -> Option<Dir> {
        Some(match s {
            "l" => Dir::Left,
            "r" => Dir::Right,
            "u" => Dir::Up,
            "d" => Dir::Down,
            "lu" => Dir::LeftUp,
            "ru" => Dir::RightUp,
            "ld" => Dir::LeftDown,
            "rd" => Dir::RightDown,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TransitionKind {
    Cut { through_black: bool },
    Fade { through_black: bool },
    Push(Dir),
    Wipe(Dir),
    /// The new slide slides in over the old one.
    Cover(Dir),
    /// The old slide slides away, uncovering the new one.
    Pull(Dir),
    /// Two halves opening (`outward`) or closing, along a vertical or horizontal seam.
    Split { vertical: bool, outward: bool },
    Dissolve,
    RandomBar { vertical: bool },
    Zoom { inward: bool },
    Circle,
    /// A type that is not played as itself (it plays as a fade): its element name.
    Other(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Transition {
    pub kind: TransitionKind,
    pub duration_ms: u32,
    /// A click advances to the next slide (`advClick`).
    pub on_click: bool,
    /// Advance automatically this long after the slide appears (`advTm`).
    pub after_ms: Option<u32>,
}

impl Default for Transition {
    fn default() -> Self {
        Self { kind: TransitionKind::Fade { through_black: false }, duration_ms: 700, on_click: true, after_ms: None }
    }
}

impl Transition {
    /// Whether showing the slide animates (a cut is instant unless it goes through black).
    pub fn animates(&self) -> bool {
        !matches!(self.kind, TransitionKind::Cut { through_black: false }) && self.duration_ms > 0
    }
}

impl TransitionKind {
    /// Short id used by the UI ("fade", "push", ...).
    pub fn id(&self) -> &str {
        match self {
            TransitionKind::Cut { .. } => "cut",
            TransitionKind::Fade { .. } => "fade",
            TransitionKind::Push(_) => "push",
            TransitionKind::Wipe(_) => "wipe",
            TransitionKind::Cover(_) => "cover",
            TransitionKind::Pull(_) => "pull",
            TransitionKind::Split { .. } => "split",
            TransitionKind::Dissolve => "dissolve",
            TransitionKind::RandomBar { .. } => "randomBar",
            TransitionKind::Zoom { .. } => "zoom",
            TransitionKind::Circle => "circle",
            TransitionKind::Other(name) => name,
        }
    }

    /// The effect option as a short id ("l", "vertOut", "thruBlk", ...), "" when there is none.
    pub fn option(&self) -> String {
        match self {
            TransitionKind::Cut { through_black } | TransitionKind::Fade { through_black } => if *through_black { "thruBlk" } else { "" }.into(),
            TransitionKind::Push(d) | TransitionKind::Wipe(d) | TransitionKind::Cover(d) | TransitionKind::Pull(d) => d.code().into(),
            TransitionKind::Split { vertical, outward } => format!("{}{}", if *vertical { "vert" } else { "horz" }, if *outward { "Out" } else { "In" }),
            TransitionKind::RandomBar { vertical } => if *vertical { "vert" } else { "horz" }.into(),
            TransitionKind::Zoom { inward } => if *inward { "in" } else { "out" }.into(),
            TransitionKind::Dissolve | TransitionKind::Circle | TransitionKind::Other(_) => String::new(),
        }
    }

    /// Builds a kind from the UI ids; unknown options fall back to the type's default.
    pub fn from_ids(id: &str, option: &str) -> Option<TransitionKind> {
        let dir = |default| Dir::from_code(option).unwrap_or(default);
        let four = |default| match Dir::from_code(option) {
            Some(d @ (Dir::Left | Dir::Right | Dir::Up | Dir::Down)) => d,
            _ => default,
        };
        Some(match id {
            "cut" => TransitionKind::Cut { through_black: option == "thruBlk" },
            "fade" => TransitionKind::Fade { through_black: option == "thruBlk" },
            "push" => TransitionKind::Push(four(Dir::Up)),
            "wipe" => TransitionKind::Wipe(four(Dir::Left)),
            "cover" => TransitionKind::Cover(dir(Dir::Left)),
            "pull" => TransitionKind::Pull(dir(Dir::Left)),
            "split" => TransitionKind::Split { vertical: option.starts_with("vert"), outward: !option.ends_with("In") },
            "dissolve" => TransitionKind::Dissolve,
            "randomBar" => TransitionKind::RandomBar { vertical: option == "vert" },
            "zoom" => TransitionKind::Zoom { inward: option == "in" },
            "circle" => TransitionKind::Circle,
            _ => return None,
        })
    }
}

/// PowerPoint's durations for the `spd` attribute.
pub fn speed_ms(spd: &str) -> u32 {
    match spd {
        "slow" => 1000,
        "med" => 750,
        _ => 500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip() {
        for k in [
            TransitionKind::Fade { through_black: true },
            TransitionKind::Push(Dir::Down),
            TransitionKind::Cover(Dir::LeftUp),
            TransitionKind::Split { vertical: true, outward: false },
            TransitionKind::RandomBar { vertical: true },
            TransitionKind::Zoom { inward: true },
            TransitionKind::Circle,
        ] {
            assert_eq!(TransitionKind::from_ids(k.id(), &k.option()), Some(k.clone()));
        }
        // Push and wipe only go in four directions.
        assert_eq!(TransitionKind::from_ids("push", "lu"), Some(TransitionKind::Push(Dir::Up)));
        assert!(!Transition { kind: TransitionKind::Cut { through_black: false }, ..Default::default() }.animates());
    }
}
