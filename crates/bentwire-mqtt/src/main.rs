//! `bentwire-mqtt`: MQTT bridge for bentwired with Home Assistant discovery (spec B-MQTT-1).

use rumqttc::{Client, Event, LastWill, MqttOptions, Outgoing, Packet, QoS};
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::{OwnedValue, Value};

const BUS_NAME: &str = "org.bentwire.Agent";
const CHAR_PATH: &str = "/org/bentwire/Character";
const CHAR_IFACE: &str = "org.bentwire.Character1";
const VERSION: &str = env!("CARGO_PKG_VERSION");
const RETRY: Duration = Duration::from_secs(2);

const USAGE: &str = "\
usage: bentwire-mqtt [--host HOST] [--port PORT] [--base TOPIC]
Reads $XDG_CONFIG_HOME/bentwire/mqtt.toml; the options override it.
";

/// B-L10N-2 result names for results -1..-15 (as the CLI prints them).
const RESULT_NAMES: [&str; 15] = [
    "ok",
    "cancel",
    "yes",
    "no",
    "back",
    "next",
    "retry",
    "abort",
    "ignore",
    "search",
    "snooze",
    "close",
    "tips",
    "options",
    "yes-to-all",
];

/// The answer text of an Ask result, as `bentwire ask` prints it (spec 5.5).
fn result_name(result: i32) -> String {
    match result {
        0 => "none".into(),
        r @ -15..=-1 => RESULT_NAMES[(-r - 1) as usize].into(),
        r if r <= -25 => format!("button:{}", -r - 24),
        r if r > 0 => format!("label:{r}"),
        r => format!("unknown:{r}"),
    }
}

#[derive(Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Config {
    host: String,
    port: u16,
    username: Option<String>,
    password: Option<String>,
    base_topic: String,
    client_id: String,
    discovery: bool,
    discovery_prefix: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            host: "localhost".into(),
            port: 1883,
            username: None,
            password: None,
            base_topic: "bentwire".into(),
            client_id: "bentwire-mqtt".into(),
            discovery: true,
            discovery_prefix: "homeassistant".into(),
        }
    }
}

fn config_path() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config")
        });
    base.join("bentwire/mqtt.toml")
}

/// A config error without the source snippet, and without the message on a line that
/// mentions `password`, so that the password never reaches the log.
fn toml_error(text: &str, e: &toml::de::Error) -> String {
    let Some(span) = e.span() else {
        return e.message().to_string();
    };
    let line = text[..span.start].matches('\n').count();
    let src = text.lines().nth(line).unwrap_or("");
    if src.contains("password") {
        format!("line {}: bad password value", line + 1)
    } else {
        format!("line {}: {}", line + 1, e.message())
    }
}

/// The config file (missing = defaults), then the command line options.
fn load_config(text: Option<&str>, args: &[String]) -> Result<Config, String> {
    let mut c: Config = match text {
        Some(t) => toml::from_str(t).map_err(|e| toml_error(t, &e))?,
        None => Config::default(),
    };
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--host" => c.host = value()?,
            "--port" => {
                let v = value()?;
                c.port = v.parse().map_err(|_| format!("bad port {v:?}"))?;
            }
            "--base" => c.base_topic = value()?,
            _ => return Err(format!("unknown option {a}")),
        }
    }
    c.base_topic = c.base_topic.trim_end_matches('/').to_string();
    for t in [&c.base_topic, &c.discovery_prefix] {
        if t.is_empty() || t.contains(['+', '#']) {
            return Err(format!("bad topic {t:?}"));
        }
    }
    Ok(c)
}

#[derive(Debug, PartialEq)]
enum AskValue {
    S(String),
    L(Vec<String>),
}

#[derive(Debug, PartialEq)]
enum Cmd {
    Say(String),
    Play(String),
    Show,
    Hide,
    Ask(HashMap<String, AskValue>),
}

/// A message on `<base>/<cmd>` as a command; None for other topics.
fn parse_command(base: &str, topic: &str, payload: &[u8]) -> Option<Result<Cmd, String>> {
    let cmd = topic.strip_prefix(base)?.strip_prefix('/')?;
    let text = || String::from_utf8(payload.to_vec()).map_err(|_| format!("{cmd}: not UTF-8"));
    Some(match cmd {
        "say" => text().map(Cmd::Say),
        "play" => text().map(|t| Cmd::Play(t.trim().to_string())),
        "show" => Ok(Cmd::Show),
        "hide" => Ok(Cmd::Hide),
        "ask" => text().and_then(|t| parse_ask(&t)).map(Cmd::Ask),
        _ => return None,
    })
}

