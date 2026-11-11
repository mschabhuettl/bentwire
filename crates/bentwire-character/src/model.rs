//! Manifest model (character-format 4). Plain data; parsing is in `parse`, writing in `write`.

use std::collections::BTreeMap;

/// Rule family (character-format 7).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Family {
    Agent,
    Office97,
}

impl Family {
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Agent => "agent",
            Family::Office97 => "office97",
        }
    }
}

/// The whole `character.toml`.
#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub format: i64,
    pub family: Family,
    pub character: CharacterSection,
    pub balloon: Option<Balloon>,
    pub voice: Option<Voice>,
    pub icon: Option<IconRef>,
    pub sheets: Vec<Sheet>,
    /// Sound id to file path, in manifest order.
    pub sounds: Vec<(String, String)>,
    /// State name to animation names, in manifest order.
    pub states: Vec<(String, Vec<String>)>,
    /// Pose id to layers (office97; agent manifests may use poses too).
    pub poses: BTreeMap<u32, Vec<Layer>>,
    pub office97: Option<Office97>,
    pub animations: Vec<Animation>,
    /// `[source]`, free form; not used for playback except `missing_animations`.
    pub source: Option<toml::Table>,
    /// Paths of unknown keys not starting with `x-` (warning in the loader, error in the validator).
    pub unknown_keys: Vec<String>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CharacterSection {
    /// GUID `{...}`.
    pub id: Option<String>,
    pub canvas: [u32; 2],
    pub display: Option<[u32; 2]>,
    pub standard_animation_set: bool,
    pub info: Vec<Info>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub lang: String,
    pub name: String,
    pub description: Option<String>,
    pub extra: Option<String>,
}

/// `[balloon]`; every key optional, raw values kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Balloon {
    pub enabled: Option<bool>,
    pub lines: Option<i64>,
    pub chars_per_line: Option<i64>,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub border: Option<String>,
    pub font: Option<String>,
    pub font_height: Option<i64>,
    pub font_weight: Option<i64>,
    pub italic: Option<bool>,
    pub font_flag2: Option<i64>,
    pub size_to_text: Option<bool>,
    pub auto_hide: Option<bool>,
    pub auto_pace: Option<bool>,
}

/// `[voice]`; every key optional, raw values kept.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Voice {
    pub engine: Option<String>,
    pub mode: Option<String>,
    pub speed: Option<i64>,
    pub pitch: Option<i64>,
    pub language: Option<String>,
    pub dialect: Option<String>,
    pub gender: Option<i64>,
    pub age: Option<i64>,
    pub style: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconRef {
    pub file: String,
}

/// `[[sheets]]` (character-format 5.1, 8).
#[derive(Clone, Debug, PartialEq)]
pub struct Sheet {
    pub id: String,
    pub file: String,
    /// Cell size in pixels of `file`.
    pub cell: [u32; 2],
    pub count: u32,
    /// True picture size per index, pixels of `file`; absent = cell size.
    pub sizes: Option<Vec<[u32; 2]>>,
    /// Pixels of `file` per canvas unit (default 1).
    pub scale: Option<u32>,
    /// Density factor to file: the same grid with every dimension multiplied by the key.
    pub hidpi: BTreeMap<u32, String>,
}

/// Reference to picture `index` of sheet `sheet`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageRef {
    pub sheet: String,
    pub index: u32,
}

/// One layer: `"sheet:n"`, `["sheet:n", x, y]` or `["sheet:n", x, y, w, h]`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Layer {
    pub image: ImageRef,
    pub x: i32,
    pub y: i32,
    /// Explicit size in canvas units.
    pub size: Option<[u32; 2]>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Transition {
    Return,
    Exit,
    None,
}

