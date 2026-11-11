//! Public API of the deterministic core (spec section 2, B-CORE-1).
//!
//! Driving contract for the daemon:
//! - Every input goes through [`crate::Core::handle`] with the current clock value.
//! - Timers: ask [`crate::Core::next_deadline`] and call [`crate::Core::tick`] when it
//!   is reached (a late tick is processed late, as on a real system). A fake clock
//!   (headless `Test1.Advance`) uses [`crate::Core::advance_to`], which visits every
//!   deadline in turn so frame clocks never see artificial lateness.
//! - The returned [`Output`] list is ordered and the order is part of the contract:
//!   a method reply comes before any signal about its request (B-Q-1), AskComplete
//!   before RequestComplete.
//! - Level state (surfaces, balloons, menu, tray) is read with [`crate::Core::scene`]
//!   after each call; properties with [`crate::Core::properties`] (diff them to emit
//!   `PropertiesChanged`).

use std::sync::Arc;

use crate::model::{CharacterModel, InfoEntry};

/// Milliseconds on the core's monotonic clock (B-CORE-3).
pub type Ms = u64;

/// B-CORE-2: the single random source. Returns a uniform integer in `[0, n)`, `n >= 1`.
pub trait Draws {
    fn draw(&mut self, n: u32) -> u32;
}

/// Font request for text measurement and drawing.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontSpec {
    /// Family name (Agent balloon: character or config font; Office balloon: Tahoma).
    pub family: String,
    /// Size in px: the em height, or the cell height (ascent + descent) when `cell`.
    pub px: u32,
    /// GDI LOGFONT rule: `true` = `px` is the cell height (positive lfHeight; Office
    /// balloon Tahoma 13/16, B-REN-8), `false` = `px` is the em (negative lfHeight).
    pub cell: bool,
    pub bold: bool,
    pub italic: bool,
}

/// Text metrics, implemented by `bentwire-render` with the real font.
pub trait TextMetrics {
    /// TM.height: ascent + descent in px, rounded (B-BAL-2).
    fn line_height(&self, font: &FontSpec) -> i32;
    /// TM.aveCharWidth (B-BAL-2).
    fn ave_char_width(&self, font: &FontSpec) -> i32;
    /// Advance width of a single-line string in px.
    fn text_width(&self, text: &str, font: &FontSpec) -> i32;
}

/// Loads characters by id (the daemon reads files; tests return synthetic models).
pub trait CharacterLoader {
    fn load(&mut self, id: &str) -> Result<Arc<CharacterModel>, String>;
}

/// An installed character as found by discovery (B-CHAR-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CatalogEntry {
    pub id: String,
    pub info: Vec<InfoEntry>,
}

/// B-L10N-1 config value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LanguageSetting {
    #[default]
    Auto,
    De,
    En,
}

/// Resolved UI language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    De,
    En,
}

/// B-CFG-1 config file (parsing lives in the daemon).
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    pub character: Option<String>,
    pub language: LanguageSetting,
    pub sound_effects: bool,
    pub balloon: bool,
    /// Empty = each character's font.
    pub balloon_font: String,
    /// 0.5 to 1.5.
    pub speaking_speed: f64,
    pub idle: bool,
    pub double_click_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            character: None,
            language: LanguageSetting::Auto,
            sound_effects: true,
            balloon: true,
            balloon_font: String::new(),
            speaking_speed: 1.0,
            idle: true,
            double_click_ms: 500,
        }
    }
}

/// B-CFG-2 state file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateFile {
    pub character: String,
    pub left: i32,
    pub top: i32,
    pub visible: bool,
}

/// Locale environment for `language = "auto"` (B-L10N-1).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LocaleEnv {
    pub lc_all: Option<String>,
    pub lc_messages: Option<String>,
    pub lang: Option<String>,
}

/// Construction parameters of [`crate::Core`].
pub struct CoreInit {
    pub config: Config,
    pub locale: LocaleEnv,
    /// The state file, if one exists (B-CHAR-4 step 1, B-CFG-3).
    pub state: Option<StateFile>,
    /// Installed characters, any order (the core sorts by id).
    pub catalog: Vec<CatalogEntry>,
    pub loader: Box<dyn CharacterLoader>,
    pub metrics: Box<dyn TextMetrics>,
    pub draws: Box<dyn Draws>,
    pub outputs: Vec<OutputInfo>,
    /// Local hour 0..23 for office97 `daypart` branches (B-O97-1).
    pub local_hour: u8,
}

/// A logical rectangle in desktop logical pixels, `w`/`h` >= 0.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    /// Half-open containment.
    pub fn contains(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.right() && py >= self.y && py < self.bottom()
    }
}

