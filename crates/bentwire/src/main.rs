//! `bentwire`: command line client of bentwired (spec 5.5).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};
use zbus::blocking::{Connection, MessageIterator};
use zbus::message::Type;
use zbus::zvariant::{OwnedValue, Value};

/// `println!` that ends quietly when stdout is closed (`bentwire characters | head -1`)
/// instead of panicking on the broken pipe.
macro_rules! outln {
    ($($t:tt)*) => {{
        use std::io::Write;
        if writeln!(std::io::stdout(), $($t)*).is_err() {
            std::process::exit(0);
        }
    }};
}

const BUS_NAME: &str = "org.bentwire.Agent";
const AGENT_PATH: &str = "/org/bentwire/Agent";
const CHAR_PATH: &str = "/org/bentwire/Character";
const AGENT_IFACE: &str = "org.bentwire.Agent1";
const CHAR_IFACE: &str = "org.bentwire.Character1";

const USAGE: &str = "\
usage: bentwire <command> [args]
  play <name> [--wait]
  say <text> [--wait]
  ask [<text>] [--heading H] [--icon I] [--buttons SET | --button CAPTION ...]
      [--label L ...] [--label-type T] [--checkbox C ...] [--mode M] [--animation NAME]
  moveto <x> <y> [--speed MS] [--wait]
  gestureat <x> <y> [--wait]
  show [--fast] [--wait]
  hide [--fast] [--wait]
  stop [<id>] [--all]
  characters
  characters install classics
  select <id>
  animations
  status
  quit
  convert <source> <dest> [--palette OFFICE.ACS|MSO97.DLL] [--lang TAG] [--codepage LABEL]
";

/// B-L10N-2 CLI result names for results -1..-15.
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

enum Fail {
    Usage(String),
    Dbus(String, String),
    /// Exit status with an optional message for stderr.
    Exit(u8, Option<String>),
}

impl From<zbus::Error> for Fail {
    fn from(e: zbus::Error) -> Self {
        match e {
            zbus::Error::MethodError(name, msg, _) => {
                let name = name.to_string();
                if name == "org.freedesktop.DBus.Error.ServiceUnknown"
                    || name == "org.freedesktop.DBus.Error.Spawn.ChildExited"
                    || name.starts_with("org.freedesktop.DBus.Error.Spawn")
                    || name == "org.freedesktop.DBus.Error.NameHasNoOwner"
                {
                    Fail::Exit(1, Some("bentwired is not running".into()))
                } else {
                    Fail::Dbus(name, msg.unwrap_or_default())
                }
            }
            other => Fail::Dbus("bentwire.Error.DBus".into(), other.to_string()),
        }
    }
}

/// `pub` for the ask result test.
pub fn result_name(result: i32) -> String {
    match result {
        0 => "none".into(),
        r @ -15..=-1 => RESULT_NAMES[(-r - 1) as usize].into(),
        r if r <= -25 => format!("button:{}", -r - 24),
        r if r > 0 => format!("label:{r}"),
        r => format!("unknown:{r}"),
    }
}

struct Opts {
    positional: Vec<String>,
    flags: Vec<String>,
    values: Vec<(String, String)>,
}

impl Opts {
    fn parse(args: &[String], bool_flags: &[&str], value_flags: &[&str]) -> Result<Opts, Fail> {
        let mut o = Opts {
            positional: vec![],
            flags: vec![],
            values: vec![],
        };
        let mut it = args.iter();
        let mut only_pos = false;
        while let Some(a) = it.next() {
            if only_pos || !a.starts_with("--") {
                o.positional.push(a.clone());
                continue;
            }
            if a == "--" {
                only_pos = true;
                continue;
            }
            let (k, inline) = match a.split_once('=') {
                Some((k, v)) => (k, Some(v.to_string())),
                None => (a.as_str(), None),
            };
            if bool_flags.contains(&k) && inline.is_none() {
                o.flags.push(k.to_string());
            } else if value_flags.contains(&k) {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .cloned()
                        .ok_or_else(|| Fail::Usage(format!("{k} needs a value")))?,
                };
                o.values.push((k.to_string(), v));
            } else {
                return Err(Fail::Usage(format!("unknown option {a}")));
            }
        }
        Ok(o)
    }
    fn flag(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
    fn value(&self, f: &str) -> Option<&str> {
        self.values
            .iter()
            .rev()
            .find(|(k, _)| k == f)
            .map(|(_, v)| v.as_str())
    }
    fn all(&self, f: &str) -> Vec<String> {
        self.values
            .iter()
            .filter(|(k, _)| k == f)
            .map(|(_, v)| v.clone())
            .collect()
    }
    fn npos(&self, n: usize, what: &str) -> Result<(), Fail> {
        if self.positional.len() == n {
            Ok(())
        } else {
            Err(Fail::Usage(format!("expected {what}")))
        }
    }
}

