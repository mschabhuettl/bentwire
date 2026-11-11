//! D-Bus layer (spec 5.1 to 5.4, T-4 Test1). Method calls are dispatched by hand, not
//! through zbus's object server, so that the daemon controls the order of replies and
//! signals (B-Q-1) and can address signals to one unique name (B-Q-2).

use bentwire_core::api::{
    AskButtons as Buttons, AskSpec, Call as CoreCall, MethodError as CallError, Properties, Signal,
    errors,
};
use std::collections::HashMap;
use zbus::blocking::Connection;
use zbus::message::{Header, Message, Type};
use zbus::zvariant::{OwnedValue, Value};

pub const BUS_NAME: &str = "org.bentwire.Agent";
pub const AGENT_PATH: &str = "/org/bentwire/Agent";
pub const CHAR_PATH: &str = "/org/bentwire/Character";
pub const TEST_PATH: &str = "/org/bentwire/Test";
pub const AGENT_IFACE: &str = "org.bentwire.Agent1";
pub const CHAR_IFACE: &str = "org.bentwire.Character1";
pub const TEST_IFACE: &str = "org.bentwire.Test1";
const PROPS_IFACE: &str = "org.freedesktop.DBus.Properties";

#[derive(Debug, Clone, PartialEq)]
pub enum TestCall {
    Advance(u32),
    Pointer(String, u32, i32, i32),
    Key(String),
    SetDraws(Vec<u32>),
    AddOutput(String),
    RemoveOutput(u32),
    Snapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Call {
    Core(CoreCall),
    Test(TestCall),
    Get(String, String),
    GetAll(String),
    Introspect(String),
    Ping,
    GetMachineId,
}

pub fn err(name: &str, message: impl Into<String>) -> CallError {
    CallError {
        name: name.into(),
        message: message.into(),
    }
}

fn invalid(msg: impl Into<String>) -> CallError {
    err(errors::INVALID_ARGS, msg)
}

fn body<T>(msg: &Message) -> Result<T, CallError>
where
    T: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
{
    msg.body()
        .deserialize::<T>()
        .map_err(|e| invalid(format!("wrong arguments: {e}")))
}

/// Parse a method call. `test` enables `org.bentwire.Test1` (headless or
/// `--test-interface`), `test_full` the headless-only members.
pub fn parse(msg: &Message, test: bool, test_full: bool) -> Option<Result<Call, CallError>> {
    let h = msg.header();
    if h.message_type() != Type::MethodCall {
        return None;
    }
    let path = h.path().map(|p| p.as_str().to_string()).unwrap_or_default();
    let iface = h
        .interface()
        .map(|i| i.as_str().to_string())
        .unwrap_or_default();
    let member = h
        .member()
        .map(|m| m.as_str().to_string())
        .unwrap_or_default();
    let unknown_method = || {
        Err(err(
            "org.freedesktop.DBus.Error.UnknownMethod",
            format!("no method {iface}.{member} on {path}"),
        ))
    };
    let known_path = matches!(
        path.as_str(),
        AGENT_PATH | CHAR_PATH | "/" | "/org" | "/org/bentwire"
    ) || (test && path == TEST_PATH);
    if !known_path {
        return Some(Err(err(
            "org.freedesktop.DBus.Error.UnknownObject",
            format!("no object {path}"),
        )));
    }
    let r = (|| -> Result<Call, CallError> {
        Ok(match (iface.as_str(), member.as_str()) {
            ("org.freedesktop.DBus.Introspectable", "Introspect") => Call::Introspect(path.clone()),
            ("org.freedesktop.DBus.Peer", "Ping") => Call::Ping,
            ("org.freedesktop.DBus.Peer", "GetMachineId") => Call::GetMachineId,
            (PROPS_IFACE, "Get") => {
                let (i, n): (String, String) = body(msg)?;
                Call::Get(i, n)
            }
            (PROPS_IFACE, "GetAll") => {
                let (i,): (String,) = body(msg)?;
                Call::GetAll(i)
            }
            (PROPS_IFACE, "Set") => {
                let (i, n, v): (String, String, OwnedValue) = body(msg)?;
                if path != CHAR_PATH || i != CHAR_IFACE {
                    return Err(err(
                        "org.freedesktop.DBus.Error.PropertyReadOnly",
                        format!("{i}.{n} is read-only"),
                    ));
                }
                if n != "IdleOn" {
                    return Err(err(
                        "org.freedesktop.DBus.Error.PropertyReadOnly",
                        format!("{n} is read-only"),
                    ));
                }
                let b = bool::try_from(&*v).map_err(|_| invalid("IdleOn takes a boolean"))?;
                Call::Core(CoreCall::SetIdleOn(b))
            }
            (AGENT_IFACE, m) if path == AGENT_PATH => match m {
                "ListCharacters" => Call::Core(CoreCall::ListCharacters),
                "Select" => Call::Core(CoreCall::Select(body::<(String,)>(msg)?.0)),
                "Quit" => Call::Core(CoreCall::Quit),
                _ => return unknown_method(),
            },
            (CHAR_IFACE, m) if path == CHAR_PATH => Call::Core(match m {
                "Show" => CoreCall::Show {
                    fast: body::<(bool,)>(msg)?.0,
                },
                "Hide" => CoreCall::Hide {
                    fast: body::<(bool,)>(msg)?.0,
                },
                "Play" => CoreCall::Play(body::<(String,)>(msg)?.0),
                "Speak" => CoreCall::Speak(body::<(String,)>(msg)?.0),
                "Ask" => CoreCall::Ask(parse_ask(&body::<(HashMap<String, OwnedValue>,)>(msg)?.0)?),
                "MoveTo" => {
                    let (x, y, s): (i32, i32, u32) = body(msg)?;
                    CoreCall::MoveTo { x, y, speed: s }
                }
                "GestureAt" => {
                    let (x, y): (i32, i32) = body(msg)?;
                    CoreCall::GestureAt { x, y }
                }
                "Stop" => CoreCall::Stop(body::<(u32,)>(msg)?.0),
                "StopAll" => {
                    let t = body::<(Vec<String>,)>(msg)?.0;
                    if let Some(bad) = t
                        .iter()
                        .find(|t| !["play", "move", "speak", "ask"].contains(&t.as_str()))
                    {
                        return Err(invalid(format!("unknown request type {bad:?} (B-Q-5)")));
                    }
                    CoreCall::StopAll(t)
                }
                _ => return unknown_method(),
            }),
            (TEST_IFACE, m) if test && path == TEST_PATH => Call::Test(match m {
                "Pointer" => {
                    let (k, b, x, y): (String, u32, i32, i32) = body(msg)?;
                    if !["press", "release", "motion", "leave"].contains(&k.as_str()) {
                        return Err(invalid(format!("unknown pointer kind {k:?}")));
                    }
                    TestCall::Pointer(k, b, x, y)
                }
                "Key" => TestCall::Key(body::<(String,)>(msg)?.0),
                "Snapshot" => TestCall::Snapshot,
                "Advance" if test_full => TestCall::Advance(body::<(u32,)>(msg)?.0),
                "SetDraws" if test_full => TestCall::SetDraws(body::<(Vec<u32>,)>(msg)?.0),
                "AddOutput" if test_full => TestCall::AddOutput(body::<(String,)>(msg)?.0),
                "RemoveOutput" if test_full => TestCall::RemoveOutput(body::<(u32,)>(msg)?.0),
                _ => return unknown_method(),
            }),
            _ => return unknown_method(),
        })
    })();
    Some(r)
}

fn get_str(v: &Value, key: &str) -> Result<String, CallError> {
    String::try_from(v.try_clone().map_err(|_| invalid(key.to_string()))?)
        .map_err(|_| invalid(format!("Ask key {key:?} must be a string")))
}

fn get_strs(v: &Value, key: &str) -> Result<Vec<String>, CallError> {
    let v = v.try_clone().map_err(|_| invalid(key.to_string()))?;
    <Vec<String>>::try_from(v)
        .map_err(|_| invalid(format!("Ask key {key:?} must be a string array")))
}

/// B-ASK-1 keys and types. Unknown keys are InvalidArgs (bentwire decision, spec 3.14).
pub fn parse_ask(m: &HashMap<String, OwnedValue>) -> Result<AskSpec, CallError> {
    let mut a = AskSpec::default();
    for (k, v) in m {
        // Unwrap a nested variant (`v` inside `v`) as some clients send it.
        let v: &Value = match &**v {
            Value::Value(inner) => inner,
            other => other,
        };
        match k.as_str() {
            "heading" => a.heading = Some(get_str(v, k)?),
            "text" => a.text = Some(get_str(v, k)?),
            "icon" => a.icon = Some(get_str(v, k)?),
            "label_type" => a.label_type = Some(get_str(v, k)?),
            "mode" => a.mode = Some(get_str(v, k)?),
            "animation" => a.animation = Some(get_str(v, k)?),
            "labels" => a.labels = Some(get_strs(v, k)?),
            "checkboxes" => a.checkboxes = Some(get_strs(v, k)?),
            "buttons" => {
                a.buttons = Some(match v {
                    Value::Str(_) => Buttons::Set(get_str(v, k)?),
                    _ => Buttons::Custom(get_strs(v, k)?),
                })
            }
            other => return Err(invalid(format!("unknown Ask key {other:?}"))),
        }
    }
    Ok(a)
}

pub fn reply_error(conn: &Connection, h: &Header<'_>, e: &CallError) {
    if let Err(err) = conn.reply_error(h, e.name.as_str(), &e.message) {
        log::warn!("D-Bus error reply failed: {err}");
    }
}

pub fn reply<B: serde::Serialize + zbus::zvariant::DynamicType>(
    conn: &Connection,
    h: &Header<'_>,
    b: &B,
) {
    if let Err(err) = conn.reply(h, b) {
        log::warn!("D-Bus reply failed: {err}");
    }
}

pub fn signal_member(s: &Signal) -> &'static str {
    match s {
        Signal::RequestStart { .. } => "RequestStart",
        Signal::RequestComplete { .. } => "RequestComplete",
        Signal::AskComplete { .. } => "AskComplete",
        Signal::Bookmark { .. } => "Bookmark",
        Signal::Click { .. } => "Click",
        Signal::DblClick { .. } => "DblClick",
        Signal::DragStart { .. } => "DragStart",
        Signal::DragComplete { .. } => "DragComplete",
        Signal::Move { .. } => "Move",
        Signal::Shown { .. } => "Shown",
        Signal::Hidden { .. } => "Hidden",
        Signal::IdleStart => "IdleStart",
        Signal::IdleComplete => "IdleComplete",
        Signal::BalloonShown => "BalloonShown",
        Signal::BalloonHidden => "BalloonHidden",
    }
}

