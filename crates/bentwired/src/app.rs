//! The daemon: one thread owns the core and drives it from D-Bus calls, the clock, the
//! backend's input and Unix signals. Replies and signals are sent from here, in the
//! order the core returns them (B-Q-1).

use crate::config::{self, State};
use crate::dbus::{self, Call, TestCall};
use crate::discovery;
use crate::library::Shared;
use crate::present::{self, Hit, Rendered};
use bentwire_core::Core;
use bentwire_core::api::{
    self, Event, Key, MenuInput, Output, OutputInfo, PointerEvent, PointerKind, Properties,
    ReplyValue, Scene,
};
use bentwire_render::font::Fonts;
use bentwire_render::theme::Theme;
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

/// Backends (Wayland, headless) present surfaces and report input as core events.
pub trait Backend {
    fn present(&mut self, r: &Rendered, scene: &Scene);
    /// Events gathered since the last call.
    fn poll(&mut self) -> Vec<Event>;
    /// Send buffered requests before the loop sleeps.
    fn flush(&mut self) {}
    /// What the backend has actually requested from the compositor (Test1.Snapshot).
    fn snapshot(&self) -> serde_json::Value {
        serde_json::Value::Null
    }
}

pub struct App {
    pub core: Core,
    pub lib: Shared,
    pub conn: Connection,
    pub clock: Clock,
    pub test: bool,
    pub test_full: bool,
    pub outputs: Vec<OutputInfo>,
    pub next_output_id: u32,
    pub backend: Box<dyn Backend>,
    pub audio: Option<crate::audio::Audio>,
    /// No audio device by design (headless, `--no-audio`): sounds count as played, so
    /// timing is as with sound. A device that failed to open reports failures instead.
    pub silent: bool,
    pub tray: Option<crate::tray::Tray>,
    pub fonts: Fonts,
    pub theme: Option<Theme>,
    pub trace: Option<std::fs::File>,
    pub dump_frames: Option<PathBuf>,
    pub dump_seq: u64,
    pub roots_cli: Vec<PathBuf>,
    pub config_paths: Vec<PathBuf>,
    pub state_path: PathBuf,
    pub next_token: u64,
    pub pending: HashMap<u64, Message>,
    pub last_scene: Option<Scene>,
    pub rendered: Rendered,
    pub last_props: Option<Properties>,
    pub quit: bool,
    pub menu_serial: u32,
    /// Surface holding the implicit grab of a Test1 press.
    pub grab: Option<Hit>,
    /// Events raised while processing a batch, fed in after it.
    pub deferred: Vec<Event>,
}

impl App {
    pub fn trace(&mut self, mut v: serde_json::Value) {
        let t = self.clock.now();
        if let Some(f) = &mut self.trace {
            v["t"] = serde_json::json!(t);
            let _ = writeln!(f, "{v}");
        }
    }

    /// Feed one event to the core and process what comes out.
    pub fn event(&mut self, ev: Event) {
        let now = self.clock.now();
        let out = self.core.handle(now, ev);
        self.process(out);
    }

