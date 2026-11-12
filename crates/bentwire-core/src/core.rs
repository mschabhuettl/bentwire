//! The request engine: queue, phases, idle, input, menu, gallery (spec section 3).

use std::collections::VecDeque;
use std::sync::Arc;

use crate::api::*;
use crate::balloon;
use crate::fallback::{self, Resolved};
use crate::geometry::{self, Slide};
use crate::l10n::{self, MenuString};
use crate::model::{CharacterModel, Family};
use crate::office::{self, AskModel, ButtonDef, Control, Interaction, LabelType, Mode};
use crate::player::{self, Env, Pic, Player};
use crate::speak::{self, SpeakText};

/// R-IDLE-1 delay.
const IDLE_DELAY: Ms = 4000;
/// R-BAL-10 auto-hide delay.
const AUTO_HIDE: Ms = 1500;

/// B-MENU-4 Office 97 Animate! ids, XP names.
const ANIMATE_NAMES: [&str; 23] = [
    "Greeting",
    "Explain",
    "Congratulate",
    "GetAttention",
    "Wave",
    "Searching",
    "Print",
    "GestureRight",
    "Writing",
    "Processing",
    "Thinking",
    "SendMail",
    "Hearing_1",
    "Show",
    "GetArtsy",
    "GetTechy",
    "GetWizardy",
    "CheckingSomething",
    "Save",
    "GestureDown",
    "GestureLeft",
    "GestureUp",
    "EmptyTrash",
];

#[derive(Clone, Debug)]
enum Kind {
    Show { fast: bool, cause: u32 },
    Hide { fast: bool, cause: u32 },
    Play(Resolved),
    Speak(Vec<SpeakText>),
    Ask(Box<AskModel>),
    MoveTo { x: i32, y: i32, speed: u32 },
    GestureAt { x: i32, y: i32 },
}

impl Kind {
    /// B-Q-5 category.
    fn category(&self) -> &'static str {
        match self {
            Kind::Show { .. } | Kind::Hide { .. } => "showhide",
            Kind::Play(_) | Kind::GestureAt { .. } => "play",
            Kind::MoveTo { .. } => "move",
            Kind::Speak(_) => "speak",
            Kind::Ask(_) => "ask",
        }
    }
}

#[derive(Clone, Debug)]
struct Req {
    id: u32,
    /// Unique name of the client; `None` for daemon requests (no signals).
    sender: Option<String>,
    kind: Kind,
}

#[derive(Clone, Debug)]
enum Stage {
    /// Waiting for the player (the request's animation).
    Anim(u8),
    Slide {
        slide: Slide,
        deadline: Ms,
    },
    /// Speak output phase, paced by `Core::speech`.
    Output,
}

#[derive(Clone, Debug)]
struct Running {
    req: Req,
    stage: Stage,
    failure: Option<&'static str>,
    /// MoveTo: the slide phase emitted Move itself (Speed 0).
    move_emitted: bool,
    speak_text: Option<SpeakText>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Owner {
    None,
    Request,
    Idle,
}

struct Char {
    model: Arc<CharacterModel>,
    visible: bool,
    x: i32,
    y: i32,
    player: Player,
    output: Option<u32>,
}

/// Agent balloon state (R-BAL-7..10).
struct Speech {
    style_lines: u32,
    font: FontSpec,
    fg: [u8; 3],
    bg: [u8; 3],
    border: [u8; 3],
    w: i32,
    h: i32,
    line_h: i32,
    /// Wrapped lines of the whole text.
    lines: Vec<speak::WrappedLine>,
    /// Words revealed.
    revealed: usize,
    words: usize,
    bookmarks: Vec<(usize, i32)>,
    sender: Option<String>,
    interval: Ms,
    next_word: Option<Ms>,
    end_at: Option<Ms>,
    auto_hide: bool,
    hide_at: Option<Ms>,
    shown: bool,
    /// Index of the first visible line.
    first_line: usize,
    scroll: Option<(Ms, Ms, i32)>,
    active: bool,
}

struct OpenAsk {
    /// 0 for the gallery.
    id: u32,
    sender: Option<String>,
    model: AskModel,
    it: Interaction,
    scene: Option<OfficeBalloonScene>,
    /// Placement candidate of `scene` (B-GAL-1 keeps it).
    candidate: Option<office::Candidate>,
    /// Hidden during a drag or slide (modal), placed again afterwards.
    hidden: bool,
}

struct Gallery {
    original: String,
    index: usize,
}

struct Press {
    button: u32,
    modifiers: u32,
    x: i32,
    y: i32,
    left: i32,
    top: i32,
    dbl: bool,
}

struct Menu {
    scene: MenuScene,
    from_tray: bool,
}

/// The deterministic core. See [`crate::api`] for the driving contract.
pub struct Core {
    now: Ms,
    config: Config,
    locale: LocaleEnv,
    lang: Lang,
    state_file: Option<StateFile>,
    /// The user chose the character (B-CHAR-4).
    chosen: bool,
    catalog: Vec<CatalogEntry>,
    loader: Box<dyn CharacterLoader>,
    metrics: Box<dyn TextMetrics>,
    draws: Box<dyn Draws>,
    outputs: Vec<OutputInfo>,
    hour: u8,
    ch: Option<Char>,
    next_id: u32,
    queue: VecDeque<Req>,
    running: Option<Running>,
    owner: Owner,
    ask: Option<OpenAsk>,
    gallery: Option<Gallery>,
    idle_on: bool,
    idle_at: Option<Ms>,
    idle_n: u32,
    idle_started: bool,
    idle_begin: Option<Ms>,
    speech: Option<Speech>,
    press: Option<Press>,
    dragging: bool,
    last_press: Option<Ms>,
    menu: Option<Menu>,
    animate: Vec<String>,
    animate_pos: usize,
    move_cause: u32,
    visibility_cause: u32,
    persist_dirty: bool,
    last_persist: Option<Ms>,
    out: Vec<Output>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Timer {
    Player,
    Idle,
    Slide,
    Word,
    SpeechEnd,
    AutoHide,
    Scroll,
    Persist,
}

fn err(name: &str, message: impl Into<String>) -> MethodError {
    MethodError {
        name: name.into(),
        message: message.into(),
    }
}

impl Core {
    /// Creates the core. Nothing happens until [`Core::start`].
    pub fn new(init: CoreInit) -> Self {
        let mut catalog = init.catalog;
        catalog.sort_by(|a, b| a.id.cmp(&b.id));
        let lang = l10n::resolve_language(init.config.language, &init.locale);
        Core {
            now: 0,
            idle_on: init.config.idle,
            config: init.config,
            locale: init.locale,
            lang,
            chosen: init.state.as_ref().is_some_and(|s| s.chosen),
            state_file: init.state,
            catalog,
            loader: init.loader,
            metrics: init.metrics,
            draws: init.draws,
            outputs: init.outputs,
            hour: init.local_hour,
            ch: None,
            next_id: 1,
            queue: VecDeque::new(),
            running: None,
            owner: Owner::None,
            ask: None,
            gallery: None,
            idle_at: None,
            idle_n: 0,
            idle_started: false,
            idle_begin: None,
            speech: None,
            press: None,
            dragging: false,
            last_press: None,
            menu: None,
            animate: Vec::new(),
            animate_pos: 0,
            move_cause: 0,
            visibility_cause: 0,
            persist_dirty: false,
            last_persist: None,
            out: Vec::new(),
        }
    }

    /// Startup: B-CHAR-4 selection, B-CFG-3 first-run position and the initial Show.
    pub fn start(&mut self, now: Ms) -> Vec<Output> {
        self.now = now;
        let ids: Vec<String> = self.catalog.iter().map(|c| c.id.clone()).collect();
        let st = self.state_file.clone();
        let cands = l10n::startup_candidates(
            st.as_ref()
                .filter(|s| s.chosen)
                .map(|s| s.character.as_str()),
            self.config.character.as_deref(),
            &ids,
        );
        let model = cands.iter().find_map(|id| self.loader.load(id).ok());
        if let Some(model) = model {
            let (w, h) = (model.display.0 as i32, model.display.1 as i32);
            let (x, y, visible) = match &st {
                Some(s) => (s.left, s.top, s.visible),
                None => match self.outputs.first() {
                    Some(o) => (o.rect.right() - 48 - w, o.rect.bottom() - 96 - h, true),
                    None => (0, 0, true),
                },
            };
            self.install(model, x, y);
            if visible {
                self.enqueue_internal(Kind::Show {
                    fast: false,
                    cause: 4,
                });
            }
        }
        self.pump();
        std::mem::take(&mut self.out)
    }

    fn install(&mut self, model: Arc<CharacterModel>, x: i32, y: i32) {
        let mut c = Char {
            model,
            visible: false,
            x,
            y,
            player: Player::new(),
            output: None,
        };
        c.output = geometry::output_for(frame_rect(&c), &self.outputs).map(|o| o.id);
        self.ch = Some(c);
        self.build_animate_list();
    }

    /// Processes one input at time `now` (after processing every deadline <= `now`).
    pub fn handle(&mut self, now: Ms, event: Event) -> Vec<Output> {
        let mut out = self.tick(now);
        match event {
            Event::Call {
                token,
                sender,
                call,
            } => self.call(token, sender, call),
            Event::Pointer(p) => self.pointer(p),
            Event::Key { key, modifiers } => self.key(key, modifiers),
            Event::Menu(m) => self.menu_input(m),
            Event::Tray(t) => self.tray(t),
            Event::Outputs(o) => self.set_outputs(o),
            Event::Config(c) => {
                self.lang = l10n::resolve_language(c.language, &self.locale);
                self.config = c;
            }
            Event::Catalog(c) => self.set_catalog(c, true),
            Event::InstallHint => self.install_hint(),
            Event::LocalHour(h) => self.hour = h,
            Event::SoundFailed => {
                if let Some(c) = &mut self.ch
                    && c.player.sound_failed()
                {
                    self.anim_done();
                }
            }
        }
        self.pump();
        out.append(&mut self.out);
        out
    }