pub fn signal_args(s: &Signal) -> serde_json::Value {
    use serde_json::json;
    match s {
        Signal::RequestStart { id } => json!([id]),
        Signal::RequestComplete { id, status, reason } => json!([id, status, reason]),
        Signal::AskComplete {
            id,
            result,
            checked,
        } => json!([id, result, checked]),
        Signal::Bookmark { n } => json!([n]),
        Signal::Click {
            button,
            modifiers,
            x,
            y,
            from_tray,
        }
        | Signal::DblClick {
            button,
            modifiers,
            x,
            y,
            from_tray,
        } => {
            json!([button, modifiers, x, y, from_tray])
        }
        Signal::DragStart {
            button,
            modifiers,
            x,
            y,
        }
        | Signal::DragComplete {
            button,
            modifiers,
            x,
            y,
        } => json!([button, modifiers, x, y]),
        Signal::Move { x, y, cause } => json!([x, y, cause]),
        Signal::Shown { cause } | Signal::Hidden { cause } => json!([cause]),
        _ => json!([]),
    }
}

pub fn emit(conn: &Connection, dest: Option<&str>, s: &Signal) {
    let m = signal_member(s);
    let r = match s {
        Signal::RequestStart { id } => conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(id,)),
        Signal::RequestComplete { id, status, reason } => {
            conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(id, status, reason))
        }
        Signal::AskComplete {
            id,
            result,
            checked,
        } => conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(id, result, checked)),
        Signal::Bookmark { n } => conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(n,)),
        Signal::Click {
            button,
            modifiers,
            x,
            y,
            from_tray,
        }
        | Signal::DblClick {
            button,
            modifiers,
            x,
            y,
            from_tray,
        } => conn.emit_signal(
            dest,
            CHAR_PATH,
            CHAR_IFACE,
            m,
            &(button, modifiers, x, y, from_tray),
        ),
        Signal::DragStart {
            button,
            modifiers,
            x,
            y,
        }
        | Signal::DragComplete {
            button,
            modifiers,
            x,
            y,
        } => conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(button, modifiers, x, y)),
        Signal::Move { x, y, cause } => {
            conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(x, y, cause))
        }
        Signal::Shown { cause } | Signal::Hidden { cause } => {
            conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &(cause,))
        }
        _ => conn.emit_signal(dest, CHAR_PATH, CHAR_IFACE, m, &()),
    };
    if let Err(err) = r {
        log::warn!("emitting {m} failed: {err}");
    }
}