/// A JSON object with the keys of B-ASK-1 (strings or lists of strings), or plain text.
fn parse_ask(payload: &str) -> Result<HashMap<String, AskValue>, String> {
    if !payload.trim_start().starts_with('{') {
        return Ok(HashMap::from([(
            "text".into(),
            AskValue::S(payload.into()),
        )]));
    }
    let obj: serde_json::Map<String, serde_json::Value> =
        serde_json::from_str(payload).map_err(|e| format!("ask: {e}"))?;
    let mut spec = HashMap::new();
    for (k, v) in obj {
        let v = match v {
            serde_json::Value::String(s) => AskValue::S(s),
            serde_json::Value::Array(a) => AskValue::L(
                a.into_iter()
                    .map(|x| x.as_str().map(String::from))
                    .collect::<Option<_>>()
                    .ok_or(format!("ask: {k} must hold strings"))?,
            ),
            _ => return Err(format!("ask: {k} must be a string or a list of strings")),
        };
        spec.insert(k, v);
    }
    Ok(spec)
}

/// Retained Home Assistant discovery configs: (topic, JSON).
fn discovery(c: &Config) -> Vec<(String, String)> {
    let node: String = c
        .base_topic
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    let b = &c.base_topic;
    let entities = [
        (
            "notify",
            "say",
            json!({"name": "Say", "command_topic": format!("{b}/say")}),
        ),
        (
            "button",
            "show",
            json!({"name": "Show", "command_topic": format!("{b}/show")}),
        ),
        (
            "button",
            "hide",
            json!({"name": "Hide", "command_topic": format!("{b}/hide")}),
        ),
        (
            "sensor",
            "answer",
            json!({"name": "Last answer", "state_topic": format!("{b}/answer")}),
        ),
        (
            "binary_sensor",
            "visible",
            json!({"name": "Visible", "state_topic": format!("{b}/state"),
                   "payload_on": "visible", "payload_off": "hidden"}),
        ),
    ];
    entities
        .into_iter()
        .map(|(component, object, mut v)| {
            let uid = format!("{node}_{object}");
            let o = v.as_object_mut().unwrap();
            o.insert("unique_id".into(), json!(uid));
            o.insert("availability_topic".into(), json!(format!("{b}/status")));
            o.insert(
                "device".into(),
                json!({"identifiers": [node], "name": "bentwire", "model": "bentwire-mqtt",
                       "sw_version": VERSION}),
            );
            o.insert(
                "origin".into(),
                json!({"name": "bentwire-mqtt", "sw_version": VERSION,
                       "support_url": "https://github.com/mschabhuettl/bentwire"}),
            );
            (
                format!("{}/{component}/{uid}/config", c.discovery_prefix),
                v.to_string(),
            )
        })
        .collect()
}

enum Ev {
    MqttUp,
    MqttDown,
    Message(String, Vec<u8>),
    /// The session bus connection, or None when it went away.
    Bus(Option<Connection>),
    /// bentwired got (true) or lost (false) its bus name.
    Daemon(bool),
    Visible(bool),
    AskComplete(u32, i32, Vec<bool>),
    RequestComplete(u32, u32, String),
}

fn signal_event(m: &Message) -> Option<Ev> {
    let h = m.header();
    if h.message_type() != Type::Signal {
        return None;
    }
    let member = h.member()?.as_str().to_string();
    let b = m.body();
    if member == "NameOwnerChanged" {
        let (name, _, new): (String, String, String) = b.deserialize().ok()?;
        return (name == BUS_NAME).then(|| Ev::Daemon(!new.is_empty()));
    }
    if h.interface()?.as_str() != CHAR_IFACE {
        return None;
    }
    match member.as_str() {
        "Shown" => Some(Ev::Visible(true)),
        "Hidden" => Some(Ev::Visible(false)),
        "AskComplete" => {
            let (id, result, checked) = b.deserialize().ok()?;
            Some(Ev::AskComplete(id, result, checked))
        }
        "RequestComplete" => {
            let (id, status, reason) = b.deserialize().ok()?;
            Some(Ev::RequestComplete(id, status, reason))
        }
        _ => None,
    }
}

fn add_match(conn: &Connection, rule: &str) -> zbus::Result<()> {
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "AddMatch",
        &(rule,),
    )
    .map(|_| ())
}