    /// Processes every deadline that is due at `now` (late ones are processed late).
    pub fn tick(&mut self, now: Ms) -> Vec<Output> {
        self.now = self.now.max(now);
        for _ in 0..100_000 {
            let Some((t, timer)) = self.timers().into_iter().min_by_key(|x| x.0) else {
                break;
            };
            if t > self.now {
                break;
            }
            self.fire(timer);
            self.pump();
        }
        std::mem::take(&mut self.out)
    }

    /// Fake-clock stepping: visits each deadline up to `target` in turn, then sets the
    /// clock to `target`.
    pub fn advance_to(&mut self, target: Ms) -> Vec<Output> {
        let mut out = Vec::new();
        while let Some(d) = self.next_deadline() {
            if d > target {
                break;
            }
            out.extend(self.tick(d.max(self.now)));
        }
        out.extend(self.tick(target));
        out
    }

    /// The earliest pending timer, if any.
    pub fn next_deadline(&self) -> Option<Ms> {
        self.timers().into_iter().map(|t| t.0).min()
    }

    /// Replaces the random source (`Test1.SetDraws`, `--seed`).
    pub fn set_draws(&mut self, draws: Box<dyn Draws>) {
        self.draws = draws;
    }

    fn timers(&self) -> Vec<(Ms, Timer)> {
        let mut v = Vec::new();
        if let Some(c) = &self.ch
            && let Some(d) = c.player.deadline()
        {
            v.push((d, Timer::Player));
        }
        if let Some(t) = self.idle_at {
            v.push((t, Timer::Idle));
        }
        if let Some(Running {
            stage: Stage::Slide { deadline, .. },
            ..
        }) = &self.running
        {
            v.push((*deadline, Timer::Slide));
        }
        if let Some(s) = &self.speech {
            if let Some(t) = s.next_word {
                v.push((t, Timer::Word));
            }
            if let Some(t) = s.end_at {
                v.push((t, Timer::SpeechEnd));
            }
            if let Some(t) = s.hide_at {
                v.push((t, Timer::AutoHide));
            }
            if let Some((start, dur, px)) = s.scroll {
                // One step per pixel, evenly over `dur`.
                let pitch = s.line_h + 2;
                let done = pitch - px;
                v.push((
                    start + dur * (done as Ms + 1) / pitch.max(1) as Ms,
                    Timer::Scroll,
                ));
            }
        }
        if self.persist_dirty {
            let t = self
                .last_persist
                .map_or(self.now, |l| (l + 1000).max(self.now));
            v.push((t, Timer::Persist));
        }
        v
    }

    fn fire(&mut self, t: Timer) {
        match t {
            Timer::Player => {
                let done = self.with_env(|p, env, now| p.tick(now, env));
                if done {
                    self.anim_done();
                }
            }
            Timer::Idle => self.idle_fire(),
            Timer::Slide => self.slide_tick(),
            Timer::Word => self.word_tick(),
            Timer::SpeechEnd => self.speech_end(),
            Timer::AutoHide => self.hide_speech_balloon(),
            Timer::Scroll => {
                if let Some(s) = &mut self.speech
                    && let Some((st, d, px)) = s.scroll
                {
                    s.scroll = if px <= 1 { None } else { Some((st, d, px - 1)) };
                }
            }
            Timer::Persist => self.persist_now(),
        }
    }

    fn with_env<R>(&mut self, f: impl FnOnce(&mut Player, &mut Env, Ms) -> R) -> R {
        let c = self.ch.as_mut().expect("character");
        let model = c.model.clone();
        let mut env = Env {
            model: &model,
            draws: self.draws.as_mut(),
            visible: c.visible,
            sounds_on: self.config.sound_effects,
            hour: self.hour,
            out: &mut self.out,
        };
        f(&mut c.player, &mut env, self.now)
    }

    // ----- signals -----

    fn emit(&mut self, s: Signal) {
        self.out.push(Output::Signal {
            target: None,
            signal: s,
        });
    }

    fn emit_to(&mut self, to: &Option<String>, s: Signal) {
        if let Some(t) = to {
            self.out.push(Output::Signal {
                target: Some(t.clone()),
                signal: s,
            });
        }
    }

    fn complete(&mut self, id: u32, sender: &Option<String>, status: u32, reason: &str) {
        self.emit_to(
            sender,
            Signal::RequestComplete {
                id,
                status,
                reason: reason.into(),
            },
        );
    }

    fn mark_dirty(&mut self) {
        self.persist_dirty = true;
    }

    fn state_snapshot(&self) -> Option<StateFile> {
        self.ch.as_ref().map(|c| StateFile {
            character: c.model.id.clone(),
            left: c.x,
            top: c.y,
            visible: c.visible,
            chosen: self.chosen,
        })
    }

    fn persist_now(&mut self) {
        self.persist_dirty = false;
        self.last_persist = Some(self.now);
        if let Some(s) = self.state_snapshot() {
            self.out.push(Output::PersistState(s));
        }
    }

    // ----- method calls -----

    fn reply(&mut self, token: u64, r: Result<ReplyValue, MethodError>) {
        self.out.push(Output::Reply { token, result: r });
    }

    fn call(&mut self, token: u64, sender: String, call: Call) {
        let sender = Some(sender);
        match call {
            Call::ListCharacters => {
                self.rescan(true);
                let v = self
                    .catalog
                    .iter()
                    .map(|c| {
                        let (n, d) = l10n::display_names(&c.info, self.lang);
                        (c.id.clone(), n, d)
                    })
                    .collect();
                self.reply(token, Ok(ReplyValue::Characters(v)));
            }
            Call::Select(id) => {
                self.rescan(false);
                let id = id.to_ascii_lowercase();
                if !self.catalog.iter().any(|c| c.id == id) {
                    return self.reply(
                        token,
                        Err(err(
                            errors::NO_SUCH_CHARACTER,
                            format!("no character \"{id}\""),
                        )),
                    );
                }
                match self.loader.load(&id) {
                    Err(e) => self.reply(token, Err(err(errors::INVALID_CHARACTER, e))),
                    Ok(m) => {
                        self.reply(token, Ok(ReplyValue::Unit));
                        if self.gallery.is_some() {
                            self.close_gallery(false, false);
                        }
                        self.chosen = true;
                        self.switch_to(m, false, false);
                    }
                }
            }
            Call::Quit => {
                self.reply(token, Ok(ReplyValue::Unit));
                self.persist_dirty = false;
                if let Some(s) = self.state_snapshot() {
                    self.out.push(Output::PersistState(s));
                }
                self.out.push(Output::Quit);
            }
            Call::SetIdleOn(on) => {
                self.reply(token, Ok(ReplyValue::Unit));
                if !on {
                    self.idle_reset();
                }
                self.idle_on = on;
            }
            Call::Stop(id) => {
                self.reply(token, Ok(ReplyValue::Unit));
                if id == 0 {
                    self.stop_scope(None);
                } else {
                    self.stop_id(id);
                }
            }
            Call::StopAll(types) => {
                const KNOWN: [&str; 4] = ["play", "move", "speak", "ask"];
                if let Some(bad) = types.iter().find(|t| !KNOWN.contains(&t.as_str())) {
                    return self.reply(
                        token,
                        Err(err(errors::INVALID_ARGS, format!("unknown type \"{bad}\""))),
                    );
                }
                self.reply(token, Ok(ReplyValue::Unit));
                self.stop_scope(Some(&types));
            }
            c => {
                let Some(ch) = &self.ch else {
                    return self
                        .reply(token, Err(err(errors::NO_CHARACTER, "no character loaded")));
                };
                let kind = match c {
                    Call::Show { fast } => Kind::Show { fast, cause: 4 },
                    Call::Hide { fast } => Kind::Hide { fast, cause: 3 },
                    Call::Play(name) => match fallback::resolve(&ch.model, &name) {
                        Ok(r) => Kind::Play(r),
                        Err(e) => {
                            return self.reply(token, Err(err(errors::UNKNOWN_ANIMATION, e)));
                        }
                    },
                    Call::Speak(text) => {
                        let mut alts = Vec::new();
                        for a in speak::alternatives(&text) {
                            match speak::parse(a) {
                                Ok(t) => alts.push(t),
                                Err(e) => {
                                    return self.reply(token, Err(err(errors::INVALID_ARGS, e)));
                                }
                            }
                        }
                        Kind::Speak(alts)
                    }
                    Call::Ask(spec) => match office::validate(&spec, self.lang) {
                        Ok(a) => Kind::Ask(Box::new(a)),
                        Err(e) => return self.reply(token, Err(e)),
                    },
                    Call::MoveTo { x, y, speed } => Kind::MoveTo { x, y, speed },
                    Call::GestureAt { x, y } => Kind::GestureAt { x, y },
                    _ => unreachable!(),
                };
                let id = self.next_id;
                self.next_id += 1;
                self.reply(token, Ok(ReplyValue::RequestId(id)));
                self.enqueue(Req { id, sender, kind });
            }
        }
    }

    fn enqueue_internal(&mut self, kind: Kind) {
        self.enqueue(Req {
            id: 0,
            sender: None,
            kind,
        });
    }