/// A Wayland output (B-WL-1).
#[derive(Clone, Debug, PartialEq)]
pub struct OutputInfo {
    /// Stable id chosen by the backend (e.g. the wl_output global name).
    pub id: u32,
    pub rect: Rect,
    pub scale: f64,
}

/// Which bentwire surface a pointer event happened on (B-WL-5).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Surface {
    Character,
    AgentBalloon,
    OfficeBalloon,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointerKind {
    Press,
    Motion,
    Release,
    /// The pointer left the surface.
    Leave,
}

/// Pointer event. `x`, `y` are global logical coordinates (surface position plus local
/// position, B-WL-5). The backend only reports events on opaque pixels (input region).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointerEvent {
    pub kind: PointerKind,
    pub surface: Surface,
    /// 1 left, 2 right, 4 middle (B-UI-1); ignored for Motion/Leave.
    pub button: u32,
    /// 1 Shift, 2 Ctrl, 4 Alt.
    pub modifiers: u32,
    pub x: i32,
    pub y: i32,
}

/// Keys the ask balloon and the menu understand (B-ASK-2, B-WL-9).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Escape,
    Enter,
    Tab,
    Space,
    Up,
    Down,
    Left,
    Right,
    /// A printable character (mnemonics).
    Char(char),
}

/// Menu input. The renderer owns the menu's item geometry, so pointer hits on the menu
/// arrive already mapped to item indices. Keys go through [`Event::Key`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuInput {
    /// The pointer is over item `Some(i)` or over no item.
    Hover(Option<usize>),
    /// Item `i` was clicked.
    Activate(usize),
    /// `popup_done` or an outside click: close without a choice.
    Dismiss,
}

/// Tray (StatusNotifierItem) input, B-TRAY-1. Coordinates as given by the SNI host.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrayInput {
    Activate {
        x: i32,
        y: i32,
    },
    SecondaryActivate {
        x: i32,
        y: i32,
    },
    /// A dbusmenu item, by its [`MenuAction`].
    Menu(MenuAction),
}

/// `buttons` key of Ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskButtons {
    Set(String),
    Custom(Vec<String>),
}

/// `Ask(a{sv})` after D-Bus type checking (wrong types are InvalidArgs in the daemon).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AskSpec {
    pub heading: Option<String>,
    pub text: Option<String>,
    pub icon: Option<String>,
    pub labels: Option<Vec<String>>,
    pub label_type: Option<String>,
    pub checkboxes: Option<Vec<String>>,
    pub buttons: Option<AskButtons>,
    pub mode: Option<String>,
    pub animation: Option<String>,
}

/// D-Bus method calls (5.2, 5.3) and the writable property `IdleOn`.
#[derive(Clone, Debug, PartialEq)]
pub enum Call {
    ListCharacters,
    Select(String),
    Quit,
    Show { fast: bool },
    Hide { fast: bool },
    Play(String),
    Speak(String),
    Ask(AskSpec),
    MoveTo { x: i32, y: i32, speed: u32 },
    GestureAt { x: i32, y: i32 },
    Stop(u32),
    StopAll(Vec<String>),
    SetIdleOn(bool),
}

/// Inputs of the core.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// A method call. `token` is echoed in [`Output::Reply`]; `sender` is the caller's
    /// D-Bus unique name (unicast target of its request signals, B-Q-2).
    Call {
        token: u64,
        sender: String,
        call: Call,
    },
    Pointer(PointerEvent),
    Key {
        key: Key,
        modifiers: u32,
    },
    Menu(MenuInput),
    Tray(TrayInput),
    /// The full current output list (added, removed or changed).
    Outputs(Vec<OutputInfo>),
    /// SIGHUP reload (B-CFG-1).
    Config(Config),
    /// Rescanned characters.
    Catalog(Vec<CatalogEntry>),
    /// The local hour changed (office97 daypart).
    LocalHour(u8),
    /// The audio device failed to play the current sound: completion does not wait for
    /// it (R-ANIM-8).
    SoundFailed,
}

/// Successful method results.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReplyValue {
    Unit,
    RequestId(u32),
    /// (id, name, description), sorted by id.
    Characters(Vec<(String, String, String)>),
}

/// A method error: `name` from spec 5.4, `message` human-readable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MethodError {
    pub name: String,
    pub message: String,
}