fn int<T: std::str::FromStr>(s: &str, what: &str) -> Result<T, Fail> {
    s.parse()
        .map_err(|_| Fail::Usage(format!("{what} must be a number, got {s:?}")))
}

/// (status, reason, ask result and check boxes).
type Completion = (u32, String, Option<(i32, Vec<bool>)>);

struct Client {
    conn: Connection,
    /// Created before the method call so that no unicast signal is missed.
    signals: Option<MessageIterator>,
}

impl Client {
    fn new(wait: bool) -> Result<Client, Fail> {
        let conn = Connection::session()
            .map_err(|e| Fail::Exit(1, Some(format!("bentwired is not running ({e})"))))?;
        let signals = if wait {
            // Notice the daemon going away while we wait for its signals.
            conn.call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "AddMatch",
                &(format!(
                    "type='signal',sender='org.freedesktop.DBus',member='NameOwnerChanged',arg0='{BUS_NAME}'"
                ),),
            )?;
            Some(MessageIterator::from(&conn))
        } else {
            None
        };
        Ok(Client { conn, signals })
    }

    fn call<B, R>(&self, path: &str, iface: &str, method: &str, body: &B) -> Result<R, Fail>
    where
        B: serde::Serialize + zbus::zvariant::DynamicType,
        R: for<'d> serde::Deserialize<'d> + zbus::zvariant::Type,
    {
        let reply = self
            .conn
            .call_method(Some(BUS_NAME), path, Some(iface), method, body)?;
        reply
            .body()
            .deserialize::<R>()
            .map_err(|e| Fail::Dbus("bentwire.Error.Reply".into(), e.to_string()))
    }

    fn prop(&self, path: &str, iface: &str, name: &str) -> Result<OwnedValue, Fail> {
        let (v,): (OwnedValue,) = self.call(
            path,
            "org.freedesktop.DBus.Properties",
            "Get",
            &(iface, name),
        )?;
        Ok(v)
    }

    /// Wait for RequestComplete of `id`; returns (status, reason, ask result).
    fn wait(&mut self, id: u32) -> Result<Completion, Fail> {
        let mut ask = None;
        let it = self.signals.as_mut().expect("created with wait");
        for msg in it {
            let msg = msg?;
            let h = msg.header();
            if h.message_type() != Type::Signal {
                continue;
            }
            if h.member().map(|m| m.as_str()) == Some("NameOwnerChanged")
                && let Ok((name, _, new)) = msg.body().deserialize::<(String, String, String)>()
                && name == BUS_NAME
                && new.is_empty()
            {
                return Err(Fail::Exit(1, Some("bentwired exited while waiting".into())));
            }
            if h.interface().map(|i| i.as_str()) != Some(CHAR_IFACE) {
                continue;
            }
            match h.member().map(|m| m.as_str().to_string()).as_deref() {
                Some("AskComplete") => {
                    if let Ok((rid, result, checked)) =
                        msg.body().deserialize::<(u32, i32, Vec<bool>)>()
                        && rid == id
                    {
                        ask = Some((result, checked));
                    }
                }
                Some("RequestComplete") => {
                    if let Ok((rid, status, reason)) =
                        msg.body().deserialize::<(u32, u32, String)>()
                        && rid == id
                    {
                        return Ok((status, reason, ask));
                    }
                }
                _ => {}
            }
        }
        Err(Fail::Exit(
            1,
            Some("D-Bus connection closed while waiting".into()),
        ))
    }
}