    fn enqueue(&mut self, r: Req) {
        // R-IDLE-4: any newly queued request resets idle.
        self.idle_reset();
        self.queue.push_back(r);
    }

    // ----- idle -----

    fn idle_reset(&mut self) {
        self.idle_at = None;
        if self.owner == Owner::Idle {
            self.with_env(|p, env, now| p.stop(now, env));
            self.owner = Owner::None;
        }
        if self.idle_started {
            self.idle_started = false;
            self.emit(Signal::IdleComplete);
        }
        self.idle_n = 0;
    }

    fn maybe_arm_idle(&mut self) {
        let Some(c) = &self.ch else { return };
        if self.queue.is_empty()
            && self.running.is_none()
            && self.idle_on
            && c.visible
            && self.idle_at.is_none()
            && self.owner == Owner::None
        {
            self.idle_at = Some(match c.model.family {
                Family::Agent => self.now + IDLE_DELAY,
                Family::Office97 => match self.idle_begin {
                    // B-IDLE-O: replay at once, at least 100 ms after the last begin.
                    Some(b) if self.idle_n > 0 => (b + 100).max(self.now),
                    _ => self.now,
                },
            });
        }
    }

    fn idle_fire(&mut self) {
        self.idle_at = None;
        if self.idle_n == 0 && !self.idle_started {
            self.idle_started = true;
            self.emit(Signal::IdleStart);
        }
        let c = self.ch.as_ref().expect("character");
        let state = match c.model.family {
            Family::Agent => match self.idle_n {
                0..=9 => "IdlingLevel1".to_string(),
                10..=19 => "IdlingLevel2".to_string(),
                _ => "IdlingLevel3".to_string(),
            },
            Family::Office97 => c
                .model
                .office97
                .as_ref()
                .map(|o| o.idle.clone())
                .unwrap_or_default(),
        };
        let model = c.model.clone();
        match player::pick_member(&model, &state, self.draws.as_mut()) {
            None => {
                // R-IDLE-5.
                if self.idle_n == 0 {
                    self.idle_n = 1;
                }
                self.idle_at = Some(self.now + IDLE_DELAY);
            }
            Some(a) => {
                self.idle_n += 1;
                self.idle_begin = Some(self.now);
                self.owner = Owner::Idle;
                if self.with_env(|p, env, now| p.play(now, env, a, false)) {
                    self.anim_done();
                }
            }
        }
    }

    // ----- queue -----

    /// Runs whatever can run now.
    fn pump(&mut self) {
        for _ in 0..1000 {
            if self.running.is_some() || self.ch.is_none() {
                break;
            }
            let Some(head) = self.queue.front() else {
                break;
            };
            let blocking = self
                .ask
                .as_ref()
                .is_some_and(|a| a.model.mode == Mode::Modal)
                || self.gallery.is_some();
            if blocking && matches!(head.kind, Kind::Speak(_) | Kind::Ask(_)) {
                break;
            }
            let req = self.queue.pop_front().expect("head");
            // B-ASK-7: a Hide reaching execution closes the ask with status 3 in both
            // modes (before the autodown dismissal of B-ASK-6).
            if matches!(req.kind, Kind::Hide { .. }) {
                if self.ask.as_ref().is_some_and(|a| a.id != 0) {
                    self.close_ask(0, 3, errors::HIDDEN);
                }
                if self.gallery.is_some() {
                    self.close_gallery(true, true);
                }
            }
            if req.sender.is_some()
                && self
                    .ask
                    .as_ref()
                    .is_some_and(|a| a.model.mode == Mode::AutoDown && a.id != 0)
            {
                // B-ASK-6: a later client request reaching execution dismisses.
                self.close_ask(0, 0, "");
            }
            if req.id != 0 {
                let (id, s) = (req.id, req.sender.clone());
                self.emit_to(&s, Signal::RequestStart { id });
            }
            self.running = Some(Running {
                req,
                stage: Stage::Anim(0),
                failure: None,
                move_emitted: false,
                speak_text: None,
            });
            self.begin();
        }
        self.maybe_arm_idle();
    }

    fn visible(&self) -> bool {
        self.ch.as_ref().is_some_and(|c| c.visible)
    }

    fn fail(&mut self, reason: &'static str) {
        if let Some(r) = &mut self.running
            && r.failure.is_none()
        {
            r.failure = Some(reason);
        }
    }

    fn finish(&mut self) {
        let Some(r) = self.running.take() else { return };
        if self.owner == Owner::Request {
            self.owner = Owner::None;
        }
        if r.req.id != 0 {
            let (status, reason) = match r.failure {
                Some(f) => (1, f),
                None => (0, ""),
            };
            self.complete(r.req.id, &r.req.sender, status, reason);
        }
    }

    /// Play an animation index for the running request; returns true if it is
    /// already complete.
    fn play_for_request(&mut self, anim: usize, drop_prev: bool) -> bool {
        self.owner = Owner::Request;
        let done = self.with_env(|p, env, now| p.play(now, env, anim, drop_prev));
        if done {
            self.owner = Owner::None;
        }
        done
    }

    /// Play a state for the request. `None` = nothing to play (missing state).
    fn play_state_for_request(&mut self, state: &str, drop_prev: bool) -> Option<bool> {
        let model = self.ch.as_ref()?.model.clone();
        let a = player::pick_member(&model, state, self.draws.as_mut())?;
        Some(self.play_for_request(a, drop_prev))
    }

    fn set_stage(&mut self, s: Stage) {
        if let Some(r) = &mut self.running {
            r.stage = s;
        }
    }

    fn stage_n(&self) -> u8 {
        match self.running.as_ref().map(|r| &r.stage) {
            Some(Stage::Anim(n)) => *n,
            _ => 0,
        }
    }

    /// Start the running request's first phase.
    fn begin(&mut self) {
        let kind = self.running.as_ref().expect("running").req.kind.clone();
        match kind {
            Kind::Show { fast, cause } => {
                if self.visible() {
                    return self.finish();
                }
                {
                    let c = self.ch.as_mut().expect("character");
                    c.visible = true;
                    c.output = geometry::output_for(frame_rect(c), &self.outputs).map(|o| o.id);
                }
                self.visibility_cause = cause;
                self.emit(Signal::Shown { cause });
                self.mark_dirty();
                if fast {
                    self.fast_show_picture();
                    return self.finish();
                }
                self.set_stage(Stage::Anim(1));
                match self.play_state_for_request("Showing", true) {
                    None | Some(true) => self.finish(),
                    Some(false) => {}
                }
            }
            Kind::Hide { fast, cause } => {
                if !self.visible() {
                    return self.finish();
                }
                if fast {
                    self.hide_now(cause);
                    return self.finish();
                }
                self.set_stage(Stage::Anim(1));
                match self.play_state_for_request("Hiding", false) {
                    None | Some(true) => {
                        self.hide_now(cause);
                        self.finish();
                    }
                    Some(false) => {}
                }
            }
            Kind::Play(r) => {
                if !self.visible() {
                    self.fail(errors::HIDDEN);
                    return self.finish();
                }
                let model = self.ch.as_ref().expect("character").model.clone();
                let anim = match r {
                    Resolved::Animation(n) => model.animation_index(&n),
                    Resolved::State(s) => player::pick_member(&model, &s, self.draws.as_mut()),
                };
                let Some(a) = anim else { return self.finish() };
                if !model.animations[a].available {
                    self.fail(errors::ANIMATION_UNAVAILABLE);
                    return self.finish();
                }
                self.set_stage(Stage::Anim(1));
                if self.play_for_request(a, false) {
                    self.anim_phase_done();
                }
            }
            Kind::GestureAt { x, y } => {
                let st = geometry::gesture_state(frame_rect(self.ch.as_ref().unwrap()), x, y);
                self.set_stage(Stage::Anim(1));
                match self.play_state_for_request(st, false) {
                    None => self.finish(),
                    Some(true) => self.anim_phase_done(),
                    Some(false) => {}
                }
            }
            Kind::Speak(alts) => {
                let i = if alts.len() > 1 {
                    self.draws.draw(alts.len() as u32) as usize
                } else {
                    0
                };
                let t = alts[i].clone();
                if t.words.is_empty() && t.bookmarks.is_empty() {
                    return self.finish();
                }
                if let Some(r) = &mut self.running {
                    r.speak_text = Some(t);
                }
                // B-PICK-A Speakable.
                let c = self.ch.as_ref().expect("character");
                let keeps = c.player.current().is_some_and(|(a, f)| {
                    c.model.animations[a]
                        .frames
                        .get(f)
                        .is_some_and(|fr| !fr.mouths.is_empty())
                });
                self.set_stage(Stage::Anim(1));
                if keeps {
                    return self.speak_output();
                }
                match self.play_state_for_request("Speaking", false) {
                    None => self.speak_output(),
                    Some(true) => self.anim_phase_done(),
                    Some(false) => {}
                }
            }
            Kind::Ask(_) => {
                if !self.visible() {
                    self.fail(errors::HIDDEN);
                    return self.finish();
                }
                let anim = match &self.running.as_ref().unwrap().req.kind {
                    Kind::Ask(a) => a.animation.clone(),
                    _ => None,
                };
                self.set_stage(Stage::Anim(1));
                let model = self.ch.as_ref().unwrap().model.clone();
                let idx = anim.and_then(|n| match fallback::resolve(&model, &n) {
                    Ok(Resolved::Animation(a)) => model.animation_index(&a),
                    Ok(Resolved::State(s)) => player::pick_member(&model, &s, self.draws.as_mut()),
                    Err(_) => None,
                });
                match idx {
                    Some(a) if model.animations[a].available => {
                        if self.play_for_request(a, false) {
                            self.anim_phase_done();
                        }
                    }
                    _ => self.ask_display(),
                }
            }
            Kind::MoveTo { x, y, .. } => {
                let c = self.ch.as_ref().expect("character");
                let (rect, visible) = (frame_rect(c), c.visible);
                self.set_stage(Stage::Anim(1));
                if !visible {
                    // R-MOVE-4.
                    self.fail(errors::HIDDEN);
                    return self.move_slide();
                }
                match geometry::moving_state(rect, x, y) {
                    None => self.move_slide(),
                    Some(st) => match self.play_state_for_request(st, false) {
                        None => self.move_slide(),
                        Some(true) => self.anim_phase_done(),
                        Some(false) => {}
                    },
                }
            }
        }
    }