/// Connects to the session bus and forwards its signals; reconnects when it goes away.
fn dbus_thread(tx: Sender<Ev>) {
    let mut logged = false;
    loop {
        let r = zbus::blocking::connection::Builder::session()
            .map(|b| b.method_timeout(Duration::from_secs(25)))
            .and_then(|b| b.build())
            .and_then(|conn| {
            // Before any call, so that no unicast AskComplete is missed.
            let it = MessageIterator::from(&conn);
            add_match(
                &conn,
                &format!(
                    "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{BUS_NAME}'"
                ),
            )?;
            add_match(&conn, &format!("type='signal',interface='{CHAR_IFACE}'"))?;
            Ok((conn, it))
        });
        match r {
            Ok((conn, it)) => {
                logged = false;
                eprintln!("bentwire-mqtt: connected to the session bus");
                if tx.send(Ev::Bus(Some(conn))).is_err() {
                    return;
                }
                for m in it {
                    let Ok(m) = m else { break };
                    if let Some(ev) = signal_event(&m)
                        && tx.send(ev).is_err()
                    {
                        return;
                    }
                }
                eprintln!("bentwire-mqtt: session bus connection lost");
                if tx.send(Ev::Bus(None)).is_err() {
                    return;
                }
            }
            Err(e) if !logged => {
                logged = true;
                eprintln!("bentwire-mqtt: session bus: {e}; retrying");
            }
            Err(_) => {}
        }
        std::thread::sleep(RETRY);
    }
}

/// Polls the MQTT connection (rumqttc reconnects on the next poll after an error).
fn mqtt_thread(
    mut connection: rumqttc::Connection,
    tx: Sender<Ev>,
    quitting: Arc<AtomicBool>,
    done: Sender<()>,
) {
    let mut logged = false;
    for ev in connection.iter() {
        match ev {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                logged = false;
                eprintln!("bentwire-mqtt: connected to the broker");
                let _ = tx.send(Ev::MqttUp);
            }
            Ok(Event::Incoming(Packet::Publish(p))) if !p.retain => {
                let _ = tx.send(Ev::Message(p.topic, p.payload.to_vec()));
            }
            Ok(Event::Outgoing(Outgoing::Disconnect)) => break,
            Ok(_) => {}
            Err(e) => {
                if quitting.load(Ordering::Relaxed) {
                    break;
                }
                if !logged {
                    logged = true;
                    eprintln!("bentwire-mqtt: broker: {e}; retrying");
                }
                let _ = tx.send(Ev::MqttDown);
                std::thread::sleep(RETRY);
            }
        }
    }
    let _ = done.send(());
}

/// Blocks SIGTERM and SIGINT in this thread and every thread spawned after it; the
/// returned set is for `sigwait`.
fn block_signals() -> libc::sigset_t {
    // SAFETY: plain libc calls on a local, initialised signal set.
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        libc::sigaddset(&mut set, libc::SIGTERM);
        libc::sigaddset(&mut set, libc::SIGINT);
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        set
    }
}

/// An open ask: had check boxes, and its AskComplete (result, checked) once seen.
type OpenAsk = (bool, Option<(i32, Vec<bool>)>);

struct Bridge {
    cfg: Config,
    mqtt: Client,
    mqtt_up: bool,
    bus: Option<Connection>,
    visible: bool,
    asks: HashMap<u32, OpenAsk>,
}

impl Bridge {
    fn topic(&self, t: &str) -> String {
        format!("{}/{t}", self.cfg.base_topic)
    }

    fn publish(&self, topic: String, retain: bool, payload: String) {
        if self.mqtt_up
            && let Err(e) = self
                .mqtt
                .try_publish(&topic, QoS::AtLeastOnce, retain, payload)
        {
            eprintln!("bentwire-mqtt: publish {topic}: {e}");
        }
    }

    fn publish_status(&self) {
        let s = if self.bus.is_some() {
            "online"
        } else {
            "offline"
        };
        self.publish(self.topic("status"), true, s.into());
    }

    fn publish_state(&self) {
        let s = if self.visible { "visible" } else { "hidden" };
        self.publish(self.topic("state"), true, s.into());
    }

    fn publish_answer(&self, checkboxes: bool, result: i32, checked: &[bool]) {
        if checkboxes {
            let v: Vec<&str> = checked.iter().map(|b| if *b { "1" } else { "0" }).collect();
            self.publish(self.topic("answer/checked"), false, v.join(","));
        }
        self.publish(self.topic("answer"), false, result_name(result));
    }