/// Signals of `org.bentwire.Character1` (5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Signal {
    RequestStart {
        id: u32,
    },
    RequestComplete {
        id: u32,
        status: u32,
        reason: String,
    },
    AskComplete {
        id: u32,
        result: i32,
        checked: Vec<bool>,
    },
    Bookmark {
        n: i32,
    },
    Click {
        button: u32,
        modifiers: u32,
        x: i32,
        y: i32,
        from_tray: bool,
    },
    DblClick {
        button: u32,
        modifiers: u32,
        x: i32,
        y: i32,
        from_tray: bool,
    },
    DragStart {
        button: u32,
        modifiers: u32,
        x: i32,
        y: i32,
    },
    DragComplete {
        button: u32,
        modifiers: u32,
        x: i32,
        y: i32,
    },
    Move {
        x: i32,
        y: i32,
        cause: u32,
    },
    Shown {
        cause: u32,
    },
    Hidden {
        cause: u32,
    },
    IdleStart,
    IdleComplete,
    BalloonShown,
    BalloonHidden,
}

/// Outputs of the core, in order.
#[derive(Clone, Debug, PartialEq)]
pub enum Output {
    Reply {
        token: u64,
        result: Result<ReplyValue, MethodError>,
    },
    /// `target = Some(unique name)` for unicast signals (B-Q-2), `None` for broadcast.
    Signal {
        target: Option<String>,
        signal: Signal,
    },
    /// Start a sound of the current character (stops any previous one).
    SoundStart {
        sound: String,
    },
    SoundStop,
    /// Write the state file (already throttled to once per second, B-CFG-2).
    PersistState(StateFile),
    /// B-MENU-3: write a default config file if none exists, then `xdg-open` it.
    OpenConfig,
    /// Save done (a PersistState precedes this); exit 0 (B-RUN-1).
    Quit,
}

/// What the character surface shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Picture {
    /// Fully transparent (blank frame, or nothing to show yet).
    Empty,
    /// A frame of an animation of the current model (agent: its layers, office97: its
    /// pose). `mouth` is a mouth type to overlay (never set in v1, B-LIP-2).
    Frame {
        animation: String,
        frame: usize,
        mouth: Option<u8>,
    },
    /// An office97 pose by id (B-SHOW-3 rest pose).
    Pose(u32),
}

/// Character surface (B-WL-2, B-WL-6). The input region is the alpha > 0 shape of the
/// composed picture (B-PIX-3); render/desktop derive it from the picture.
#[derive(Clone, Debug, PartialEq)]
pub struct CharacterScene {
    pub id: String,
    pub mapped: bool,
    /// Top-left, global logical coordinates (not clamped, B-WL-4).
    pub x: i32,
    pub y: i32,
    /// Display size.
    pub width: u32,
    pub height: u32,
    pub picture: Picture,
    /// Output the surface lives on (B-WL-2); only changes outside drags.
    pub output: Option<u32>,
    pub dragging: bool,
    /// `keyboard_interactivity = on_demand` while an ask balloon is open (B-WL-8).
    pub keyboard_on_demand: bool,
}

/// Which body edge the balloon tail is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TailSide {
    Bottom,
    Left,
    Right,
}

/// Agent speech balloon (R-BAL-3..10). All coordinates global logical.
#[derive(Clone, Debug, PartialEq)]
pub struct AgentBalloonScene {
    /// Window rectangle: body plus tail reservation.
    pub window: Rect,
    /// Rounded body rectangle (16 x 20 corner ellipse, R-BAL-4).
    pub body: Rect,
    pub tail_side: TailSide,
    /// Tail triangle: tip, then the two base points on the body edge.
    pub tail: [(i32, i32); 3],
    pub font: FontSpec,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub border: [u8; 3],
    /// Text area origin (body inset by 10 px).
    pub text_origin: (i32, i32),
    /// Line pitch, TM.height + 2.
    pub line_pitch: i32,
    /// Visible lines, top to bottom, with only the words revealed so far.
    pub lines: Vec<String>,
    /// Vertical scroll offset in px (R-BAL-9): lines are drawn `scroll_px` lower and
    /// clipped to the text area; it counts down to 0.
    pub scroll_px: i32,
}

/// A run of Office balloon text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    /// Left edge, global.
    pub x: i32,
    pub text: String,
    pub color: [u8; 3],
    pub underline: bool,
}

/// One laid-out line: baseline-independent top `y` (global) and its runs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextLine {
    pub y: i32,
    pub runs: Vec<TextRun>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconKind {
    Alert,
    Tip,
    Info,
    Warning,
    Query,
    Critical,
}

/// Label prefix (R-OB-7).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LabelPrefix {
    /// Clickable label: the round bullet (15 x 15).
    Bullet,
    /// `label_type = bullets`: U+2022.
    Char(String),
    /// `label_type = numbers`: "n.".
    Number(String),
}

