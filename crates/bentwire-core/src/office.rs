//! Office balloon: Ask contract and validation, button sets, markup, layout and
//! placement (B-ASK-1..5, B-OB-1..4, office-assistant R-OB-*).

use crate::api::{
    AskButtons, AskSpec, ButtonScene, CheckboxScene, ControlState, FontSpec, IconKind, LabelPrefix,
    LabelScene, Lang, MethodError, OfficeBalloonScene, Rect, TailSide, TextLine, TextMetrics,
    TextRun, errors,
};
use crate::l10n::button_caption;

/// B-ASK-2: set names, in MsoButtonSetType order 0..14.
pub const SET_NAMES: [&str; 15] = [
    "none",
    "ok",
    "cancel",
    "ok-cancel",
    "yes-no",
    "yes-no-cancel",
    "back-close",
    "next-close",
    "back-next-close",
    "retry-cancel",
    "abort-retry-ignore",
    "search-close",
    "back-next-snooze",
    "tips-options-close",
    "yes-all-no-cancel",
];

/// R-OB-32 button sets: button ids left to right.
pub fn set_buttons(set: usize) -> &'static [usize] {
    const SETS: [&[usize]; 15] = [
        &[],
        &[0],
        &[1],
        &[0, 1],
        &[2, 3],
        &[2, 3, 1],
        &[4, 11],
        &[5, 11],
        &[4, 5, 11],
        &[6, 1],
        &[7, 6, 8],
        &[9, 11],
        &[4, 5, 10],
        &[12, 13, 11],
        &[2, 14, 3, 1],
    ];
    SETS[set]
}

/// Result of a button id 0..14 (B-ASK-2).
pub fn button_result(id: usize) -> i32 {
    -(id as i32) - 1
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabelType {
    Buttons,
    Bullets,
    Numbers,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Modal,
    AutoDown,
}

/// A button of the balloon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ButtonDef {
    /// Caption with `&`.
    pub caption: String,
    pub result: i32,
    pub enabled: bool,
}

/// A styled piece of text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Piece {
    pub text: String,
    pub color: Option<[u8; 3]>,
    pub underline: bool,
}

/// A validated Ask.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AskModel {
    pub heading: String,
    /// Body text after markup (B-ASK-3), `\n` splits lines.
    pub text: Vec<Piece>,
    pub icon: Option<IconKind>,
    pub labels: Vec<String>,
    pub label_type: LabelType,
    pub checkboxes: Vec<String>,
    pub buttons: Vec<ButtonDef>,
    pub mode: Mode,
    /// Result of Esc, if bound (Cancel or Close).
    pub esc_result: Option<i32>,
    /// `None` = no animation; `Some(name)` resolved by B-FB-1 when the ask runs.
    pub animation: Option<String>,
}

fn invalid_args(m: String) -> MethodError {
    MethodError {
        name: errors::INVALID_ARGS.into(),
        message: m,
    }
}

fn invalid_balloon(office: &str, m: &str) -> MethodError {
    MethodError {
        name: errors::INVALID_BALLOON.into(),
        message: format!("{office}: {m}"),
    }
}

/// B-ASK-1, B-ASK-2, B-ASK-5, B-ASK-6 validation.
pub fn validate(spec: &AskSpec, lang: Lang) -> Result<AskModel, MethodError> {
    if spec.heading.is_none() && spec.text.is_none() && spec.labels.is_none() {
        return Err(invalid_args(
            "one of heading, text, labels must be present".into(),
        ));
    }
    let mode = match spec.mode.as_deref().unwrap_or("modal") {
        "modal" => Mode::Modal,
        "autodown" => Mode::AutoDown,
        "modeless" => {
            return Err(MethodError {
                name: errors::NOT_SUPPORTED.into(),
                message: "mode modeless is not supported".into(),
            });
        }
        m => return Err(invalid_args(format!("unknown mode \"{m}\""))),
    };
    let icon = match spec.icon.as_deref().unwrap_or("none") {
        "none" => None,
        "alert" => Some(IconKind::Alert),
        "tip" => Some(IconKind::Tip),
        "info" => Some(IconKind::Info),
        "warning" => Some(IconKind::Warning),
        "query" => Some(IconKind::Query),
        "critical" => Some(IconKind::Critical),
        i => return Err(invalid_args(format!("unknown icon \"{i}\""))),
    };
    let label_type = match spec.label_type.as_deref().unwrap_or("buttons") {
        "buttons" => LabelType::Buttons,
        "bullets" => LabelType::Bullets,
        "numbers" => LabelType::Numbers,
        t => return Err(invalid_args(format!("unknown label_type \"{t}\""))),
    };
    let labels = spec.labels.clone().unwrap_or_default();
    let checkboxes = spec.checkboxes.clone().unwrap_or_default();
    if labels.len() > 5 {
        return Err(invalid_balloon("TooManyControls", "more than 5 labels"));
    }
    if checkboxes.len() > 5 {
        return Err(invalid_balloon(
            "TooManyControls",
            "more than 5 check boxes",
        ));
    }
    let (buttons, esc_result) = match spec.buttons.clone().unwrap_or(AskButtons::Set("ok".into())) {
        AskButtons::Set(name) => {
            let set = SET_NAMES
                .iter()
                .position(|n| *n == name)
                .ok_or_else(|| invalid_args(format!("unknown button set \"{name}\"")))?;
            let ids = set_buttons(set);
            let b: Vec<ButtonDef> = ids
                .iter()
                .map(|&id| ButtonDef {
                    caption: button_caption(lang, id).into(),
                    result: button_result(id),
                    enabled: true,
                })
                .collect();
            let esc = ids
                .iter()
                .find(|&&id| id == 1 || id == 11)
                .map(|&id| button_result(id));
            (b, esc)
        }
        AskButtons::Custom(c) => {
            if c.is_empty() {
                return Err(invalid_args("custom buttons list is empty".into()));
            }
            if c.len() > 6 {
                return Err(invalid_balloon(
                    "TooManyControls",
                    "more than 6 custom buttons",
                ));
            }
            let b = c
                .into_iter()
                .enumerate()
                .map(|(i, caption)| ButtonDef {
                    caption,
                    result: -25 - i as i32,
                    enabled: true,
                })
                .collect();
            (b, None)
        }
    };
    let clickable_labels = label_type == LabelType::Buttons && !labels.is_empty();
    if mode == Mode::Modal && buttons.is_empty() && !clickable_labels {
        return Err(invalid_balloon(
            "ButtonlessModal",
            "a modal balloon needs a button or a clickable label",
        ));
    }
    if !checkboxes.is_empty() && buttons.is_empty() {
        return Err(invalid_balloon(
            "CheckboxesNeedButton",
            "check boxes need a button",
        ));
    }
    let animation = match spec.animation.as_deref() {
        None => Some("Explain".to_string()),
        Some("") => None,
        Some(a) => Some(a.to_string()),
    };
    Ok(AskModel {
        heading: spec.heading.clone().unwrap_or_default(),
        text: markup(spec.text.as_deref().unwrap_or("")),
        icon,
        labels,
        label_type,
        checkboxes,
        buttons,
        mode,
        esc_result,
        animation,
    })
}

/// R-OB-16 colour table.
pub fn cf_color(n: u32) -> Option<[u8; 3]> {
    Some(match n {
        0 => [0, 0, 0],
        1 => [0x80, 0, 0],
        2 => [0, 0x80, 0],
        3 => [0x80, 0x80, 0],
        4 => [0, 0, 0x80],
        5 => [0x80, 0, 0x80],
        6 => [0, 0x80, 0x80],
        7 => [0xC0, 0xC0, 0xC0],
        248 => [0x80, 0x80, 0x80],
        249 => [0xFF, 0, 0],
        250 => [0, 0xFF, 0],
        251 => [0xFF, 0xFF, 0],
        252 => [0, 0, 0xFF],
        253 => [0xFF, 0, 0xFF],
        254 => [0, 0xFF, 0xFF],
        255 => [0xFF, 0xFF, 0xFF],
        _ => return None,
    })
}