impl Transition {
    pub fn as_str(self) -> &'static str {
        match self {
            Transition::Return => "return",
            Transition::Exit => "exit",
            Transition::None => "none",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Animation {
    pub name: String,
    /// agent only.
    pub transition: Option<Transition>,
    /// agent only: return animation name.
    pub return_animation: Option<String>,
    pub frames: Vec<Frame>,
}

/// Frame exit target (agent).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Exit {
    /// `"end"` (ACS -1).
    End,
    /// Frame index.
    Frame(u32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    /// `[target, p]`: percent for agent, n/32768 for office97.
    Random { target: u32, p: u32 },
    /// `[target, "daypart", v]` (office97).
    Daypart { target: u32, value: u32 },
    /// `[target, "host", v]` (office97).
    Host { target: u32, value: u32 },
}

impl Branch {
    pub fn target(&self) -> u32 {
        match *self {
            Branch::Random { target, .. }
            | Branch::Daypart { target, .. }
            | Branch::Host { target, .. } => target,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Frame {
    pub ms: u32,
    /// Back to front. Empty when absent.
    pub layers: Vec<Layer>,
    /// Instead of `layers`: id into `[poses]`.
    pub pose: Option<u32>,
    pub sound: Option<String>,
    /// office97 raw sound argument.
    pub sound_arg: Option<u32>,
    pub branch: Vec<Branch>,
    /// agent; `None` = continue normally (ACS -2).
    pub exit: Option<Exit>,
    /// agent.
    pub mouths: Vec<Mouth>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mouth {
    /// 0..6, phoneme class.
    pub kind: u8,
    pub layer: Layer,
    /// Mouth point in canvas coordinates.
    pub anchor: Option<[i32; 2]>,
    /// Replaces the frame layers under the mouth when present.
    pub base: Option<Vec<Layer>>,
}

/// `[office97]` (character-format 7.3).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Office97 {
    pub rest_pose: Option<u32>,
    pub idle: String,
    /// `[from_pose, to_pose, state]`.
    pub transitions: Vec<(u32, u32, String)>,
    /// State name to MsoAnimationType id, manifest order.
    pub ids: Vec<(String, u32)>,
    /// String group id to strings.
    pub strings: BTreeMap<u32, Vec<String>>,
}

/// The 16 Agent state names in canonical mixed case (character-format 4).
pub const AGENT_STATES: [&str; 16] = [
    "Showing",
    "Hiding",
    "Speaking",
    "Moving",
    "MovingUp",
    "MovingDown",
    "MovingLeft",
    "MovingRight",
    "Gesturing",
    "GesturingUp",
    "GesturingDown",
    "GesturingLeft",
    "GesturingRight",
    "IdlingLevel1",
    "IdlingLevel2",
    "IdlingLevel3",
];

impl Manifest {
    /// Display size in pixels (default = canvas).
    pub fn display(&self) -> [u32; 2] {
        self.character.display.unwrap_or(self.character.canvas)
    }

    pub fn canvas(&self) -> [u32; 2] {
        self.character.canvas
    }

    /// Animation by name, ignoring ASCII case.
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
    }

    pub fn animation_index(&self, name: &str) -> Option<usize> {
        self.animations
            .iter()
            .position(|a| a.name.eq_ignore_ascii_case(name))
    }

    /// State members by state name, ignoring ASCII case.
    pub fn state(&self, name: &str) -> Option<&[String]> {
        self.states
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_slice())
    }

    pub fn sound_file(&self, id: &str) -> Option<&str> {
        self.sounds
            .iter()
            .find(|(k, _)| k == id)
            .map(|(_, v)| v.as_str())
    }

    pub fn sheet(&self, id: &str) -> Option<&Sheet> {
        self.sheets.iter().find(|s| s.id == id)
    }

    /// `source.missing_animations`, as listed.
    pub fn missing_animations(&self) -> Vec<String> {
        self.source_strings("missing_animations")
    }

    /// `source.unplayable_sounds`, as listed.
    pub fn unplayable_sounds(&self) -> Vec<String> {
        self.source_strings("unplayable_sounds")
    }

    fn source_strings(&self, key: &str) -> Vec<String> {
        self.source
            .as_ref()
            .and_then(|s| s.get(key))
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// True when `name` is listed in `source.missing_animations` (ASCII case ignored).
    pub fn is_missing(&self, name: &str) -> bool {
        self.missing_animations()
            .iter()
            .any(|m| m.eq_ignore_ascii_case(name))
    }

    /// The layers a frame shows: its pose's layers, or its own.
    pub fn frame_layers<'a>(&'a self, frame: &'a Frame) -> &'a [Layer] {
        match frame.pose {
            Some(p) => self.poses.get(&p).map(Vec::as_slice).unwrap_or(&[]),
            None => &frame.layers,
        }
    }
}

impl Frame {
    /// Agent invisible branch point: no layers and `ms = 0` (character-format 7.2).
    pub fn is_invisible_branch_point(&self) -> bool {
        self.layers.is_empty() && self.pose.is_none() && self.ms == 0
    }
}
