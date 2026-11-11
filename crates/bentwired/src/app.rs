//! The daemon: one thread owns the engine and drives it from D-Bus calls, the clock,
//! the backend's input and Unix signals. Replies and signals are sent from here, in
//! order (B-Q-1).

use crate::config::{self, Config, State, StateWriter};
use crate::dbus::{self, Call, TestCall};
use crate::discovery::{self, Entry};
use crate::engine::*;
use crate::layout;
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::message::Message;
use zbus::zvariant::Value;

pub enum Clock {
    Real(Instant),
    Fake(u64),
}

impl Clock {
    pub fn now(&self) -> u64 {
        match self {
            Clock::Real(t) => t.elapsed().as_millis() as u64,
            Clock::Fake(t) => *t,
        }
    }
}

/// What the backend tells the app; also used for Test1 input.
pub enum BackendEvent {
    Input(Input),
    Outputs(Vec<Output>),
}

pub trait Backend {
    fn present(&mut self, scene: &Scene, outputs: &[Output]);
    /// Events gathered since the last call.
    fn poll(&mut self) -> Vec<BackendEvent>;
}

pub struct App {
    pub engine: Box<dyn Engine>,
    pub conn: Connection,
    pub clock: Clock,
    pub test: bool,
    pub test_full: bool,
    pub outputs: Vec<Output>,
    pub next_output_id: u32,
    pub backend: Box<dyn Backend>,
    pub trace: Option<std::fs::File>,
    pub dump_frames: Option<PathBuf>,
    pub dump_seq: u64,
    pub entries: Vec<Entry>,
    pub roots_cli: Vec<PathBuf>,
    pub config: Config,
    pub german: bool,
    pub state_path: PathBuf,
    pub writer: StateWriter,
    pub write_at: Option<u64>,
    pub wall: Instant,
    pub last_scene: Scene,
    pub last_char_props: Option<HashMap<&'static str, Value<'static>>>,
    pub last_agent_props: HashMap<&'static str, Value<'static>>,
    pub quit: bool,
    pub menu_serial: u32,
}

impl App {
    pub fn trace(&mut self, mut v: serde_json::Value) {
        let t = self.clock.now();
        if let Some(f) = &mut self.trace {
            v["t"] = serde_json::json!(t);
            let _ = writeln!(f, "{v}");
        }
    }

    pub fn current_id(&self) -> String {
        self.engine.props().map(|p| p.id).unwrap_or_default()
    }

    pub fn handle_dbus(&mut self, msg: Message) {
        let Some(parsed) = dbus::parse(&msg, self.test, self.test_full) else {
            return;
        };
        let h = msg.header();
        let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
        let now = self.clock.now();
        let call = match parsed {
            Ok(c) => c,
            Err(e) => return dbus::reply_error(&self.conn, &h, &e),
        };
        let conn = self.conn.clone();
        match call {
            Call::Ping => dbus::reply(&conn, &h, &()),
            Call::GetMachineId => {
                let id = std::fs::read_to_string("/etc/machine-id").unwrap_or_default();
                dbus::reply(&conn, &h, &(id.trim(),))
            }
            Call::Introspect(p) => dbus::reply(
                &conn,
                &h,
                &(dbus::introspect(&p, self.test, self.test_full),),
            ),
            Call::Get(iface, name) => match self
                .prop_map(&iface)
                .and_then(|m| m.get(name.as_str()).cloned())
            {
                Some(v) => dbus::reply(&conn, &h, &(v,)),
                None => dbus::reply_error(
                    &conn,
                    &h,
                    &CallError::new(
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        format!("no property {iface}.{name}"),
                    ),
                ),
            },
            Call::GetAll(iface) => {
                let m = self.prop_map(&iface).unwrap_or_default();
                dbus::reply(&conn, &h, &(m,))
            }
            Call::ListCharacters => {
                let lang = if self.german { "de" } else { "en" };
                let v: Vec<(String, String, String)> = self
                    .entries
                    .iter()
                    .map(|e| {
                        let (n, d) = discovery::display_names(&e.info, lang);
                        (e.id.clone(), n, d)
                    })
                    .collect();
                dbus::reply(&conn, &h, &(v,))
            }
            Call::Select(id) => {
                let id = id.to_ascii_lowercase();
                match self.entries.iter().find(|e| e.id == id).cloned() {
                    None => dbus::reply_error(
                        &conn,
                        &h,
                        &CallError::new(E_NO_SUCH_CHARACTER, format!("no character {id:?}")),
                    ),
                    Some(e) => match self.engine.select(now, Some(&e)) {
                        Ok(()) => dbus::reply(&conn, &h, &()),
                        Err(err) => dbus::reply_error(&conn, &h, &err),
                    },
                }
            }
            Call::Quit => {
                dbus::reply(&conn, &h, &());
                self.quit = true;
            }
            Call::Char(m) => match self.engine.call(now, &sender, m) {
                Ok(0) => dbus::reply(&conn, &h, &()),
                Ok(id) => dbus::reply(&conn, &h, &(id,)),
                Err(e) => dbus::reply_error(&conn, &h, &e),
            },
            Call::Test(t) => self.handle_test(&h, t),
        }
        self.flush();
    }