/// B-ASK-3 markup to styled pieces.
pub fn markup(text: &str) -> Vec<Piece> {
    let mut out: Vec<Piece> = Vec::new();
    let mut color: Option<[u8; 3]> = None;
    let mut ul = false;
    let mut buf = String::new();
    let flush = |buf: &mut String, out: &mut Vec<Piece>, color, ul| {
        if let Some(last) = out.last_mut()
            && last.color == color
            && last.underline == ul
        {
            last.text.push_str(&std::mem::take(buf));
            return;
        }
        if !buf.is_empty() {
            out.push(Piece {
                text: std::mem::take(buf),
                color,
                underline: ul,
            });
        }
    };
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        buf.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            buf.push_str(&rest[open..]);
            rest = "";
            break;
        };
        let inner = &after[..close];
        let mut words = inner.split_whitespace();
        let kw = words.next().unwrap_or("").to_ascii_lowercase();
        let arg = words.next();
        let handled = match kw.as_str() {
            "cf" => match arg.and_then(|a| a.parse::<u32>().ok()) {
                Some(n) => {
                    flush(&mut buf, &mut out, color, ul);
                    if let Some(c) = cf_color(n) {
                        color = Some(c);
                    }
                    true
                }
                None => false,
            },
            "ul" => match arg {
                Some("1") | Some("0") => {
                    flush(&mut buf, &mut out, color, ul);
                    ul = arg == Some("1");
                    true
                }
                _ => false,
            },
            "bmp" | "wmf" | "pict" | "dlx" | "cbmp" => true,
            _ => false,
        };
        if handled {
            rest = &after[close + 1..];
        } else {
            buf.push('{');
            rest = after;
        }
    }
    buf.push_str(rest);
    flush(&mut buf, &mut out, color, ul);
    out
}

/// Strips `&` mnemonic markers: (caption, mnemonic char index, mnemonic letter).
pub fn mnemonic(caption: &str) -> (String, Option<usize>, Option<char>) {
    let mut s = String::new();
    let mut idx = None;
    let mut letter = None;
    let mut chars = caption.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '&' {
            match chars.next() {
                Some('&') => s.push('&'),
                Some(n) => {
                    if idx.is_none() {
                        idx = Some(s.chars().count());
                        letter = Some(n.to_lowercase().next().unwrap_or(n));
                    }
                    s.push(n);
                }
                None => {}
            }
        } else {
            s.push(c);
        }
    }
    (s, idx, letter)
}

/// Fonts of the Office balloon (B-OB-1): Tahoma 13, or 16 when the output is 1024 or
/// more logical px wide.
pub fn fonts(output_width: i32) -> (FontSpec, FontSpec) {
    let px = if output_width >= 1024 { 16 } else { 13 };
    let body = FontSpec {
        family: "Tahoma".into(),
        px,
        cell: true,
        bold: false,
        italic: false,
    };
    let heading = FontSpec {
        bold: true,
        ..body.clone()
    };
    (body, heading)
}

const BODY_COLOR: [u8; 3] = [0, 0, 0];
const LABEL_COLOR: [u8; 3] = [0, 0, 0x80];
const DISABLED_COLOR: [u8; 3] = [0x80, 0x80, 0x80];

/// Metrics bundle.
pub struct Ctx<'a> {
    pub m: &'a dyn TextMetrics,
    pub body: FontSpec,
    pub heading: FontSpec,
    /// Button height, 22 or 24 (R-OB-11).
    pub button_h: i32,
}

impl Ctx<'_> {
    pub fn new(m: &dyn TextMetrics, output_width: i32) -> Ctx<'_> {
        let (body, heading) = fonts(output_width);
        Ctx {
            m,
            body,
            heading,
            button_h: if output_width >= 1024 { 24 } else { 22 },
        }
    }
    fn tm(&self) -> i32 {
        self.m.line_height(&self.body)
    }
    fn g(&self) -> i32 {
        self.tm() / 2
    }
    /// R-OB-7 prefix column.
    fn prefix_col(&self) -> i32 {
        17.max(self.m.text_width("1.", &self.body))
            .max(self.m.text_width("\u{2022}", &self.body))
    }
    /// R-OB-32 uniform button width.
    fn button_w(&self, buttons: &[ButtonDef]) -> i32 {
        let widest = buttons
            .iter()
            .map(|b| self.m.text_width(&mnemonic(&b.caption).0, &self.body))
            .max()
            .unwrap_or(0)
            .max(5);
        (widest + 4) / 5 * 5 + 22
    }
    fn row_w(&self, buttons: &[ButtonDef]) -> i32 {
        if buttons.is_empty() {
            0
        } else {
            buttons.len() as i32 * (self.button_w(buttons) + 10) - 10
        }
    }
}

/// Wrap styled pieces at `width`, lines start at x = 0, y counted by the caller.
/// Returns lines of `(x, piece)`.
fn wrap_pieces(
    pieces: &[Piece],
    width: i32,
    m: &dyn TextMetrics,
    font: &FontSpec,
) -> Vec<Vec<(i32, Piece)>> {
    // Tokenise into words made of styled fragments, spaces and newlines.
    enum Tok {
        Word(Vec<Piece>),
        Space,
        Newline,
    }
    let mut toks: Vec<Tok> = Vec::new();
    let mut word: Vec<Piece> = Vec::new();
    let push_frag = |word: &mut Vec<Piece>, p: &Piece, s: String| {
        if let Some(last) = word.last_mut()
            && last.color == p.color
            && last.underline == p.underline
        {
            last.text.push_str(&s);
            return;
        }
        word.push(Piece {
            text: s,
            color: p.color,
            underline: p.underline,
        });
    };
    for p in pieces {
        let mut cur = String::new();
        for c in p.text.chars() {
            if c == ' ' || c == '\t' || c == '\n' {
                if !cur.is_empty() {
                    push_frag(&mut word, p, std::mem::take(&mut cur));
                }
                if !word.is_empty() {
                    toks.push(Tok::Word(std::mem::take(&mut word)));
                }
                toks.push(if c == '\n' { Tok::Newline } else { Tok::Space });
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            push_frag(&mut word, p, cur);
        }
    }
    if !word.is_empty() {
        toks.push(Tok::Word(word));
    }
    let space = m.text_width(" ", font);
    let ww = |w: &[Piece]| w.iter().map(|p| m.text_width(&p.text, font)).sum::<i32>();
    let mut lines: Vec<Vec<(i32, Piece)>> = vec![Vec::new()];
    let mut x = 0;
    let mut pending_space = false;
    let place = |line: &mut Vec<(i32, Piece)>, x: &mut i32, w: Vec<Piece>| {
        for p in w {
            let pw = m.text_width(&p.text, font);
            line.push((*x, p));
            *x += pw;
        }
    };
    for t in toks {
        match t {
            Tok::Newline => {
                lines.push(Vec::new());
                x = 0;
                pending_space = false;
            }
            Tok::Space => pending_space = !lines.last().unwrap().is_empty(),
            Tok::Word(w) => {
                let wwid = ww(&w);
                let lead = if pending_space && x > 0 { space } else { 0 };
                if x + lead + wwid <= width {
                    x += lead;
                    place(lines.last_mut().unwrap(), &mut x, w);
                } else if wwid <= width {
                    if x > 0 {
                        lines.push(Vec::new());
                    }
                    x = 0;
                    place(lines.last_mut().unwrap(), &mut x, w);
                } else {
                    // Break the word character by character.
                    if x > 0 {
                        lines.push(Vec::new());
                        x = 0;
                    }
                    for p in w {
                        for c in p.text.chars() {
                            let cw = m.text_width(&c.to_string(), font);
                            if x + cw > width && x > 0 {
                                lines.push(Vec::new());
                                x = 0;
                            }
                            let line = lines.last_mut().unwrap();
                            match line.last_mut() {
                                Some((lx, lp))
                                    if lp.color == p.color
                                        && lp.underline == p.underline
                                        && *lx + m.text_width(&lp.text, font) == x =>
                                {
                                    lp.text.push(c)
                                }
                                _ => line.push((
                                    x,
                                    Piece {
                                        text: c.to_string(),
                                        color: p.color,
                                        underline: p.underline,
                                    },
                                )),
                            }
                            x += cw;
                        }
                    }
                }
                pending_space = false;
            }
        }
    }
    if lines.len() == 1 && lines[0].is_empty() {
        lines.clear();
    }
    lines
}

/// Find caption char `idx` in the wrapped runs: (line, run, char index in run).
fn locate(caption: &str, idx: usize, lines: &[TextLine]) -> Option<(usize, usize, usize)> {
    let chars: Vec<char> = caption.chars().collect();
    let mut cursor = 0;
    for (li, l) in lines.iter().enumerate() {
        for (ri, r) in l.runs.iter().enumerate() {
            let rc: Vec<char> = r.text.chars().collect();
            let start = (cursor..=chars.len().saturating_sub(rc.len()))
                .find(|&p| chars[p..p + rc.len()] == rc[..])?;
            if idx >= start && idx < start + rc.len() {
                return Some((li, ri, idx - start));
            }
            cursor = start + rc.len();
        }
    }
    None
}

fn plain(s: &str) -> Vec<Piece> {
    vec![Piece {
        text: s.to_string(),
        color: None,
        underline: false,
    }]
}

fn to_lines(
    wrapped: Vec<Vec<(i32, Piece)>>,
    ox: i32,
    oy: i32,
    pitch: i32,
    default: [u8; 3],
) -> Vec<TextLine> {
    wrapped
        .into_iter()
        .enumerate()
        .map(|(i, l)| TextLine {
            y: oy + i as i32 * pitch,
            runs: l
                .into_iter()
                .map(|(x, p)| TextRun {
                    x: ox + x,
                    text: p.text,
                    color: p.color.unwrap_or(default),
                    underline: p.underline,
                })
                .collect(),
        })
        .collect()
}

/// Interactive state of an open balloon.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Interaction {
    pub checked: Vec<bool>,
    /// Index in the focus order (labels, check boxes, buttons).
    pub focus: Option<usize>,
    pub hot: Option<Control>,
    pub pressed: Option<Control>,
}