    pub fn handle_dbus(&mut self, msg: Message) {
        let Some(parsed) = dbus::parse(&msg, self.test, self.test_full) else {
            return;
        };
        let h = msg.header();
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
            Call::Get(iface, name) => {
                match self
                    .prop_map(&iface)
                    .and_then(|m| m.get(name.as_str()).cloned())
                {
                    Some(v) => dbus::reply(&conn, &h, &(v,)),
                    None => dbus::reply_error(
                        &conn,
                        &h,
                        &dbus::err(
                            "org.freedesktop.DBus.Error.UnknownProperty",
                            format!("no property {iface}.{name}"),
                        ),
                    ),
                }
            }
            Call::GetAll(iface) => {
                let m = self.prop_map(&iface).unwrap_or_default();
                dbus::reply(&conn, &h, &(m,))
            }
            Call::Core(c) => {
                let token = self.next_token;
                self.next_token += 1;
                let sender = h.sender().map(|s| s.to_string()).unwrap_or_default();
                self.pending.insert(token, msg.clone());
                self.event(Event::Call {
                    token,
                    sender,
                    call: c,
                });
            }
            Call::Test(t) => self.handle_test(&msg, t),
        }
    }

    fn handle_test(&mut self, msg: &Message, t: TestCall) {
        let conn = self.conn.clone();
        let h = msg.header();
        match t {
            TestCall::Advance(ms) => {
                let target = self.clock.now() + ms as u64;
                // Deadline by deadline, so traces carry the exact time of each step.
                while let Some(d) = self.core.next_deadline().filter(|d| *d <= target) {
                    self.clock = Clock::Fake(d.max(self.clock.now()));
                    let out = self.core.tick(d);
                    self.process(out);
                }
                self.clock = Clock::Fake(target);
                let out = self.core.advance_to(target);
                self.process(out);
                dbus::reply(&conn, &h, &());
            }
            TestCall::Pointer(kind, button, x, y) => {
                dbus::reply(&conn, &h, &());
                let kind = match kind.as_str() {
                    "press" => PointerKind::Press,
                    "release" => PointerKind::Release,
                    "motion" => PointerKind::Motion,
                    _ => PointerKind::Leave,
                };
                self.synthetic_pointer(kind, button, x, y);
            }
            TestCall::Key(k) => match parse_key(&k) {
                Some((key, modifiers)) => {
                    dbus::reply(&conn, &h, &());
                    self.event(Event::Key { key, modifiers });
                }
                None => dbus::reply_error(
                    &conn,
                    &h,
                    &dbus::err(api::errors::INVALID_ARGS, format!("unknown key {k:?}")),
                ),
            },
            TestCall::SetDraws(d) => {
                self.core
                    .set_draws(Box::new(bentwire_core::draws::ScriptedDraws::new(d)));
                dbus::reply(&conn, &h, &());
            }
            TestCall::AddOutput(spec) => match crate::args::parse_output(&spec) {
                Ok(o) => {
                    let id = self.next_output_id;
                    self.next_output_id += 1;
                    self.outputs.push(OutputInfo {
                        id,
                        rect: api::Rect::new(o.x, o.y, o.w as i32, o.h as i32),
                        scale: o.scale,
                    });
                    dbus::reply(&conn, &h, &(id,));
                    self.trace(serde_json::json!({"type": "output_added", "id": id, "spec": spec}));
                    self.event(Event::Outputs(self.outputs.clone()));
                }
                Err(e) => dbus::reply_error(&conn, &h, &dbus::err(api::errors::INVALID_ARGS, e)),
            },
            TestCall::RemoveOutput(id) => {
                if !self.outputs.iter().any(|o| o.id == id) {
                    return dbus::reply_error(
                        &conn,
                        &h,
                        &dbus::err(api::errors::INVALID_ARGS, format!("no output {id}")),
                    );
                }
                self.outputs.retain(|o| o.id != id);
                dbus::reply(&conn, &h, &());
                self.trace(serde_json::json!({"type": "output_removed", "id": id}));
                self.event(Event::Outputs(self.outputs.clone()));
            }
            TestCall::Snapshot => {
                let s = self.snapshot();
                dbus::reply(&conn, &h, &(s,));
            }
        }
    }

    /// A Test1 pointer event: hit-tested like a compositor would, with an implicit grab
    /// from press to release, and menu clicks mapped to items (B-WL-5, B-WL-9).
    pub fn synthetic_pointer(&mut self, kind: PointerKind, button: u32, x: i32, y: i32) {
        let target = match kind {
            PointerKind::Press => {
                let h = present::hit(&self.rendered, x, y);
                self.menu_serial += 1;
                if self.rendered.menu.is_some() && !matches!(h, Some(Hit::Menu(_))) {
                    // A press outside the grabbed popup: popup_done.
                    return self.event(Event::Menu(MenuInput::Dismiss));
                }
                self.grab = h;
                h
            }
            PointerKind::Release => self.grab.take().map(|g| match g {
                Hit::Menu(_) => match present::hit(&self.rendered, x, y) {
                    Some(Hit::Menu(i)) => Hit::Menu(i),
                    _ => Hit::Menu(None),
                },
                other => other,
            }),
            _ => self.grab.or_else(|| present::hit(&self.rendered, x, y)),
        };
        match target {
            Some(Hit::Menu(i)) => match kind {
                PointerKind::Release => {
                    if let Some(i) = i {
                        self.event(Event::Menu(MenuInput::Activate(i)))
                    }
                }
                PointerKind::Motion => self.event(Event::Menu(MenuInput::Hover(i))),
                _ => {}
            },
            Some(Hit::Surface(surface)) => self.event(Event::Pointer(PointerEvent {
                kind,
                surface,
                button,
                modifiers: 0,
                x,
                y,
            })),
            None => {}
        }
    }

    pub fn snapshot(&self) -> String {
        use serde_json::json;
        let s = self.last_scene.clone().unwrap_or_else(|| self.core.scene());
        let r = &self.rendered;
        let rect = |q: api::Rect| json!({"x": q.x, "y": q.y, "w": q.w, "h": q.h});
        let mut surfaces = Vec::new();
        if let Some(c) = &r.character {
            surfaces.push(
                json!({"kind": "character", "x": c.x, "y": c.y, "w": c.buf.w,
                "h": c.buf.h, "mapped": true, "output": c.output, "buffer_scale": c.buf.scale}),
            );
        }
        for b in r.balloons.iter().flatten() {
            let kind = if b.office {
                "office_balloon"
            } else {
                "agent_balloon"
            };
            surfaces.push(
                json!({"kind": kind, "x": b.x, "y": b.y, "w": b.buf.w, "h": b.buf.h,
                "mapped": true}),
            );
        }
        if let Some(m) = &r.menu {
            surfaces.push(
                json!({"kind": "menu", "x": m.x, "y": m.y, "w": m.buf.w, "h": m.buf.h,
                "mapped": true}),
            );
        }
        let office = s.office_balloon.as_ref().map(|b| {
            json!({
                "buttons": b.buttons.iter().map(|x| json!({"caption": x.caption,
                    "rect": rect(x.rect), "state": format!("{:?}", x.state)})).collect::<Vec<_>>(),
                "labels": b.labels.iter().map(|x| json!({"rect": rect(x.rect),
                    "clickable": x.clickable})).collect::<Vec<_>>(),
                "checkboxes": b.checkboxes.iter().map(|x| json!({"rect": rect(x.rect),
                    "checked": x.checked})).collect::<Vec<_>>(),
            })
        });
        let menu = r.menu.as_ref().map(|m| {
            m.entries
                .iter()
                .zip(&m.geometry.items)
                .map(|(e, q)| match e {
                    api::MenuEntry::Item { label, .. } => {
                        json!({"label": label, "x": m.x + q.x, "y": m.y + q.y, "w": q.w, "h": q.h})
                    }
                    api::MenuEntry::Separator => json!({"separator": true}),
                })
                .collect::<Vec<_>>()
        });
        let p = self.core.properties();
        json!({
            "character": p.character,
            "visible": p.visible,
            "left": p.left,
            "top": p.top,
            "keyboard_interactivity": if r.keyboard { "on_demand" } else { "none" },
            "surfaces": surfaces,
            "office_balloon": office,
            "agent_balloon_lines": s.agent_balloon.as_ref().map(|b| b.lines.clone()),
            "menu": menu,
            "tray": {"title": s.tray.title, "tooltip": s.tray.tooltip},
            "backend": self.backend.snapshot(),
            "outputs": self.outputs.iter().map(|o| json!({"id": o.id, "x": o.rect.x,
                "y": o.rect.y, "w": o.rect.w, "h": o.rect.h, "scale": o.scale})).collect::<Vec<_>>(),
        })
        .to_string()
    }

    fn prop_map(&self, iface: &str) -> Option<HashMap<&'static str, Value<'static>>> {
        let p = self.core.properties();
        match iface {
            dbus::AGENT_IFACE => Some(dbus::agent_props_map(&p.character)),
            dbus::CHAR_IFACE => Some(dbus::char_props_map(&p)),
            _ => None,
        }
    }

    pub fn backend_events(&mut self) {
        for ev in self.backend.poll() {
            if let Event::Outputs(o) = &ev {
                self.outputs = o.clone();
            }
            self.event(ev);
        }
    }

    /// Run the core's outputs in order, then update properties and the scene.
    pub fn process(&mut self, out: Vec<Output>) {
        for o in out {
            match o {
                Output::Reply { token, result } => {
                    let Some(msg) = self.pending.remove(&token) else {
                        continue;
                    };
                    let h = msg.header();
                    match result {
                        Ok(ReplyValue::Unit) => dbus::reply(&self.conn, &h, &()),
                        Ok(ReplyValue::RequestId(id)) => dbus::reply(&self.conn, &h, &(id,)),
                        Ok(ReplyValue::Characters(v)) => dbus::reply(&self.conn, &h, &(v,)),
                        Err(e) => dbus::reply_error(&self.conn, &h, &e),
                    }
                }
                Output::Signal { target, signal } => {
                    self.trace(serde_json::json!({"type": "signal",
                        "member": dbus::signal_member(&signal),
                        "args": dbus::signal_args(&signal), "dest": target}));
                    dbus::emit(&self.conn, target.as_deref(), &signal);
                }
                Output::SoundStart { sound } => self.start_sound(&sound),
                Output::SoundStop => {
                    self.trace(serde_json::json!({"type": "sound_stop"}));
                    if let Some(a) = &mut self.audio {
                        a.stop();
                    }
                }
                Output::PersistState(s) => {
                    let st = State {
                        character: Some(s.character).filter(|c| !c.is_empty()),
                        left: Some(s.left),
                        top: Some(s.top),
                        visible: Some(s.visible),
                    };
                    if let Err(e) = st.save(&self.state_path) {
                        log::warn!("writing {}: {e}", self.state_path.display());
                    }
                }
                Output::OpenConfig => open_options(),
                Output::Quit => self.quit = true,
            }
        }
        self.update();
        for ev in std::mem::take(&mut self.deferred) {
            self.event(ev);
        }
    }

    /// Sounds are logged with their decoded duration (T-4); a failure is reported to
    /// the core so completion does not wait for it (R-ANIM-8).
    fn start_sound(&mut self, sound: &str) {
        let id = self.core.properties().character;
        let pcm = self
            .lib
            .borrow()
            .get(&id)
            .and_then(|c| c.sound(sound).cloned());
        let dur = pcm.as_ref().map(|p| p.duration_ms());
        self.trace(serde_json::json!({"type": "sound_start", "sound": sound, "duration_ms": dur}));
        let ok = match (&mut self.audio, pcm) {
            (Some(a), Some(p)) => a.play(&p),
            (None, Some(_)) => self.silent,
            (_, None) => false,
        };
        if !ok {
            log::warn!("sound {sound} could not be played");
            self.deferred.push(Event::SoundFailed);
        }
    }

    /// PropertiesChanged and the scene, after every batch of outputs.
    fn update(&mut self) {
        let p = self.core.properties();
        if let Some(old) = &self.last_props
            && old != &p
        {
            dbus::emit_props_changed(
                &self.conn,
                dbus::AGENT_PATH,
                dbus::AGENT_IFACE,
                &dbus::agent_props_map(&old.character),
                &dbus::agent_props_map(&p.character),
            );
            dbus::emit_props_changed(
                &self.conn,
                dbus::CHAR_PATH,
                dbus::CHAR_IFACE,
                &dbus::char_props_map(old),
                &dbus::char_props_map(&p),
            );
        }
        self.last_props = Some(p);
        let scene = self.core.scene();
        if self.last_scene.as_ref() == Some(&scene) {
            return;
        }
        if let Some(t) = &mut self.tray {
            let icon = scene.tray.icon_character.as_ref().and_then(|id| {
                let lib = self.lib.borrow();
                let i = lib.get(id)?.icon.as_ref()?;
                Some(crate::tray::sni_icon(i.width, i.height, &i.data))
            });
            t.update(&scene.tray, icon);
        }
        let r = present::render(&scene, &self.lib.borrow(), &self.fonts, self.theme.as_ref());
        for q in present::requests(&self.rendered, &r, &scene.outputs, self.menu_serial) {
            self.trace(q);
        }
        if r != self.rendered {
            self.backend.present(&r, &scene);
            self.dump(&scene);
        }
        self.rendered = r;
        self.last_scene = Some(scene);
    }

    fn dump(&mut self, scene: &Scene) {
        let Some(dir) = self.dump_frames.clone() else {
            return;
        };
        self.dump_seq += 1;
        for o in &scene.outputs {
            let k = bentwire_render::scene::buffer_scale(o.scale);
            let pic = present::character_picture(scene, &self.lib.borrow(), k);
            let img = bentwire_render::scene::compose_output(
                scene,
                o,
                pic.as_ref(),
                &self.fonts,
                self.theme.as_ref(),
            );
            let p = dir.join(format!(
                "{:06}-t{:08}-out{}.png",
                self.dump_seq,
                self.clock.now(),
                o.id
            ));
            if let Err(e) = bentwire_render::scene::write_png(&p, &img) {
                log::warn!("dump {}: {e}", p.display());
            }
        }
    }

    /// Time until the next core deadline (real clock only).
    pub fn timeout(&self) -> Option<Duration> {
        match self.clock {
            Clock::Real(_) => self
                .core
                .next_deadline()
                .map(|d| Duration::from_millis(d.saturating_sub(self.clock.now()))),
            Clock::Fake(_) => None,
        }
    }

    /// Run due timers on the real clock.
    pub fn tick(&mut self) {
        if let Clock::Real(_) = self.clock {
            let now = self.clock.now();
            if self.core.next_deadline().is_some_and(|d| d <= now) {
                let out = self.core.tick(now);
                self.process(out);
            }
        }
    }

    /// SIGHUP: reload config and rediscover characters (B-CFG-1).
    pub fn reload(&mut self) {
        match config::Config::load(&config::config_path()) {
            Ok(c) => {
                self.config_paths = c.character_paths.clone();
                self.event(Event::Config(c.to_core()));
            }
            Err(e) => log::warn!("config not reloaded: {e}"),
        }
        let roots = discovery::roots(&self.roots_cli, &self.config_paths);
        let entries = discovery::discover(&roots);
        log::info!("reloaded: {} characters", entries.len());
        self.lib.borrow_mut().entries = entries;
        let catalog = self.lib.borrow().catalog();
        self.event(Event::Catalog(catalog));
    }
}