/// Visual state of a control (R-OB-11 states).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ControlState {
    Normal,
    Hot,
    Pressed,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LabelScene {
    /// Whole row rectangle (hit area).
    pub rect: Rect,
    pub prefix: LabelPrefix,
    pub prefix_rect: Rect,
    pub lines: Vec<TextLine>,
    pub clickable: bool,
    pub state: ControlState,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CheckboxScene {
    pub rect: Rect,
    /// The 13 x 13 glyph.
    pub box_rect: Rect,
    pub checked: bool,
    pub lines: Vec<TextLine>,
    /// Mnemonic letter (from `&` in the caption), if any.
    pub mnemonic: Option<char>,
    pub state: ControlState,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ButtonScene {
    pub rect: Rect,
    /// Caption without `&`.
    pub caption: String,
    /// Character index (in `caption`) of the underlined mnemonic.
    pub mnemonic_index: Option<usize>,
    pub state: ControlState,
    pub focused: bool,
}

/// Office balloon (B-OB-1..4, R-OB-*). All coordinates global logical.
#[derive(Clone, Debug, PartialEq)]
pub struct OfficeBalloonScene {
    pub window: Rect,
    /// Body, 8 px corner radius, #FFFFCC, 1 px black outline (B-OB-2).
    pub body: Rect,
    pub tail_side: TailSide,
    /// Tail right triangle: three corners (R-OB-25).
    pub tail: [(i32, i32); 3],
    /// Content rectangle (body inset by 6 px).
    pub content: Rect,
    pub body_font: FontSpec,
    pub heading_font: FontSpec,
    pub icon: Option<(IconKind, Rect)>,
    pub heading: Vec<TextLine>,
    pub text: Vec<TextLine>,
    pub labels: Vec<LabelScene>,
    pub checkboxes: Vec<CheckboxScene>,
    pub buttons: Vec<ButtonScene>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuAction {
    Hide,
    Show,
    Options,
    ChooseAssistant,
    Animate,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuEntry {
    /// Label with `&` mnemonic marker, as in B-MENU-2.
    Item {
        label: String,
        action: MenuAction,
    },
    Separator,
}

/// The character menu (B-MENU-1), positioned at the pointer (B-WL-9).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuScene {
    pub x: i32,
    pub y: i32,
    pub entries: Vec<MenuEntry>,
    pub highlighted: Option<usize>,
}

/// Tray item model (B-TRAY-1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrayScene {
    /// Character id whose `icon.png` to use; `None` = bentwire's own icon.
    pub icon_character: Option<String>,
    pub title: String,
    pub tooltip: String,
    pub menu: Vec<MenuEntry>,
}

/// Everything the backends draw.
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub character: Option<CharacterScene>,
    pub agent_balloon: Option<AgentBalloonScene>,
    pub office_balloon: Option<OfficeBalloonScene>,
    pub menu: Option<MenuScene>,
    pub tray: TrayScene,
    pub outputs: Vec<OutputInfo>,
}

/// `org.bentwire.Agent1` and `Character1` properties (5.2, 5.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Properties {
    /// `""` if none.
    pub character: String,
    pub name: String,
    pub description: String,
    pub visible: bool,
    pub left: i32,
    pub top: i32,
    pub width: u32,
    pub height: u32,
    pub idle_on: bool,
    pub move_cause: u32,
    pub visibility_cause: u32,
    pub balloon_enabled: bool,
    pub animation_names: Vec<String>,
}

/// Error names (5.4).
pub mod errors {
    pub const NO_CHARACTER: &str = "org.bentwire.Error.NoCharacter";
    pub const NO_SUCH_CHARACTER: &str = "org.bentwire.Error.NoSuchCharacter";
    pub const INVALID_CHARACTER: &str = "org.bentwire.Error.InvalidCharacter";
    pub const UNKNOWN_ANIMATION: &str = "org.bentwire.Error.UnknownAnimation";
    pub const INVALID_BALLOON: &str = "org.bentwire.Error.InvalidBalloon";
    pub const NOT_SUPPORTED: &str = "org.bentwire.Error.NotSupported";
    pub const INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";
    pub const HIDDEN: &str = "org.bentwire.Error.Hidden";
    pub const INTERRUPTED: &str = "org.bentwire.Error.Interrupted";
    pub const HIDDEN_BY_USER: &str = "org.bentwire.Error.HiddenByUser";
    pub const DRAGGED: &str = "org.bentwire.Error.Dragged";
    pub const CHARACTER_CHANGED: &str = "org.bentwire.Error.CharacterChanged";
    pub const BALLOON_TOO_BIG: &str = "org.bentwire.Error.BalloonTooBig";
    pub const ANIMATION_UNAVAILABLE: &str = "org.bentwire.Error.AnimationUnavailable";
}