    /// Reads the daemon's Visible property (false when it does not run).
    fn sync_visible(&mut self) {
        self.visible = self.bus.as_ref().is_some_and(|c| {
            let owned = c
                .call_method(
                    Some("org.freedesktop.DBus"),
                    "/org/freedesktop/DBus",
                    Some("org.freedesktop.DBus"),
                    "NameHasOwner",
                    &(BUS_NAME,),
                )
                .ok()
                .and_then(|m| m.body().deserialize::<bool>().ok())
                .unwrap_or(false);
            owned
                && c.call_method(
                    Some(BUS_NAME),
                    CHAR_PATH,
                    Some("org.freedesktop.DBus.Properties"),
                    "Get",
                    &(CHAR_IFACE, "Visible"),
                )
                .ok()
                .and_then(|m| m.body().deserialize::<OwnedValue>().ok())
                .and_then(|v| bool::try_from(v).ok())
                .unwrap_or(false)
        });
        self.publish_state();
    }

    /// Ends every open ask with `none` (the daemon went away).
    fn drop_asks(&mut self) {
        for (_, (cb, _)) in std::mem::take(&mut self.asks) {
            self.publish_answer(cb, 0, &[]);
        }
    }

    fn on_mqtt_up(&mut self) {
        self.mqtt_up = true;
        for t in ["say", "play", "show", "hide", "ask"] {
            if let Err(e) = self.mqtt.try_subscribe(self.topic(t), QoS::AtLeastOnce) {
                eprintln!("bentwire-mqtt: subscribe: {e}");
            }
        }
        if self.cfg.discovery {
            for (topic, json) in discovery(&self.cfg) {
                self.publish(topic, true, json);
            }
        }
        self.publish_status();
        self.publish_state();
    }

    fn call(
        &self,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
    ) -> Result<u32, String> {
        let c = self.bus.as_ref().ok_or("no session bus")?;
        let reply = c
            .call_method(Some(BUS_NAME), CHAR_PATH, Some(CHAR_IFACE), method, body)
            .map_err(|e| match e {
                zbus::Error::MethodError(name, msg, _) => {
                    format!("{name}: {}", msg.unwrap_or_default())
                }
                e => e.to_string(),
            })?;
        reply.body().deserialize::<u32>().map_err(|e| e.to_string())
    }

    fn on_command(&mut self, cmd: Cmd) {
        let r = match cmd {
            Cmd::Say(t) => self.call("Speak", &(t,)),
            Cmd::Play(n) => self.call("Play", &(n,)),
            Cmd::Show => self.call("Show", &(false,)),
            Cmd::Hide => self.call("Hide", &(false,)),
            Cmd::Ask(spec) => {
                let checkboxes = spec.contains_key("checkboxes");
                let spec: HashMap<&str, Value> = spec
                    .iter()
                    .map(|(k, v)| {
                        let v = match v {
                            AskValue::S(s) => Value::from(s.clone()),
                            AskValue::L(l) => Value::from(l.clone()),
                        };
                        (k.as_str(), v)
                    })
                    .collect();
                let r = self.call("Ask", &(spec,));
                match &r {
                    Ok(id) => {
                        self.asks.insert(*id, (checkboxes, None));
                    }
                    Err(_) => self.publish_answer(checkboxes, 0, &[]),
                }
                r
            }
        };
        if let Err(e) = r {
            eprintln!("bentwire-mqtt: {e}");
        }
    }