/// `Test1.Key` names: `Escape`, `Enter` (`Return`), `Tab`, `space`, `Up`, `Down`, `Left`,
/// `Right`, or one character; `Alt+`, `Ctrl+` and `Shift+` prefixes set modifiers.
pub fn parse_key(s: &str) -> Option<(Key, u32)> {
    let mut mods = 0;
    let mut rest = s;
    'prefix: loop {
        for (p, m) in [("shift+", 1), ("ctrl+", 2), ("alt+", 4)] {
            if rest.len() > p.len()
                && rest
                    .get(..p.len())
                    .is_some_and(|x| x.eq_ignore_ascii_case(p))
            {
                mods |= m;
                rest = &rest[p.len()..];
                continue 'prefix;
            }
        }
        break;
    }
    let key = match rest.to_ascii_lowercase().as_str() {
        "escape" | "esc" => Key::Escape,
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "up" => Key::Up,
        "down" => Key::Down,
        "left" => Key::Left,
        "right" => Key::Right,
        _ => {
            let mut it = rest.chars();
            let c = it.next()?;
            if it.next().is_some() {
                return None;
            }
            Key::Char(c)
        }
    };
    Some((key, mods))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn t_4_key_names() {
        assert_eq!(parse_key("Escape"), Some((Key::Escape, 0)));
        assert_eq!(parse_key("Return"), Some((Key::Enter, 0)));
        assert_eq!(parse_key("Alt+n"), Some((Key::Char('n'), 4)));
        assert_eq!(parse_key("ä"), Some((Key::Char('ä'), 0)));
        assert_eq!(parse_key("Alt+ä"), Some((Key::Char('ä'), 4)));
        assert_eq!(parse_key("äää+x"), None);
        assert_eq!(parse_key("+"), Some((Key::Char('+'), 0)));
        assert_eq!(parse_key("bogus"), None);
    }
}
