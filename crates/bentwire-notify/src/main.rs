//! `bentwire-notify`: the character says desktop notifications (spec B-NOTIFY-1).

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::OwnedValue;

const BUS_NAME: &str = "org.bentwire.Agent";
const CHAR_PATH: &str = "/org/bentwire/Character";
const CHAR_IFACE: &str = "org.bentwire.Character1";
const NOTIFY_IFACE: &str = "org.freedesktop.Notifications";
const RULE: &str = "type='method_call',interface='org.freedesktop.Notifications',member='Notify'";

const USAGE: &str = "usage: bentwire-notify [--only APP ...] [--ignore APP ...]\n";

/// Longest text handed to Speak, in characters.
const MAX_CHARS: usize = 300;

/// Body markup the notification spec allows (`<b>`, `<i>`, `<u>`, `<a href>`, `<img>`).
const TAGS: [&str; 5] = ["b", "i", "u", "a", "img"];

/// Notify arguments: app_name, replaces_id, app_icon, summary, body, actions, hints,
/// expire_timeout.
type Notify = (
    String,
    u32,
    String,
    String,
    String,
    Vec<String>,
    HashMap<String, OwnedValue>,
    i32,
);

#[derive(Debug, Default, PartialEq, serde::Deserialize)]
struct Filter {
    #[serde(default)]
    only: Vec<String>,
    #[serde(default)]
    ignore: Vec<String>,
}

impl Filter {
    /// `ignore` wins; an empty `only` allows every app. Case-insensitive.
    fn allows(&self, app: &str) -> bool {
        let app = app.to_lowercase();
        let has = |l: &[String]| l.iter().any(|a| a.to_lowercase() == app);
        !has(&self.ignore) && (self.only.is_empty() || has(&self.only))
    }
}

/// Flags replace the config file's list of the same key.
fn filter(args: &[String], file: Option<&str>) -> Result<Filter, String> {
    let mut only = vec![];
    let mut ignore = vec![];
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let (k, inline) = match a.split_once('=') {
            Some((k, v)) => (k, Some(v.to_string())),
            None => (a.as_str(), None),
        };
        let list = match k {
            "--only" => &mut only,
            "--ignore" => &mut ignore,
            _ => return Err(format!("unknown argument {a}")),
        };
        match inline.or_else(|| it.next().cloned()) {
            Some(v) => list.push(v),
            None => return Err(format!("{k} needs a value")),
        }
    }
    let mut f = match file {
        Some(s) => toml::from_str::<Filter>(s).map_err(|e| format!("notify.toml: {e}"))?,
        None => Filter::default(),
    };
    if !only.is_empty() {
        f.only = only;
    }
    if !ignore.is_empty() {
        f.ignore = ignore;
    }
    Ok(f)
}

fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("bentwire/notify.toml"))
}

/// Removes the allowed markup tags; any other `<` stays text.
fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(p) = rest.find('<') {
        out.push_str(&rest[..p]);
        let tag = &rest[p + 1..];
        let name = tag.strip_prefix('/').unwrap_or(tag);
        let n = name
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(name.len());
        let known = TAGS.iter().any(|t| t.eq_ignore_ascii_case(&name[..n]));
        let ends = name[n..]
            .chars()
            .next()
            .is_some_and(|c| c == '>' || c == '/' || c.is_whitespace());
        match tag.find('>') {
            Some(end) if known && ends => rest = &tag[end + 1..],
            _ => {
                out.push('<');
                rest = tag;
            }
        }
    }
    out.push_str(rest);
    out
}

fn decode_entities(s: &str) -> String {
    s.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&amp;", "&")
}

/// The Speak text for a notification, `None` when there is nothing to say.
fn speech(summary: &str, body: &str) -> Option<String> {
    let body = decode_entities(&strip_tags(body));
    let text = match (summary.trim().is_empty(), body.trim().is_empty()) {
        (true, true) => return None,
        (false, true) => summary.to_string(),
        (true, false) => body,
        (false, false) => format!("{summary}: {body}"),
    };
    // `|` separates Speak alternatives (R-SPEAK-3) and `\` starts tags (B-SPEAK-1).
    let text: String = text
        .chars()
        .map(|c| if c == '|' || c == '\\' { '/' } else { c })
        .collect();
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if text.chars().count() <= MAX_CHARS {
        return Some(text);
    }
    let cut: String = text.chars().take(MAX_CHARS - 3).collect();
    Some(format!("{}...", cut.trim_end()))
}

extern "C" fn quit(_: libc::c_int) {
    unsafe { libc::_exit(0) }
}

