//! Template for your own bridge to bentwired (docs/dbus.md, "Writing a bridge"). It has
//! the structure of crates/bentwire-mqtt: one thread per event source, one thread that
//! owns the session bus connection, and a main loop that maps events to calls.
//!
//! The external source here is stdin, one command per line: `say TEXT`, `play NAME`,
//! `ask QUESTION` (Yes and No buttons; prints `answer <id> yes|no|none`). Replace
//! `source_thread` with your own (a socket, a broker, a file watch). End of input exits.
//!
//! Built in this workspace as `cargo run -p bentwire --example bridge-template`; on its
//! own it needs `zbus = "5"` and `serde`.

use std::collections::HashMap;
use std::sync::mpsc::{Sender, channel};
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::{Message, Type};
use zbus::zvariant::Value;

const BUS_NAME: &str = "org.bentwire.Agent";
const CHAR_PATH: &str = "/org/bentwire/Character";
const CHAR_IFACE: &str = "org.bentwire.Character1";
const RETRY: Duration = Duration::from_secs(2);

enum Ev {
    /// A line from the source; `None` when it ends.
    Line(Option<String>),
    /// Connected to the session bus, or lost it.
    Bus(Option<Connection>),
    /// bentwired got or lost the bus name (it quit, crashed or was restarted).
    Daemon(bool),
    AskComplete(u32, i32),
    RequestComplete(u32, u32, String),
}

fn source_thread(tx: Sender<Ev>) {
    for line in std::io::stdin().lines() {
        let Ok(line) = line else { break };
        if tx.send(Ev::Line(Some(line))).is_err() {
            return;
        }
    }
    let _ = tx.send(Ev::Line(None));
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

fn signal_event(m: &Message) -> Option<Ev> {
    let h = m.header();
    if h.message_type() != Type::Signal {
        return None;
    }
    let b = m.body();
    match h.member()?.as_str() {
        "NameOwnerChanged" => {
            let (name, _, new): (String, String, String) = b.deserialize().ok()?;
            (name == BUS_NAME).then(|| Ev::Daemon(!new.is_empty()))
        }
        "AskComplete" => {
            let (id, result, _checked): (u32, i32, Vec<bool>) = b.deserialize().ok()?;
            Some(Ev::AskComplete(id, result))
        }
        "RequestComplete" => {
            let (id, status, reason) = b.deserialize().ok()?;
            Some(Ev::RequestComplete(id, status, reason))
        }
        _ => None,
    }
}

/// Owns the session bus connection: forwards signals, reconnects every RETRY.
fn dbus_thread(tx: Sender<Ev>) {
    loop {
        let r = Connection::session().and_then(|conn| {
            // Before any call, so that no unicast RequestComplete or AskComplete is missed.
            let it = MessageIterator::from(&conn);
            add_match(
                &conn,
                &format!(
                    "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{BUS_NAME}'"
                ),
            )?;
            // Only needed for broadcast signals (Click, Shown, ...); unicast ones arrive anyway.
            add_match(&conn, &format!("type='signal',interface='{CHAR_IFACE}'"))?;
            Ok((conn, it))
        });
        match r {
            Ok((conn, it)) => {
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
                if tx.send(Ev::Bus(None)).is_err() {
                    return;
                }
            }
            Err(e) => eprintln!("bridge: session bus: {e}; retrying"),
        }
        std::thread::sleep(RETRY);
    }
}

/// A Character1 call. A call while bentwired does not run starts it (D-Bus activation).
fn call(
    bus: Option<&Connection>,
    method: &str,
    body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
) -> Result<u32, String> {
    let reply = bus
        .ok_or("no session bus")?
        .call_method(Some(BUS_NAME), CHAR_PATH, Some(CHAR_IFACE), method, body)
        .map_err(|e| match e {
            // The D-Bus error name, e.g. org.bentwire.Error.UnknownAnimation.
            zbus::Error::MethodError(name, msg, _) => {
                format!("{name}: {}", msg.unwrap_or_default())
            }
            e => e.to_string(),
        })?;
    reply.body().deserialize::<u32>().map_err(|e| e.to_string())
}

fn answer(id: u32, result: i32) {
    let name = match result {
        -3 => "yes",
        -4 => "no",
        _ => "none",
    };
    println!("answer {id} {name}");
}

fn main() {
    let (tx, rx) = channel();
    let mut source = Some(tx.clone());
    std::thread::spawn(move || dbus_thread(tx));

    let mut bus: Option<Connection> = None;
    // Open asks and their AskComplete result once seen.
    let mut asks: HashMap<u32, Option<i32>> = HashMap::new();
    for ev in rx {
        match ev {
            Ev::Line(None) => break,
            Ev::Line(Some(line)) => {
                let (cmd, arg) = line.split_once(' ').unwrap_or((line.as_str(), ""));
                let r = match cmd {
                    "say" => call(bus.as_ref(), "Speak", &(arg,)),
                    "play" => call(bus.as_ref(), "Play", &(arg.trim(),)),
                    "ask" => {
                        let spec = HashMap::from([
                            ("text", Value::from(arg)),
                            ("buttons", Value::from("yes-no")),
                        ]);
                        let r = call(bus.as_ref(), "Ask", &(spec,));
                        if let Ok(id) = r {
                            asks.insert(id, None);
                        }
                        r
                    }
                    _ => Err(format!("unknown command {cmd:?}")),
                };
                if let Err(e) = r {
                    eprintln!("bridge: {e}");
                }
            }
            Ev::Bus(b) => {
                // Read the source only once connected, so that no early command is lost.
                if b.is_some()
                    && let Some(tx) = source.take()
                {
                    std::thread::spawn(move || source_thread(tx));
                }
                bus = b;
                // A new connection gets no signals for requests of the old one.
                for (id, _) in asks.drain() {
                    answer(id, 0);
                }
            }
            Ev::Daemon(up) => {
                if !up {
                    for (id, _) in asks.drain() {
                        answer(id, 0);
                    }
                }
            }
            Ev::AskComplete(id, result) => {
                if let Some(a) = asks.get_mut(&id) {
                    *a = Some(result);
                }
            }
            // The last signal of every request (B-Q-2); a failed ask has no AskComplete.
            Ev::RequestComplete(id, status, reason) => {
                if let Some(result) = asks.remove(&id) {
                    if status == 1 {
                        eprintln!("bridge: ask {id} failed: {reason}");
                    }
                    answer(id, result.unwrap_or(0));
                }
            }
        }
    }
    // Clean exit: nothing to undo here. A bridge with external state (an MQTT last will,
    // a socket file) blocks SIGTERM and SIGINT and cleans up after sigwait, as
    // bentwire-mqtt does.
}