/// A control of the balloon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    Label(usize),
    Checkbox(usize),
    Button(usize),
}

impl AskModel {
    /// Focus order (B-ASK-2): clickable labels, check boxes, enabled buttons.
    pub fn focus_order(&self) -> Vec<Control> {
        let mut v = Vec::new();
        if self.label_type == LabelType::Buttons {
            v.extend((0..self.labels.len()).map(Control::Label));
        }
        v.extend((0..self.checkboxes.len()).map(Control::Checkbox));
        v.extend(
            self.buttons
                .iter()
                .enumerate()
                .filter(|(_, b)| b.enabled)
                .map(|(i, _)| Control::Button(i)),
        );
        v
    }

    /// Initial focus: the first button, else the first control.
    pub fn initial_focus(&self) -> Option<usize> {
        let o = self.focus_order();
        o.iter()
            .position(|c| matches!(c, Control::Button(_)))
            .or(if o.is_empty() { None } else { Some(0) })
    }
}

/// Content layout relative to the content origin.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub content_w: i32,
    pub content_h: i32,
    scene: OfficeBalloonScene,
}

/// R-OB-35 vertical layout at content width `cw`, relative to (0, 0).
pub fn layout(a: &AskModel, cw: i32, cx: &Ctx, it: &Interaction) -> Layout {
    let tm = cx.tm();
    let g = cx.g();
    let hlh = cx.m.line_height(&cx.heading);
    let mut y = 0;
    let mut any = false;
    let mut icon = None;
    let mut heading = Vec::new();
    if a.icon.is_some() || !a.heading.is_empty() {
        let hx = if a.icon.is_some() { 37 } else { 0 };
        let hw = wrap_pieces(&plain(&a.heading), cw - hx, cx.m, &cx.heading);
        let hh = hw.len() as i32 * hlh;
        let block = if a.icon.is_some() { hh.max(32) } else { hh };
        if let Some(k) = a.icon {
            icon = Some((k, Rect::new(0, block - 32, 32, 32)));
        }
        heading = to_lines(hw, hx, block - hh, hlh, BODY_COLOR);
        y += block;
        any = true;
    }
    let mut text = Vec::new();
    if a.text.iter().any(|p| !p.text.is_empty()) {
        if any {
            y += g;
        }
        let w = wrap_pieces(&a.text, cw, cx.m, &cx.body);
        let n = w.len() as i32;
        text = to_lines(w, 0, y, tm, BODY_COLOR);
        y += n * tm;
        any = true;
    }
    let order = a.focus_order();
    let focused = it.focus.and_then(|i| order.get(i).copied());
    let state_of = |c: Control, enabled: bool| {
        if !enabled {
            ControlState::Disabled
        } else if it.pressed == Some(c) {
            ControlState::Pressed
        } else if it.hot == Some(c) {
            ControlState::Hot
        } else {
            ControlState::Normal
        }
    };
    let pc = cx.prefix_col();
    let tx = pc + 5;
    let mut labels = Vec::new();
    for (i, l) in a.labels.iter().enumerate() {
        y += if any { g } else { 1 };
        any = true;
        let w = wrap_pieces(&markup(l), cw - tx, cx.m, &cx.body);
        let h = (w.len() as i32 * tm).max(15).max(tm);
        let first_mid = y + tm / 2;
        let (prefix, prefix_rect) = match a.label_type {
            LabelType::Buttons => (LabelPrefix::Bullet, Rect::new(0, first_mid - 7, 15, 15)),
            LabelType::Bullets => (
                LabelPrefix::Char("\u{2022}".into()),
                Rect::new(0, y, pc, tm),
            ),
            LabelType::Numbers => (
                LabelPrefix::Number(format!("{}.", i + 1)),
                Rect::new(0, y, pc, tm),
            ),
        };
        let ty = if tm < 15 && a.label_type == LabelType::Buttons {
            y + (15 - tm) / 2
        } else {
            y
        };
        let clickable = a.label_type == LabelType::Buttons;
        let c = Control::Label(i);
        labels.push(LabelScene {
            rect: Rect::new(0, y, cw, h),
            prefix,
            prefix_rect,
            lines: to_lines(w, tx, ty, tm, LABEL_COLOR),
            clickable,
            state: if clickable {
                state_of(c, true)
            } else {
                ControlState::Normal
            },
            focused: focused == Some(c),
        });
        y += h;
    }
    let mut checkboxes = Vec::new();
    for (i, l) in a.checkboxes.iter().enumerate() {
        if any {
            y += g;
        }
        any = true;
        let (cap, midx, letter) = mnemonic(l);
        let w = wrap_pieces(&plain(&cap), cw - tx, cx.m, &cx.body);
        let h = (w.len() as i32 * tm).max(13);
        let c = Control::Checkbox(i);
        checkboxes.push(CheckboxScene {
            rect: Rect::new(0, y, cw, h),
            box_rect: Rect::new((pc - 13) / 2, y + (tm - 13) / 2, 13, 13),
            checked: it.checked.get(i).copied().unwrap_or(false),
            lines: to_lines(w, tx, y, tm, BODY_COLOR),
            mnemonic: letter,
            mnemonic_at: None,
            state: state_of(c, true),
            focused: focused == Some(c),
        });
        if let (Some(cb), Some(m)) = (checkboxes.last_mut(), midx) {
            cb.mnemonic_at = locate(&cap, m, &cb.lines);
        }
        y += h;
    }
    let mut buttons = Vec::new();
    if !a.buttons.is_empty() {
        if any {
            y += 2 * g;
        }
        let bw = cx.button_w(&a.buttons);
        let x0 = (cw - cx.row_w(&a.buttons)) / 2;
        for (i, b) in a.buttons.iter().enumerate() {
            let (cap, idx, _) = mnemonic(&b.caption);
            let c = Control::Button(i);
            buttons.push(ButtonScene {
                rect: Rect::new(x0 + i as i32 * (bw + 10), y, bw, cx.button_h),
                caption: cap,
                mnemonic_index: idx,
                state: state_of(c, b.enabled),
                focused: focused == Some(c),
            });
        }
        y += cx.button_h;
    }
    let _ = DISABLED_COLOR;
    Layout {
        content_w: cw,
        content_h: y,
        scene: OfficeBalloonScene {
            window: Rect::default(),
            body: Rect::default(),
            tail_side: TailSide::Bottom,
            tail: [(0, 0); 3],
            content: Rect::new(0, 0, cw, y),
            body_font: cx.body.clone(),
            heading_font: cx.heading.clone(),
            icon,
            heading,
            text,
            labels,
            checkboxes,
            buttons,
        },
    }
}

fn pieces_width(p: &[Piece], cx: &Ctx, font: &FontSpec) -> Vec<i32> {
    // Unwrapped widths per line.
    let mut v = vec![0];
    for x in p {
        for (i, seg) in x.text.split('\n').enumerate() {
            if i > 0 {
                v.push(0);
            }
            *v.last_mut().unwrap() += cx.m.text_width(seg, font);
        }
    }
    v
}

