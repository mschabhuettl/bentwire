//! bentwire D-Bus example in Rust with zbus 5 (blocking API), docs/dbus.md.
//! Speaks, plays an animation, asks a question with two buttons and prints the answer,
//! then prints Click, Shown and Hidden until Ctrl+C.
//!
//! Built in this workspace as `cargo run -p bentwire --example dbus-client`; on its own it
//! needs only `zbus = "5"`.

use std::collections::HashMap;
use zbus::zvariant::Value;

#[zbus::proxy(
    interface = "org.bentwire.Character1",
    default_service = "org.bentwire.Agent",
    default_path = "/org/bentwire/Character"
)]
trait Character {
    fn show(&self, fast: bool) -> zbus::Result<u32>;
    fn play(&self, name: &str) -> zbus::Result<u32>;
    fn speak(&self, text: &str) -> zbus::Result<u32>;
    fn ask(&self, spec: HashMap<&str, Value<'_>>) -> zbus::Result<u32>;

    #[zbus(signal)]
    fn request_complete(&self, id: u32, status: u32, reason: String) -> zbus::Result<()>;
    #[zbus(signal)]
    fn ask_complete(&self, id: u32, result: i32, checked: Vec<bool>) -> zbus::Result<()>;
    #[zbus(signal)]
    fn click(
        &self,
        button: u32,
        modifiers: u32,
        x: i32,
        y: i32,
        from_tray: bool,
    ) -> zbus::Result<()>;
    #[zbus(signal)]
    fn shown(&self, cause: u32) -> zbus::Result<()>;
    #[zbus(signal)]
    fn hidden(&self, cause: u32) -> zbus::Result<()>;
}

fn result_name(result: i32) -> String {
    match result {
        0 => "none (dismissed or interrupted)".into(),
        1.. => format!("label {result}"),
        ..=-25 => format!("button {}", -24 - result),
        _ => format!("standard button {result}"), // -1 OK, -2 Cancel, ... (B-ASK-2)
    }
}

fn main() -> zbus::Result<()> {
    let conn = zbus::blocking::Connection::session()?;
    let ch = CharacterProxyBlocking::new(&conn)?;

    // Subscribe before the first call so that no signal is missed. RequestComplete and
    // AskComplete are unicast to this connection (B-Q-2); the others are broadcast.
    let mut completes = ch.receive_request_complete()?;
    let mut asks = ch.receive_ask_complete()?;
    let clicks = ch.receive_click()?;
    let shown = ch.receive_shown()?;
    let hidden = ch.receive_hidden()?;

    // The first call D-Bus-activates bentwired if it is not running.
    ch.show(false)?;
    ch.speak("Hello from Rust!")?;
    ch.play("Wave")?;
    let spec = HashMap::from([
        ("heading", Value::from("Rust example")),
        ("text", Value::from("Coffee or tea?")),
        ("buttons", Value::from(vec!["&Coffee", "&Tea"])),
    ]);
    let ask = ch.ask(spec)?;

    // AskComplete comes before RequestComplete (B-ASK-4); a failed ask has none.
    for c in completes.by_ref() {
        let a = c.args()?;
        if a.id == ask {
            if a.status != 0 {
                println!("ask ended with status {} {}", a.status, a.reason);
                break;
            }
            for s in asks.by_ref() {
                let a = s.args()?;
                if a.id == ask {
                    println!("answer: {}", result_name(a.result));
                    break;
                }
            }
            break;
        }
    }

    println!("Listening for Click, Shown, Hidden (Ctrl+C to quit)");
    std::thread::spawn(move || {
        for s in shown {
            if let Ok(a) = s.args() {
                println!("Shown {}", a.cause);
            }
        }
    });
    std::thread::spawn(move || {
        for s in hidden {
            if let Ok(a) = s.args() {
                println!("Hidden {}", a.cause);
            }
        }
    });
    for s in clicks {
        let a = s.args()?;
        println!(
            "Click {} {} {} {} {}",
            a.button, a.modifiers, a.x, a.y, a.from_tray
        );
    }
    Ok(())
}
