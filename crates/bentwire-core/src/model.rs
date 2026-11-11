//! The core's own playback model of a character (plain data).
//!
//! The daemon builds it from a `bentwire-character` manifest through an adapter. Facts
//! that need pixels or audio are precomputed by the adapter: sound durations, mouth
//! anchors (B-BAL-4, see [`mouth_point_from_bbox`]) and unavailable animations
//! (B-CHAR-2).

/// Playback family (character-format 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Agent,
    Office97,
}

/// One `[[character.info]]` entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InfoEntry {
    pub lang: String,
    pub name: String,
    pub description: String,
}

/// `[balloon]` of the manifest, with the R-BAL-1/R-BAL-2 defaults applied by the adapter
/// or by [`BalloonStyle::default`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalloonStyle {
    pub enabled: bool,
    /// 0 means 2 (R-BAL-1).
    pub lines: u32,
    /// 0 means 28 (R-BAL-1).
    pub chars_per_line: u32,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub border: [u8; 3],
    pub font: String,
    pub font_height: i32,
    pub font_weight: u32,
    pub italic: bool,
    pub size_to_text: bool,
    pub auto_hide: bool,
    pub auto_pace: bool,
}

impl Default for BalloonStyle {
    fn default() -> Self {
        BalloonStyle {
            enabled: true,
            lines: 2,
            chars_per_line: 28,
            fg: [0, 0, 0],
            bg: [0xFF, 0xFF, 0xE1],
            border: [0, 0, 0],
            font: "MS Sans Serif".into(),
            font_height: -13,
            font_weight: 400,
            italic: false,
            size_to_text: false,
            auto_hide: true,
            auto_pace: true,
        }
    }
}

/// Animation-level `transition` key (agent family).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Transition {
    Return,
    Exit,
    #[default]
    None,
}

/// Frame `exit` key (agent family).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Exit {
    /// Absent: continue normally (ACS -2).
    #[default]
    None,
    /// `"end"` (ACS -1).
    End,
    /// A frame index.
    To(usize),
}

/// A frame branch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    /// `[target, p]`: percent for agent, n/32768 for office97.
    Prob { target: usize, p: u32 },
    /// office97 `[target, "daypart", v]`.
    Daypart { target: usize, period: u8 },
    /// office97 `[target, "host", v]`; never taken in v1 (B-O97-1).
    Host { target: usize, value: i32 },
}

impl Branch {
    pub fn target(&self) -> usize {
        match *self {
            Branch::Prob { target, .. }
            | Branch::Daypart { target, .. }
            | Branch::Host { target, .. } => target,
        }
    }
}

/// A mouth overlay (agent family). `anchor` is already resolved (B-BAL-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mouth {
    pub kind: u8,
    /// Mouth point in canvas coordinates.
    pub anchor: (i32, i32),
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Frame {
    pub ms: u32,
    /// The frame has `layers` (agent) or a `pose` (office97). Frames without either are
    /// empty/blank.
    pub has_picture: bool,
    /// office97 pose id.
    pub pose: Option<u32>,
    /// Sound id from `[sounds]`.
    pub sound: Option<String>,
    pub branches: Vec<Branch>,
    pub exit: Exit,
    pub mouths: Vec<Mouth>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Animation {
    pub name: String,
    pub frames: Vec<Frame>,
    pub transition: Transition,
    pub return_animation: Option<String>,
    /// False when a picture or sound it uses failed to decode (B-CHAR-2).
    pub available: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sound {
    pub id: String,
    /// Decoded duration; `None` if the sound cannot play (completion does not wait).
    pub duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Office97 {
    pub rest_pose: Option<u32>,
    /// State name of the idle group.
    pub idle: String,
    /// `[from_pose, to_pose, state]`.
    pub transitions: Vec<(u32, u32, String)>,
}

/// The playback model of one loaded character.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CharacterModel {
    pub id: String,
    pub family: Family,
    pub canvas: (u32, u32),
    pub display: (u32, u32),
    pub info: Vec<InfoEntry>,
    pub balloon: BalloonStyle,
    /// `[states]` in manifest order.
    pub states: Vec<(String, Vec<String>)>,
    /// `[[animations]]` in manifest order. Animations in `source.missing_animations`
    /// are simply not in this list.
    pub animations: Vec<Animation>,
    pub sounds: Vec<Sound>,
    pub office97: Option<Office97>,
}

impl CharacterModel {
    /// A minimal empty character, for tests and builders.
    pub fn new(id: &str, family: Family, size: (u32, u32)) -> Self {
        CharacterModel {
            id: id.into(),
            family,
            canvas: size,
            display: size,
            info: Vec::new(),
            balloon: BalloonStyle::default(),
            states: Vec::new(),
            animations: Vec::new(),
            sounds: Vec::new(),
            office97: if family == Family::Office97 {
                Some(Office97::default())
            } else {
                None
            },
        }
    }

    /// Index of an animation, ignoring ASCII case.
    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// Members of a state, ignoring ASCII case (R-STATE-2).
    pub fn state(&self, name: &str) -> Option<&[String]> {
        self.states
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, m)| m.as_slice())
    }

    pub fn sound(&self, id: &str) -> Option<&Sound> {
        self.sounds.iter().find(|s| s.id == id)
    }

    /// `AnimationNames`: manifest names, then office97 group names (5.3).
    pub fn animation_names(&self) -> Vec<String> {
        let mut v: Vec<String> = self.animations.iter().map(|a| a.name.clone()).collect();
        if self.family == Family::Office97 {
            for (n, _) in &self.states {
                if !v.iter().any(|x| x.eq_ignore_ascii_case(n)) {
                    v.push(n.clone());
                }
            }
        }
        v
    }
}

/// B-BAL-4: the mouth point of a mouth without `anchor` is the centre of the mouth
/// layer's opaque bounding box `(x0, y0, x1, y1)` (exclusive right/bottom), floored, in
/// canvas coordinates.
pub fn mouth_point_from_bbox(x0: i32, y0: i32, x1: i32, y1: i32) -> (i32, i32) {
    ((x0 + x1 - 1).div_euclid(2), (y0 + y1 - 1).div_euclid(2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_bal_4_mouth_point_from_bbox() {
        // Opaque pixels x 10..=19, y 4..=8: centre (14.5, 6) floored.
        assert_eq!(mouth_point_from_bbox(10, 4, 20, 9), (14, 6));
        assert_eq!(mouth_point_from_bbox(0, 0, 1, 1), (0, 0));
        assert_eq!(mouth_point_from_bbox(-3, -3, 0, 0), (-2, -2));
    }

    #[test]
    fn r_bal_2_defaults() {
        let b = BalloonStyle::default();
        assert_eq!((b.lines, b.chars_per_line), (2, 28));
        assert_eq!(
            (b.fg, b.bg, b.border),
            ([0, 0, 0], [0xFF, 0xFF, 0xE1], [0, 0, 0])
        );
        assert_eq!(
            (b.font.as_str(), b.font_height, b.font_weight),
            ("MS Sans Serif", -13, 400)
        );
    }

    #[test]
    fn r_state_2_names_ignore_case() {
        let mut m = CharacterModel::new("x", Family::Agent, (1, 1));
        m.states.push(("Showing".into(), vec!["A".into()]));
        assert!(m.state("SHOWING").is_some());
        m.animations.push(Animation {
            name: "Wave".into(),
            ..Default::default()
        });
        assert_eq!(m.animation_index("wAVE"), Some(0));
    }
}