/// A-CORE-1: area of unwrapped text, one line per `\n`, each line `lh` high:
/// (area, height). Empty text has none.
fn text_area(p: &[Piece], cx: &Ctx, font: &FontSpec, lh: i32) -> (i64, i32) {
    if p.iter().all(|x| x.text.is_empty()) {
        return (0, 0);
    }
    let w = pieces_width(p, cx, font);
    let area = w.iter().map(|&w| i64::from(w) * i64::from(lh)).sum();
    (area, w.len() as i32 * lh)
}

/// R-OB-31, A-CORE-1: the two candidate body sizes (landscape, portrait), each widened
/// until it is not too narrow (R-OB-34), and the minimum body size `(min_w, min_h)`.
pub fn sizes(a: &AskModel, cx: &Ctx) -> ([(i32, i32); 2], (i32, i32)) {
    sizes_within(a, cx, i32::MAX)
}

/// [`sizes`] with the too-narrow widening clamped to body width `max_w` (R-OB-34: the fit
/// widens within the free space).
fn sizes_within(a: &AskModel, cx: &Ctx, max_w: i32) -> ([(i32, i32); 2], (i32, i32)) {
    let tm = cx.tm();
    let g = cx.g();
    let pc = cx.prefix_col();
    let (mut area, mut min_w, mut min_h) = (0i64, 0, 0);
    if a.icon.is_some() {
        (area, min_w, min_h) = (1024, 32, 32);
    }
    if a.icon.is_some() || !a.heading.is_empty() {
        let hlh = cx.m.line_height(&cx.heading);
        let (ha, hh) = text_area(&plain(&a.heading), cx, &cx.heading, hlh);
        area += ha;
        min_h = min_h.max(hh);
        area += 12 * i64::from(min_h);
        min_h += 6;
    }
    let (ta, th) = text_area(&a.text, cx, &cx.body, tm);
    if th > 0 {
        area += ta + 12 * i64::from(th);
        min_h += th;
    }
    let label_floor = if a.label_type == LabelType::Buttons {
        15
    } else {
        tm
    };
    let rows = a
        .labels
        .iter()
        .map(|l| (markup(l), label_floor))
        .chain(a.checkboxes.iter().map(|l| (plain(&mnemonic(l).0), 13)));
    for (p, floor) in rows {
        let (ta, th) = text_area(&p, cx, &cx.body, tm);
        let row = g + floor.max(th);
        area += ta + i64::from(pc + 17) * i64::from(row);
        min_h += row;
        min_w = min_w.max(pc + 5);
    }
    let row = cx.row_w(&a.buttons);
    if row > 0 {
        min_w = min_w.max(row);
        area += i64::from(row + 12) * i64::from(cx.button_h);
        min_h += cx.button_h;
    }
    let (min_w, min_h) = (min_w + 12, min_h + 6);
    area += 12 * i64::from(min_w);
    let grow = |landscape: bool| {
        let (mut w, mut h) = (min_w, min_h);
        let prop = |w: i32, h: i32| {
            let (w, h) = (i64::from(w), i64::from(h));
            if landscape {
                618 * w >= 1000 * h
            } else {
                1000 * w >= 618 * h
            }
        };
        let mut phase = if prop(w, h) { 1 } else { 0 };
        while i64::from(w) * i64::from(h) < area {
            match phase {
                0 => {
                    w += 10;
                    if prop(w, h) {
                        phase = 2;
                    }
                }
                1 => {
                    h += 10;
                    if !prop(w, h) {
                        phase = 2;
                    }
                }
                _ if landscape => {
                    w += 10;
                    h = w * 618 / 1000;
                }
                _ => {
                    h += 10;
                    w = h * 618 / 1000;
                }
            }
        }
        (wide_enough(a, w, cx, max_w), h)
    };
    ([grow(true), grow(false)], (min_w, min_h))
}

/// Widest word (split at spaces, tabs and newlines, as `wrap_pieces` does) of `p`.
fn widest_word(p: &[Piece], cx: &Ctx, font: &FontSpec) -> i32 {
    let text: String = p.iter().map(|x| x.text.as_str()).collect();
    text.split([' ', '\t', '\n'])
        .map(|w| cx.m.text_width(w, font))
        .max()
        .unwrap_or(0)
}

/// A-CORE-1, R-OB-34 "too narrow": body width `w`, grown by 10 px until the heading's
/// widest word fits beside the icon's 37 px indent and every word of the heading, text,
/// labels and check boxes fits its line, or until it reaches `max_w`.
fn wide_enough(a: &AskModel, mut w: i32, cx: &Ctx, max_w: i32) -> i32 {
    let tx = cx.prefix_col() + 5;
    let hx = if a.icon.is_some() { 37 } else { 0 };
    let heading = if a.heading.is_empty() {
        0
    } else {
        hx + widest_word(&plain(&a.heading), cx, &cx.heading)
    };
    let need = [heading, widest_word(&a.text, cx, &cx.body)]
        .into_iter()
        .chain(
            a.labels
                .iter()
                .map(|l| tx + widest_word(&markup(l), cx, &cx.body)),
        )
        .chain(
            a.checkboxes
                .iter()
                .map(|l| tx + widest_word(&plain(&mnemonic(l).0), cx, &cx.body)),
        )
        .max()
        .unwrap_or(0);
    while w - 12 < need && w < max_w {
        w = (w + 10).min(max_w);
    }
    w
}

/// Which candidate (R-OB-34).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Candidate {
    AboveLeftAligned,
    AboveRightAligned,
    BesideRight,
    BesideLeft,
}

/// R-OB-17 window and body rectangles of a candidate, body `w` x `h`.
pub fn candidate_rects(k: Candidate, c: Rect, w: i32, h: i32) -> (Rect, Rect) {
    match k {
        Candidate::AboveLeftAligned => {
            let b = Rect::new(c.x, c.y - h - 4, w, h);
            (Rect::new(b.x, b.y, w, h + 16), b)
        }
        Candidate::AboveRightAligned => {
            let b = Rect::new(c.right() - w, c.y - h - 4, w, h);
            (Rect::new(b.x, b.y, w, h + 16), b)
        }
        Candidate::BesideRight => (
            Rect::new(c.right() - 12, c.y, w + 16, h),
            Rect::new(c.right() + 4, c.y, w, h),
        ),
        Candidate::BesideLeft => (
            Rect::new(c.x - w - 4, c.y, w + 16, h),
            Rect::new(c.x - w - 4, c.y, w, h),
        ),
    }
}

/// R-OB-34 candidate order for character rect `c` in output `m`.
pub fn candidate_order(c: Rect, m: Rect) -> [Candidate; 4] {
    use Candidate::*;
    if c.x + c.w / 2 >= m.x + m.w / 2 {
        [AboveRightAligned, AboveLeftAligned, BesideLeft, BesideRight]
    } else {
        [AboveLeftAligned, AboveRightAligned, BesideRight, BesideLeft]
    }
}

/// B-OB-3 score: window pixels outside the output.
pub fn score(win: Rect, m: Rect) -> i64 {
    let ix = (win.right().min(m.right()) - win.x.max(m.x)).max(0);
    let iy = (win.bottom().min(m.bottom()) - win.y.max(m.y)).max(0);
    i64::from(win.w) * i64::from(win.h) - i64::from(ix) * i64::from(iy)
}

fn intersect(a: Rect, b: Rect) -> Rect {
    let x = a.x.max(b.x);
    let y = a.y.max(b.y);
    let r = a.right().min(b.right());
    let bt = a.bottom().min(b.bottom());
    Rect::new(x, y, (r - x).max(0), (bt - y).max(0))
}