    fn handle(&mut self, ev: Ev) {
        match ev {
            Ev::MqttUp => self.on_mqtt_up(),
            Ev::MqttDown => self.mqtt_up = false,
            Ev::Message(topic, payload) => {
                match parse_command(&self.cfg.base_topic, &topic, &payload) {
                    Some(Ok(cmd)) => self.on_command(cmd),
                    Some(Err(e)) => eprintln!("bentwire-mqtt: {e}"),
                    None => {}
                }
            }
            Ev::Bus(bus) => {
                self.bus = bus;
                self.drop_asks();
                self.publish_status();
                self.sync_visible();
            }
            Ev::Daemon(up) => {
                if !up {
                    self.drop_asks();
                }
                self.sync_visible();
            }
            Ev::Visible(v) => {
                self.visible = v;
                self.publish_state();
            }
            Ev::AskComplete(id, result, checked) => {
                if let Some(a) = self.asks.get_mut(&id) {
                    a.1 = Some((result, checked));
                }
            }
            Ev::RequestComplete(id, status, reason) => {
                if let Some((cb, done)) = self.asks.remove(&id) {
                    if status == 1 {
                        eprintln!("bentwire-mqtt: ask failed: {reason}");
                    }
                    let (result, checked) = done.unwrap_or((0, vec![]));
                    self.publish_answer(cb, result, &checked);
                }
            }
        }
    }
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return std::process::ExitCode::SUCCESS;
    }
    if args.iter().any(|a| a == "--version") {
        println!("bentwire-mqtt {VERSION}");
        return std::process::ExitCode::SUCCESS;
    }
    let path = config_path();
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => Some(t),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => {
            eprintln!("bentwire-mqtt: {}: {e}", path.display());
            return std::process::ExitCode::from(1);
        }
    };
    let cfg = match load_config(text.as_deref(), &args) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("bentwire-mqtt: {}: {e}", path.display());
            eprint!("{USAGE}");
            return std::process::ExitCode::from(1);
        }
    };
    let status = format!("{}/status", cfg.base_topic);
    let mut opts = MqttOptions::new(&cfg.client_id, &cfg.host, cfg.port);
    opts.set_keep_alive(Duration::from_secs(30))
        .set_last_will(LastWill::new(&status, "offline", QoS::AtLeastOnce, true));
    if let Some(u) = &cfg.username {
        opts.set_credentials(u, cfg.password.clone().unwrap_or_default());
    }
    eprintln!(
        "bentwire-mqtt {VERSION}: broker {}:{}, base topic {}",
        cfg.host, cfg.port, cfg.base_topic
    );

    let signals = block_signals();
    let (tx, rx) = channel();
    let (mqtt, connection) = Client::new(opts, 64);
    let quitting = Arc::new(AtomicBool::new(false));
    let (done_tx, done_rx) = channel();
    {
        let (tx, q) = (tx.clone(), quitting.clone());
        std::thread::spawn(move || mqtt_thread(connection, tx, q, done_tx));
    }
    std::thread::spawn(move || dbus_thread(tx));
    let mut b = Bridge {
        cfg,
        mqtt: mqtt.clone(),
        mqtt_up: false,
        bus: None,
        visible: false,
        asks: HashMap::new(),
    };
    // The bridge runs on its own thread so that a D-Bus call that hangs cannot block the
    // shutdown below.
    std::thread::spawn(move || {
        for ev in rx.iter() {
            b.handle(ev);
        }
    });
    let mut sig = 0;
    // SAFETY: `signals` is a valid signal set, `sig` a valid out pointer.
    unsafe { libc::sigwait(&signals, &mut sig) };
    // Shutdown: offline (retained), then a clean disconnect so the last will is not sent.
    quitting.store(true, Ordering::Relaxed);
    let _ = mqtt.try_publish(&status, QoS::AtLeastOnce, true, "offline");
    let _ = mqtt.try_disconnect();
    let _ = done_rx.recv_timeout(Duration::from_secs(3));
    eprintln!("bentwire-mqtt: stopped");
    std::process::ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_mqtt_1_topics_map_to_commands() {
        let p = |t: &str, m: &str| parse_command("home/bw", t, m.as_bytes());
        assert_eq!(
            p("home/bw/say", "Hello"),
            Some(Ok(Cmd::Say("Hello".into())))
        );
        assert_eq!(
            p("home/bw/play", " Wave\n"),
            Some(Ok(Cmd::Play("Wave".into())))
        );
        assert_eq!(p("home/bw/show", "PRESS"), Some(Ok(Cmd::Show)));
        assert_eq!(p("home/bw/hide", ""), Some(Ok(Cmd::Hide)));
        assert_eq!(p("home/bw/answer", "ok"), None);
        assert_eq!(p("home/bwx/say", "x"), None);
        assert_eq!(p("other/say", "x"), None);
        assert!(matches!(
            parse_command("bw", "bw/say", &[0xff]),
            Some(Err(_))
        ));
    }

    #[test]
    fn b_mqtt_1_ask_payloads() {
        let plain = parse_ask("Coffee?").unwrap();
        assert_eq!(
            plain,
            HashMap::from([("text".into(), AskValue::S("Coffee?".into()))])
        );
        let j = parse_ask(r#"{"text": "Coffee?", "buttons": ["Yes", "Later"], "animation": ""}"#)
            .unwrap();
        assert_eq!(
            j["buttons"],
            AskValue::L(vec!["Yes".into(), "Later".into()])
        );
        assert_eq!(j["animation"], AskValue::S(String::new()));
        assert_eq!(
            parse_ask(r#"{"buttons": "yes-no"}"#).unwrap()["buttons"],
            AskValue::S("yes-no".into())
        );
        assert!(parse_ask(r#"{"text": 1}"#).is_err());
        assert!(parse_ask(r#"{"labels": ["a", 2]}"#).is_err());
        assert!(parse_ask("{not json").is_err());
    }

    #[test]
    fn b_mqtt_1_answer_names_match_the_cli() {
        assert_eq!(result_name(-1), "ok");
        assert_eq!(result_name(-15), "yes-to-all");
        assert_eq!(result_name(-25), "button:1");
        assert_eq!(result_name(-27), "button:3");
        assert_eq!(result_name(2), "label:2");
        assert_eq!(result_name(0), "none");
    }

    #[test]
    fn b_mqtt_1_config_and_flags() {
        let c = load_config(None, &[]).unwrap();
        assert_eq!(
            (c.host.as_str(), c.port, c.base_topic.as_str()),
            ("localhost", 1883, "bentwire")
        );
        assert!(c.discovery && c.discovery_prefix == "homeassistant");
        let c = load_config(
            Some("host = \"broker\"\nport = 8883\nusername = \"u\"\npassword = \"p\"\nbase_topic = \"a/b/\"\ndiscovery = false\n"),
            &["--port".into(), "1884".into(), "--base".into(), "x".into()],
        )
        .unwrap();
        assert_eq!(
            (c.host.as_str(), c.port, c.base_topic.as_str()),
            ("broker", 1884, "x")
        );
        assert_eq!(c.password.as_deref(), Some("p"));
        assert!(!c.discovery);
        assert_eq!(
            load_config(Some("base_topic = \"a/b/\""), &[])
                .unwrap()
                .base_topic,
            "a/b"
        );
        assert!(load_config(Some("hots = \"x\""), &[]).is_err());
        for bad in [
            "host = \"h\"\npassword = \"s3cret\\q\"\n",
            "password = 12345\n",
            "password = s3cret\n",
            "\"password\" = 12345\n",
            "password = \"s3cret\"\npassword = \"s3cret2\"\n",
        ] {
            let e = load_config(Some(bad), &[]).err().unwrap();
            assert!(!e.contains("s3cret") && !e.contains("12345"), "{e}");
        }
        assert!(load_config(None, &["--base".into(), "a/#".into()]).is_err());
        assert!(load_config(None, &["--port".into()]).is_err());
        assert!(load_config(None, &["--bogus".into()]).is_err());
    }

    #[test]
    fn b_mqtt_1_discovery_configs() {
        let cfg = load_config(None, &["--base".into(), "home/bw".into()]).unwrap();
        let d = discovery(&cfg);
        let topics: Vec<&str> = d.iter().map(|(t, _)| t.as_str()).collect();
        assert_eq!(
            topics,
            [
                "homeassistant/notify/home_bw_say/config",
                "homeassistant/button/home_bw_show/config",
                "homeassistant/button/home_bw_hide/config",
                "homeassistant/sensor/home_bw_answer/config",
                "homeassistant/binary_sensor/home_bw_visible/config",
            ]
        );
        let v: Vec<serde_json::Value> = d
            .iter()
            .map(|(_, j)| serde_json::from_str(j).unwrap())
            .collect();
        assert_eq!(v[0]["command_topic"], "home/bw/say");
        assert_eq!(v[1]["command_topic"], "home/bw/show");
        assert_eq!(v[2]["command_topic"], "home/bw/hide");
        assert_eq!(v[3]["state_topic"], "home/bw/answer");
        assert_eq!(v[4]["state_topic"], "home/bw/state");
        assert_eq!(
            (v[4]["payload_on"].as_str(), v[4]["payload_off"].as_str()),
            (Some("visible"), Some("hidden"))
        );
        for (x, (t, _)) in v.iter().zip(&d) {
            assert_eq!(x["availability_topic"], "home/bw/status");
            assert_eq!(x["device"]["identifiers"], json!(["home_bw"]));
            assert_eq!(x["origin"]["name"], "bentwire-mqtt");
            assert!(t.contains(x["unique_id"].as_str().unwrap()));
        }
    }
}