    /// The player completed the owner's animation.
    fn anim_done(&mut self) {
        match self.owner {
            Owner::Idle => {
                self.owner = Owner::None;
            }
            Owner::Request => {
                self.owner = Owner::None;
                self.anim_phase_done();
            }
            Owner::None => {}
        }
    }

    /// The running request's animation phase completed.
    fn anim_phase_done(&mut self) {
        let Some(r) = &self.running else { return };
        let kind = r.req.kind.clone();
        let stage = self.stage_n();
        // R-ANIM-9: an animation that completes while hidden fails.
        if !self.visible() && !matches!(kind, Kind::Hide { .. }) {
            self.fail(errors::HIDDEN);
        }
        match kind {
            Kind::Hide { cause, .. } => {
                self.hide_now(cause);
                self.finish();
            }
            Kind::Speak(_) => self.speak_output(),
            Kind::Ask(_) => {
                if !self.visible() {
                    return self.finish();
                }
                self.ask_display()
            }
            Kind::MoveTo { x, y, .. } => {
                if stage == 3 {
                    if !self.running.as_ref().unwrap().move_emitted {
                        self.move_cause = 2;
                        self.emit(Signal::Move { x, y, cause: 2 });
                    }
                    self.finish();
                } else {
                    self.move_slide();
                }
            }
            _ => self.finish(),
        }
    }

    fn fast_show_picture(&mut self) {
        // B-SHOW-3.
        let c = self.ch.as_mut().expect("character");
        let pic = player::rest_picture(&c.model);
        c.player.set_picture(pic);
    }

    fn hide_now(&mut self, cause: u32) {
        let c = self.ch.as_mut().expect("character");
        if !c.visible {
            return;
        }
        if self.gallery.is_some() {
            self.close_gallery(true, true);
        }
        if let Some(c) = &mut self.ch {
            c.visible = false;
        }
        // An unmapped surface gets no release: forget the press and the drag.
        self.press = None;
        self.dragging = false;
        self.hide_speech_balloon();
        if self.ask.as_ref().is_some_and(|a| a.id != 0) {
            self.close_ask(0, 3, errors::HIDDEN);
        }
        self.visibility_cause = cause;
        self.emit(Signal::Hidden { cause });
        self.idle_at = None;
        self.mark_dirty();
    }

    // ----- MoveTo -----

    fn move_slide(&mut self) {
        let Some(Running {
            req:
                Req {
                    kind: Kind::MoveTo { x, y, speed },
                    ..
                },
            ..
        }) = self.running.clone()
        else {
            return;
        };
        if self.dragging {
            return self.slide_cancelled_by_drag();
        }
        let c = self.ch.as_ref().expect("character");
        let start = (c.x, c.y);
        if speed == 0 || !c.visible {
            if start != (x, y) {
                self.set_position(x, y);
                self.on_moved_by_program();
            }
            self.move_cause = 2;
            self.emit(Signal::Move { x, y, cause: 2 });
            self.mark_dirty();
            if let Some(r) = &mut self.running {
                r.move_emitted = true;
            }
            self.slide_finished();
            return;
        }
        let slide = Slide::new(start, (x, y), speed);
        if slide.done() {
            return self.slide_finished();
        }
        let deadline = self.now + slide.period;
        self.set_stage(Stage::Slide { slide, deadline });
    }

    fn slide_tick(&mut self) {
        let Some(Running {
            stage: Stage::Slide { slide, .. },
            ..
        }) = &mut self.running
        else {
            return;
        };
        let p = slide.step();
        let period = slide.period;
        let done = slide.done();
        let before = self.ch.as_ref().map(|c| (c.x, c.y));
        if before != Some(p) {
            self.set_position(p.0, p.1);
            self.on_moved_by_program();
        }
        if done {
            self.slide_finished();
        } else if let Some(Running {
            stage: Stage::Slide { deadline, .. },
            ..
        }) = &mut self.running
        {
            *deadline += period;
        }
    }

    /// Hide balloons when the frame actually moves (R-BAL-10, B-ASK-7).
    fn on_moved_by_program(&mut self) {
        self.hide_speech_balloon();
        if let Some(a) = &mut self.ask
            && a.scene.is_some()
        {
            a.scene = None;
            a.hidden = true;
            self.emit(Signal::BalloonHidden);
        }
    }

    fn slide_finished(&mut self) {
        self.update_output();
        self.mark_dirty();
        self.reshow_ask();
        self.move_end_phase();
    }

    /// A drag cancels the slide (R-MOVE-2): the phase fails with Dragged, the end
    /// phase still runs (B-Q-3) but emits no Move (the drag reports its own), and the
    /// output and balloons are left to the drag end (B-WL-4).
    fn slide_cancelled_by_drag(&mut self) {
        self.fail(errors::DRAGGED);
        if let Some(r) = &mut self.running {
            r.move_emitted = true;
        }
        self.move_end_phase();
    }

    /// R-MOVE-3 end phase.
    fn move_end_phase(&mut self) {
        self.set_stage(Stage::Anim(3));
        if !self.visible() {
            self.fail(errors::HIDDEN);
            return self.finish();
        }
        match self.play_state_for_request("Speaking", false) {
            None | Some(true) => self.anim_phase_done(),
            Some(false) => {}
        }
    }

    fn set_position(&mut self, x: i32, y: i32) {
        if let Some(c) = &mut self.ch {
            c.x = x;
            c.y = y;
        }
    }

    fn update_output(&mut self) {
        if let Some(c) = &mut self.ch {
            c.output = geometry::output_for(frame_rect(c), &self.outputs).map(|o| o.id);
        }
    }

    fn output_rect(&self) -> Rect {
        let c = self.ch.as_ref();
        let o = c
            .and_then(|c| self.outputs.iter().find(|o| Some(o.id) == c.output))
            .or_else(|| c.and_then(|c| geometry::output_for(frame_rect(c), &self.outputs)))
            .or(self.outputs.first());
        o.map_or(Rect::new(0, 0, 1024, 768), |o| o.rect)
    }

    // ----- Speak -----

    fn speak_output(&mut self) {
        self.set_stage(Stage::Output);
        if !self.visible() {
            self.fail(errors::HIDDEN);
            return self.finish();
        }
        let r = self.running.as_ref().expect("running");
        let t = r.speak_text.clone().unwrap_or_default();
        let sender = r.req.sender.clone();
        let c = self.ch.as_ref().expect("character");
        let st = &c.model.balloon;
        let (lines_n, cpl) = balloon::effective_lines_cpl(st.lines, st.chars_per_line);
        let family = if self.config.balloon_font.is_empty() {
            st.font.clone()
        } else {
            self.config.balloon_font.clone()
        };
        let font = FontSpec {
            family,
            px: st.font_height.unsigned_abs(),
            cell: st.font_height > 0,
            bold: st.font_weight >= 600,
            italic: st.italic,
        };
        let m = self.output_rect();
        let lh = self.metrics.line_height(&font);
        let ave = self.metrics.ave_char_width(&font);
        let (w, _) = balloon::body_size(cpl, ave, lh, lines_n, m.w);
        let wrapped = speak::wrap(&t.words, w - 20, self.metrics.as_ref(), &font);
        let lines = if st.size_to_text {
            balloon::size_to_text_lines(wrapped.len(), lh, m.h)
        } else {
            lines_n
        };
        let (_, h) = balloon::body_size(cpl, ave, lh, lines, m.w);
        let interval = speak::word_interval(self.config.speaking_speed);
        let enabled = st.enabled && self.config.balloon;
        let (fg, bg, border, auto_hide, auto_pace) =
            (st.fg, st.bg, st.border, st.auto_hide, st.auto_pace);
        // R-BAL-10: the next Speak hides an old balloon.
        self.hide_speech_balloon();
        let words = t.words.len();
        let shown = enabled && !self.dragging;
        let mut sp = Speech {
            style_lines: lines,
            font,
            fg,
            bg,
            border,
            w,
            h,
            line_h: lh,
            lines: wrapped,
            revealed: 0,
            words,
            bookmarks: t.bookmarks,
            sender,
            interval,
            next_word: None,
            end_at: Some(self.now + words as Ms * interval),
            auto_hide,
            hide_at: None,
            shown: false,
            first_line: 0,
            scroll: None,
            active: true,
        };
        if shown {
            sp.shown = true;
            self.emit(Signal::BalloonShown);
        }
        self.speech = Some(sp);
        if !enabled {
            // B-BAL-1: paced without a balloon.
            self.speech.as_mut().unwrap().shown = false;
        }
        if auto_pace {
            self.reveal(1);
            if words > 1 {
                self.speech.as_mut().unwrap().next_word = Some(self.now + interval);
            }
        } else {
            self.reveal(words);
        }
        if words == 0 {
            self.speech_end();
        }
    }