pub fn char_props_map(p: &Properties) -> HashMap<&'static str, Value<'static>> {
    let mut m = HashMap::new();
    m.insert("Id", Value::from(p.character.clone()));
    m.insert("Name", Value::from(p.name.clone()));
    m.insert("Description", Value::from(p.description.clone()));
    m.insert("Visible", Value::from(p.visible));
    m.insert("Left", Value::from(p.left));
    m.insert("Top", Value::from(p.top));
    m.insert("Width", Value::from(p.width));
    m.insert("Height", Value::from(p.height));
    m.insert("IdleOn", Value::from(p.idle_on));
    m.insert("MoveCause", Value::from(p.move_cause));
    m.insert("VisibilityCause", Value::from(p.visibility_cause));
    m.insert("BalloonEnabled", Value::from(p.balloon_enabled));
    m.insert("AnimationNames", Value::from(p.animation_names.clone()));
    m
}

pub fn agent_props_map(character: &str) -> HashMap<&'static str, Value<'static>> {
    let mut m = HashMap::new();
    m.insert("Character", Value::from(character.to_string()));
    m.insert(
        "Version",
        Value::from(env!("CARGO_PKG_VERSION").to_string()),
    );
    m
}

/// PropertiesChanged for the keys whose values differ.
pub fn emit_props_changed(
    conn: &Connection,
    path: &str,
    iface: &str,
    old: &HashMap<&'static str, Value<'static>>,
    new: &HashMap<&'static str, Value<'static>>,
) {
    let changed: HashMap<&str, &Value> = new
        .iter()
        .filter(|(k, v)| old.get(*k) != Some(*v))
        .map(|(k, v)| (*k, v))
        .collect();
    if changed.is_empty() {
        return;
    }
    let invalidated: Vec<&str> = Vec::new();
    if let Err(e) = conn.emit_signal(
        None::<&str>,
        path,
        PROPS_IFACE,
        "PropertiesChanged",
        &(iface, changed, invalidated),
    ) {
        log::warn!("PropertiesChanged failed: {e}");
    }
}

