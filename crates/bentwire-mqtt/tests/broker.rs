//! B-MQTT-1 end to end against a real broker: `scripts/mqtt-check.sh` starts mosquitto and
//! sets BENTWIRE_MQTT_PORT, BENTWIRE_MQTT_USERNAME and BENTWIRE_MQTT_PASSWORD. This test
//! starts a private dbus-daemon, `bentwired --headless` (fake clock, driven through
//! org.bentwire.Test1) and the bridge.

use rumqttc::{Client, Event, MqttOptions, Packet, QoS};
use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};
use zbus::blocking::Connection;

fn env(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| panic!("NOT RUN: set {name} (scripts/mqtt-check.sh)"))
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap()
}

fn daemon_bin() -> PathBuf {
    let st = Command::new(env!("CARGO"))
        .args(["build", "-q", "-p", "bentwired", "--bin", "bentwired"])
        .current_dir(repo())
        .status()
        .expect("cargo build bentwired");
    assert!(st.success(), "building bentwired failed");
    let p = PathBuf::from(env!("CARGO_BIN_EXE_bentwire-mqtt")).with_file_name("bentwired");
    assert!(p.exists(), "{} missing", p.display());
    p
}

struct Kill(Child);
impl Drop for Kill {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

struct Rig {
    dir: PathBuf,
    addr: String,
    conn: Connection,
    daemon: Option<Kill>,
    msgs: Receiver<(String, String)>,
    _bus: Kill,
}

impl Drop for Rig {
    fn drop(&mut self) {
        self.daemon.take();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Rig {
    fn envs(&self) -> Vec<(&'static str, String)> {
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
            ("RUST_LOG", "warn,zbus=error,tracing=error".into()),
        ]
    }

    fn start_daemon(&mut self, bin: &Path) {
        let fx = repo().join("crates/bentwired/tests/fixtures/characters");
        let mut c = Command::new(bin);
        c.args(["--headless", "--seed", "1", "--characters"])
            .arg(fx);
        c.envs(self.envs());
        self.daemon = Some(Kill(c.spawn().expect("bentwired")));
        let t = Instant::now();
        while !self.name_owned() {
            assert!(
                t.elapsed() < Duration::from_secs(10),
                "bentwired did not start"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    fn name_owned(&self) -> bool {
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

    fn test<B>(&self, method: &str, body: &B) -> zbus::message::Message
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
    {
        self.conn
            .call_method(
                Some("org.bentwire.Agent"),
                "/org/bentwire/Test",
                Some("org.bentwire.Test1"),
                method,
                body,
            )
            .unwrap_or_else(|e| panic!("Test1.{method}: {e}"))
    }

    fn advance(&self, ms: u32) {
        if self.daemon.is_some() {
            self.test("Advance", &(ms,));
        }
    }

    fn snapshot(&self) -> serde_json::Value {
        let s: String = self.test("Snapshot", &()).body().deserialize().unwrap();
        serde_json::from_str(&s).unwrap()
    }

    /// Waits for a message on `topic` with `payload`, advancing the fake clock meanwhile.
    fn expect(&self, topic: &str, payload: &str) {
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(20) {
            if let Ok((tp, p)) = self.msgs.recv_timeout(Duration::from_millis(30)) {
                if tp == topic && p == payload {
                    return;
                }
                continue;
            }
            self.advance(100);
        }
        panic!("no {topic} = {payload:?}");
    }

    /// Advances until `f(snapshot)` holds.
    fn until(&self, what: &str, f: impl Fn(&serde_json::Value) -> bool) {
        for _ in 0..100 {
            self.advance(100);
            if f(&self.snapshot()) {
                return;
            }
        }
        panic!("{what} did not happen: {}", self.snapshot());
    }
}

fn private_bus(dir: &Path) -> (Kill, String) {
    let conf = dir.join("bus.conf");
    std::fs::write(
        &conf,
        format!(
            r#"<!DOCTYPE busconfig PUBLIC "-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd">
<busconfig>
  <type>session</type>
  <listen>unix:path={}/bus</listen>
  <policy context="default">
    <allow send_destination="*" eavesdrop="true"/>
    <allow eavesdrop="true"/>
    <allow own="*"/>
  </policy>
</busconfig>"#,
            dir.display()
        ),
    )
    .unwrap();
    let mut bus = Command::new("dbus-daemon")
        .arg(format!("--config-file={}", conf.display()))
        .args(["--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("dbus-daemon");
    let mut line = String::new();
    BufReader::new(bus.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    (Kill(bus), line.trim().to_string())
}

/// The retained messages a new subscriber gets for `filters` (collected for one second).
fn retained(port: u16, user: &str, pass: &str, filters: &[&str]) -> HashMap<String, String> {
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let mut o = MqttOptions::new(format!("kept{}-{n}", std::process::id()), "127.0.0.1", port);
    o.set_credentials(user, pass);
    let (c, mut conn) = Client::new(o, 16);
    for f in filters {
        c.subscribe(*f, QoS::AtLeastOnce).unwrap();
    }
    let mut kept = HashMap::new();
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(1) {
        if let Ok(Ok(Event::Incoming(Packet::Publish(p)))) =
            conn.recv_timeout(Duration::from_millis(100))
        {
            assert!(
                p.retain,
                "{} delivered on subscribe must be retained",
                p.topic
            );
            kept.insert(p.topic, String::from_utf8_lossy(&p.payload).to_string());
        }
    }
    kept
}

#[test]
#[ignore = "needs mosquitto"]
fn b_mqtt_1_round_trips_with_a_broker() {
    let port: u16 = env("BENTWIRE_MQTT_PORT").parse().unwrap();
    let user = env("BENTWIRE_MQTT_USERNAME");
    let pass = env("BENTWIRE_MQTT_PASSWORD");
    let bin = daemon_bin();
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("mqtt-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    for d in ["cfg/bentwire", "state", "data", "run"] {
        std::fs::create_dir_all(dir.join(d)).unwrap();
    }
    let (bus, addr) = private_bus(&dir);
    let conn = zbus::blocking::connection::Builder::address(addr.as_str())
        .unwrap()
        .build()
        .unwrap();

    // Test client: sees everything under the base topic and the discovery prefix.
    let tag = std::process::id();
    let (topic, prefix) = (format!("bwtest{tag}"), format!("hatest{tag}"));
    let mut o = MqttOptions::new(format!("probe{tag}"), "127.0.0.1", port);
    o.set_credentials(&user, &pass);
    let (probe, mut pconn) = Client::new(o, 64);
    probe
        .subscribe(format!("{topic}/#"), QoS::AtLeastOnce)
        .unwrap();
    let (tx, msgs) = channel();
    std::thread::spawn(move || {
        for ev in pconn.iter() {
            match ev {
                Ok(Event::Incoming(Packet::Publish(p))) => {
                    let s = String::from_utf8_lossy(&p.payload).to_string();
                    if tx.send((p.topic, s)).is_err() {
                        return;
                    }
                }
                Ok(_) => {}
                Err(e) => panic!("probe: {e}"),
            }
        }
    });

    let mut r = Rig {
        dir: dir.clone(),
        addr,
        conn,
        daemon: None,
        msgs,
        _bus: bus,
    };
    r.start_daemon(&bin);
    r.advance(1000);

    std::fs::write(
        dir.join("cfg/bentwire/mqtt.toml"),
        format!(
            "username = \"{user}\"\npassword = \"{pass}\"\nbase_topic = \"{topic}\"\n\
             discovery_prefix = \"{prefix}\"\nclient_id = \"bridge{tag}\"\n"
        ),
    )
    .unwrap();
    // Outside `dir`, so it survives a failure.
    let log_path = base.join(format!("mqtt-{tag}-bridge.log"));
    eprintln!("bridge log file: {}", log_path.display());
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_bentwire-mqtt"));
    bridge
        .args(["--host", "127.0.0.1", "--port", &port.to_string()])
        .envs(r.envs())
        .stderr(std::fs::File::create(&log_path).unwrap());
    let mut bridge = Kill(bridge.spawn().expect("bentwire-mqtt"));

    let t = |s: &str| format!("{topic}/{s}");
    r.expect(&t("status"), "online");
    r.expect(&t("state"), "visible");
    let send = |s: &str, payload: &str| {
        probe
            .publish(t(s), QoS::AtLeastOnce, false, payload.as_bytes().to_vec())
            .unwrap()
    };
    let visible = |r: &Rig| {
        let m = r
            .conn
            .call_method(
                Some("org.bentwire.Agent"),
                "/org/bentwire/Character",
                Some("org.freedesktop.DBus.Properties"),
                "Get",
                &("org.bentwire.Character1", "Visible"),
            )
            .unwrap();
        bool::try_from(
            m.body()
                .deserialize::<zbus::zvariant::OwnedValue>()
                .unwrap(),
        )
        .unwrap()
    };

    // hide / show
    send("hide", "PRESS");
    r.expect(&t("state"), "hidden");
    assert!(!visible(&r));
    send("show", "PRESS");
    r.expect(&t("state"), "visible");
    assert!(visible(&r));

    // say: the Agent balloon shows the text.
    send("say", "Hello from MQTT");
    r.until("say balloon", |s| {
        s["agent_balloon_lines"]
            .as_array()
            .is_some_and(|l| l.iter().any(|x| x.as_str().unwrap_or("").contains("Hello")))
    });

    // ask with custom buttons: Enter activates the first one.
    send(
        "ask",
        r#"{"text": "Coffee?", "buttons": ["Yes please", "Later"], "animation": ""}"#,
    );
    r.until("ask balloon", |s| !s["office_balloon"].is_null());
    r.test("Key", &("Enter",));
    r.expect(&t("answer"), "button:1");

    // ask as plain text: the default OK button.
    send("ask", "Ready?");
    r.until("ask balloon", |s| !s["office_balloon"].is_null());
    r.test("Key", &("Enter",));
    r.expect(&t("answer"), "ok");

    // ask while hidden fails: answer none.
    send("hide", "");
    r.expect(&t("state"), "hidden");
    send("ask", "Nobody sees this");
    r.expect(&t("answer"), "none");
    send("show", "");
    r.expect(&t("state"), "visible");

    // Retained: the discovery configs, status and state, but not the answer.
    let kept = retained(
        port,
        &user,
        &pass,
        &[&format!("{topic}/#"), &format!("{prefix}/#")],
    );
    assert_eq!(kept.get(&t("status")).map(String::as_str), Some("online"));
    assert_eq!(kept.get(&t("state")).map(String::as_str), Some("visible"));
    assert!(!kept.contains_key(&t("answer")), "{kept:?}");
    for (component, object, key, value) in [
        ("notify", "say", "command_topic", t("say")),
        ("button", "show", "command_topic", t("show")),
        ("button", "hide", "command_topic", t("hide")),
        ("sensor", "answer", "state_topic", t("answer")),
        ("binary_sensor", "visible", "state_topic", t("state")),
    ] {
        let want = format!("{prefix}/{component}/{topic}_{object}/config");
        let v: serde_json::Value = serde_json::from_str(
            kept.get(&want)
                .unwrap_or_else(|| panic!("no {want}: {kept:?}")),
        )
        .unwrap();
        assert_eq!(v[key], value);
        assert_eq!(v["unique_id"], format!("{topic}_{object}"));
        assert_eq!(v["availability_topic"], t("status"));
    }

    // The daemon goes away and comes back (visible, from its state file).
    r.advance(2000);
    r.daemon.take();
    r.expect(&t("state"), "hidden");
    r.start_daemon(&bin);
    r.expect(&t("state"), "visible");

    // SIGTERM: offline (retained), exit 0, no password in the log.
    let pid = bridge.0.id().to_string();
    Command::new("kill").args(["-TERM", &pid]).status().unwrap();
    r.expect(&t("status"), "offline");
    let st = bridge.0.wait().unwrap();
    assert_eq!(st.code(), Some(0));
    let kept = retained(port, &user, &pass, &[&t("status")]);
    assert_eq!(kept.get(&t("status")).map(String::as_str), Some("offline"));
    let log = std::fs::read_to_string(&log_path).unwrap();
    let _ = std::fs::remove_file(&log_path);
    assert!(!log.contains(&pass), "password in the log: {log}");
    eprintln!("bridge log:\n{log}");
}