    /// Reveal up to `n` words, firing bookmarks and scrolling.
    fn reveal(&mut self, n: usize) {
        let Some(s) = &mut self.speech else { return };
        let target = n.min(s.words);
        let mut marks = Vec::new();
        while s.revealed < target {
            let w = s.revealed;
            marks.extend(s.bookmarks.iter().filter(|b| b.0 == w).map(|b| b.1));
            s.revealed += 1;
            // R-BAL-9: scroll when the new word starts a line beyond the visible lines.
            let line = s
                .lines
                .iter()
                .position(|l| l.iter().any(|(wi, _)| *wi == w))
                .unwrap_or(0);
            let visible = s.style_lines as usize;
            if line >= s.first_line + visible {
                s.first_line = line + 1 - visible;
                let dur = speak::scroll_duration(self.config.speaking_speed);
                s.scroll = Some((self.now, dur, s.line_h + 2));
            }
        }
        let sender = s.sender.clone();
        for n in marks {
            self.emit_to(&sender, Signal::Bookmark { n });
        }
    }

    fn word_tick(&mut self) {
        let Some(s) = &mut self.speech else { return };
        let next = s.revealed + 1;
        let more = next < s.words;
        let iv = s.interval;
        s.next_word = if more {
            s.next_word.map(|t| t + iv)
        } else {
            None
        };
        self.reveal(next);
    }

    fn speech_end(&mut self) {
        let Some(s) = &mut self.speech else { return };
        s.end_at = None;
        s.next_word = None;
        s.active = false;
        let words = s.words;
        let w = words;
        let marks: Vec<i32> = s
            .bookmarks
            .iter()
            .filter(|b| b.0 == w)
            .map(|b| b.1)
            .collect();
        let sender = s.sender.clone();
        if s.revealed < words {
            s.revealed = words;
        }
        if s.auto_hide && s.shown {
            s.hide_at = Some(self.now + AUTO_HIDE);
        }
        for n in marks {
            self.emit_to(&sender, Signal::Bookmark { n });
        }
        if matches!(self.running.as_ref().map(|r| &r.stage), Some(Stage::Output)) {
            self.finish();
        }
    }

    fn hide_speech_balloon(&mut self) {
        let mut hidden = false;
        if let Some(sp) = &mut self.speech {
            sp.hide_at = None;
            if sp.shown {
                sp.shown = false;
                hidden = true;
            }
            if !sp.active {
                self.speech = None;
            }
        }
        if hidden {
            self.emit(Signal::BalloonHidden);
        }
    }

    fn speaking(&self) -> bool {
        self.speech.as_ref().is_some_and(|s| s.active)
    }

    // ----- Ask -----

    fn ask_display(&mut self) {
        let r = self.running.take().expect("running");
        if self.owner == Owner::Request {
            self.owner = Owner::None;
        }
        let Kind::Ask(model) = r.req.kind.clone() else {
            return;
        };
        let it = Interaction {
            checked: vec![false; model.checkboxes.len()],
            focus: model.initial_focus(),
            hot: None,
            pressed: None,
        };
        let a = OpenAsk {
            id: r.req.id,
            sender: r.req.sender.clone(),
            model: *model,
            it,
            scene: None,
            candidate: None,
            hidden: false,
        };
        match self.place_ask(&a) {
            Err(_) => {
                self.running = Some(r);
                self.fail(errors::BALLOON_TOO_BIG);
                self.finish();
            }
            Ok(p) => {
                self.ask = Some(a);
                self.apply_placement(p);
            }
        }
    }

    fn place_ask(&self, a: &OpenAsk) -> Result<office::Placed, office::TooBig> {
        let c = self.ch.as_ref().expect("character");
        let m = self.output_rect();
        let cx = office::Ctx::new(self.metrics.as_ref(), m.w);
        office::place(&a.model, &a.it, frame_rect(c), m, &cx)
    }

    fn apply_placement(&mut self, p: office::Placed) {
        if p.move_by != (0, 0) {
            let (x, y) = {
                let c = self.ch.as_ref().unwrap();
                (c.x + p.move_by.0, c.y + p.move_by.1)
            };
            self.set_position(x, y);
            self.move_cause = 2;
            self.emit(Signal::Move { x, y, cause: 2 });
            self.mark_dirty();
        }
        let dragging = self.dragging;
        let a = self.ask.as_mut().expect("ask");
        if dragging {
            // B-ASK-7: placed at drag end.
            a.hidden = true;
            return;
        }
        let was = a.scene.is_some();
        a.scene = Some(p.scene);
        a.candidate = Some(p.candidate);
        a.hidden = false;
        if !was {
            self.emit(Signal::BalloonShown);
        }
    }

    /// Lay out the open balloon again after an interaction change. The character is
    /// not moved again (B-OB-4 moves it once per placement).
    fn refresh_ask(&mut self) {
        let Some(a) = &self.ask else { return };
        if a.hidden || a.scene.is_none() {
            return;
        }
        if let Ok(mut p) = self.place_ask(a) {
            office::shift(&mut p.scene, -p.move_by.0, -p.move_by.1);
            p.move_by = (0, 0);
            self.apply_placement(p);
        }
    }

    /// Place the open balloon again (output change, gallery switch), B-OB-4 included.
    fn replace_ask(&mut self) {
        let Some(a) = &self.ask else { return };
        if a.hidden || a.scene.is_none() {
            return;
        }
        if let Ok(p) = self.place_ask(a) {
            self.apply_placement(p);
        }
    }

    fn reshow_ask(&mut self) {
        let Some(a) = &self.ask else { return };
        if !a.hidden {
            return;
        }
        if let Ok(p) = self.place_ask(a) {
            self.apply_placement(p);
        }
    }

    /// Close a client ask: AskComplete then RequestComplete.
    fn close_ask(&mut self, result: i32, status: u32, reason: &str) {
        let Some(a) = self.ask.take() else { return };
        if a.scene.is_some() {
            self.emit(Signal::BalloonHidden);
        }
        if a.id == 0 {
            return;
        }
        self.emit_to(
            &a.sender,
            Signal::AskComplete {
                id: a.id,
                result,
                checked: a.it.checked.clone(),
            },
        );
        self.complete(a.id, &a.sender, status, reason);
    }

    /// A control was activated.
    fn activate(&mut self, c: Control) {
        let Some(a) = &mut self.ask else { return };
        match c {
            Control::Checkbox(i) => {
                if let Some(v) = a.it.checked.get_mut(i) {
                    *v = !*v;
                }
                self.refresh_ask();
            }
            Control::Label(i) => {
                if a.model.label_type == LabelType::Buttons {
                    if a.id == 0 {
                        return;
                    }
                    self.close_ask(i as i32 + 1, 0, "");
                }
            }
            Control::Button(i) => {
                let Some(b) = a.model.buttons.get(i) else {
                    return;
                };
                if !b.enabled {
                    return;
                }
                let r = b.result;
                if a.id == 0 {
                    self.gallery_button(r);
                } else {
                    self.close_ask(r, 0, "");
                }
            }
        }
    }

    // ----- gallery -----

    fn open_gallery(&mut self) {
        let Some(c) = &self.ch else { return };
        let id = c.model.id.clone();
        if self.ask.as_ref().is_some_and(|a| a.id != 0) {
            self.close_ask(0, 3, errors::INTERRUPTED);
        }
        // An Ask still in its animation phase would replace the gallery balloon.
        if self
            .running
            .as_ref()
            .is_some_and(|r| matches!(r.req.kind, Kind::Ask(_)))
        {
            self.interrupt_running(errors::INTERRUPTED);
        }
        self.rescan(false);
        let index = self.catalog.iter().position(|e| e.id == id).unwrap_or(0);
        self.gallery = Some(Gallery {
            original: id,
            index,
        });
        let model = self.gallery_model();
        let focus = model.initial_focus();
        self.ask = Some(OpenAsk {
            id: 0,
            sender: None,
            model,
            it: Interaction {
                focus,
                ..Default::default()
            },
            scene: None,
            candidate: None,
            hidden: false,
        });
        let a = self.ask.as_ref().unwrap();
        if let Ok(p) = self.place_ask(a) {
            self.apply_placement(p);
        }
    }

    fn gallery_model(&self) -> AskModel {
        let g = self.gallery.as_ref().expect("gallery");
        let (name, desc) = self
            .catalog
            .get(g.index)
            .map(|e| l10n::display_names(&e.info, self.lang))
            .unwrap_or_default();
        let caps = l10n::gallery_captions(self.lang);
        let n = self.catalog.len();
        let enabled = [g.index > 0, g.index + 1 < n, true, true];
        AskModel {
            heading: name,
            text: office::markup(&desc),
            icon: None,
            labels: Vec::new(),
            label_type: LabelType::Buttons,
            checkboxes: Vec::new(),
            buttons: (0..4)
                .map(|i| ButtonDef {
                    caption: caps[i].into(),
                    result: -25 - i as i32,
                    enabled: enabled[i],
                })
                .collect(),
            mode: Mode::Modal,
            esc_result: Some(-28),
            animation: None,
        }
    }