    fn handle_test(&mut self, h: &zbus::message::Header<'_>, t: TestCall) {
        let conn = self.conn.clone();
        let now = self.clock.now();
        match t {
            TestCall::Advance(ms) => {
                // Step through every deadline so timers fire in order.
                let target = now + ms as u64;
                loop {
                    let next = self.engine.next_deadline().filter(|d| *d <= target);
                    let step = next.unwrap_or(target).max(self.clock.now());
                    self.clock = Clock::Fake(step);
                    self.engine.advance(step);
                    self.flush();
                    if next.is_none() || step >= target {
                        break;
                    }
                }
                self.clock = Clock::Fake(target);
                self.engine.advance(target);
                dbus::reply(&conn, h, &());
            }
            TestCall::Pointer(kind, button, x, y) => {
                dbus::reply(&conn, h, &());
                let kind = PointerKind::parse(&kind).expect("validated in dbus::parse");
                self.engine.input(
                    now,
                    Input::Pointer {
                        kind,
                        button,
                        x,
                        y,
                        modifiers: 0,
                    },
                );
            }
            TestCall::Key(k) => {
                dbus::reply(&conn, h, &());
                self.engine.input(now, Input::Key(k));
            }
            TestCall::SetDraws(d) => {
                self.engine.set_draws(d);
                dbus::reply(&conn, h, &());
            }
            TestCall::AddOutput(spec) => match crate::args::parse_output(&spec) {
                Ok(o) => {
                    let id = self.next_output_id;
                    self.next_output_id += 1;
                    self.outputs.push(Output {
                        id,
                        x: o.x,
                        y: o.y,
                        w: o.w,
                        h: o.h,
                        scale: o.scale,
                    });
                    dbus::reply(&conn, h, &(id,));
                    let outs = self.outputs.clone();
                    self.trace(serde_json::json!({"type": "output_added", "id": id, "spec": spec}));
                    self.engine.set_outputs(now, &outs);
                }
                Err(e) => dbus::reply_error(&conn, h, &CallError::new(E_INVALID_ARGS, e)),
            },
            TestCall::RemoveOutput(id) => {
                if !self.outputs.iter().any(|o| o.id == id) {
                    return dbus::reply_error(
                        &conn,
                        h,
                        &CallError::new(E_INVALID_ARGS, format!("no output {id}")),
                    );
                }
                self.outputs.retain(|o| o.id != id);
                dbus::reply(&conn, h, &());
                let outs = self.outputs.clone();
                self.trace(serde_json::json!({"type": "output_removed", "id": id}));
                self.engine.set_outputs(now, &outs);
            }
            TestCall::Snapshot => {
                let s = self.snapshot();
                dbus::reply(&conn, h, &(s,));
            }
            TestCall::Answer(r) => {
                dbus::reply(&conn, h, &());
                self.engine.answer(now, r);
            }
        }
    }