/// R-OB-27 tail for a placed body.
pub fn tail(k: Candidate, body: Rect, c: Rect) -> (TailSide, [(i32, i32); 3]) {
    let (w, h) = (body.w, body.h);
    match k {
        Candidate::AboveLeftAligned | Candidate::AboveRightAligned => {
            let d = w.min(c.w) * 7 / 10;
            let p = if k == Candidate::AboveLeftAligned {
                body.x + d
            } else {
                body.right() - d
            };
            let bb = body.bottom() - 1;
            let l = 16.min(c.bottom() - bb).max(0);
            let centre = c.x + c.w / 2;
            if p > centre {
                let t = (p - body.x).clamp(8, (w - 18).max(8));
                let x = body.x + t;
                (TailSide::Bottom, [(x, bb), (x, bb + l), (x + 10, bb)])
            } else {
                let t = (p - body.x - 10).clamp(8, (w - 18).max(8));
                let x = body.x + t;
                (TailSide::Bottom, [(x, bb), (x + 10, bb + l), (x + 10, bb)])
            }
        }
        Candidate::BesideRight => {
            let t = ((h * 7 / 10).min(c.h * 3 / 10)).clamp(8, (h - 18).max(8));
            let bx = body.x + 1;
            let l = 16.min(bx - c.x).max(0);
            let y = body.y + t;
            (TailSide::Left, [(bx, y), (bx - l, y), (bx, y + 10)])
        }
        Candidate::BesideLeft => {
            let t = ((h * 7 / 10).min(c.h * 3 / 10)).clamp(8, (h - 18).max(8));
            let bx = body.right() - 1;
            let l = 16.min(c.right() - bx).max(0);
            let y = body.y + t;
            (TailSide::Right, [(bx, y), (bx + l, y), (bx, y + 10)])
        }
    }
}

/// BalloonTooBig: even the minimum size does not fit the output (B-ASK-4).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooBig;

/// Result of placing an Office balloon.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub scene: OfficeBalloonScene,
    /// B-OB-4: character displacement needed so the balloon fits (0, 0 if it fits).
    pub move_by: (i32, i32),
    pub candidate: Candidate,
}

/// B-GAL-1: one body size for every gallery candidate: the widest landscape shape
/// (R-OB-31) of `models`, and at that width the tallest layout.
pub fn common_size(models: &[AskModel], cx: &Ctx) -> (i32, i32) {
    let w = models
        .iter()
        .map(|a| sizes(a, cx).0[0].0)
        .max()
        .unwrap_or(0);
    let h = models
        .iter()
        .map(|a| layout(a, w - 12, cx, &Interaction::default()).content_h + 12)
        .max()
        .unwrap_or(0);
    (w, h)
}

/// Layout for a body `w` wide; with `fixed`, at least that body height, the button row
/// moved to the bottom of the body (B-GAL-1).
fn layout_in(
    a: &AskModel,
    w: i32,
    cx: &Ctx,
    it: &Interaction,
    fixed: Option<(i32, i32)>,
) -> Layout {
    let mut lay = layout(a, w - 12, cx, it);
    if let Some((_, h)) = fixed {
        let dy = h - 12 - lay.content_h;
        if dy > 0 {
            for b in &mut lay.scene.buttons {
                b.rect.y += dy;
            }
            lay.content_h += dy;
            lay.scene.content.h = lay.content_h;
        }
    }
    lay
}

/// B-OB-1..4: size, lay out and place. `character` is the frame rect, `m` the output.
/// `Err` = BalloonTooBig.
pub fn place(
    a: &AskModel,
    it: &Interaction,
    character: Rect,
    m: Rect,
    cx: &Ctx,
) -> Result<Placed, TooBig> {
    place_sized(a, it, character, m, cx, None)
}

/// [`place`] with the body size `fixed` instead of the R-OB-31 shapes (B-GAL-1; the
/// height grows if the text needs more).
pub fn place_sized(
    a: &AskModel,
    it: &Interaction,
    character: Rect,
    m: Rect,
    cx: &Ctx,
    fixed: Option<(i32, i32)>,
) -> Result<Placed, TooBig> {
    let (shapes, (min_w, min_h)) = sizes_within(a, cx, m.w - 16);
    if min_w + 16 > m.w || min_h > m.h {
        return Err(TooBig);
    }
    let one = fixed.map(|f| [f]);
    let shapes = one.as_ref().map_or(&shapes[..], |f| &f[..]);
    let lays: Vec<Layout> = shapes
        .iter()
        .map(|&(w, _)| layout_in(a, w, cx, it, fixed))
        .collect();
    let c = intersect(character, m);
    let c = if c.w == 0 || c.h == 0 {
        // Character fully off its output: anchor at the bottom-right corner (R-OB-17).
        Rect::new(m.right() - 10, m.bottom() - 10, 10, 10)
    } else {
        c
    };
    let order = candidate_order(c, m);
    let rows = a.labels.len() + a.checkboxes.len();
    let mut best: Option<(i64, Candidate, &Layout)> = None;
    for &k in &order {
        // B-OB-3, R-OB-34: the better of the two shapes counts for the candidate.
        let mut shape: Option<(i64, &Layout)> = None;
        for lay in &lays {
            let (win, _) = candidate_rects(k, c, lay.content_w + 12, lay.content_h + 12);
            let s = score(win, m);
            if shape.is_none_or(|(bs, bl)| s < bs || (s == bs && rows > 1 && squarer(lay, bl))) {
                shape = Some((s, lay));
            }
        }
        let (s, lay) = shape.expect("a shape");
        if best.is_none_or(|b| s < b.0) {
            best = Some((s, k, lay));
        }
        if s == 0 {
            break;
        }
    }
    let (s, k, lay) = best.expect("four candidates");
    let lay = lay.clone();
    let (w, h) = (lay.content_w + 12, lay.content_h + 12);
    let (mut win, _) = candidate_rects(k, c, w, h);
    let mut move_by = (0, 0);
    if s > 0 {
        let dx = if win.x < m.x {
            m.x - win.x
        } else if win.right() > m.right() {
            m.right() - win.right()
        } else {
            0
        };
        let dy = if win.y < m.y {
            m.y - win.y
        } else if win.bottom() > m.bottom() {
            m.bottom() - win.bottom()
        } else {
            0
        };
        move_by = (dx, dy);
        win.x += dx;
        win.y += dy;
    }
    let c2 = Rect::new(c.x + move_by.0, c.y + move_by.1, c.w, c.h);
    let (_, body) = candidate_rects(k, c2, w, h);
    Ok(assemble(k, lay, body, win, c2, move_by))
}

/// R-OB-34 shape tie: body `a` is squarer than `b` (smaller integer long / short side
/// ratio, then smaller area).
fn squarer(a: &Layout, b: &Layout) -> bool {
    let key = |l: &Layout| {
        let (w, h) = (l.content_w + 12, l.content_h + 12);
        (w.max(h) / w.min(h).max(1), i64::from(w) * i64::from(h))
    };
    key(a) < key(b)
}

fn assemble(k: Candidate, lay: Layout, body: Rect, win: Rect, c: Rect, mv: (i32, i32)) -> Placed {
    let (side, tail_pts) = tail(k, body, c);
    let content = Rect::new(body.x + 6, body.y + 6, lay.content_w, lay.content_h);
    let mut sc = lay.scene;
    translate(&mut sc, content.x, content.y);
    sc.window = win;
    sc.body = body;
    sc.tail_side = side;
    sc.tail = tail_pts;
    sc.content = content;
    Placed {
        scene: sc,
        move_by: mv,
        candidate: k,
    }
}

/// B-GAL-1: the balloon kept at its place for a new character, if it still fits next to
/// it. `old` is the body of placement `k`, `character` the envelope of all candidates at
/// the new character's foot, `visible` the new character's opaque rest box (or its frame),
/// `fixed` the gallery's common body size. The new body keeps the corner at the tail:
/// bottom-left or bottom-right above the envelope, top-left or top-right beside it (with
/// the common size it is the old body unless the text needs more height). It is kept when
/// its window is inside the output, the body does not cover `visible` and it still overlaps
/// the envelope along the other axis, however far the new character is from it. `None`:
/// place again.
#[allow(clippy::too_many_arguments)]
pub fn place_kept(
    a: &AskModel,
    it: &Interaction,
    k: Candidate,
    old: Rect,
    character: Rect,
    visible: Rect,
    m: Rect,
    cx: &Ctx,
    fixed: Option<(i32, i32)>,
) -> Option<Placed> {
    use Candidate::*;
    let c = intersect(character, m);
    if c.w == 0 || c.h == 0 {
        return None;
    }
    let shapes = fixed.map_or_else(|| sizes(a, cx).0, |f| [f, f]);
    for &(sw, _) in &shapes {
        let lay = layout_in(a, sw, cx, it, fixed);
        let (w, h) = (lay.content_w + 12, lay.content_h + 12);
        // Body, window, overlap along the other axis.
        let (body, win, overlap) = match k {
            AboveLeftAligned | AboveRightAligned => {
                let x = if k == AboveLeftAligned {
                    old.x
                } else {
                    old.right() - w
                };
                let b = Rect::new(x, old.bottom() - h, w, h);
                let o = b.x < c.right() && b.right() > c.x;
                (b, Rect::new(b.x, b.y, w, h + 16), o)
            }
            BesideRight | BesideLeft => {
                let (b, win) = if k == BesideRight {
                    let b = Rect::new(old.x, old.y, w, h);
                    (b, Rect::new(b.x - 16, b.y, w + 16, h))
                } else {
                    let b = Rect::new(old.right() - w, old.y, w, h);
                    (b, Rect::new(b.x, b.y, w + 16, h))
                };
                let o = b.y < c.bottom() && b.bottom() > c.y;
                (b, win, o)
            }
        };
        let i = intersect(body, visible);
        let covers = i.w > 0 && i.h > 0;
        if score(win, m) == 0 && !covers && overlap {
            return Some(assemble(k, lay, body, win, c, (0, 0)));
        }
    }
    None
}