    fn gallery_button(&mut self, r: i32) {
        let Some(g) = &mut self.gallery else { return };
        match r {
            -25 | -26 => {
                // Step to the next candidate that loads (B-GAL-1), skipping broken ones.
                let mut ni = g.index;
                let found = loop {
                    let next = if r == -25 {
                        ni.checked_sub(1)
                    } else {
                        Some(ni + 1).filter(|&i| i < self.catalog.len())
                    };
                    let Some(n) = next else { break None };
                    ni = n;
                    if let Ok(m) = self.loader.load(&self.catalog[n].id) {
                        break Some((n, m));
                    }
                };
                let Some((ni, m)) = found else { return };
                self.gallery.as_mut().unwrap().index = ni;
                self.switch_to(m, true, false);
                if let Some(c) = &self.ch
                    && let Ok(res) = fallback::resolve(&c.model, "Wave")
                {
                    self.enqueue_internal(Kind::Play(res));
                }
                let model = self.gallery_model();
                let focus_btn = if r == -25 { 0 } else { 1 };
                if let Some(a) = &mut self.ask {
                    a.model = model;
                    let order = a.model.focus_order();
                    a.it.focus = order
                        .iter()
                        .position(|c| *c == Control::Button(focus_btn))
                        .or(a.model.initial_focus());
                    a.it.pressed = None;
                    a.it.hot = None;
                }
                if !self.keep_gallery_balloon() {
                    self.replace_ask();
                }
            }
            -27 => {
                self.close_gallery(false, false);
                self.chosen = true;
                self.mark_dirty();
            }
            _ => self.close_gallery(true, false),
        }
    }

    /// B-GAL-1: after a gallery switch the balloon stays where it is while it still fits
    /// next to the new character (office::place_kept).
    fn keep_gallery_balloon(&mut self) -> bool {
        let Some(a) = &self.ask else { return false };
        let (Some(sc), Some(k), false) = (&a.scene, a.candidate, a.hidden) else {
            return false;
        };
        let c = self.ch.as_ref().expect("character");
        let fr = frame_rect(c);
        let visible = c.model.rest_box.map_or(fr, |(x0, y0, x1, y1)| {
            Rect::new(c.x + x0, c.y + y0, x1 - x0, y1 - y0)
        });
        let m = self.output_rect();
        let cx = office::Ctx::new(self.metrics.as_ref(), m.w);
        match office::place_kept(&a.model, &a.it, k, sc.body, fr, visible, m, &cx) {
            Some(p) => {
                self.apply_placement(p);
                true
            }
            None => false,
        }
    }

    /// Close the gallery; `cancel` switches back to the original character. With
    /// `for_hide` (B-GAL-1: hiding closes the gallery as Cancel) the switch keeps the
    /// visibility instead of queuing a Show, so the Hide runs on the original.
    fn close_gallery(&mut self, cancel: bool, for_hide: bool) {
        let Some(g) = self.gallery.take() else { return };
        if self.ask.as_ref().is_some_and(|a| a.id == 0) {
            self.close_ask(0, 0, "");
        }
        if cancel
            && self.ch.as_ref().is_some_and(|c| c.model.id != g.original)
            && let Ok(m) = self.loader.load(&g.original)
        {
            self.switch_to(m, false, for_hide);
        }
    }

    // ----- catalog -----

    /// B-CHAR-1 on use (ListCharacters, Select, gallery). `default`: apply the B-CHAR-4
    /// default afterwards.
    fn rescan(&mut self, default: bool) {
        if let Some(c) = self.loader.rescan() {
            self.set_catalog(c, default);
        }
    }

    /// A new catalog. An open gallery keeps its candidate by id. With `default` and no
    /// chosen character, the B-CHAR-4 default replaces the current character (not while
    /// the gallery is open).
    fn set_catalog(&mut self, mut c: Vec<CatalogEntry>, default: bool) {
        c.sort_by(|a, b| a.id.cmp(&b.id));
        self.catalog = c;
        if self.gallery.is_some() {
            let cur = self.ch.as_ref().map(|c| c.model.id.clone());
            let i = self
                .catalog
                .iter()
                .position(|e| Some(&e.id) == cur.as_ref());
            if let Some(g) = &mut self.gallery {
                g.index = i.unwrap_or(0);
            }
            let model = self.gallery_model();
            if let Some(a) = self.ask.as_mut().filter(|a| a.id == 0) {
                a.model = model;
            }
            self.refresh_ask();
        } else if default && !self.chosen {
            self.apply_default();
        }
    }

    /// B-CHAR-4 steps 2 to 4 for a running daemon whose user never chose a character.
    fn apply_default(&mut self) {
        let Some(cur) = self.ch.as_ref().map(|c| c.model.id.clone()) else {
            return;
        };
        let ids: Vec<String> = self.catalog.iter().map(|c| c.id.clone()).collect();
        for id in l10n::startup_candidates(None, self.config.character.as_deref(), &ids) {
            if id == cur {
                return;
            }
            if let Ok(m) = self.loader.load(&id) {
                return self.switch_to(m, false, false);
            }
        }
    }

    /// B-CHAR-6: an internal Speak of the install hint.
    fn install_hint(&mut self) {
        if self.ch.is_none() {
            return;
        }
        if let Ok(t) = speak::parse(l10n::install_hint(self.lang)) {
            self.enqueue_internal(Kind::Speak(vec![t]));
        }
    }

    // ----- character switch -----

    /// B-CHAR-5.
    fn switch_to(&mut self, model: Arc<CharacterModel>, gallery: bool, keep_visible: bool) {
        self.interrupt_all(errors::CHARACTER_CHANGED);
        if !gallery && self.ask.as_ref().is_some_and(|a| a.id != 0) {
            self.close_ask(0, 3, errors::CHARACTER_CHANGED);
        }
        self.idle_reset();
        self.hide_speech_balloon();
        self.speech = None;
        let m = self.output_rect();
        let (x, y, was_visible) = match &mut self.ch {
            Some(c) => {
                let mut env_out = Vec::new();
                if c.player.busy() {
                    env_out.push(Output::SoundStop);
                }
                self.out.extend(env_out);
                // The new character stands where the old one stood, inside the output.
                let (fx, fy) = geometry::foot(&c.model);
                let at = (c.x.saturating_add(fx), c.y.saturating_add(fy));
                let (x, y) = geometry::stand_at(at, geometry::foot(&model), model.display, m);
                (x, y, c.visible)
            }
            None => (0, 0, false),
        };
        self.install(model, x, y);
        self.mark_dirty();
        if keep_visible && was_visible {
            if let Some(c) = &mut self.ch {
                c.visible = true;
            }
            self.fast_show_picture();
        } else if was_visible {
            self.enqueue_internal(Kind::Show {
                fast: false,
                cause: 4,
            });
        }
    }

    // ----- stopping -----

    /// Interrupt the running request (status 3).
    fn interrupt_running(&mut self, reason: &str) {
        let Some(r) = self.running.take() else { return };
        if self.owner == Owner::Request {
            self.with_env(|p, env, now| p.stop(now, env));
            self.owner = Owner::None;
        }
        if matches!(r.stage, Stage::Output) {
            if let Some(s) = &mut self.speech {
                s.active = false;
                s.next_word = None;
                s.end_at = None;
            }
            self.hide_speech_balloon();
        }
        if matches!(r.stage, Stage::Slide { .. }) {
            self.update_output();
            self.mark_dirty();
            self.reshow_ask();
        }
        if r.req.id != 0 {
            if let Kind::Ask(a) = &r.req.kind {
                let checked = vec![false; a.checkboxes.len()];
                let (id, s) = (r.req.id, r.req.sender.clone());
                self.emit_to(
                    &s,
                    Signal::AskComplete {
                        id,
                        result: 0,
                        checked,
                    },
                );
            }
            self.complete(r.req.id, &r.req.sender, 3, reason);
        }
    }

    /// Every queued and running request ends with status 3.
    fn interrupt_all(&mut self, reason: &str) {
        let q: Vec<Req> = self.queue.drain(..).collect();
        self.interrupt_running(reason);
        for r in q {
            if r.id != 0 {
                self.complete(r.id, &r.sender, 3, reason);
            }
        }
    }

    /// Stop(0) (`types = None`, a running Show/Hide continues), StopAll([]) (`Some([])`,
    /// everything) and StopAll(types) (B-Q-4, B-Q-5).
    fn stop_scope(&mut self, types: Option<&[String]>) {
        let matches = |k: &Kind| match types {
            None | Some([]) => true,
            Some(t) => t.iter().any(|t| t == k.category()),
        };
        let (removed, keep): (Vec<Req>, Vec<Req>) =
            self.queue.drain(..).partition(|r| matches(&r.kind));
        self.queue = keep.into();
        if let Some(r) = &self.running {
            let showhide = matches!(r.req.kind, Kind::Show { .. } | Kind::Hide { .. });
            let stop = if showhide {
                matches!(types, Some([]))
            } else {
                matches(&r.req.kind)
            };
            if stop {
                self.interrupt_running(errors::INTERRUPTED);
            }
        }
        for r in removed {
            if r.id != 0 {
                self.complete(r.id, &r.sender, 3, errors::INTERRUPTED);
            }
        }
        let ask = match types {
            None | Some([]) => true,
            Some(t) => t.iter().any(|t| t == "ask"),
        };
        if ask && self.ask.as_ref().is_some_and(|a| a.id != 0) {
            self.close_ask(0, 3, errors::INTERRUPTED);
        }
    }

    fn stop_id(&mut self, id: u32) {
        if let Some(pos) = self.queue.iter().position(|r| r.id == id) {
            let r = self.queue.remove(pos).unwrap();
            self.complete(r.id, &r.sender, 3, errors::INTERRUPTED);
        } else if self.running.as_ref().is_some_and(|r| r.req.id == id) {
            self.interrupt_running(errors::INTERRUPTED);
        } else if self.ask.as_ref().is_some_and(|a| a.id == id) {
            self.close_ask(0, 3, errors::INTERRUPTED);
        }
    }