    pub fn snapshot(&self) -> String {
        let scene = self.engine.scene();
        let surfaces: Vec<_> = scene
            .surfaces
            .iter()
            .map(|s| {
                serde_json::json!({
                    "kind": s.kind,
                    "x": s.x, "y": s.y, "w": s.w, "h": s.h,
                    "mapped": true,
                    "output": layout::output_for(&self.outputs, s.x, s.y, s.w, s.h).map(|o| o.id),
                })
            })
            .collect();
        let p = self.engine.props();
        serde_json::json!({
            "character": p.as_ref().map(|p| p.id.clone()),
            "visible": p.as_ref().map(|p| p.visible),
            "left": p.as_ref().map(|p| p.left),
            "top": p.as_ref().map(|p| p.top),
            "keyboard_interactivity": if scene.keyboard { "on_demand" } else { "none" },
            "surfaces": surfaces,
            "outputs": self.outputs.iter().map(|o| serde_json::json!({"id": o.id, "x": o.x, "y": o.y, "w": o.w, "h": o.h, "scale": o.scale})).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn prop_map(&self, iface: &str) -> Option<HashMap<&'static str, Value<'static>>> {
        match iface {
            dbus::AGENT_IFACE => Some(dbus::agent_props_map(&self.current_id())),
            dbus::CHAR_IFACE => Some(dbus::char_props_map(
                &self.engine.props().unwrap_or_default(),
            )),
            _ => None,
        }
    }

    pub fn backend_events(&mut self) {
        for ev in self.backend.poll() {
            let now = self.clock.now();
            match ev {
                BackendEvent::Input(i) => self.engine.input(now, i),
                BackendEvent::Outputs(o) => {
                    self.outputs = o;
                    let outs = self.outputs.clone();
                    self.engine.set_outputs(now, &outs);
                }
            }
        }
        self.flush();
    }

    /// Send what the engine produced, update properties, present the scene.
    pub fn flush(&mut self) {
        let effects = self.engine.drain();
        for e in effects {
            match e {
                Effect::Signal(dest, s) => {
                    self.trace(serde_json::json!({"type": "signal", "member": s.member(), "args": s.args_json(), "dest": dest}));
                    match dest.as_deref() {
                        // Requests the daemon made itself (startup Show) have no client.
                        Some("") => {}
                        d => dbus::emit(&self.conn, d, &s),
                    }
                }
                Effect::SoundStart { sound, duration_ms } => {
                    self.trace(serde_json::json!({"type": "sound_start", "sound": sound, "duration_ms": duration_ms}));
                    crate::audio::start(&sound);
                }
                Effect::SoundStop => {
                    self.trace(serde_json::json!({"type": "sound_stop"}));
                    crate::audio::stop();
                }
                Effect::OpenOptions => open_options(),
                Effect::Persist => self.schedule_state(),
            }
        }
        // Properties.
        let agent = dbus::agent_props_map(&self.current_id());
        dbus::emit_props_changed(
            &self.conn,
            dbus::AGENT_PATH,
            dbus::AGENT_IFACE,
            &self.last_agent_props,
            &agent,
        );
        if agent != self.last_agent_props {
            self.schedule_state();
        }
        self.last_agent_props = agent;
        let ch = self.engine.props().map(|p| dbus::char_props_map(&p));
        if let (Some(old), Some(new)) = (&self.last_char_props, &ch) {
            dbus::emit_props_changed(&self.conn, dbus::CHAR_PATH, dbus::CHAR_IFACE, old, new);
            let keys = ["Left", "Top", "Visible"];
            if keys.iter().any(|k| old.get(k) != new.get(k)) {
                self.schedule_state();
            }
        } else if let Some(new) = &ch {
            dbus::emit_props_changed(
                &self.conn,
                dbus::CHAR_PATH,
                dbus::CHAR_IFACE,
                &HashMap::new(),
                new,
            );
        }
        self.last_char_props = ch;
        // Scene.
        let scene = self.engine.scene();
        if scene != self.last_scene {
            for r in layout::requests(&self.last_scene, &scene, &self.outputs, self.menu_serial) {
                self.trace(r);
            }
            self.backend.present(&scene, &self.outputs);
            self.dump(&scene);
            self.last_scene = scene;
        }
    }

    fn dump(&mut self, scene: &Scene) {
        let Some(dir) = self.dump_frames.clone() else {
            return;
        };
        self.dump_seq += 1;
        for o in &self.outputs {
            let (w, h, rgba) = layout::compose(scene, o);
            let p = dir.join(format!(
                "{:06}-t{:08}-out{}.png",
                self.dump_seq,
                self.clock.now(),
                o.id
            ));
            if let Err(e) = write_png(&p, w, h, &rgba) {
                log::warn!("dump {}: {e}", p.display());
            }
        }
    }

    pub fn current_state(&self) -> State {
        let p = self.engine.props();
        State {
            character: p.as_ref().map(|p| p.id.clone()).filter(|s| !s.is_empty()),
            left: p.as_ref().map(|p| p.left),
            top: p.as_ref().map(|p| p.top),
            visible: p.as_ref().map(|p| p.visible),
        }
    }

    fn wall_ms(&self) -> u64 {
        self.wall.elapsed().as_millis() as u64
    }

    fn schedule_state(&mut self) {
        let s = self.current_state();
        let due = self.writer.due(&s, self.wall_ms());
        self.write_at = match (self.write_at, due) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
    }

    /// Write the state file if due (B-CFG-2). `force` at exit.
    pub fn write_state(&mut self, force: bool) {
        let now = self.wall_ms();
        if !force && self.write_at.is_none_or(|t| t > now) {
            return;
        }
        self.write_at = None;
        let s = self.current_state();
        if !force && self.writer.due(&s, now).is_none() {
            return;
        }
        if let Err(e) = s.save(&self.state_path) {
            log::warn!("writing {}: {e}", self.state_path.display());
        }
        self.writer.written(s, now);
    }

    /// Time until the next thing to do, for the event loop's timeout.
    pub fn timeout(&self) -> Option<Duration> {
        let mut t: Option<u64> = None;
        if let Clock::Real(_) = self.clock
            && let Some(d) = self.engine.next_deadline()
        {
            t = Some(d.saturating_sub(self.clock.now()));
        }
        if let Some(w) = self.write_at {
            let d = w.saturating_sub(self.wall_ms());
            t = Some(t.map_or(d, |x| x.min(d)));
        }
        t.map(Duration::from_millis)
    }

    /// Advance the real clock (no-op for the fake clock) and write state if due.
    pub fn tick(&mut self) {
        if let Clock::Real(_) = self.clock {
            let now = self.clock.now();
            self.engine.advance(now);
            self.flush();
        }
        self.write_state(false);
    }

    /// SIGHUP: reload config and rediscover characters (B-CFG-1).
    pub fn reload(&mut self) {
        match Config::load(&config::config_path()) {
            Ok(c) => {
                self.config = c;
                self.german = self.config.german();
            }
            Err(e) => log::warn!("config not reloaded: {e}"),
        }
        let roots = discovery::roots(&self.roots_cli, &self.config.character_paths);
        self.entries = discovery::discover(&roots);
        log::info!("reloaded: {} characters", self.entries.len());
    }
}

pub fn write_png(path: &std::path::Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    let f = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(f), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())
}

/// B-MENU-3: write a default config if none exists, then xdg-open it.
fn open_options() {
    let p = config::config_path();
    if !p.exists() {
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Err(e) = std::fs::write(&p, config::DEFAULT_CONFIG_TEXT) {
            log::warn!("writing {}: {e}", p.display());
        }
    }
    if let Err(e) = std::process::Command::new("xdg-open").arg(&p).spawn() {
        log::warn!("xdg-open {}: {e}", p.display());
    }
}