fn status_exit(status: u32, reason: &str) -> Result<(), Fail> {
    match status {
        0 => Ok(()),
        3 => Err(Fail::Exit(3, Some(format!("interrupted: {reason}")))),
        _ => Err(Fail::Exit(2, Some(format!("request failed: {reason}")))),
    }
}

fn request<B>(method: &str, body: &B, wait: bool) -> Result<(), Fail>
where
    B: serde::Serialize + zbus::zvariant::DynamicType,
{
    let mut c = Client::new(wait)?;
    let (id,): (u32,) = c.call(CHAR_PATH, CHAR_IFACE, method, body)?;
    outln!("{id}");
    if wait {
        use std::io::Write;
        std::io::stdout().flush().ok();
        let (status, reason, _) = c.wait(id)?;
        status_exit(status, &reason)?;
    }
    Ok(())
}

fn ask(args: &[String]) -> Result<(), Fail> {
    let o = Opts::parse(
        args,
        &[],
        &[
            "--heading",
            "--icon",
            "--buttons",
            "--button",
            "--label",
            "--label-type",
            "--checkbox",
            "--mode",
            "--animation",
        ],
    )?;
    if o.positional.len() > 1 {
        return Err(Fail::Usage("ask takes at most one text argument".into()));
    }
    let mut spec: HashMap<&str, Value> = HashMap::new();
    if let Some(t) = o.positional.first() {
        spec.insert("text", Value::from(t.clone()));
    }
    for (flag, key) in [
        ("--heading", "heading"),
        ("--icon", "icon"),
        ("--label-type", "label_type"),
        ("--mode", "mode"),
        ("--animation", "animation"),
    ] {
        if let Some(v) = o.value(flag) {
            spec.insert(key, Value::from(v.to_string()));
        }
    }
    let custom = o.all("--button");
    match (o.value("--buttons"), custom.is_empty()) {
        (Some(_), false) => return Err(Fail::Usage("use either --buttons or --button".into())),
        (Some(set), true) => {
            spec.insert("buttons", Value::from(set.to_string()));
        }
        (None, false) => {
            spec.insert("buttons", Value::from(custom));
        }
        (None, true) => {}
    }
    for (flag, key) in [("--label", "labels"), ("--checkbox", "checkboxes")] {
        let v = o.all(flag);
        if !v.is_empty() {
            spec.insert(key, Value::from(v));
        }
    }
    let mut c = Client::new(true)?;
    let (id,): (u32,) = c.call(CHAR_PATH, CHAR_IFACE, "Ask", &(spec,))?;
    let (status, reason, result) = c.wait(id)?;
    let (result, checked) = result.unwrap_or((0, vec![]));
    if status == 1 {
        return Err(Fail::Exit(2, Some(format!("request failed: {reason}"))));
    }
    outln!("{}", result_name(result));
    if !o.all("--checkbox").is_empty() {
        let v: Vec<&str> = checked.iter().map(|b| if *b { "1" } else { "0" }).collect();
        outln!("checked:{}", v.join(","));
    }
    if status == 3 {
        return Err(Fail::Exit(3, Some(format!("interrupted: {reason}"))));
    }
    if result == 0 {
        return Err(Fail::Exit(3, None));
    }
    Ok(())
}