pub fn introspect(path: &str, test: bool, test_full: bool) -> String {
    let std_ifaces = r#"<interface name="org.freedesktop.DBus.Introspectable"><method name="Introspect"><arg type="s" direction="out"/></method></interface>
<interface name="org.freedesktop.DBus.Peer"><method name="Ping"/><method name="GetMachineId"><arg type="s" direction="out"/></method></interface>
<interface name="org.freedesktop.DBus.Properties"><method name="Get"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="out"/></method><method name="GetAll"><arg type="s" direction="in"/><arg type="a{sv}" direction="out"/></method><method name="Set"><arg type="s" direction="in"/><arg type="s" direction="in"/><arg type="v" direction="in"/></method><signal name="PropertiesChanged"><arg type="s"/><arg type="a{sv}"/><arg type="as"/></signal></interface>"#;
    let body = match path {
        AGENT_PATH => r#"<interface name="org.bentwire.Agent1">
<method name="ListCharacters"><arg type="a(sss)" direction="out"/></method>
<method name="Select"><arg name="id" type="s" direction="in"/></method>
<method name="Quit"/>
<property name="Character" type="s" access="read"/>
<property name="Version" type="s" access="read"/>
</interface>"#
            .to_string(),
        CHAR_PATH => {
            let mut s = String::from(r#"<interface name="org.bentwire.Character1">"#);
            for (name, ins) in [
                ("Show", "b"),
                ("Hide", "b"),
                ("Play", "s"),
                ("Speak", "s"),
                ("Ask", "a{sv}"),
                ("MoveTo", "iiu"),
                ("GestureAt", "ii"),
            ] {
                s += &format!("<method name=\"{name}\">");
                for c in sig_parts(ins) {
                    s += &format!("<arg type=\"{c}\" direction=\"in\"/>");
                }
                s += "<arg type=\"u\" direction=\"out\"/></method>";
            }
            s += r#"<method name="Stop"><arg type="u" direction="in"/></method><method name="StopAll"><arg type="as" direction="in"/></method>"#;
            for (n, t) in [
                ("RequestStart", "u"),
                ("RequestComplete", "uus"),
                ("AskComplete", "uiab"),
                ("Bookmark", "i"),
                ("Click", "uuiib"),
                ("DblClick", "uuiib"),
                ("DragStart", "uuii"),
                ("DragComplete", "uuii"),
                ("Move", "iiu"),
                ("Shown", "u"),
                ("Hidden", "u"),
                ("IdleStart", ""),
                ("IdleComplete", ""),
                ("BalloonShown", ""),
                ("BalloonHidden", ""),
            ] {
                s += &format!("<signal name=\"{n}\">");
                for c in sig_parts(t) {
                    s += &format!("<arg type=\"{c}\"/>");
                }
                s += "</signal>";
            }
            for (n, t) in [
                ("Id", "s"),
                ("Name", "s"),
                ("Description", "s"),
                ("Visible", "b"),
                ("Left", "i"),
                ("Top", "i"),
                ("Width", "u"),
                ("Height", "u"),
                ("MoveCause", "u"),
                ("VisibilityCause", "u"),
                ("BalloonEnabled", "b"),
                ("AnimationNames", "as"),
            ] {
                s += &format!("<property name=\"{n}\" type=\"{t}\" access=\"read\"/>");
            }
            s += r#"<property name="IdleOn" type="b" access="readwrite"/></interface>"#;
            s
        }
        TEST_PATH if test => {
            let mut s = String::from(
                r#"<interface name="org.bentwire.Test1"><method name="Pointer"><arg type="s" direction="in"/><arg type="u" direction="in"/><arg type="i" direction="in"/><arg type="i" direction="in"/></method><method name="Key"><arg type="s" direction="in"/></method><method name="Snapshot"><arg type="s" direction="out"/></method>"#,
            );
            if test_full {
                s += r#"<method name="Advance"><arg type="u" direction="in"/></method><method name="SetDraws"><arg type="au" direction="in"/></method><method name="AddOutput"><arg type="s" direction="in"/><arg type="u" direction="out"/></method><method name="RemoveOutput"><arg type="u" direction="in"/></method>"#;
            }
            s + "</interface>"
        }
        _ => String::new(),
    };
    let children = match path {
        "/" => "<node name=\"org\"/>".to_string(),
        "/org" => "<node name=\"bentwire\"/>".to_string(),
        "/org/bentwire" => {
            let mut c = "<node name=\"Agent\"/><node name=\"Character\"/>".to_string();
            if test {
                c += "<node name=\"Test\"/>";
            }
            c
        }
        _ => String::new(),
    };
    format!(
        "<!DOCTYPE node PUBLIC \"-//freedesktop//DTD D-BUS Object Introspection 1.0//EN\" \
         \"http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd\">\n<node>\n{std_ifaces}\n{body}\n{children}</node>\n"
    )
}

/// Split a signature into complete types (only the shapes used above).
fn sig_parts(s: &str) -> Vec<&str> {
    let mut v = Vec::new();
    let mut i = 0;
    let b = s.as_bytes();
    while i < b.len() {
        let start = i;
        while b[i] == b'a' {
            i += 1;
        }
        if b[i] == b'{' {
            while b[i] != b'}' {
                i += 1;
            }
        }
        i += 1;
        v.push(&s[start..i]);
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sig_parts_split() {
        assert_eq!(sig_parts("a{sv}"), ["a{sv}"]);
        assert_eq!(sig_parts("uiab"), ["u", "i", "ab"]);
        assert_eq!(sig_parts(""), Vec::<&str>::new());
    }

    #[test]
    fn b_ask_1_parse_types() {
        let mut m: HashMap<String, OwnedValue> = HashMap::new();
        m.insert(
            "text".into(),
            OwnedValue::try_from(Value::from("hi")).unwrap(),
        );
        m.insert(
            "buttons".into(),
            OwnedValue::try_from(Value::from("yes-no")).unwrap(),
        );
        m.insert(
            "labels".into(),
            OwnedValue::try_from(Value::from(vec!["a".to_string()])).unwrap(),
        );
        let a = parse_ask(&m).unwrap();
        assert_eq!(a.text.as_deref(), Some("hi"));
        assert_eq!(a.buttons, Some(Buttons::Set("yes-no".into())));
        assert_eq!(a.labels, Some(vec!["a".to_string()]));
        m.insert(
            "buttons".into(),
            OwnedValue::try_from(Value::from(vec!["X".to_string(), "Y".to_string()])).unwrap(),
        );
        assert_eq!(
            parse_ask(&m).unwrap().buttons,
            Some(Buttons::Custom(vec!["X".into(), "Y".into()]))
        );
        m.insert("heading".into(), OwnedValue::from(3u32));
        assert_eq!(parse_ask(&m).unwrap_err().name, errors::INVALID_ARGS);
        m.remove("heading");
        m.insert("bogus".into(), OwnedValue::from(3u32));
        assert_eq!(parse_ask(&m).unwrap_err().name, errors::INVALID_ARGS);
    }
}
