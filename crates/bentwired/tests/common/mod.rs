//! Harness for the D-Bus end-to-end tests (spec T-5): a private dbus-daemon per test,
//! `bentwired --headless` on it, clients, the CLI and org.bentwire.Test1.

#![allow(dead_code)]

use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::{OwnedValue, Value as ZValue};

pub const CHAR: &str = "/org/bentwire/Character";
pub const AGENT: &str = "/org/bentwire/Agent";
pub const TEST: &str = "/org/bentwire/Test";
pub const CHAR_IF: &str = "org.bentwire.Character1";
pub const AGENT_IF: &str = "org.bentwire.Agent1";
pub const TEST_IF: &str = "org.bentwire.Test1";

pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/characters")
}

pub fn daemon_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_bentwired"))
}

/// The `bentwire` CLI, built on first use (it lives in another package).
pub fn cli_bin() -> PathBuf {
    static ONCE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| {
        let p = daemon_bin().with_file_name("bentwire");
        let st = Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "bentwire", "--bin", "bentwire"])
            .current_dir(repo())
            .status()
            .expect("cargo build bentwire");
        assert!(st.success(), "building the bentwire CLI failed");
        assert!(p.exists(), "{} missing", p.display());
        p
    })
    .clone()
}

pub fn tempdir(tag: &str) -> PathBuf {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    static N: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    // Short: the bus socket path must stay below 108 bytes.
    let d = base.join(format!("e2e-{tag}-{}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// A private session bus plus an isolated XDG environment.
pub struct Env {
    pub dir: PathBuf,
    pub addr: String,
    bus: Child,
    pub daemon: Option<Child>,
    pub conn: Connection,
}

impl Drop for Env {
    fn drop(&mut self) {
        if let Some(d) = &mut self.daemon {
            let _ = d.kill();
            let _ = d.wait();
        }
        let _ = self.bus.kill();
        let _ = self.bus.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Env {
    pub fn new(tag: &str) -> Env {
        let dir = tempdir(tag);
        std::fs::create_dir_all(dir.join("services")).unwrap();
        for d in ["cfg", "state", "data", "run", "bin"] {
            std::fs::create_dir_all(dir.join(d)).unwrap();
        }
        let conf = dir.join("bus.conf");
        std::fs::write(
            &conf,
            format!(
                r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}/bus</listen>
  <servicedir>{}/services</servicedir>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
                dir.display(),
                dir.display()
            ),
        )
        .unwrap();
        let mut bus = Command::new("dbus-daemon")
            .arg(format!("--config-file={}", conf.display()))
            .args(["--nofork", "--print-address=1"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("dbus-daemon (needed for the e2e tests)");
        let mut line = String::new();
        BufReader::new(bus.stdout.take().unwrap())
            .read_line(&mut line)
            .unwrap();
        let addr = line.trim().to_string();
        let conn = connect(&addr);
        Env {
            dir,
            addr,
            bus,
            daemon: None,
            conn,
        }
    }

    pub fn envs(&self) -> Vec<(&'static str, String)> {
        let d = self.dir.display();
        vec![
            ("DBUS_SESSION_BUS_ADDRESS", self.addr.clone()),
            ("HOME", format!("{d}")),
            ("XDG_CONFIG_HOME", format!("{d}/cfg")),
            ("XDG_STATE_HOME", format!("{d}/state")),
            ("XDG_DATA_HOME", format!("{d}/data")),
            ("XDG_DATA_DIRS", format!("{d}/nonexistent")),
            ("XDG_RUNTIME_DIR", format!("{d}/run")),
            ("LANG", "C".into()),
            ("LC_ALL", String::new()),
            ("LC_MESSAGES", String::new()),
            ("RUST_LOG", "warn,zbus=error,tracing=error".into()),
            (
                "PATH",
                format!("{d}/bin:{}", std::env::var("PATH").unwrap_or_default()),
            ),
        ]
    }

    pub fn daemon_cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(daemon_bin());
        c.args(["--headless", "--seed", "1"]).args(args);
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c
    }

    /// Start bentwired with the fixture characters (plus `args`) and wait for its name.
    pub fn start(&mut self, args: &[&str]) {
        let fx = fixtures();
        let mut all = vec!["--characters", fx.to_str().unwrap()];
        all.extend_from_slice(args);
        self.start_raw(&all);
    }

    pub fn start_raw(&mut self, args: &[&str]) {
        let child = self.daemon_cmd(args).spawn().expect("bentwired");
        self.daemon = Some(child);
        let t = Instant::now();
        while !self.name_owned() {
            assert!(
                t.elapsed() < Duration::from_secs(10),
                "bentwired did not start"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    pub fn name_owned(&self) -> bool {
        self.conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &("org.bentwire.Agent",),
            )
            .ok()
            .and_then(|m| m.body().deserialize::<bool>().ok())
            .unwrap_or(false)
    }

    pub fn cli(&self, args: &[&str]) -> Output {
        let mut c = Command::new(cli_bin());
        c.args(args);
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c.output().expect("bentwire")
    }

    /// The CLI with extra environment variables.
    pub fn cli_env(&self, args: &[&str], extra: &[(&str, String)]) -> Output {
        let mut c = Command::new(cli_bin());
        c.args(args);
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        for (k, v) in extra {
            c.env(k, v);
        }
        c.output().expect("bentwire")
    }

    /// Write the state file (B-CFG-2) before the daemon starts.
    pub fn seed_state(&self, text: &str) {
        let d = self.dir.join("state/bentwire");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("state.toml"), text).unwrap();
    }

    pub fn state(&self) -> String {
        std::fs::read_to_string(self.dir.join("state/bentwire/state.toml")).unwrap_or_default()
    }

    /// Run the CLI in the background (for --wait and ask).
    pub fn cli_bg(&self, args: &[&str]) -> Child {
        let mut c = Command::new(cli_bin());
        c.args(args).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (k, v) in self.envs() {
            c.env(k, v);
        }
        c.spawn().expect("bentwire")
    }

    /// Wait for a background CLI while advancing the fake clock in 100 ms steps
    /// (at most 60 s of fake time).
    pub fn finish_advancing(&self, mut c: Child) -> (i32, String, String) {
        std::thread::sleep(Duration::from_millis(200));
        for _ in 0..600 {
            if c.try_wait().unwrap().is_some() {
                return finish(c);
            }
            self.advance(100);
            std::thread::sleep(Duration::from_millis(5));
        }
        let _ = c.kill();
        let r = finish(c);
        panic!("CLI did not finish: {r:?}");
    }

    pub fn client(&self) -> Client {
        Client::new(&self.addr)
    }

    pub fn test<B>(&self, method: &str, body: &B) -> Message
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.conn
            .call_method(
                Some("org.bentwire.Agent"),
                TEST,
                Some(TEST_IF),
                method,
                body,
            )
            .unwrap_or_else(|e| panic!("Test1.{method}: {e}"))
    }

    pub fn advance(&self, ms: u32) {
        self.test("Advance", &(ms,));
    }

    pub fn pointer(&self, kind: &str, button: u32, x: i32, y: i32) {
        self.test("Pointer", &(kind, button, x, y));
    }

    pub fn click(&self, button: u32, x: i32, y: i32) {
        self.pointer("press", button, x, y);
        self.pointer("release", button, x, y);
    }

    pub fn key(&self, k: &str) {
        self.test("Key", &(k,));
    }

    pub fn snapshot(&self) -> Value {
        let m = self.test("Snapshot", &());
        let s: String = m.body().deserialize().unwrap();
        serde_json::from_str(&s).unwrap()
    }

    pub fn prop(&self, path: &str, iface: &str, name: &str) -> OwnedValue {
        let m = self
            .conn
            .call_method(
                Some("org.bentwire.Agent"),
                path,
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &(iface, name),
            )
            .unwrap();
        m.body().deserialize::<OwnedValue>().unwrap()
    }

    pub fn character(&self) -> String {
        String::try_from(self.prop(AGENT, AGENT_IF, "Character")).unwrap()
    }

    /// Wait for the daemon to exit; returns its status code.
    pub fn wait_exit(&mut self, secs: u64) -> Option<i32> {
        let d = self.daemon.as_mut()?;
        let t = Instant::now();
        loop {
            if let Some(s) = d.try_wait().unwrap() {
                return s.code();
            }
            if t.elapsed() > Duration::from_secs(secs) {
                return None;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

pub fn copy_dir(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).unwrap();
    for e in std::fs::read_dir(from).unwrap() {
        let p = e.unwrap().path();
        let dst = to.join(p.file_name().unwrap());
        if p.is_dir() {
            copy_dir(&p, &dst);
        } else {
            std::fs::copy(&p, &dst).unwrap();
        }
    }
}

pub fn connect(addr: &str) -> Connection {
    zbus::blocking::connection::Builder::address(addr)
        .unwrap()
        .build()
        .unwrap()
}

/// A recorded message: kind (`signal`, `return`, `error`), member, reply serial, destination
/// and decoded arguments.
#[derive(Debug, Clone)]
pub struct Rec {
    pub kind: &'static str,
    pub member: String,
    pub reply_serial: Option<u32>,
    pub dest: Option<String>,
    pub args: Value,
}

/// A D-Bus client that records every message it receives, in order.
pub struct Client {
    pub conn: Connection,
    rx: Receiver<Rec>,
    pub seen: Vec<Rec>,
}

pub fn decode_signal(member: &str, m: &Message) -> Value {
    let b = m.body();
    match member {
        "RequestStart" => json!([b.deserialize::<(u32,)>().unwrap().0]),
        "RequestComplete" => {
            let (a, c, d): (u32, u32, String) = b.deserialize().unwrap();
            json!([a, c, d])
        }
        "AskComplete" => {
            let (a, c, d): (u32, i32, Vec<bool>) = b.deserialize().unwrap();
            json!([a, c, d])
        }
        "Bookmark" => json!([b.deserialize::<(i32,)>().unwrap().0]),
        "Click" | "DblClick" => {
            let (a, c, d, e, f): (u32, u32, i32, i32, bool) = b.deserialize().unwrap();
            json!([a, c, d, e, f])
        }
        "DragStart" | "DragComplete" => {
            let (a, c, d, e): (u32, u32, i32, i32) = b.deserialize().unwrap();
            json!([a, c, d, e])
        }
        "Move" => {
            let (a, c, d): (i32, i32, u32) = b.deserialize().unwrap();
            json!([a, c, d])
        }
        "Shown" | "Hidden" => json!([b.deserialize::<(u32,)>().unwrap().0]),
        _ => json!([]),
    }
}

impl Client {
    pub fn new(addr: &str) -> Client {
        let conn = connect(addr);
        // Broadcast Character1 signals need a match rule; unicast ones arrive anyway.
        conn.call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "AddMatch",
            &("type='signal',interface='org.bentwire.Character1'",),
        )
        .unwrap();
        let it = MessageIterator::from(&conn);
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            for m in it.flatten() {
                let h = m.header();
                let member = h.member().map(|x| x.to_string()).unwrap_or_default();
                let rec = match h.message_type() {
                    Type::Signal if h.interface().map(|i| i.as_str()) == Some(CHAR_IF) => Rec {
                        kind: "signal",
                        args: decode_signal(&member, &m),
                        member,
                        reply_serial: None,
                        dest: h.destination().map(|d| d.to_string()),
                    },
                    Type::MethodReturn => Rec {
                        kind: "return",
                        member,
                        reply_serial: h.reply_serial().map(|s| s.get()),
                        dest: None,
                        args: Value::Null,
                    },
                    Type::Error => Rec {
                        kind: "error",
                        member: h.error_name().map(|e| e.to_string()).unwrap_or_default(),
                        reply_serial: h.reply_serial().map(|s| s.get()),
                        dest: None,
                        args: Value::Null,
                    },
                    _ => continue,
                };
                if tx.send(rec).is_err() {
                    break;
                }
            }
        });
        Client {
            conn,
            rx,
            seen: Vec::new(),
        }
    }

    pub fn unique_name(&self) -> String {
        self.conn.unique_name().unwrap().to_string()
    }

    /// Send a Character1 call without waiting; returns its serial.
    pub fn send<B>(&self, method: &str, body: &B) -> u32
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        let m = Message::method_call(CHAR, method)
            .unwrap()
            .destination("org.bentwire.Agent")
            .unwrap()
            .interface(CHAR_IF)
            .unwrap()
            .build(body)
            .unwrap();
        let serial = m.primary_header().serial_num().get();
        self.conn.send(&m).unwrap();
        serial
    }

    /// A Character1 call returning a request id.
    pub fn call_id<B>(&self, method: &str, body: &B) -> Result<u32, String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        match self.conn.call_method(
            Some("org.bentwire.Agent"),
            CHAR,
            Some(CHAR_IF),
            method,
            body,
        ) {
            Ok(m) => Ok(m.body().deserialize::<u32>().unwrap()),
            Err(zbus::Error::MethodError(n, _, _)) => Err(n.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn call_unit<B>(
        &self,
        path: &str,
        iface: &str,
        method: &str,
        body: &B,
    ) -> Result<(), String>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        match self
            .conn
            .call_method(Some("org.bentwire.Agent"), path, Some(iface), method, body)
        {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(n, _, _)) => Err(n.to_string()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn ask(&self, spec: &[(&str, ZValue<'_>)]) -> Result<u32, String> {
        let m: HashMap<&str, &ZValue> = spec.iter().map(|(k, v)| (*k, v)).collect();
        self.call_id("Ask", &(m,))
    }

    /// Collect what has arrived (waiting `ms` for stragglers).
    pub fn drain(&mut self, ms: u64) -> &[Rec] {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(r) => self.seen.push(r),
                Err(_) => break,
            }
        }
        &self.seen
    }

    /// Wait until a signal matching `pred` arrives (10 s at most).
    pub fn wait_for(&mut self, what: &str, pred: impl Fn(&Rec) -> bool) -> Rec {
        if let Some(r) = self.seen.iter().find(|r| pred(r)) {
            return r.clone();
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.rx.recv_timeout(left) {
                Ok(r) => {
                    self.seen.push(r.clone());
                    if pred(&r) {
                        return r;
                    }
                }
                Err(_) => panic!("timed out waiting for {what}; seen: {:#?}", self.signals()),
            }
        }
    }

    pub fn signals(&self) -> Vec<(String, Value)> {
        self.seen
            .iter()
            .filter(|r| r.kind == "signal")
            .map(|r| (r.member.clone(), r.args.clone()))
            .collect()
    }

    pub fn members(&self) -> Vec<String> {
        self.signals().into_iter().map(|s| s.0).collect()
    }

    /// RequestComplete of request `id`: (status, reason).
    pub fn complete(&mut self, id: u32) -> (u64, String) {
        let r = self.wait_for(&format!("RequestComplete({id})"), |r| {
            r.member == "RequestComplete" && r.args[0] == id
        });
        (
            r.args[1].as_u64().unwrap(),
            r.args[2].as_str().unwrap().into(),
        )
    }
}

pub fn out(o: &Output) -> (i32, String, String) {
    (
        o.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

/// Block until a background `--wait` CLI has printed its request id (the call has been
/// answered), and return the id. Its stdout is consumed.
pub fn started(c: &mut Child) -> u32 {
    let mut line = String::new();
    BufReader::new(c.stdout.take().expect("piped stdout"))
        .read_line(&mut line)
        .unwrap();
    line.trim()
        .parse()
        .unwrap_or_else(|_| panic!("expected a request id, got {line:?}"))
}

/// Wait for a background CLI and return (code, stdout, stderr).
pub fn finish(c: Child) -> (i32, String, String) {
    out(&c.wait_with_output().unwrap())
}