fn run(args: &[String]) -> Result<(), Fail> {
    let Some((cmd, rest)) = args.split_first() else {
        return Err(Fail::Usage(String::new()));
    };
    match cmd.as_str() {
        "play" | "say" => {
            let o = Opts::parse(rest, &["--wait"], &[])?;
            o.npos(
                1,
                if cmd == "play" {
                    "an animation name"
                } else {
                    "a text"
                },
            )?;
            let m = if cmd == "play" { "Play" } else { "Speak" };
            request(m, &(o.positional[0].as_str(),), o.flag("--wait"))
        }
        "ask" => ask(rest),
        "moveto" => {
            let o = Opts::parse(rest, &["--wait"], &["--speed"])?;
            o.npos(2, "<x> <y>")?;
            let x: i32 = int(&o.positional[0], "x")?;
            let y: i32 = int(&o.positional[1], "y")?;
            let speed: u32 = match o.value("--speed") {
                Some(s) => int(s, "--speed")?,
                None => 1000,
            };
            request("MoveTo", &(x, y, speed), o.flag("--wait"))
        }
        "gestureat" => {
            let o = Opts::parse(rest, &["--wait"], &[])?;
            o.npos(2, "<x> <y>")?;
            let x: i32 = int(&o.positional[0], "x")?;
            let y: i32 = int(&o.positional[1], "y")?;
            request("GestureAt", &(x, y), o.flag("--wait"))
        }
        "show" | "hide" => {
            let o = Opts::parse(rest, &["--fast", "--wait"], &[])?;
            o.npos(0, "no arguments")?;
            let m = if cmd == "show" { "Show" } else { "Hide" };
            request(m, &(o.flag("--fast"),), o.flag("--wait"))
        }
        "stop" => {
            let o = Opts::parse(rest, &["--all"], &[])?;
            let c = Client::new(false)?;
            if o.flag("--all") {
                o.npos(0, "no id with --all")?;
                c.call::<_, ()>(CHAR_PATH, CHAR_IFACE, "StopAll", &(Vec::<String>::new(),))
            } else {
                let id: u32 = match o.positional.as_slice() {
                    [] => 0,
                    [id] => int(id, "id")?,
                    _ => return Err(Fail::Usage("stop takes at most one id".into())),
                };
                c.call::<_, ()>(CHAR_PATH, CHAR_IFACE, "Stop", &(id,))
            }
        }
        "characters" if rest.first().is_some_and(|a| a == "install") => {
            Opts::parse(&rest[1..], &[], &[])?.npos(1, "a pack name (classics)")?;
            if rest[1] != "classics" {
                return Err(Fail::Usage(format!(
                    "unknown pack {:?} (only classics can be installed)",
                    rest[1]
                )));
            }
            install_classics()
        }
        "characters" => {
            Opts::parse(rest, &[], &[])?.npos(0, "no arguments")?;
            let c = Client::new(false)?;
            let (list,): (Vec<(String, String, String)>,) =
                c.call(AGENT_PATH, AGENT_IFACE, "ListCharacters", &())?;
            let cur =
                String::try_from(c.prop(AGENT_PATH, AGENT_IFACE, "Character")?).unwrap_or_default();
            for (id, name, _) in list {
                if id == cur {
                    outln!("{id}\t{name}\t*");
                } else {
                    outln!("{id}\t{name}");
                }
            }
            Ok(())
        }
        "select" => {
            let o = Opts::parse(rest, &[], &[])?;
            o.npos(1, "a character id")?;
            let c = Client::new(false)?;
            let id = o.positional[0].as_str();
            match c.call::<_, ()>(AGENT_PATH, AGENT_IFACE, "Select", &(id,)) {
                Err(Fail::Dbus(name, msg)) if name == "org.bentwire.Error.NoSuchCharacter" => {
                    let list: Vec<String> = c
                        .call::<_, (Vec<(String, String, String)>,)>(
                            AGENT_PATH,
                            AGENT_IFACE,
                            "ListCharacters",
                            &(),
                        )
                        .map(|(l,)| l.into_iter().map(|e| e.0).collect())
                        .unwrap_or_default();
                    let hints: String = select_hints(id, &list)
                        .iter()
                        .map(|h| format!("\nbentwire: {h}"))
                        .collect();
                    Err(Fail::Dbus(name, msg + &hints))
                }
                r => r,
            }
        }
        "animations" => {
            Opts::parse(rest, &[], &[])?.npos(0, "no arguments")?;
            let c = Client::new(false)?;
            let v = c.prop(CHAR_PATH, CHAR_IFACE, "AnimationNames")?;
            let names = <Vec<String>>::try_from(v).unwrap_or_default();
            for n in names {
                outln!("{n}");
            }
            Ok(())
        }
        "status" => {
            Opts::parse(rest, &[], &[])?.npos(0, "no arguments")?;
            let c = Client::new(false)?;
            let character =
                String::try_from(c.prop(AGENT_PATH, AGENT_IFACE, "Character")?).unwrap_or_default();
            outln!("character={character}");
            if character.is_empty() {
                return Ok(());
            }
            let (all,): (HashMap<String, OwnedValue>,) = c.call(
                CHAR_PATH,
                "org.freedesktop.DBus.Properties",
                "GetAll",
                &(CHAR_IFACE,),
            )?;
            let show = |k: &str| -> String {
                match all.get(k).map(|v| &**v) {
                    Some(Value::Bool(b)) => b.to_string(),
                    Some(Value::I32(i)) => i.to_string(),
                    Some(other) => other.to_string(),
                    None => String::new(),
                }
            };
            outln!("visible={}", show("Visible"));
            outln!("left={}", show("Left"));
            outln!("top={}", show("Top"));
            outln!("idle={}", show("IdleOn"));
            Ok(())
        }
        "quit" => {
            Opts::parse(rest, &[], &[])?.npos(0, "no arguments")?;
            Client::new(false)?.call::<_, ()>(AGENT_PATH, AGENT_IFACE, "Quit", &())
        }
        "convert" => {
            let o = Opts::parse(rest, &[], &["--palette", "--lang", "--codepage"])?;
            o.npos(2, "<source> <dest>")?;
            convert(&o)
        }
        "help" | "--help" | "-h" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(Fail::Usage(format!("unknown command {other:?}"))),
    }
}

