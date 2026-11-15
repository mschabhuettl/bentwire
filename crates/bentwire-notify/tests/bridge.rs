//! B-NOTIFY-1 end to end: a private dbus-daemon, a fake org.bentwire.Agent that records
//! Speak, a fake notification server, and bentwire-notify watching the bus.

use std::collections::HashMap;
use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};
use zbus::blocking::Connection;
use zbus::zvariant::Value;

struct Character(Sender<String>);

#[zbus::interface(name = "org.bentwire.Character1")]
impl Character {
    fn speak(&self, text: String) -> u32 {
        self.0.send(text).unwrap();
        1
    }
}

struct Notifications;

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        _app: String,
        _replaces: u32,
        _icon: String,
        _summary: String,
        _body: String,
        _actions: Vec<String>,
        _hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _timeout: i32,
    ) -> u32 {
        7
    }
}

struct Bus {
    dir: PathBuf,
    addr: String,
    daemon: Child,
}

impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.daemon.kill();
        let _ = self.daemon.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn bus(tag: &str) -> Bus {
    let base = std::env::var_os("CARGO_TARGET_TMPDIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(format!("notify-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("cfg/bentwire")).unwrap();
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
    let mut daemon = Command::new("dbus-daemon")
        .arg(format!("--config-file={}", conf.display()))
        .args(["--nofork", "--print-address=1"])
        .stdout(Stdio::piped())
        .spawn()
        .expect("dbus-daemon (needed for this test)");
    let mut line = String::new();
    BufReader::new(daemon.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    Bus {
        dir,
        addr: line.trim().to_string(),
        daemon,
    }
}

fn connect(addr: &str) -> Connection {
    zbus::blocking::connection::Builder::address(addr)
        .unwrap()
        .build()
        .unwrap()
}

/// Fake bentwired and notification server; returns their connections (kept alive) and
/// the Speak texts.
fn fakes(addr: &str) -> (Vec<Connection>, Receiver<String>) {
    let (tx, rx) = channel();
    let agent = zbus::blocking::connection::Builder::address(addr)
        .unwrap()
        .name("org.bentwire.Agent")
        .unwrap()
        .serve_at("/org/bentwire/Character", Character(tx))
        .unwrap()
        .build()
        .unwrap();
    let notifications = zbus::blocking::connection::Builder::address(addr)
        .unwrap()
        .name("org.freedesktop.Notifications")
        .unwrap()
        .serve_at("/org/freedesktop/Notifications", Notifications)
        .unwrap()
        .build()
        .unwrap();
    (vec![agent, notifications], rx)
}

fn notify(c: &Connection, app: &str, summary: &str, body: &str) {
    let hints: HashMap<&str, Value> = HashMap::new();
    let r = c.call_method(
        Some("org.freedesktop.Notifications"),
        "/org/freedesktop/Notifications",
        Some("org.freedesktop.Notifications"),
        "Notify",
        &(
            app,
            0u32,
            "",
            summary,
            body,
            Vec::<&str>::new(),
            hints,
            -1i32,
        ),
    );
    let (id,): (u32,) = r.unwrap().body().deserialize().unwrap();
    assert_eq!(id, 7);
}

#[test]
fn b_notify_1_bridge_speaks_filtered_notifications() {
    let b = bus("e2e");
    let (_keep, spoken) = fakes(&b.addr);
    // The file allows probe and mail and ignores mail; the flag replaces the ignore list.
    std::fs::write(
        b.dir.join("cfg/bentwire/notify.toml"),
        "only = [\"probe\", \"mail\"]\nignore = [\"mail\"]\n",
    )
    .unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_bentwire-notify"))
        .args(["--ignore", "spammy"])
        .env("DBUS_SESSION_BUS_ADDRESS", &b.addr)
        .env("XDG_CONFIG_HOME", b.dir.join("cfg"))
        .spawn()
        .unwrap();
    let client = connect(&b.addr);

    // Wait until the bridge watches the bus.
    let t = Instant::now();
    loop {
        assert!(t.elapsed() < Duration::from_secs(10), "bridge never spoke");
        notify(&client, "probe", "probe", "");
        if spoken.recv_timeout(Duration::from_millis(200)).is_ok() {
            break;
        }
    }
    while spoken.recv_timeout(Duration::from_millis(200)).is_ok() {}

    notify(&client, "Spammy", "ignored", "x");
    notify(&client, "Other", "not in only", "x");
    notify(
        &client,
        "MAIL",
        "New mail",
        "<b>Hi</b> &amp; <a href=\"https://x\">there</a>\n  you",
    );
    let next = spoken.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(next, "New mail: Hi & there you");
    // Still running after several notifications.
    notify(&client, "probe", "again", "");
    assert_eq!(
        spoken.recv_timeout(Duration::from_secs(5)).unwrap(),
        "again"
    );

    unsafe { libc::kill(bridge.id() as i32, libc::SIGTERM) };
    let t = Instant::now();
    let status = loop {
        if let Some(s) = bridge.try_wait().unwrap() {
            break s;
        }
        assert!(t.elapsed() < Duration::from_secs(5), "no exit on SIGTERM");
        std::thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(status.code(), Some(0));
}