/// Watches Notify calls until the bus connection ends.
fn run(f: &Filter) -> zbus::Result<()> {
    // A monitor may not send anything, so Speak goes out on its own connection.
    let speaker = Connection::session()?;
    let monitor = Connection::session()?;
    let messages = MessageIterator::from(&monitor);
    monitor.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus.Monitoring"),
        "BecomeMonitor",
        &(&[RULE][..], 0u32),
    )?;
    let mut failing = false;
    for msg in messages {
        let Ok(msg) = msg else { break };
        let h = msg.header();
        if h.message_type() != Type::MethodCall
            || h.interface().map(|i| i.as_str()) != Some(NOTIFY_IFACE)
            || h.member().map(|m| m.as_str()) != Some("Notify")
        {
            continue;
        }
        let Ok((app, _, _, summary, body, ..)) = msg.body().deserialize::<Notify>() else {
            continue;
        };
        if !f.allows(&app) {
            continue;
        }
        let Some(text) = speech(&summary, &body) else {
            continue;
        };
        // D-Bus activation starts bentwired if it is installed and not running.
        match speaker.call_method(
            Some(BUS_NAME),
            CHAR_PATH,
            Some(CHAR_IFACE),
            "Speak",
            &(text,),
        ) {
            Ok(_) => failing = false,
            Err(e) => {
                if !failing {
                    eprintln!("bentwire-notify: Speak failed ({e}); dropping notifications");
                }
                failing = true;
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let file = config_path().and_then(|p| std::fs::read_to_string(p).ok());
    let f = match filter(&args, file.as_deref()) {
        Ok(f) => f,
        Err(m) => {
            eprint!("bentwire-notify: {m}\n{USAGE}");
            return ExitCode::from(1);
        }
    };
    let handler = quit as extern "C" fn(libc::c_int) as libc::sighandler_t;
    unsafe {
        libc::signal(libc::SIGTERM, handler);
        libc::signal(libc::SIGINT, handler);
    }
    let mut warned = false;
    loop {
        match run(&f) {
            // The session bus went away: the session is over.
            Ok(()) => return ExitCode::SUCCESS,
            Err(e) => {
                if !warned {
                    eprintln!("bentwire-notify: {e}; retrying");
                    warned = true;
                }
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn b_notify_1_markup_entities_whitespace() {
        assert_eq!(
            strip_tags("<b>bold</b> <I>it</I> <a href=\"x\">link</a><img src=\"p.png\"/>"),
            "bold it link"
        );
        assert_eq!(strip_tags("a < b <br> <bx>"), "a < b <br> <bx>");
        assert_eq!(strip_tags("<b unclosed"), "<b unclosed");
        assert_eq!(
            speech("New mail", "<b>Hi</b> &amp; &lt;b&gt;\n\n  there").unwrap(),
            "New mail: Hi & <b> there"
        );
        assert_eq!(speech("  Title ", "").unwrap(), "Title");
        assert_eq!(speech("", "<i></i>"), None);
        assert_eq!(speech("", "only body").unwrap(), "only body");
    }

    #[test]
    fn b_notify_1_length_cap() {
        let long = "é".repeat(400);
        let t = speech("x", &long).unwrap();
        assert_eq!(t.chars().count(), MAX_CHARS);
        assert!(t.ends_with("..."));
    }

    #[test]
    fn b_notify_1_speak_syntax_is_neutralised() {
        // The text reaches the balloon as one alternative, with no tags and no bookmark.
        let t = speech("a|b", "\\Mrk=0\\ \\map=\"x\"=\"y\"\\ c:\\pau=9\\").unwrap();
        let alts = bentwire_core::speak::alternatives(&t);
        assert_eq!(alts.len(), 1);
        let p = bentwire_core::speak::parse(alts[0]).unwrap();
        assert!(p.bookmarks.is_empty());
        assert_eq!(p.words.join(" "), t);
    }

    #[test]
    fn b_notify_1_filter() {
        let f = filter(&s(&["--only", "Mail", "--only=Chat"]), None).unwrap();
        assert!(f.allows("mail") && f.allows("CHAT") && !f.allows("other"));
        let f = filter(&s(&["--ignore", "Spotify"]), None).unwrap();
        assert!(!f.allows("spotify") && f.allows("mail"));
        // Flags replace the file's list of the same key; ignore wins over only.
        let file = "only = [\"mail\", \"chat\"]\nignore = [\"chat\"]\n";
        let f = filter(&[], Some(file)).unwrap();
        assert!(f.allows("Mail") && !f.allows("chat") && !f.allows("x"));
        let f = filter(&s(&["--ignore", "mail"]), Some(file)).unwrap();
        assert!(!f.allows("mail") && f.allows("chat"));
        assert!(filter(&s(&["--only"]), None).is_err());
        assert!(filter(&s(&["--bogus"]), None).is_err());
        assert!(filter(&[], Some("only = 3")).is_err());
    }
}