/// Edit distance (insert, delete, substitute) between two ids.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut row: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut prev = row[0];
        row[0] = i + 1;
        for (j, cb) in b.iter().enumerate() {
            let cur = row[j + 1];
            row[j + 1] = (prev + usize::from(ca != *cb)).min(row[j] + 1).min(cur + 1);
            prev = cur;
        }
    }
    row[b.len()]
}

/// `select` with an unknown id: up to three close ids (edit distance 2 or less, or the id
/// with a suffix such as `-15`), and the install command when the classics pack (which
/// holds `clippit`) is not installed.
fn select_hints(id: &str, installed: &[String]) -> Vec<String> {
    let id = id.to_ascii_lowercase();
    let mut close: Vec<(usize, &String)> = installed
        .iter()
        .map(|i| (distance(&id, i), i))
        .filter(|(d, i)| *d <= 2 || i.starts_with(&format!("{id}-")))
        .collect();
    close.sort();
    let close: Vec<&str> = close.iter().take(3).map(|(_, i)| i.as_str()).collect();
    let mut v = Vec::new();
    if !close.is_empty() {
        v.push(format!("did you mean {}?", close.join(", ")));
    }
    if !installed.iter().any(|i| i == "clippit") {
        v.push("more characters: bentwire characters install classics".into());
    }
    v
}

/// Where `characters install` puts packs: `$XDG_DATA_HOME/bentwire/characters` (A-PACK-1).
fn characters_dir() -> PathBuf {
    let data = match std::env::var_os("XDG_DATA_HOME") {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_else(|| "/".into()))
            .join(".local/share"),
    };
    data.join("bentwire/characters")
}

/// Release assets of bentwire-classics; `BENTWIRE_CLASSICS_URL` replaces it (tests).
const CLASSICS_URL: &str =
    "https://github.com/mschabhuettl/bentwire-classics/releases/latest/download";

fn fail(m: impl Into<String>) -> Fail {
    Fail::Exit(1, Some(m.into()))
}

fn run_tool(cmd: &mut Command) -> Result<(), Fail> {
    let what = format!("{:?}", cmd.get_program());
    match cmd.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(fail(format!("{what} failed ({s})"))),
        Err(e) => Err(fail(format!("cannot run {what}: {e}"))),
    }
}

