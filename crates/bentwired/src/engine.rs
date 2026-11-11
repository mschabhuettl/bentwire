//! The daemon's view of the behaviour core: what goes in (calls, time, input) and what
//! comes out (replies, signals, sounds, scene, effects). The D-Bus layer and the
//! backends only speak these types; `core_adapter` maps them onto bentwire-core.

/// A D-Bus client, identified by its unique bus name (B-Q-2).
pub type Client = String;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct AskSpec {
    pub heading: Option<String>,
    pub text: Option<String>,
    pub icon: Option<String>,
    pub labels: Option<Vec<String>>,
    pub label_type: Option<String>,
    pub checkboxes: Option<Vec<String>>,
    /// `Ok(set name)` or `Err(custom captions)`.
    pub buttons: Option<Buttons>,
    pub mode: Option<String>,
    pub animation: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Buttons {
    Set(String),
    Custom(Vec<String>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Method {
    Show(bool),
    Hide(bool),
    Play(String),
    Speak(String),
    Ask(AskSpec),
    MoveTo(i32, i32, u32),
    GestureAt(i32, i32),
    Stop(u32),
    StopAll(Vec<String>),
    SetIdleOn(bool),
}

#[derive(Debug, Clone, PartialEq)]
pub struct CallError {
    pub name: String,
    pub message: String,
}

impl CallError {
    pub fn new(name: &str, message: impl Into<String>) -> Self {
        CallError {
            name: name.into(),
            message: message.into(),
        }
    }
}

pub const E_NO_CHARACTER: &str = "org.bentwire.Error.NoCharacter";
pub const E_NO_SUCH_CHARACTER: &str = "org.bentwire.Error.NoSuchCharacter";
pub const E_INVALID_CHARACTER: &str = "org.bentwire.Error.InvalidCharacter";
pub const E_INVALID_ARGS: &str = "org.freedesktop.DBus.Error.InvalidArgs";

#[derive(Debug, Clone, PartialEq)]
pub enum Signal {
    RequestStart(u32),
    RequestComplete(u32, u32, String),
    AskComplete(u32, i32, Vec<bool>),
    Bookmark(i32),
    Click(u32, u32, i32, i32, bool),
    DblClick(u32, u32, i32, i32, bool),
    DragStart(u32, u32, i32, i32),
    DragComplete(u32, u32, i32, i32),
    Move(i32, i32, u32),
    Shown(u32),
    Hidden(u32),
    IdleStart,
    IdleComplete,
    BalloonShown,
    BalloonHidden,
}

impl Signal {
    pub fn member(&self) -> &'static str {
        match self {
            Signal::RequestStart(..) => "RequestStart",
            Signal::RequestComplete(..) => "RequestComplete",
            Signal::AskComplete(..) => "AskComplete",
            Signal::Bookmark(..) => "Bookmark",
            Signal::Click(..) => "Click",
            Signal::DblClick(..) => "DblClick",
            Signal::DragStart(..) => "DragStart",
            Signal::DragComplete(..) => "DragComplete",
            Signal::Move(..) => "Move",
            Signal::Shown(..) => "Shown",
            Signal::Hidden(..) => "Hidden",
            Signal::IdleStart => "IdleStart",
            Signal::IdleComplete => "IdleComplete",
            Signal::BalloonShown => "BalloonShown",
            Signal::BalloonHidden => "BalloonHidden",
        }
    }

    pub fn args_json(&self) -> serde_json::Value {
        use serde_json::json;
        match self {
            Signal::RequestStart(a) => json!([a]),
            Signal::RequestComplete(a, b, c) => json!([a, b, c]),
            Signal::AskComplete(a, b, c) => json!([a, b, c]),
            Signal::Bookmark(a) => json!([a]),
            Signal::Click(a, b, c, d, e) | Signal::DblClick(a, b, c, d, e) => {
                json!([a, b, c, d, e])
            }
            Signal::DragStart(a, b, c, d) | Signal::DragComplete(a, b, c, d) => {
                json!([a, b, c, d])
            }
            Signal::Move(a, b, c) => json!([a, b, c]),
            Signal::Shown(a) | Signal::Hidden(a) => json!([a]),
            _ => json!([]),
        }
    }
}

/// Property snapshot of `org.bentwire.Character1` (section 5.3).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CharacterProps {
    pub id: String,
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

/// Pointer and keyboard input in desktop logical coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PointerKind {
    Press,
    Release,
    Motion,
    Leave,
}

impl PointerKind {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "press" => PointerKind::Press,
            "release" => PointerKind::Release,
            "motion" => PointerKind::Motion,
            "leave" => PointerKind::Leave,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Input {
    /// `button`: 1 left, 2 right, 4 middle (B-UI-1).
    Pointer {
        kind: PointerKind,
        button: u32,
        x: i32,
        y: i32,
        modifiers: u32,
    },
    Key(String),
    /// Tray: Activate (left click), SecondaryActivate (middle click).
    TrayActivate(i32, i32),
    TraySecondary(i32, i32),
    /// Tray menu item by id (`hide`, `show`, `options`, `choose`, `animate`).
    TrayMenu(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub id: u32,
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: f64,
}

/// Side effects the core asks the daemon to perform.
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    Signal(Option<Client>, Signal),
    SoundStart {
        sound: String,
        duration_ms: u64,
    },
    SoundStop,
    /// B-MENU-3.
    OpenOptions,
    /// The persisted state changed (B-CFG-2): character, left, top, visible.
    Persist,
}

/// What `bentwired` needs from the behaviour core. Implemented by `core_adapter` (the
/// real core) and by `stub` (plumbing tests before the core existed).
pub trait Engine {
    /// A Character1 method. Returns the request id (0 for immediate calls). Signals the
    /// call causes are queued for `drain`, so the reply can go out first (B-Q-1).
    fn call(&mut self, now: u64, client: &str, m: Method) -> Result<u32, CallError>;
    /// Switch character (B-CHAR-5); `None` unloads. Errors are InvalidCharacter.
    fn select(
        &mut self,
        now: u64,
        entry: Option<&crate::discovery::Entry>,
    ) -> Result<(), CallError>;
    fn input(&mut self, now: u64, i: Input);
    fn set_outputs(&mut self, now: u64, outputs: &[Output]);
    fn set_draws(&mut self, draws: Vec<u32>);
    /// Place the character without animation or signal (startup, B-CFG-3).
    fn set_position(&mut self, left: i32, top: i32);
    /// Run timers up to `now`.
    fn advance(&mut self, now: u64);
    fn next_deadline(&self) -> Option<u64>;
    fn drain(&mut self) -> Vec<Effect>;
    /// None when no character is loaded.
    fn props(&self) -> Option<CharacterProps>;
    /// The scene for the backends.
    fn scene(&self) -> Scene;
    /// Answer the open ask balloon with a result code (Test1.Answer).
    fn answer(&mut self, now: u64, result: i32);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Character,
    Balloon,
    Menu,
}

/// One drawn surface. Pixels are straight-alpha RGBA8 at `scale` times the logical size.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    pub kind: SurfaceKind,
    /// Global logical position of the top-left corner.
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub rgba: Vec<u8>,
    /// Input region in logical surface coordinates (x, y, w, h).
    pub input: Vec<(i32, i32, u32, u32)>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Scene {
    /// Character first, then balloon(s), then the menu.
    pub surfaces: Vec<Surface>,
    /// True while an ask balloon is open (B-WL-8 keyboard on_demand).
    pub keyboard: bool,
}