/// Move a whole placed scene (window, body, tail and content) by (dx, dy).
pub fn shift(s: &mut OfficeBalloonScene, dx: i32, dy: i32) {
    for r in [&mut s.window, &mut s.body, &mut s.content] {
        r.x += dx;
        r.y += dy;
    }
    for p in &mut s.tail {
        p.0 += dx;
        p.1 += dy;
    }
    translate(s, dx, dy);
}

fn translate(s: &mut OfficeBalloonScene, dx: i32, dy: i32) {
    let mv = |r: &mut Rect| {
        r.x += dx;
        r.y += dy;
    };
    let mvl = |l: &mut Vec<TextLine>| {
        for line in l {
            line.y += dy;
            for r in &mut line.runs {
                r.x += dx;
            }
        }
    };
    if let Some((_, r)) = &mut s.icon {
        mv(r);
    }
    mvl(&mut s.heading);
    mvl(&mut s.text);
    for l in &mut s.labels {
        mv(&mut l.rect);
        mv(&mut l.prefix_rect);
        mvl(&mut l.lines);
    }
    for c in &mut s.checkboxes {
        mv(&mut c.rect);
        mv(&mut c.box_rect);
        mvl(&mut c.lines);
    }
    for b in &mut s.buttons {
        mv(&mut b.rect);
    }
}