fn download(url: &str, to: &Path) -> Result<(), Fail> {
    use std::io::IsTerminal;
    let mut c = Command::new("curl");
    c.args(["-fL", "--proto", "=https,file", "--retry", "2", "-o"])
        .arg(to);
    if std::io::stderr().is_terminal() {
        c.arg("--progress-bar");
    } else {
        c.arg("-sS");
    }
    run_tool(c.arg(url)).map_err(|_| fail(format!("downloading {url} failed")))
}

fn sha256_hex(p: &Path) -> Result<String, Fail> {
    use sha2::{Digest, Sha256};
    let mut f = std::fs::File::open(p).map_err(|e| fail(format!("{}: {e}", p.display())))?;
    let mut h = Sha256::new();
    std::io::copy(&mut f, &mut h).map_err(|e| fail(format!("{}: {e}", p.display())))?;
    Ok(h.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// `characters install classics`: the latest bentwire-classics release, checked against
/// its `sha256sums.txt`, unpacked next to the destination and renamed into place. A git
/// checkout or a symlink at the destination is left alone. A running daemon is told to
/// rescan (it lists its characters).
fn install_classics() -> Result<(), Fail> {
    let dir = characters_dir();
    let dest = dir.join("classics");
    if dest.join(".git").exists() || dest.is_symlink() {
        eprintln!(
            "bentwire: {} is a git checkout or a link; left alone (update it there)",
            dest.display()
        );
        return Ok(());
    }
    std::fs::create_dir_all(&dir).map_err(|e| fail(format!("{}: {e}", dir.display())))?;
    let tmp = dir.join(format!(".classics-install-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir(&tmp).map_err(|e| fail(format!("{}: {e}", tmp.display())))?;
    let r = install_into(&tmp, &dest);
    let _ = std::fs::remove_dir_all(&tmp);
    let version = r?;
    outln!(
        "installed bentwire-classics {version} in {}",
        dest.display()
    );
    // A running daemon rescans when it lists its characters; none is started for this.
    let running = |c: &Client| {
        c.conn
            .call_method(
                Some("org.freedesktop.DBus"),
                "/org/freedesktop/DBus",
                Some("org.freedesktop.DBus"),
                "NameHasOwner",
                &(BUS_NAME,),
            )
            .is_ok_and(|m| m.body().deserialize::<bool>().unwrap_or(false))
    };
    if let Ok(c) = Client::new(false)
        && running(&c)
        && let Ok((list,)) = c.call::<_, (Vec<(String, String, String)>,)>(
            AGENT_PATH,
            AGENT_IFACE,
            "ListCharacters",
            &(),
        )
    {
        outln!("bentwired now has {} characters", list.len());
    }
    Ok(())
}

/// Download, check and unpack into `tmp`, then swap `tmp/unpack/classics` into `dest`.
/// Returns the release version.
fn install_into(tmp: &Path, dest: &Path) -> Result<String, Fail> {
    let base = std::env::var("BENTWIRE_CLASSICS_URL").unwrap_or_else(|_| CLASSICS_URL.into());
    let base = base.trim_end_matches('/');
    let sums = tmp.join("sha256sums.txt");
    download(&format!("{base}/sha256sums.txt"), &sums)?;
    let text = std::fs::read_to_string(&sums).map_err(|e| fail(format!("sha256sums.txt: {e}")))?;
    let (hash, name) = text
        .lines()
        .filter_map(|l| l.split_once(char::is_whitespace))
        .map(|(h, n)| (h.to_ascii_lowercase(), n.trim().trim_start_matches('*')))
        .find(|(_, n)| {
            n.starts_with("bentwire-classics-") && n.ends_with(".tar.gz") && !n.contains('/')
        })
        .ok_or_else(|| fail("sha256sums.txt names no bentwire-classics-*.tar.gz"))?;
    let version = name["bentwire-classics-".len()..name.len() - ".tar.gz".len()].to_string();
    let archive = tmp.join(name);
    download(&format!("{base}/{name}"), &archive)?;
    if sha256_hex(&archive)? != hash {
        return Err(fail(format!(
            "{name}: SHA-256 does not match sha256sums.txt"
        )));
    }
    let list = Command::new("tar")
        .arg("-tzf")
        .arg(&archive)
        .output()
        .map_err(|e| fail(format!("cannot run tar: {e}")))?;
    let names = String::from_utf8_lossy(&list.stdout);
    if !list.status.success()
        || names.lines().any(|n| {
            !(n == "classics/" || n.starts_with("classics/")) || n.split('/').any(|c| c == "..")
        })
    {
        return Err(fail(format!("{name}: not a classics pack archive")));
    }
    let unpack = tmp.join("unpack");
    std::fs::create_dir(&unpack).map_err(|e| fail(format!("{}: {e}", unpack.display())))?;
    run_tool(
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&unpack)
            .arg("--no-same-owner")
            // Extraction time as mtime: releases carry one fixed time, and discovery caches
            // by mtime and size.
            .arg("-m"),
    )?;
    let new = unpack.join("classics");
    if !new.join("pack.toml").is_file() {
        return Err(fail(format!("{name}: no classics/pack.toml")));
    }
    let old = tmp.join("old");
    let had_old = dest.symlink_metadata().is_ok();
    if had_old {
        std::fs::rename(dest, &old).map_err(|e| fail(format!("{}: {e}", dest.display())))?;
    }
    if let Err(e) = std::fs::rename(&new, dest) {
        if had_old {
            let _ = std::fs::rename(&old, dest);
        }
        return Err(fail(format!("{}: {e}", dest.display())));
    }
    Ok(version)
}

/// Local conversion (character-format 10); a `.bwc` destination writes a ZIP.
fn convert(o: &Opts) -> Result<(), Fail> {
    let opts = bentwire_convert::ConvertOptions {
        office_palette: o.value("--palette").map(Into::into),
        lang: o.value("--lang").map(Into::into),
        codepage: o.value("--codepage").map(Into::into),
    };
    let (src, dest) = (&o.positional[0], &o.positional[1]);
    bentwire_convert::convert_with(src.as_ref(), dest.as_ref(), &opts)
        .map_err(|e| Fail::Exit(1, Some(format!("{src}: {e}"))))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match run(&args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(Fail::Usage(m)) => {
            if !m.is_empty() {
                eprintln!("bentwire: {m}");
            }
            eprint!("{USAGE}");
            ExitCode::from(1)
        }
        Err(Fail::Dbus(name, msg)) => {
            eprintln!("{name}: {msg}");
            ExitCode::from(1)
        }
        Err(Fail::Exit(code, msg)) => {
            if let Some(m) = msg {
                eprintln!("bentwire: {m}");
            }
            ExitCode::from(code)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_5_5_select_hints() {
        let ids: Vec<String> = ["bentwire-placeholder", "merlin", "merlin-15", "genie"]
            .map(String::from)
            .to_vec();
        assert_eq!(distance("merln", "merlin"), 1);
        assert_eq!(distance("", "abc"), 3);
        assert_eq!(distance("kitten", "sitting"), 3);
        assert_eq!(
            select_hints("Merln", &ids),
            [
                "did you mean merlin?",
                "more characters: bentwire characters install classics"
            ]
        );
        assert_eq!(
            select_hints("merlin", &ids)[0],
            "did you mean merlin, merlin-15?"
        );
        let mut with = ids.clone();
        with.push("clippit".into());
        assert_eq!(select_hints("clippy", &with), ["did you mean clippit?"]);
        assert!(select_hints("zzzzzz", &with).is_empty());
    }

    #[test]
    fn b_l10n_2_b_ask_2_result_names() {
        assert_eq!(result_name(-1), "ok");
        assert_eq!(result_name(-15), "yes-to-all");
        assert_eq!(result_name(-12), "close");
        assert_eq!(result_name(-25), "button:1");
        assert_eq!(result_name(-30), "button:6");
        assert_eq!(result_name(2), "label:2");
        assert_eq!(result_name(0), "none");
    }
}