    /// B-UI-4 user hide.
    fn user_hide(&mut self, cause: u32) {
        if !self.visible() {
            return;
        }
        self.interrupt_all(errors::HIDDEN_BY_USER);
        if self.ask.as_ref().is_some_and(|a| a.id != 0) {
            self.close_ask(0, 3, errors::HIDDEN_BY_USER);
        }
        if self.gallery.is_some() {
            self.close_gallery(true, true);
        }
        self.enqueue_internal(Kind::Hide { fast: false, cause });
    }

    // ----- pointer -----

    fn pointer(&mut self, p: PointerEvent) {
        if self.ch.is_none() {
            return;
        }
        match p.surface {
            Surface::Character => self.char_pointer(p),
            Surface::OfficeBalloon => self.balloon_pointer(p),
            // B-WL-8: clicks on the Agent balloon do nothing.
            Surface::AgentBalloon => {}
        }
    }

    fn char_pointer(&mut self, p: PointerEvent) {
        match p.kind {
            PointerKind::Press => {
                if self.press.is_some() {
                    return;
                }
                // B-ASK-6: a click on the character dismisses an autodown ask.
                if self
                    .ask
                    .as_ref()
                    .is_some_and(|a| a.id != 0 && a.model.mode == Mode::AutoDown)
                {
                    self.close_ask(0, 0, "");
                }
                let dbl = self
                    .last_press
                    .is_some_and(|t| self.now - t <= Ms::from(self.config.double_click_ms));
                let c = self.ch.as_ref().unwrap();
                self.press = Some(Press {
                    button: p.button,
                    modifiers: p.modifiers,
                    x: p.x,
                    y: p.y,
                    left: c.x,
                    top: c.y,
                    dbl,
                });
                if dbl {
                    self.last_press = None;
                    self.emit(Signal::DblClick {
                        button: p.button,
                        modifiers: p.modifiers,
                        x: p.x,
                        y: p.y,
                        from_tray: false,
                    });
                } else {
                    self.last_press = Some(self.now);
                }
            }
            PointerKind::Motion => {
                let Some(pr) = &self.press else { return };
                let (dx, dy) = (p.x - pr.x, p.y - pr.y);
                if !self.dragging {
                    if !(pr.button == 1 || pr.button == 2) || (dx.abs() < 2 && dy.abs() < 2) {
                        return;
                    }
                    self.dragging = true;
                    let (b, m) = (pr.button, pr.modifiers);
                    self.drag_start();
                    self.emit(Signal::DragStart {
                        button: b,
                        modifiers: m,
                        x: p.x,
                        y: p.y,
                    });
                }
                let pr = self.press.as_ref().unwrap();
                let (l, t) = (pr.left.saturating_add(dx), pr.top.saturating_add(dy));
                self.set_position(l, t);
            }
            PointerKind::Release => {
                let Some(pr) = self.press.take() else { return };
                if self.dragging {
                    self.dragging = false;
                    self.emit(Signal::DragComplete {
                        button: pr.button,
                        modifiers: pr.modifiers,
                        x: p.x,
                        y: p.y,
                    });
                    let (x, y) = {
                        let c = self.ch.as_ref().unwrap();
                        (c.x, c.y)
                    };
                    self.move_cause = 1;
                    self.emit(Signal::Move { x, y, cause: 1 });
                    self.update_output();
                    self.mark_dirty();
                    self.reshow_ask();
                    return;
                }
                if pr.dbl {
                    return;
                }
                self.emit(Signal::Click {
                    button: pr.button,
                    modifiers: pr.modifiers,
                    x: p.x,
                    y: p.y,
                    from_tray: false,
                });
                // R-BAL-10: a click on a character that is not speaking.
                if !self.speaking() {
                    self.hide_speech_balloon();
                }
                if pr.button == 2 {
                    self.open_menu(p.x, p.y, false);
                }
            }
            PointerKind::Leave => {}
        }
    }

    fn drag_start(&mut self) {
        self.hide_speech_balloon();
        // R-MOVE-2: a drag cancels a running slide.
        if matches!(
            self.running.as_ref().map(|r| &r.stage),
            Some(Stage::Slide { .. })
        ) {
            self.slide_cancelled_by_drag();
        }
        if let Some(a) = &mut self.ask
            && a.scene.is_some()
        {
            a.scene = None;
            a.hidden = true;
            self.emit(Signal::BalloonHidden);
        }
    }

    fn balloon_pointer(&mut self, p: PointerEvent) {
        let Some(a) = &mut self.ask else { return };
        let Some(scene) = &a.scene else { return };
        let hit = office::hit(scene, p.x, p.y);
        let enabled = |a: &OpenAsk, c: Control| match c {
            Control::Button(i) => a.model.buttons[i].enabled,
            _ => true,
        };
        match p.kind {
            PointerKind::Motion | PointerKind::Leave => {
                let h = if p.kind == PointerKind::Leave {
                    None
                } else {
                    hit
                };
                let h = h.filter(|&c| enabled(a, c));
                if a.it.hot != h {
                    a.it.hot = h;
                    self.refresh_ask();
                }
            }
            PointerKind::Press => {
                if p.button != 1 {
                    return;
                }
                match hit.filter(|&c| enabled(a, c)) {
                    Some(c) => {
                        a.it.pressed = Some(c);
                        self.refresh_ask();
                    }
                    None => {
                        if a.model.mode == Mode::AutoDown && a.id != 0 {
                            // B-ASK-6: click on the balloon outside its controls.
                            self.close_ask(0, 0, "");
                        }
                    }
                }
            }
            PointerKind::Release => {
                let pressed = a.it.pressed.take();
                if let Some(c) = pressed {
                    if hit == Some(c) {
                        let order = a.model.focus_order();
                        if let Some(i) = order.iter().position(|x| *x == c) {
                            a.it.focus = Some(i);
                        }
                        self.refresh_ask();
                        self.activate(c);
                    } else {
                        self.refresh_ask();
                    }
                }
            }
        }
    }

    // ----- keyboard -----

    fn key(&mut self, key: Key, modifiers: u32) {
        if self.menu.is_some() {
            return self.menu_key(key);
        }
        let Some(a) = &mut self.ask else { return };
        if a.scene.is_none() {
            return;
        }
        let order = a.model.focus_order();
        match key {
            Key::Escape => {
                if let Some(r) = a.model.esc_result {
                    if a.id == 0 {
                        self.gallery_button(r);
                    } else {
                        self.close_ask(r, 0, "");
                    }
                } else if a.model.mode == Mode::AutoDown {
                    self.close_ask(0, 0, "");
                }
            }
            Key::Enter => {
                if let Some(c) = a.it.focus.and_then(|i| order.get(i).copied()) {
                    self.activate(c);
                }
            }
            Key::Tab => {
                if !order.is_empty() {
                    let n = order.len();
                    let cur = a.it.focus.unwrap_or(n - 1);
                    a.it.focus = Some(if modifiers & 1 != 0 {
                        (cur + n - 1) % n
                    } else {
                        (cur + 1) % n
                    });
                    self.refresh_ask();
                }
            }
            Key::Space => {
                if let Some(Control::Checkbox(i)) = a.it.focus.and_then(|i| order.get(i).copied()) {
                    self.activate(Control::Checkbox(i));
                }
            }
            Key::Char(ch) => {
                let ch = ch.to_lowercase().next().unwrap_or(ch);
                let hit = a
                    .model
                    .buttons
                    .iter()
                    .position(|b| b.enabled && office::mnemonic(&b.caption).2 == Some(ch))
                    .map(Control::Button)
                    .or_else(|| {
                        a.model
                            .checkboxes
                            .iter()
                            .position(|c| office::mnemonic(c).2 == Some(ch))
                            .map(Control::Checkbox)
                    });
                if let Some(c) = hit {
                    if let Some(i) = order.iter().position(|x| *x == c) {
                        a.it.focus = Some(i);
                    }
                    self.activate(c);
                }
            }
            _ => {}
        }
    }

    // ----- menu and tray -----

    fn menu_entries(&self, tray: bool) -> Vec<MenuEntry> {
        let item = |s, a| MenuEntry::Item {
            label: l10n::menu_string(self.lang, s).into(),
            action: a,
        };
        if self.ch.is_none() {
            return Vec::new();
        }
        if tray && !self.visible() {
            return vec![item(MenuString::Show, MenuAction::Show)];
        }
        let mut v = vec![
            item(MenuString::Hide, MenuAction::Hide),
            MenuEntry::Separator,
            item(MenuString::Options, MenuAction::Options),
            item(MenuString::ChooseAssistant, MenuAction::ChooseAssistant),
        ];
        if !self.animate.is_empty() {
            v.push(item(MenuString::Animate, MenuAction::Animate));
        }
        v
    }

    fn open_menu(&mut self, x: i32, y: i32, from_tray: bool) {
        let entries = self.menu_entries(from_tray);
        self.menu = Some(Menu {
            scene: MenuScene {
                x,
                y,
                entries,
                highlighted: None,
            },
            from_tray,
        });
    }

    fn menu_input(&mut self, m: MenuInput) {
        let Some(menu) = &mut self.menu else { return };
        match m {
            MenuInput::Hover(h) => menu.scene.highlighted = h,
            MenuInput::Dismiss => self.menu = None,
            MenuInput::Activate(i) => {
                let menu = self.menu.take().unwrap();
                if let Some(MenuEntry::Item { action, .. }) = menu.scene.entries.get(i) {
                    self.menu_action(*action, menu.from_tray);
                }
            }
        }
    }