/// Hit test on a placed scene.
pub fn hit(s: &OfficeBalloonScene, x: i32, y: i32) -> Option<Control> {
    if let Some(i) = s.buttons.iter().position(|b| b.rect.contains(x, y)) {
        return Some(Control::Button(i));
    }
    if let Some(i) = s
        .labels
        .iter()
        .position(|l| l.clickable && l.rect.contains(x, y))
    {
        return Some(Control::Label(i));
    }
    s.checkboxes
        .iter()
        .position(|c| c.rect.contains(x, y))
        .map(Control::Checkbox)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::speak::tests::Fixed;

    fn spec() -> AskSpec {
        AskSpec {
            text: Some("Hello".into()),
            ..Default::default()
        }
    }

    fn err(r: Result<AskModel, MethodError>) -> String {
        let e = r.unwrap_err();
        format!("{} {}", e.name, e.message)
    }

    #[test]
    fn b_ask_2_sets_and_results() {
        let want: [&[i32]; 15] = [
            &[],
            &[-1],
            &[-2],
            &[-1, -2],
            &[-3, -4],
            &[-3, -4, -2],
            &[-5, -12],
            &[-6, -12],
            &[-5, -6, -12],
            &[-7, -2],
            &[-8, -7, -9],
            &[-10, -12],
            &[-5, -6, -11],
            &[-13, -14, -12],
            &[-3, -15, -4, -2],
        ];
        for (i, name) in SET_NAMES.iter().enumerate() {
            let s = AskSpec {
                buttons: Some(AskButtons::Set(name.to_string())),
                labels: Some(vec!["x".into()]),
                ..spec()
            };
            let a = validate(&s, Lang::En).unwrap();
            let r: Vec<i32> = a.buttons.iter().map(|b| b.result).collect();
            assert_eq!(r, want[i], "{name}");
        }
        let s = AskSpec {
            buttons: Some(AskButtons::Set("yes-no-cancel".into())),
            ..spec()
        };
        let a = validate(&s, Lang::De).unwrap();
        let caps: Vec<&str> = a.buttons.iter().map(|b| b.caption.as_str()).collect();
        assert_eq!(caps, vec!["&Ja", "&Nein", "Abbrechen"]);
        assert_eq!(a.esc_result, Some(-2));
        let s = AskSpec {
            buttons: Some(AskButtons::Set("back-next-close".into())),
            ..spec()
        };
        assert_eq!(validate(&s, Lang::En).unwrap().esc_result, Some(-12));
        let s = AskSpec {
            buttons: Some(AskButtons::Set("yes-no".into())),
            ..spec()
        };
        assert_eq!(validate(&s, Lang::En).unwrap().esc_result, None);
        let s = AskSpec {
            buttons: Some(AskButtons::Custom(vec!["A".into(), "B".into()])),
            ..spec()
        };
        let a = validate(&s, Lang::En).unwrap();
        assert_eq!(a.buttons[1].result, -26);
        assert_eq!(
            validate(&spec(), Lang::En).unwrap().buttons[0].caption,
            "OK"
        );
    }

    #[test]
    fn b_ask_5_validation() {
        let s = AskSpec {
            labels: Some(vec!["l".into(); 6]),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("TooManyControls"));
        let s = AskSpec {
            checkboxes: Some(vec!["c".into(); 6]),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("TooManyControls"));
        let s = AskSpec {
            buttons: Some(AskButtons::Custom(vec!["b".into(); 7])),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("TooManyControls"));
        let s = AskSpec {
            buttons: Some(AskButtons::Set("none".into())),
            ..spec()
        };
        let e = err(validate(&s, Lang::En));
        assert!(
            e.contains("InvalidBalloon") && e.contains("ButtonlessModal"),
            "{e}"
        );
        // Clickable labels are enough for modal.
        let s = AskSpec {
            buttons: Some(AskButtons::Set("none".into())),
            labels: Some(vec!["a".into()]),
            ..spec()
        };
        assert!(validate(&s, Lang::En).is_ok());
        // Bullet labels are not clickable.
        let s = AskSpec {
            label_type: Some("bullets".into()),
            ..s
        };
        assert!(err(validate(&s, Lang::En)).contains("ButtonlessModal"));
        // Autodown without buttons is fine.
        let s = AskSpec {
            buttons: Some(AskButtons::Set("none".into())),
            mode: Some("autodown".into()),
            ..spec()
        };
        assert!(validate(&s, Lang::En).is_ok());
        let s = AskSpec {
            buttons: Some(AskButtons::Set("none".into())),
            mode: Some("autodown".into()),
            checkboxes: Some(vec!["c".into()]),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("CheckboxesNeedButton"));
    }

    #[test]
    fn b_ask_1_contract() {
        assert!(err(validate(&AskSpec::default(), Lang::En)).contains("InvalidArgs"));
        let s = AskSpec {
            mode: Some("modeless".into()),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("NotSupported"));
        let s = AskSpec {
            icon: Some("bogus".into()),
            ..spec()
        };
        assert!(err(validate(&s, Lang::En)).contains("InvalidArgs"));
        assert_eq!(
            validate(&spec(), Lang::En).unwrap().animation.as_deref(),
            Some("Explain")
        );
        let s = AskSpec {
            animation: Some(String::new()),
            heading: Some("H".into()),
            text: None,
            ..spec()
        };
        assert_eq!(validate(&s, Lang::En).unwrap().animation, None);
    }

    #[test]
    fn b_ask_3_r_ob_16_markup() {
        let p = markup("a{cf 249}red{cf 300}still{ul 1}u{ul 0} {bmp x.bmp}{foo}");
        assert_eq!(
            p,
            vec![
                Piece {
                    text: "a".into(),
                    color: None,
                    underline: false
                },
                Piece {
                    text: "redstill".into(),
                    color: Some([0xFF, 0, 0]),
                    underline: false
                },
                Piece {
                    text: "u".into(),
                    color: Some([0xFF, 0, 0]),
                    underline: true
                },
                Piece {
                    text: " {foo}".into(),
                    color: Some([0xFF, 0, 0]),
                    underline: false
                },
            ]
        );
        assert_eq!(cf_color(7), Some([0xC0, 0xC0, 0xC0]));
        assert_eq!(cf_color(8), None);
        assert_eq!(markup("x {wmf a}{pict}{dlx 1}{CBMP 1 2}y")[0].text, "x y");
        assert_eq!(markup("{unclosed")[0].text, "{unclosed");
    }

    #[test]
    fn mnemonics() {
        assert_eq!(mnemonic("&Yes"), ("Yes".into(), Some(0), Some('y')));
        assert_eq!(
            mnemonic("Yes To &All"),
            ("Yes To All".into(), Some(7), Some('a'))
        );
        assert_eq!(mnemonic("OK"), ("OK".into(), None, None));
        assert_eq!(mnemonic("A && B"), ("A & B".into(), None, None));
    }

    fn cx(m: &Fixed) -> Ctx<'_> {
        Ctx::new(m, 800)
    }

    #[test]
    fn r_ob_32_r_ob_11_b_ob_1_r_ob_4_button_row_and_fonts() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let a = validate(
            &AskSpec {
                buttons: Some(AskButtons::Set("yes-no-cancel".into())),
                ..spec()
            },
            Lang::En,
        )
        .unwrap();
        // Widest "Cancel" = 42 -> 45 + 22 = 67.
        assert_eq!(c.button_w(&a.buttons), 67);
        let lay = layout(&a, 300, &c, &Interaction::default());
        let b = &lay.scene.buttons;
        let row = 3 * (67 + 10) - 10;
        assert_eq!(b[0].rect.x, (300 - row) / 2);
        assert_eq!(b[1].rect.x - b[0].rect.x, 77);
        assert_eq!(b[0].rect.h, 22);
        assert_eq!(b[0].caption, "Yes");
        assert_eq!(b[0].mnemonic_index, Some(0));
        assert_eq!(Ctx::new(&m, 1024).button_h, 24);
        assert_eq!(fonts(1023).0.px, 13);
        assert_eq!(fonts(1024).0.px, 16);
    }

    #[test]
    fn r_ob_35_r_ob_5_vertical_spacing() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let g = 8;
        let a = validate(
            &AskSpec {
                heading: Some("Head".into()),
                icon: Some("info".into()),
                text: Some("Body".into()),
                labels: Some(vec!["L1".into(), "L2".into()]),
                checkboxes: Some(vec!["C1".into()]),
                ..Default::default()
            },
            Lang::En,
        )
        .unwrap();
        let lay = layout(&a, 300, &c, &Interaction::default());
        let s = &lay.scene;
        // Heading block: max(32, 16) = 32, bottom-aligned heading.
        assert_eq!(s.icon.unwrap().1, Rect::new(0, 0, 32, 32));
        assert_eq!(s.heading[0].y, 16);
        assert_eq!(s.heading[0].runs[0].x, 37);
        assert_eq!(s.text[0].y, 32 + g);
        let ly = 32 + g + 16 + g;
        assert_eq!(s.labels[0].rect.y, ly);
        assert_eq!(s.labels[1].rect.y, ly + 16 + g);
        let cy = ly + 16 + g + 16 + g;
        assert_eq!(s.checkboxes[0].rect.y, cy);
        assert_eq!(s.buttons[0].rect.y, cy + 16 + 2 * g);
        assert_eq!(lay.content_h, cy + 16 + 2 * g + 22);
        // Label prefix column: floor 17, text at 22.
        assert_eq!(s.labels[0].lines[0].runs[0].x, 22);
        // A label list that follows nothing gets 1 px.
        let a = validate(
            &AskSpec {
                labels: Some(vec!["only".into()]),
                ..Default::default()
            },
            Lang::En,
        )
        .unwrap();
        let lay = layout(&a, 300, &c, &Interaction::default());
        assert_eq!(lay.scene.labels[0].rect.y, 1);
        assert_eq!(lay.scene.labels[0].prefix, LabelPrefix::Bullet);
    }

    #[test]
    fn r_ob_31_sizes_landscape_portrait() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let a = validate(
            &AskSpec {
                text: Some("word ".repeat(60)),
                ..Default::default()
            },
            Lang::En,
        )
        .unwrap();
        let ([l, p], (mw, mh)) = sizes(&a, &c);
        assert!(l.0 >= mw && l.1 >= mh);
        // Landscape is wider than tall, portrait the reverse.
        assert!(l.0 > l.1, "{l:?}");
        assert!(p.1 > p.0, "{p:?}");
        // Both reach the area (A-CORE-1, worked out in r_ob_31_growth_phases).
        assert!(i64::from(l.0) * i64::from(l.1) >= 35458, "{l:?}");
        assert!(i64::from(p.0) * i64::from(p.1) >= 35458, "{p:?}");
    }

    // A-CORE-1, R-OB-31: the size method's terms, worked by hand. Fixed metrics: 7 px per
    // character, TM.height 12, G 6, prefix column 17, button height 22.
    #[test]
    fn a_core_1_r_ob_31_area_terms_and_minimum() {
        let m = Fixed { cw: 7, lh: 12 };
        let c = cx(&m);
        let spec = |label_type: &str| AskSpec {
            heading: Some("Hi".into()),
            icon: Some("info".into()),
            text: Some("abc\nde".into()),
            labels: Some(vec!["one".into(), "two".into()]),
            label_type: Some(label_type.into()),
            checkboxes: Some(vec!["&Box".into()]),
            ..Default::default()
        };
        // Icon: A 1024, 32 x 32. Heading "Hi": A += 14 x 12, min h max(32, 12) = 32,
        // A += 12 x 32, min h 38. Text: A += 35 x 12 + 12 x 24, min h 62. Two button
        // labels, row 6 + max(15, 12) = 21: A += 2 x (21 x 12 + 34 x 21), min h 104,
        // min w 22. Check box, row 6 + max(13, 12) = 19: A += 21 x 12 + 34 x 19, min h
        // 123. Button row 37 ("OK" 14 -> 15 + 22): min w 37, A += 49 x 22, min h 145.
        // Body: 49 x 151, A += 12 x 49 = 6780 <= 49 x 151, so no growth. Too narrow:
        // "Hi" beside the icon needs 37 + 14 = 51 px of content, so both widen to 69.
        let a = validate(&spec("buttons"), Lang::En).unwrap();
        assert_eq!(sizes(&a, &c), ([(69, 151), (69, 151)], (49, 151)));
        // Bullet labels: the row floor is TM.height, row 6 + 12 = 18, so 6 px less each.
        let a = validate(&spec("bullets"), Lang::En).unwrap();
        assert_eq!(sizes(&a, &c), ([(69, 145), (69, 145)], (49, 145)));
    }

    // R-OB-31: the three growth phases in 10 px steps with the integer 618 / 1000 test.
    #[test]
    fn r_ob_31_growth_phases() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let a = validate(
            &AskSpec {
                text: Some("word ".repeat(60)),
                ..Default::default()
            },
            Lang::En,
        )
        .unwrap();
        // Text: A = 2100 x 16 + 12 x 16, min h 16. Button row 37: A += 49 x 22, min h 38.
        // Body 49 x 44, A += 12 x 49: A = 35458.
        // Landscape: phase 0 widens to 79 (618 x 79 >= 1000 x 44), then phase 2 sets
        // h = w x 618 / 1000 up to 249 x 153 = 38097 (239 x 147 = 35133 is short).
        // Portrait: in proportion, phase 1 raises h to 84 (1000 x 49 < 618 x 84), then
        // phase 2 up to 150 x 244 (144 x 234 = 33696 is short).
        let (shapes, min) = sizes(&a, &c);
        assert_eq!(min, (49, 44));
        assert_eq!(shapes, [(249, 153), (150, 244)]);
    }

    // R-OB-34, A-CORE-1 "too narrow": a width whose line cannot hold the widest word
    // grows by 10 px until it can.
    #[test]
    fn r_ob_34_fit_widens_refused_width() {
        let m = Fixed { cw: 7, lh: 12 };
        let c = cx(&m);
        let base = AskSpec {
            text: Some("x".into()),
            ..Default::default()
        };
        let a = validate(&base, Lang::En).unwrap();
        // Minimum 49 x 40 (content 37 wide), no growth needed for so little text.
        assert_eq!(sizes(&a, &c).0[0].0, 49);
        // A 10-character word (70 px) in the text: the shapes 79 x 48 and 49 x 60 (area
        // 2818) both widen to 89 (content 77 >= 70).
        let a = validate(
            &AskSpec {
                text: Some("x abcdefghij".into()),
                ..base.clone()
            },
            Lang::En,
        )
        .unwrap();
        assert_eq!(sizes(&a, &c).0.map(|s| s.0), [89, 89]);
        // A heading word beside the icon: 37 + 6 x 7 = 79, content >= 79. The shapes are
        // 59 x 78 and 49 x 88 (area 3974), both widened to 99.
        let a = validate(
            &AskSpec {
                heading: Some("a abcdef".into()),
                icon: Some("info".into()),
                ..base.clone()
            },
            Lang::En,
        )
        .unwrap();
        assert_eq!(sizes(&a, &c).0.map(|s| s.0), [99, 99]);
        // A label word after the prefix column + 5: 22 + 70 = 92. The shapes are 59 x 61
        // and 49 x 71 (area 3448), both widened to 109 (content 97).
        let a = validate(
            &AskSpec {
                labels: Some(vec!["abcdefghij".into()]),
                ..base.clone()
            },
            Lang::En,
        )
        .unwrap();
        assert_eq!(sizes(&a, &c).0.map(|s| s.0), [109, 109]);
        // The wrapped layout at that width keeps the word on one line.
        let lay = layout(&a, 97, &c, &Interaction::default());
        assert_eq!(lay.scene.labels[0].lines.len(), 1);
    }

    // R-OB-34: the too-narrow widening is clamped to the free space; here the output
    // (body plus the 16 px tail). A word wider than that is broken as before.
    #[test]
    fn r_ob_34_widening_clamped_to_output() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let out = Rect::new(0, 0, 800, 600);
        let a = validate(
            &AskSpec {
                text: Some("x".repeat(150)),
                ..Default::default()
            },
            Lang::En,
        )
        .unwrap();
        let p = place(
            &a,
            &Interaction::default(),
            Rect::new(500, 400, 100, 100),
            out,
            &c,
        )
        .unwrap();
        let w = p.scene.window;
        assert!(w.x >= 0 && w.right() <= 800, "{w:?}");
        assert_eq!(p.scene.body.w, 784);
    }

    // B-OB-3, R-OB-34: both shapes score 0 for the first candidate. With more than one
    // label and check-box row the squarer body wins (integer long / short ratio, then the
    // smaller area); otherwise the landscape shape stays.
    #[test]
    fn r_ob_34_shape_tie_rule() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let out = Rect::new(0, 0, 1000, 800);
        let ch = Rect::new(100, 500, 100, 100);
        let it = Interaction::default();
        let ask = |text: Option<&str>, labels: &[String]| {
            validate(
                &AskSpec {
                    text: text.map(Into::into),
                    labels: Some(labels.to_vec()),
                    ..Default::default()
                },
                Lang::En,
            )
            .unwrap()
        };
        let long = "a".repeat(27);
        // Shapes 89 x 92 and 75 x 122, both widened for the 189 px label word to 229 and
        // 225. Laid out, both bodies are 114 high: ratios 229 / 114 = 2 and 225 / 114 = 1.
        let a = ask(Some("word word "), &[long.clone(), "x".into()]);
        assert_eq!(sizes(&a, &c).0.map(|s| s.0), [229, 225]);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.candidate, Candidate::AboveLeftAligned);
        assert_eq!((p.scene.body.w, p.scene.body.h), (225, 114));
        // One row: the landscape shape stays.
        let a = ask(Some("word word "), std::slice::from_ref(&long));
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.scene.body.w, sizes(&a, &c).0[0].0);
        // Equal ratios: 389 x 91 and 387 x 91 (both 4), the smaller area wins.
        let a = ask(None, &["a".repeat(50), "x".into()]);
        assert_eq!(sizes(&a, &c).0.map(|s| s.0), [389, 387]);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!((p.scene.body.w, p.scene.body.h), (387, 91));
    }

    #[test]
    fn b_ob_3_b_ob_4_r_ob_17_r_ob_20_r_ob_34_placement_and_fit() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let out = Rect::new(0, 0, 1000, 800);
        let a = validate(&spec(), Lang::En).unwrap();
        let it = Interaction::default();
        // Character in the left half, room above: above, left-aligned.
        let ch = Rect::new(100, 500, 100, 100);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.move_by, (0, 0));
        assert_eq!(p.scene.window.x, 100);
        assert_eq!(p.scene.window.bottom(), 500 + 12);
        assert_eq!(p.scene.tail_side, TailSide::Bottom);
        // Right half: above, right-aligned.
        let ch = Rect::new(800, 500, 100, 100);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.scene.window.right(), 900);
        // No room above, right half: beside, left of the character.
        let ch = Rect::new(800, 0, 100, 100);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.scene.tail_side, TailSide::Right);
        assert_eq!(p.scene.window.right(), 800 + 12);
        // No room above, left half: beside, to the right.
        let ch = Rect::new(10, 0, 100, 100);
        let p = place(&a, &it, ch, out, &c).unwrap();
        assert_eq!(p.scene.tail_side, TailSide::Left);
        assert_eq!(p.scene.body.x, 114);
        // Nothing fits: move the character (B-OB-4).
        let small = Rect::new(0, 0, 160, 120);
        let ch = Rect::new(40, 10, 80, 80);
        let p = place(&a, &it, ch, small, &c).unwrap();
        assert_ne!(p.move_by, (0, 0));
        let w = p.scene.window;
        assert!(
            w.x >= 0 && w.y >= 0 && w.right() <= 160 && w.bottom() <= 120,
            "{w:?}"
        );
        // Too big.
        assert!(place(&a, &it, ch, Rect::new(0, 0, 40, 40), &c).is_err());
    }

    #[test]
    fn b_ob_2_r_ob_27_tail() {
        let c = Rect::new(100, 500, 100, 100);
        let body = Rect::new(100, 400, 200, 80);
        let (side, t) = tail(Candidate::AboveLeftAligned, body, c);
        assert_eq!(side, TailSide::Bottom);
        // 70 % of min(200, 100) = 70: at x 170, right of centre 150: variant 6.
        assert_eq!(t, [(170, 479), (170, 495), (180, 479)]);
        // Side tail: min(70 % of 80, 30 % of 100) = 30.
        let body = Rect::new(204, 500, 200, 80);
        let (side, t) = tail(Candidate::BesideRight, body, c);
        assert_eq!(side, TailSide::Left);
        assert_eq!(t, [(205, 530), (189, 530), (205, 540)]);
        // R-OB-26: the length is clamped so the tip stays within C.
        let short = Rect::new(100, 470, 100, 15);
        let body = Rect::new(100, 400, 200, 70);
        let (_, t) = tail(Candidate::AboveLeftAligned, body, short);
        assert_eq!(t[1].1 - t[0].1, 485 - 469);
    }

    #[test]
    fn focus_order_and_hit() {
        let m = Fixed { cw: 7, lh: 16 };
        let c = cx(&m);
        let a = validate(
            &AskSpec {
                labels: Some(vec!["a".into()]),
                checkboxes: Some(vec!["&check".into()]),
                buttons: Some(AskButtons::Set("ok-cancel".into())),
                ..spec()
            },
            Lang::En,
        )
        .unwrap();
        assert_eq!(
            a.focus_order(),
            vec![
                Control::Label(0),
                Control::Checkbox(0),
                Control::Button(0),
                Control::Button(1)
            ]
        );
        assert_eq!(a.initial_focus(), Some(2));
        let p = place(
            &a,
            &Interaction::default(),
            Rect::new(100, 500, 100, 100),
            Rect::new(0, 0, 1000, 800),
            &c,
        )
        .unwrap();
        let b = p.scene.buttons[1].rect;
        assert_eq!(hit(&p.scene, b.x + 1, b.y + 1), Some(Control::Button(1)));
        let l = p.scene.labels[0].rect;
        assert_eq!(hit(&p.scene, l.x + 1, l.y + 1), Some(Control::Label(0)));
        assert_eq!(p.scene.checkboxes[0].mnemonic, Some('c'));
        assert_eq!(p.scene.checkboxes[0].mnemonic_at, Some((0, 0, 0)));
        let lines = vec![TextLine {
            y: 0,
            runs: vec![
                TextRun {
                    x: 0,
                    text: "Enable".into(),
                    color: [0; 3],
                    underline: false,
                },
                TextRun {
                    x: 50,
                    text: "network".into(),
                    color: [0; 3],
                    underline: false,
                },
            ],
        }];
        let (cap, idx, _) = mnemonic("Enable &network");
        assert_eq!(locate(&cap, idx.unwrap(), &lines), Some((0, 1, 0)));
    }
}