    fn menu_key(&mut self, key: Key) {
        let Some(menu) = &mut self.menu else { return };
        let items: Vec<usize> = menu
            .scene
            .entries
            .iter()
            .enumerate()
            .filter(|(_, e)| matches!(e, MenuEntry::Item { .. }))
            .map(|(i, _)| i)
            .collect();
        if items.is_empty() {
            self.menu = None;
            return;
        }
        let pos = menu
            .scene
            .highlighted
            .and_then(|h| items.iter().position(|&i| i == h));
        match key {
            Key::Escape => self.menu = None,
            Key::Down => {
                let n = pos.map_or(0, |p| (p + 1) % items.len());
                menu.scene.highlighted = Some(items[n]);
            }
            Key::Up => {
                let n = pos.map_or(items.len() - 1, |p| (p + items.len() - 1) % items.len());
                menu.scene.highlighted = Some(items[n]);
            }
            Key::Enter => {
                if let Some(h) = menu.scene.highlighted {
                    self.menu_input(MenuInput::Activate(h));
                }
            }
            Key::Char(c) => {
                let c = c.to_lowercase().next().unwrap_or(c);
                let hit = items
                    .iter()
                    .copied()
                    .find(|&i| match &menu.scene.entries[i] {
                        MenuEntry::Item { label, .. } => office::mnemonic(label).2 == Some(c),
                        _ => false,
                    });
                if let Some(i) = hit {
                    self.menu_input(MenuInput::Activate(i));
                }
            }
            _ => {}
        }
    }

    fn menu_action(&mut self, a: MenuAction, from_tray: bool) {
        match a {
            MenuAction::Hide => self.user_hide(if from_tray { 1 } else { 7 }),
            MenuAction::Show => self.enqueue_internal(Kind::Show {
                fast: false,
                cause: 2,
            }),
            MenuAction::Options => self.out.push(Output::OpenConfig),
            MenuAction::ChooseAssistant => {
                if self.gallery.is_none() && self.visible() {
                    self.open_gallery();
                }
            }
            MenuAction::Animate => self.animate_next(),
        }
    }

    fn tray(&mut self, t: TrayInput) {
        match t {
            TrayInput::Activate { x, y } => {
                if self.ch.is_some() {
                    self.enqueue_internal(Kind::Show {
                        fast: false,
                        cause: 2,
                    });
                }
                self.emit(Signal::Click {
                    button: 1,
                    modifiers: 0,
                    x,
                    y,
                    from_tray: true,
                });
            }
            TrayInput::SecondaryActivate { x, y } => self.emit(Signal::Click {
                button: 4,
                modifiers: 0,
                x,
                y,
                from_tray: true,
            }),
            TrayInput::Menu(a) => {
                if self.ch.is_some() {
                    self.menu_action(a, true)
                }
            }
        }
    }

    // ----- Animate! -----

    fn build_animate_list(&mut self) {
        self.animate.clear();
        self.animate_pos = 0;
        let Some(c) = &self.ch else { return };
        self.animate = ANIMATE_NAMES
            .iter()
            .filter(|n| fallback::resolve_direct(&c.model, n).is_some())
            .map(|n| n.to_string())
            .collect();
        // Shuffled on first use (B-MENU-4), so loading takes no draws.
        self.animate_pos = self.animate.len();
    }

    fn shuffle(&mut self) {
        let n = self.animate.len();
        for i in (1..n).rev() {
            let j = self.draws.draw(i as u32 + 1) as usize;
            self.animate.swap(i, j);
        }
        self.animate_pos = 0;
    }

    fn animate_next(&mut self) {
        if self.animate.is_empty() {
            return;
        }
        if self.animate_pos >= self.animate.len() {
            self.shuffle();
        }
        let name = self.animate[self.animate_pos].clone();
        self.animate_pos += 1;
        if let Some(c) = &self.ch
            && let Some(r) = fallback::resolve_direct(&c.model, &name)
        {
            self.enqueue_internal(Kind::Play(r));
        }
    }

    // ----- outputs -----

    fn set_outputs(&mut self, o: Vec<OutputInfo>) {
        self.outputs = o;
        let Some(c) = &self.ch else { return };
        let fr = frame_rect(c);
        if let Some((x, y)) = geometry::pull_on_output_change(fr, &self.outputs)
            && (x, y) != (c.x, c.y)
        {
            self.set_position(x, y);
            self.move_cause = 4;
            self.emit(Signal::Move { x, y, cause: 4 });
            self.mark_dirty();
        }
        self.update_output();
        self.replace_ask();
    }

    // ----- scene -----

    /// Current scene.
    pub fn scene(&self) -> Scene {
        let character = self.ch.as_ref().map(|c| CharacterScene {
            id: c.model.id.clone(),
            mapped: c.visible,
            x: c.x,
            y: c.y,
            width: c.model.display.0,
            height: c.model.display.1,
            picture: match c.player.picture() {
                Pic::Empty => Picture::Empty,
                Pic::Pose(p) => Picture::Pose(p),
                Pic::Frame(a, f) => Picture::Frame {
                    animation: c.model.animations[a].name.clone(),
                    frame: f,
                    mouth: None,
                },
            },
            output: c.output,
            dragging: self.dragging,
            keyboard_on_demand: self.ask.as_ref().is_some_and(|a| a.scene.is_some()),
        });
        let (tname, ttip) = match &self.ch {
            Some(c) => {
                let info = self
                    .catalog
                    .iter()
                    .find(|e| e.id == c.model.id)
                    .map_or(c.model.info.as_slice(), |e| e.info.as_slice());
                let n = l10n::display_names(info, self.lang).0;
                let t = l10n::tray_title(&n);
                (t.clone(), t)
            }
            None => ("bentwire".to_string(), "bentwire".to_string()),
        };
        Scene {
            character,
            agent_balloon: self.agent_balloon_scene(),
            office_balloon: self.ask.as_ref().and_then(|a| a.scene.clone()),
            menu: self.menu.as_ref().map(|m| m.scene.clone()),
            tray: TrayScene {
                icon_character: self.ch.as_ref().map(|c| c.model.id.clone()),
                title: tname,
                tooltip: ttip,
                menu: self.menu_entries(true),
            },
            outputs: self.outputs.clone(),
        }
    }

    fn agent_balloon_scene(&self) -> Option<AgentBalloonScene> {
        let s = self.speech.as_ref()?;
        if !s.shown {
            return None;
        }
        let c = self.ch.as_ref()?;
        let fr = frame_rect(c);
        let m = self.output_rect();
        // B-BAL-3 anchor: mouth point of the lowest mouth type, else the frame centre.
        let anchor = match c.player.picture() {
            Pic::Frame(a, f) => c.model.animations[a].frames[f]
                .mouths
                .iter()
                .min_by_key(|m| m.kind)
                .map(|mo| {
                    let (cw, ch) = (
                        c.model.canvas.0.max(1) as i32,
                        c.model.canvas.1.max(1) as i32,
                    );
                    (
                        fr.x.saturating_add(mo.anchor.0.saturating_mul(fr.w) / cw),
                        fr.y.saturating_add(mo.anchor.1.saturating_mul(fr.h) / ch),
                    )
                }),
            _ => None,
        }
        .unwrap_or((fr.x.saturating_add(fr.w / 2), fr.y.saturating_add(fr.h / 2)));
        let p = balloon::place(fr, m, s.w, s.h, anchor);
        // R-BAL-9: during a scroll the outgoing line stays first and slides out.
        let pitch = s.line_h + 2;
        let scrolling = s.scroll.is_some() && s.first_line > 0;
        let (from, count) = if scrolling {
            (s.first_line - 1, s.style_lines as usize + 1)
        } else {
            (s.first_line, s.style_lines as usize)
        };
        let mut lines = Vec::new();
        for l in s.lines.iter().skip(from).take(count) {
            let words: Vec<&str> = l
                .iter()
                .filter(|(wi, _)| *wi < s.revealed)
                .map(|(_, t)| t.as_str())
                .collect();
            lines.push(words.join(" "));
        }
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        Some(AgentBalloonScene {
            window: p.window,
            body: p.body,
            tail_side: p.side,
            tail: p.tail,
            font: s.font.clone(),
            fg: s.fg,
            bg: s.bg,
            border: s.border,
            text_origin: (p.body.x + 10, p.body.y + 10),
            line_pitch: pitch,
            lines,
            scroll_px: if scrolling {
                s.scroll.map_or(0, |x| x.2 - pitch)
            } else {
                0
            },
        })
    }

    /// Current properties.
    pub fn properties(&self) -> Properties {
        let (name, description) = self
            .ch
            .as_ref()
            .map(|c| {
                let info = self
                    .catalog
                    .iter()
                    .find(|e| e.id == c.model.id)
                    .map_or(c.model.info.as_slice(), |e| e.info.as_slice());
                l10n::display_names(info, self.lang)
            })
            .unwrap_or_default();
        Properties {
            character: self
                .ch
                .as_ref()
                .map(|c| c.model.id.clone())
                .unwrap_or_default(),
            name,
            description,
            visible: self.visible(),
            left: self.ch.as_ref().map_or(0, |c| c.x),
            top: self.ch.as_ref().map_or(0, |c| c.y),
            width: self.ch.as_ref().map_or(0, |c| c.model.display.0),
            height: self.ch.as_ref().map_or(0, |c| c.model.display.1),
            idle_on: self.idle_on,
            move_cause: self.move_cause,
            visibility_cause: self.visibility_cause,
            balloon_enabled: self
                .ch
                .as_ref()
                .is_some_and(|c| c.model.balloon.enabled && self.config.balloon),
            animation_names: self
                .ch
                .as_ref()
                .map(|c| c.model.animation_names())
                .unwrap_or_default(),
        }
    }
}

fn frame_rect(c: &Char) -> Rect {
    Rect::new(c.x, c.y, c.model.display.0 as i32, c.model.display.1 as i32)
}
