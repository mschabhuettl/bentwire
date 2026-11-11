//! D-Bus end to end (spec T-5): `bentwired --headless` on a private bus, driven by
//! clients, the CLI and org.bentwire.Test1. Test names carry the rule IDs.

mod common;

use common::*;
use serde_json::json;
use zbus::zvariant::Value;

/// Daemon started and its startup Show (B-CFG-3) finished.
fn ready(tag: &str) -> Env {
    let mut e = Env::new(tag);
    e.start(&[]);
    e.advance(1000);
    e
}

#[test]
fn b_run_1_second_instance_exits_already_running() {
    let e = ready("run1");
    let o = e.daemon_cmd(&[]).output().unwrap();
    assert_eq!(o.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&o.stderr).contains("already running"));
}

#[test]
fn b_run_1_quit_and_sigterm_save_state_and_exit_0() {
    let mut e = ready("run1q");
    let c = e.client();
    c.call_id("MoveTo", &(10i32, 20i32, 0u32)).unwrap();
    e.advance(1000);
    c.call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
    let st = std::fs::read_to_string(e.dir.join("state/bentwire/state.toml")).unwrap();
    assert!(st.contains("left = 10") && st.contains("top = 20"), "{st}");
    assert!(
        st.contains("character = \"pebble\"") && st.contains("visible = true"),
        "{st}"
    );
    // SIGTERM: same, without animation.
    e.start(&[]);
    let pid = e.daemon.as_ref().unwrap().id().to_string();
    std::process::Command::new("kill")
        .args(["-TERM", &pid])
        .status()
        .unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
}

#[test]
fn b_q_1_reply_before_signal_and_b_q_2_unicast() {
    let e = ready("q1");
    let mut a = e.client();
    let mut b = e.client();
    let serial = a.send("Play", &("Wave",));
    e.advance(2000);
    let seen = a.drain(300).to_vec();
    let reply = seen
        .iter()
        .position(|r| r.kind == "return" && r.reply_serial == Some(serial))
        .expect("reply");
    let start = seen
        .iter()
        .position(|r| r.member == "RequestStart")
        .expect("RequestStart");
    assert!(reply < start, "reply must precede RequestStart: {seen:#?}");
    let id = seen[start].args[0].as_u64().unwrap();
    assert_eq!(a.complete(id as u32), (0, String::new()));
    // Unicast: addressed to A's unique name, never seen by B.
    assert_eq!(seen[start].dest.as_deref(), Some(a.unique_name().as_str()));
    b.drain(300);
    assert!(
        !b.members().iter().any(|m| m.starts_with("Request")),
        "B saw {:?}",
        b.members()
    );
}

#[test]
fn b_q_2_removed_before_start_gets_only_complete_status_3() {
    let e = ready("q2");
    let mut c = e.client();
    let looping = c.call_id("Play", &("Loop",)).unwrap();
    let queued = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(500);
    c.call_unit(CHAR, CHAR_IF, "Stop", &(0u32,)).unwrap();
    e.advance(1000);
    assert_eq!(
        c.complete(looping),
        (3, "org.bentwire.Error.Interrupted".into())
    );
    assert_eq!(
        c.complete(queued),
        (3, "org.bentwire.Error.Interrupted".into())
    );
    let starts: Vec<_> = c
        .signals()
        .into_iter()
        .filter(|s| s.0 == "RequestStart")
        .map(|s| s.1[0].as_u64().unwrap() as u32)
        .collect();
    assert!(
        starts.contains(&looping) && !starts.contains(&queued),
        "{starts:?}"
    );
}

#[test]
fn b_q_5_stopall_unknown_type_is_invalid_args() {
    let e = ready("q5");
    let c = e.client();
    let r = c.call_unit(CHAR, CHAR_IF, "StopAll", &(vec!["dance"],));
    assert_eq!(r, Err("org.freedesktop.DBus.Error.InvalidArgs".into()));
}

#[test]
fn spec_5_4_method_errors() {
    let e = ready("err");
    let c = e.client();
    assert_eq!(
        c.call_id("Play", &("NoSuchThing",)),
        Err("org.bentwire.Error.UnknownAnimation".into())
    );
    assert_eq!(
        c.call_unit(AGENT, AGENT_IF, "Select", &("nobody",)),
        Err("org.bentwire.Error.NoSuchCharacter".into())
    );
    assert_eq!(
        c.ask(&[
            ("text", Value::from("x")),
            ("mode", Value::from("modeless"))
        ]),
        Err("org.bentwire.Error.NotSupported".into())
    );
    assert_eq!(
        c.ask(&[("text", Value::from("x")), ("buttons", Value::from("none"))]),
        Err("org.bentwire.Error.InvalidBalloon".into())
    );
    assert_eq!(
        c.ask(&[("text", Value::from(7u32))]),
        Err("org.freedesktop.DBus.Error.InvalidArgs".into())
    );
}

#[test]
fn spec_5_2_5_3_properties_and_properties_changed() {
    let e = ready("props");
    assert_eq!(e.character(), "pebble");
    let v = String::try_from(e.prop(AGENT, AGENT_IF, "Version")).unwrap();
    assert_eq!(v, env!("CARGO_PKG_VERSION"));
    assert_eq!(
        String::try_from(e.prop(CHAR, CHAR_IF, "Name")).unwrap(),
        "Pebble"
    );
    assert!(bool::try_from(e.prop(CHAR, CHAR_IF, "Visible")).unwrap());
    let names = <Vec<String>>::try_from(e.prop(CHAR, CHAR_IF, "AnimationNames")).unwrap();
    assert!(names.contains(&"Wave".to_string()));
    // PropertiesChanged on a move.
    let conn = connect(&e.addr);
    conn.call_method(
        Some("org.freedesktop.DBus"),
        "/org/freedesktop/DBus",
        Some("org.freedesktop.DBus"),
        "AddMatch",
        &("type='signal',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged'",),
    )
    .unwrap();
    let it = zbus::blocking::MessageIterator::from(&conn);
    let c = e.client();
    c.call_id("MoveTo", &(5i32, 6i32, 0u32)).unwrap();
    e.advance(1000);
    let mut found = false;
    for m in it.flatten().take(20) {
        if m.header().member().map(|x| x.as_str()) != Some("PropertiesChanged") {
            continue;
        }
        let (iface, changed, _): (
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            Vec<String>,
        ) = m.body().deserialize().unwrap();
        if iface == CHAR_IF && changed.contains_key("Left") {
            assert_eq!(i32::try_from(&changed["Left"]).unwrap(), 5);
            found = true;
            break;
        }
    }
    assert!(found);
    // IdleOn is writable.
    e.conn
        .call_method(
            Some("org.bentwire.Agent"),
            CHAR,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(CHAR_IF, "IdleOn", Value::from(false)),
        )
        .unwrap();
    assert!(!bool::try_from(e.prop(CHAR, CHAR_IF, "IdleOn")).unwrap());
}

#[test]
fn spec_5_5_cli_output_and_exit_codes() {
    let e = ready("cli");
    // status
    let (code, so, _) = out(&e.cli(&["status"]));
    assert_eq!(code, 0);
    assert!(
        so.starts_with("character=pebble\nvisible=true\nleft="),
        "{so}"
    );
    assert!(so.contains("idle=true"), "{so}");
    // characters: current marked with `*`
    let (_, so, _) = out(&e.cli(&["characters"]));
    assert!(so.lines().any(|l| l == "pebble\tPebble\t*"), "{so}");
    // animations
    let (_, so, _) = out(&e.cli(&["animations"]));
    assert!(so.lines().any(|l| l == "Wave"), "{so}");
    // play prints the id
    let (code, so, _) = out(&e.cli(&["play", "Wave"]));
    assert_eq!(code, 0);
    assert!(so.trim().parse::<u32>().unwrap() > 0);
    e.advance(1000);
    // method error: exit 1, error name on stderr
    let (code, _, se) = out(&e.cli(&["play", "Nope"]));
    assert_eq!(code, 1);
    assert!(se.contains("org.bentwire.Error.UnknownAnimation"), "{se}");
    let (code, _, se) = out(&e.cli(&["select", "nobody"]));
    assert_eq!(code, 1);
    assert!(se.contains("org.bentwire.Error.NoSuchCharacter"), "{se}");
    // usage error: exit 1
    assert_eq!(out(&e.cli(&["play"])).0, 1);
    assert_eq!(out(&e.cli(&["frobnicate"])).0, 1);
    // --wait: 0 on success
    let mut w = e.cli_bg(&["play", "Wave", "--wait"]);
    started(&mut w);
    e.advance(2000);
    assert_eq!(finish(w).0, 0);
    // --wait on an interrupted request: 3
    let mut w = e.cli_bg(&["play", "Wave", "--wait"]);
    started(&mut w);
    e.advance(100);
    assert_eq!(out(&e.cli(&["stop"])).0, 0);
    e.advance(300);
    let (code, _, se) = finish(w);
    assert_eq!(code, 3, "{se}");
    // a failed request: 2 (Speak while hidden fails with Hidden, R-SPEAK-2)
    assert_eq!(out(&e.cli(&["hide", "--fast"])).0, 0);
    e.advance(500);
    let w = e.cli_bg(&["say", "hello", "--wait"]);
    let (code, _, se) = e.finish_advancing(w);
    assert_eq!(code, 2, "{se}");
    assert!(se.contains("org.bentwire.Error.Hidden"), "{se}");
}

#[test]
fn spec_5_5_cli_ask_results() {
    let e = ready("cliask");
    let ask_open = |e: &Env| {
        for _ in 0..50 {
            e.advance(200);
            if !e.snapshot()["office_balloon"].is_null() {
                return;
            }
        }
        panic!("ask balloon did not open");
    };
    // Set button: Enter activates the focused (first) button.
    let w = e.cli_bg(&[
        "ask",
        "Continue?",
        "--buttons",
        "yes-no",
        "--checkbox",
        "&Remember",
        "--animation",
        "",
    ]);
    ask_open(&e);
    e.key("space"); // focus starts on the first button; Space on a button does nothing
    e.key("Enter");
    let (code, so, _) = finish(w);
    assert_eq!((code, so.as_str()), (0, "yes\nchecked:0\n"));
    // Custom buttons: click the second.
    let w = e.cli_bg(&[
        "ask",
        "Build failed.",
        "--button",
        "Show log",
        "--button",
        "Ignore",
    ]);
    ask_open(&e);
    let r = &e.snapshot()["office_balloon"]["buttons"][1]["rect"];
    let (x, y) = (
        r["x"].as_i64().unwrap() as i32 + 5,
        r["y"].as_i64().unwrap() as i32 + 5,
    );
    e.click(1, x, y);
    assert_eq!(finish(w), (0, "button:2\n".into(), String::new()));
    // Autodown dismissed with Esc: `none`, exit 3.
    let w = e.cli_bg(&["ask", "Tip", "--mode", "autodown", "--buttons", "ok"]);
    ask_open(&e);
    e.key("Escape");
    let (code, so, _) = finish(w);
    assert_eq!((code, so.as_str()), (3, "none\n"));
}

#[test]
fn spec_5_5_cli_not_running() {
    let e = Env::new("norun");
    let (code, _, se) = out(&e.cli(&["status"]));
    assert_eq!(code, 1);
    assert!(se.contains("bentwired is not running"), "{se}");
}

#[test]
fn b_run_1_dbus_activation_starts_the_daemon() {
    let e = Env::new("activ");
    let fx = fixtures();
    std::fs::write(
        e.dir.join("services/org.bentwire.Agent.service"),
        format!(
            "[D-BUS Service]\nName=org.bentwire.Agent\nExec={} --headless --characters {}\n",
            daemon_bin().display(),
            fx.display()
        ),
    )
    .unwrap();
    let (code, so, se) = out(&e.cli(&["status"]));
    assert_eq!(code, 0, "{se}");
    assert!(so.starts_with("character=pebble"), "{so}");
    // Activated daemons inherit the bus's environment, so stop it explicitly.
    assert_eq!(out(&e.cli(&["quit"])).0, 0);
}

#[test]
fn b_ui_4_user_hide_from_menu_interrupts_with_hidden_by_user() {
    let e = ready("uhide");
    let mut c = e.client();
    let id = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(100);
    let s = e.snapshot();
    let (x, y) = (
        s["left"].as_i64().unwrap() as i32 + 16,
        s["top"].as_i64().unwrap() as i32 + 16,
    );
    e.click(2, x, y);
    let s = e.snapshot();
    let items = s["menu"].as_array().expect("menu open").clone();
    let hide = items
        .iter()
        .find(|i| i["label"] == "&Hide")
        .expect("Hide item");
    e.click(
        1,
        hide["x"].as_i64().unwrap() as i32 + 3,
        hide["y"].as_i64().unwrap() as i32 + 3,
    );
    e.advance(2000);
    assert_eq!(
        c.complete(id),
        (3, "org.bentwire.Error.HiddenByUser".into())
    );
    c.wait_for("Hidden(7)", |r| {
        r.member == "Hidden" && r.args == json!([7])
    });
    assert!(e.snapshot()["surfaces"].as_array().unwrap().is_empty());
}

#[test]
fn b_ui_1_click_and_r_ui_2_drag_signals() {
    let e = ready("click");
    let mut c = e.client();
    let s = e.snapshot();
    let (x, y) = (
        s["left"].as_i64().unwrap() as i32 + 16,
        s["top"].as_i64().unwrap() as i32 + 16,
    );
    e.click(1, x, y);
    c.wait_for("Click", |r| {
        r.member == "Click" && r.args == json!([1, 0, x, y, false])
    });
    e.advance(1000);
    // Drag: press, move 10 px, release.
    e.pointer("press", 1, x, y);
    e.pointer("motion", 1, x + 10, y + 5);
    e.pointer("release", 1, x + 10, y + 5);
    c.wait_for("DragStart", |r| r.member == "DragStart");
    c.wait_for("DragComplete", |r| r.member == "DragComplete");
    c.wait_for("Move cause 1", |r| r.member == "Move" && r.args[2] == 1);
    let s = e.snapshot();
    assert_eq!(s["left"].as_i64().unwrap() as i32, x - 16 + 10);
}

#[test]
fn b_wl_4_output_removal_pulls_the_frame_back() {
    let e = ready("wl4");
    let mut c = e.client();
    let o: u32 = e
        .test("AddOutput", &("640x480+1920+0",))
        .body()
        .deserialize()
        .unwrap();
    c.call_id("MoveTo", &(2400i32, 300i32, 0u32)).unwrap();
    e.advance(1000);
    let s = e.snapshot();
    assert_eq!(s["surfaces"][0]["output"], o);
    e.test("RemoveOutput", &(o,));
    e.advance(100);
    c.wait_for("Move cause 4", |r| r.member == "Move" && r.args[2] == 4);
    let s = e.snapshot();
    let left = s["left"].as_i64().unwrap();
    assert!((0..=1920 - 32).contains(&left), "{s}");
}

#[test]
fn t_4_trace_and_dump_frames() {
    let mut e = Env::new("trace");
    let trace = e.dir.join("trace.jsonl");
    let frames = e.dir.join("frames");
    e.start(&[
        "--trace",
        trace.to_str().unwrap(),
        "--dump-frames",
        frames.to_str().unwrap(),
        "--outputs",
        "800x600+0+0",
    ]);
    e.advance(1000);
    let c = e.client();
    c.call_id("Speak", &("Hi",)).unwrap();
    e.advance(500);
    let t = std::fs::read_to_string(&trace).unwrap();
    let lines: Vec<serde_json::Value> = t
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let layer = lines
        .iter()
        .find(|l| l["req"] == "layer_surface")
        .expect("layer_surface");
    assert_eq!(layer["layer"], "top");
    assert_eq!(layer["anchor"], "top|left");
    assert_eq!(layer["exclusive_zone"], -1);
    assert_eq!(layer["namespace"], "bentwire");
    // B-CFG-3 first run: 48 px from the right, 96 px from the bottom.
    assert_eq!(
        layer["margin"],
        json!({"left": 800 - 48 - 32, "top": 600 - 96 - 32})
    );
    assert!(
        lines
            .iter()
            .any(|l| l["req"] == "subsurface" && l["surface"] == "agent_balloon")
    );
    assert!(
        lines
            .iter()
            .any(|l| l["type"] == "signal" && l["member"] == "Shown")
    );
    assert!(std::fs::read_dir(&frames).unwrap().count() > 0);
    // A-DESK-4: accepted calls with method, arguments and request id.
    let call = lines
        .iter()
        .find(|l| l["type"] == "call" && l["method"] == "Speak")
        .expect("call line");
    assert_eq!(call["args"], json!(["Hi"]));
    assert!(call["id"].as_u64().unwrap() > 0);
    assert!(call["sender"].as_str().unwrap().starts_with(':'));
}

#[test]
fn b_cfg_1_sighup_reloads_language() {
    let e = ready("hup");
    let c = e.client();
    let list = |c: &Client| -> Vec<(String, String, String)> {
        c.conn
            .call_method(
                Some("org.bentwire.Agent"),
                AGENT,
                Some(AGENT_IF),
                "ListCharacters",
                &(),
            )
            .unwrap()
            .body()
            .deserialize()
            .unwrap()
    };
    assert_eq!(list(&c)[0].1, "Pebble");
    std::fs::create_dir_all(e.dir.join("cfg/bentwire")).unwrap();
    std::fs::write(
        e.dir.join("cfg/bentwire/config.toml"),
        "language = \"de\"\n",
    )
    .unwrap();
    let pid = e.daemon.as_ref().unwrap().id().to_string();
    std::process::Command::new("kill")
        .args(["-HUP", &pid])
        .status()
        .unwrap();
    let t = std::time::Instant::now();
    while list(&c)[0].1 != "Kiesel" {
        assert!(t.elapsed().as_secs() < 5, "language not reloaded");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[test]
fn a_char_1_only_builtin_installed() {
    let mut e = Env::new("builtin");
    let builtin = repo().join("characters/builtin");
    assert!(
        builtin.join("pack.toml").is_file(),
        "characters/builtin missing"
    );
    let trace = e.dir.join("trace.jsonl");
    e.start_raw(&[
        "--characters",
        builtin.to_str().unwrap(),
        "--trace",
        trace.to_str().unwrap(),
    ]);
    e.advance(3000);
    assert_eq!(e.character(), "bentwire-placeholder");
    // T-4: no audio device; sounds are logged with their decoded duration.
    let t = std::fs::read_to_string(&trace).unwrap();
    let pop = t
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|l| l["type"] == "sound_start")
        .expect("the startup Show plays a sound");
    assert_eq!(pop["sound"], "pop");
    assert!(pop["duration_ms"].as_u64().unwrap() > 0, "{pop}");
    let c = e.client();
    let mut c = c;
    let id = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(10000);
    assert_eq!(c.complete(id).0, 0);
    assert!(bool::try_from(e.prop(CHAR, CHAR_IF, "Visible")).unwrap());
}

#[test]
fn r_q_6_stop_lets_next_request_run_after_loop() {
    let e = ready("q6");
    let mut c = e.client();
    let looping = c.call_id("Play", &("Loop",)).unwrap();
    let next = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(300);
    // Stop(id) of the loop only: the queued Wave must then run.
    c.call_unit(CHAR, CHAR_IF, "Stop", &(looping,)).unwrap();
    e.advance(2000);
    assert_eq!(
        c.complete(looping),
        (3, "org.bentwire.Error.Interrupted".into())
    );
    assert_eq!(c.complete(next), (0, String::new()));
    // B-UI-4: user Hide during a loop interrupts it and then hides.
    let looping = c.call_id("Play", &("Loop",)).unwrap();
    e.advance(300);
    let s = e.snapshot();
    let (x, y) = (
        s["left"].as_i64().unwrap() as i32 + 16,
        s["top"].as_i64().unwrap() as i32 + 16,
    );
    e.click(2, x, y);
    let s = e.snapshot();
    let hide = s["menu"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "&Hide")
        .unwrap()
        .clone();
    e.click(
        1,
        hide["x"].as_i64().unwrap() as i32 + 3,
        hide["y"].as_i64().unwrap() as i32 + 3,
    );
    e.advance(2000);
    assert_eq!(
        c.complete(looping),
        (3, "org.bentwire.Error.HiddenByUser".into())
    );
    c.wait_for("Hidden(7)", |r| {
        r.member == "Hidden" && r.args == json!([7])
    });
}

/// The centre of the character surface.
fn centre(e: &Env) -> (i32, i32) {
    let s = e.snapshot();
    let c = &s["surfaces"][0];
    assert_eq!(c["kind"], "character", "{s}");
    let v = |k: &str| c[k].as_i64().unwrap() as i32;
    (v("x") + v("w") / 2, v("y") + v("h") / 2)
}

/// Opens an ask balloon and advances until it is shown; returns the request id.
fn open_ask(e: &Env, c: &Client, mode: &str) -> u32 {
    let id = c
        .ask(&[
            ("text", Value::from("Question?")),
            ("buttons", Value::from("ok-cancel")),
            ("mode", Value::from(mode)),
            ("animation", Value::from("")),
        ])
        .unwrap();
    for _ in 0..50 {
        e.advance(100);
        if !e.snapshot()["office_balloon"].is_null() {
            return id;
        }
    }
    panic!("ask balloon did not open");
}

fn ask_complete(c: &mut Client, id: u32) -> (i64, u64, String) {
    let a = c.wait_for("AskComplete", |r| {
        r.member == "AskComplete" && r.args[0] == id
    });
    let (status, reason) = c.complete(id);
    (a.args[1].as_i64().unwrap(), status, reason)
}

fn balloon_open(e: &Env) -> bool {
    !e.snapshot()["office_balloon"].is_null()
}

#[test]
fn b_ask_7_play_modal_stays_autodown_dismissed() {
    let e = ready("ask7a");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "modal");
    let p = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(2000);
    assert_eq!(c.complete(p).0, 0);
    assert!(balloon_open(&e), "modal ask stays during Play");
    e.key("Escape"); // ok-cancel: Esc activates Cancel
    e.advance(200);
    assert_eq!(ask_complete(&mut c, ask), (-2, 0, String::new()));

    let ask = open_ask(&e, &c, "autodown");
    let p = c.call_id("Play", &("Wave",)).unwrap();
    e.advance(2000);
    assert_eq!(ask_complete(&mut c, ask), (0, 0, String::new()));
    assert_eq!(c.complete(p).0, 0);
    // The dismissal comes before the Play starts.
    let order: Vec<String> = c
        .signals()
        .into_iter()
        .filter(|(m, a)| (m == "AskComplete" && a[0] == ask) || (m == "RequestStart" && a[0] == p))
        .map(|s| s.0)
        .collect();
    assert_eq!(order, ["AskComplete", "RequestStart"]);
}

#[test]
fn b_ask_7_speak_waits_for_modal_ask() {
    let e = ready("ask7b");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "modal");
    let s = c.call_id("Speak", &("later",)).unwrap();
    e.advance(2000);
    c.drain(200);
    assert!(
        !c.signals()
            .iter()
            .any(|(m, a)| m == "RequestStart" && a[0] == s),
        "Speak must wait: {:?}",
        c.signals()
    );
    e.key("Enter"); // focus starts on the first button: OK
    e.advance(200);
    assert_eq!(ask_complete(&mut c, ask), (-1, 0, String::new()));
    e.advance(10000);
    c.wait_for("Speak starts", |r| {
        r.member == "RequestStart" && r.args[0] == s
    });
}

#[test]
fn b_ask_7_hide_stop_and_stopall_close_the_ask() {
    let e = ready("ask7c");
    let mut c = e.client();
    // StopAll without "ask": no effect.
    let ask = open_ask(&e, &c, "modal");
    c.call_unit(CHAR, CHAR_IF, "StopAll", &(vec!["play", "speak"],))
        .unwrap();
    e.advance(200);
    assert!(balloon_open(&e));
    // Stop(0): result 0, status 3, Interrupted.
    c.call_unit(CHAR, CHAR_IF, "Stop", &(0u32,)).unwrap();
    e.advance(200);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.Interrupted".into())
    );
    // StopAll(["ask"]).
    let ask = open_ask(&e, &c, "autodown");
    c.call_unit(CHAR, CHAR_IF, "StopAll", &(vec!["ask"],))
        .unwrap();
    e.advance(200);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.Interrupted".into())
    );
    // A Hide reaching execution: status 3, Hidden.
    let ask = open_ask(&e, &c, "modal");
    let h = c.call_id("Hide", &(false,)).unwrap();
    e.advance(2000);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.Hidden".into())
    );
    assert_eq!(c.complete(h).0, 0);
}

#[test]
fn b_ask_7_drag_hides_and_replaces_modal_balloon() {
    let e = ready("ask7d");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "modal");
    let s = e.snapshot();
    let (x, y) = (
        s["left"].as_i64().unwrap() as i32 + 16,
        s["top"].as_i64().unwrap() as i32 + 16,
    );
    e.pointer("press", 1, x, y);
    e.pointer("motion", 1, x - 40, y - 30);
    assert!(!balloon_open(&e), "balloon hides at drag start");
    e.pointer("release", 1, x - 40, y - 30);
    e.advance(200);
    assert!(balloon_open(&e), "balloon placed again at drag end");
    e.key("Escape");
    e.advance(200);
    assert_eq!(ask_complete(&mut c, ask).0, -2);
}

#[test]
fn b_ask_7_b_char_5_character_switch_closes_ask() {
    let mut e = Env::new("ask7e");
    let builtin = repo().join("characters/builtin");
    e.start(&["--characters", builtin.to_str().unwrap()]);
    e.advance(3000);
    let mut c = e.client();
    let before = e.character();
    let other = if before == "pebble" {
        "bentwire-placeholder"
    } else {
        "pebble"
    };
    let ask = open_ask(&e, &c, "modal");
    let left = e.snapshot()["left"].clone();
    c.call_unit(AGENT, AGENT_IF, "Select", &(other,)).unwrap();
    e.advance(3000);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.CharacterChanged".into())
    );
    assert_eq!(e.character(), other);
    // B-CHAR-5: the new character takes the old top-left.
    assert_eq!(e.snapshot()["left"], left);
}

#[test]
fn r_speak_1_builtin_placeholder_say_wait_completes() {
    // Only the builtin pack: its Speaking animation must end, or phase 1 never does.
    let mut e = Env::new("bisay");
    let builtin = repo().join("characters/builtin");
    e.start_raw(&["--characters", builtin.to_str().unwrap()]);
    e.advance(3000);
    assert_eq!(e.character(), "bentwire-placeholder");
    let mut c = e.client();
    let w = e.cli_bg(&["say", "hello", "--wait"]);
    let (code, _, se) = e.finish_advancing(w);
    assert_eq!(code, 0, "{se}");
    e.advance(10000);
    c.drain(200);
    let m = c.members();
    assert!(m.iter().any(|s| s == "BalloonShown"), "{m:?}");
    assert!(m.iter().any(|s| s == "BalloonHidden"), "{m:?}");
}

#[test]
fn b_gal_1_gallery_next_cancel_and_ok() {
    let mut e = Env::new("gal");
    let builtin = repo().join("characters/builtin");
    e.start(&["--characters", builtin.to_str().unwrap()]);
    e.advance(3000);
    let first = e.character();
    let open_gallery = |e: &Env| {
        let (x, y) = centre(e);
        e.click(2, x, y);
        let s = e.snapshot();
        let item = s["menu"]
            .as_array()
            .expect("menu")
            .iter()
            .find(|i| i["label"] == "&Choose Assistant...")
            .expect("Choose Assistant item")
            .clone();
        e.click(
            1,
            item["x"].as_i64().unwrap() as i32 + 3,
            item["y"].as_i64().unwrap() as i32 + 3,
        );
        e.advance(500);
        let s = e.snapshot();
        assert!(!s["office_balloon"].is_null(), "gallery balloon");
        s
    };
    let button = |s: &serde_json::Value, caption: &str| -> (i32, i32) {
        let b = s["office_balloon"]["buttons"]
            .as_array()
            .unwrap()
            .iter()
            .find(|b| b["caption"].as_str().unwrap().contains(caption))
            .unwrap_or_else(|| panic!("button {caption}: {s}"))
            .clone();
        let r = &b["rect"];
        (
            r["x"].as_i64().unwrap() as i32 + 5,
            r["y"].as_i64().unwrap() as i32 + 5,
        )
    };
    let s = open_gallery(&e);
    let captions: Vec<String> = s["office_balloon"]["buttons"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b["caption"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(captions, ["<Back", "Next>", "OK", "Cancel"]);
    // Next switches the live character; the balloon stays.
    let (x, y) = button(&s, "Next");
    e.click(1, x, y);
    e.advance(3000);
    let second = e.character();
    assert_ne!(second, first);
    let s = e.snapshot();
    assert!(
        !s["office_balloon"].is_null(),
        "gallery survives its own switch"
    );
    // Cancel switches back.
    let (x, y) = button(&s, "Cancel");
    e.click(1, x, y);
    e.advance(3000);
    assert_eq!(e.character(), first);
    assert!(e.snapshot()["office_balloon"].is_null());
    // Next + OK keeps and persists the choice.
    let s = open_gallery(&e);
    let (x, y) = button(&s, "Next");
    e.click(1, x, y);
    e.advance(3000);
    let s = e.snapshot();
    let (x, y) = button(&s, "OK");
    e.click(1, x, y);
    e.advance(3000);
    assert_eq!(e.character(), second);
    let c = e.client();
    c.call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
    let st = std::fs::read_to_string(e.dir.join("state/bentwire/state.toml")).unwrap();
    assert!(st.contains(&format!("character = \"{second}\"")), "{st}");
}

#[test]
fn spec_5_5_cli_wait_ends_when_the_daemon_exits() {
    let mut e = ready("cliquit");
    let mut w = e.cli_bg(&["play", "Loop", "--wait"]);
    started(&mut w);
    e.advance(300);
    e.client().call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
    let (code, _, se) = finish(w);
    assert_eq!(code, 1, "{se}");
    assert!(se.contains("bentwired exited"), "{se}");
}

#[test]
fn b_ask_7_moveto_modal_replaced_autodown_dismissed() {
    let e = ready("ask7f");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "modal");
    let m = c.call_id("MoveTo", &(300i32, 300i32, 1000u32)).unwrap();
    let mut hidden_during_slide = false;
    for _ in 0..30 {
        e.advance(100);
        if e.snapshot()["office_balloon"].is_null() {
            hidden_during_slide = true;
        }
    }
    assert_eq!(c.complete(m).0, 0);
    assert!(
        hidden_during_slide,
        "balloon hides while the slide moves the frame"
    );
    assert!(balloon_open(&e), "and is shown again at the end");
    e.key("Escape");
    e.advance(200);
    assert_eq!(ask_complete(&mut c, ask).0, -2);
    let ask = open_ask(&e, &c, "autodown");
    let m = c.call_id("MoveTo", &(100i32, 100i32, 0u32)).unwrap();
    e.advance(1000);
    assert_eq!(ask_complete(&mut c, ask), (0, 0, String::new()));
    assert_eq!(c.complete(m).0, 0);
}

#[test]
fn b_ask_7_autodown_speak_and_second_ask_dismiss() {
    let e = ready("ask7g");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "autodown");
    let s = c.call_id("Speak", &("now",)).unwrap();
    e.advance(500);
    assert_eq!(ask_complete(&mut c, ask), (0, 0, String::new()));
    c.wait_for("Speak starts", |r| {
        r.member == "RequestStart" && r.args[0] == s
    });
    e.advance(10000);
    assert_eq!(c.complete(s).0, 0);
    let first = open_ask(&e, &c, "autodown");
    let second = open_ask(&e, &c, "modal");
    assert_eq!(ask_complete(&mut c, first), (0, 0, String::new()));
    assert!(balloon_open(&e));
    e.key("Escape");
    e.advance(200);
    assert_eq!(ask_complete(&mut c, second).0, -2);
}

#[test]
fn b_ask_7_user_hide_closes_ask_with_hidden_by_user() {
    let e = ready("ask7h");
    let mut c = e.client();
    let ask = open_ask(&e, &c, "modal");
    let (x, y) = centre(&e);
    e.click(2, x, y);
    let s = e.snapshot();
    let hide = s["menu"]
        .as_array()
        .expect("menu")
        .iter()
        .find(|i| i["label"] == "&Hide")
        .unwrap()
        .clone();
    e.click(
        1,
        hide["x"].as_i64().unwrap() as i32 + 3,
        hide["y"].as_i64().unwrap() as i32 + 3,
    );
    e.advance(2000);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.HiddenByUser".into())
    );
}

#[test]
fn b_ask_7_output_change_keeps_the_ask() {
    let e = ready("ask7i");
    let c = e.client();
    open_ask(&e, &c, "modal");
    let o: u32 = e
        .test("AddOutput", &("800x600+1920+0",))
        .body()
        .deserialize()
        .unwrap();
    e.advance(200);
    assert!(balloon_open(&e));
    e.test("RemoveOutput", &(o,));
    e.advance(200);
    assert!(balloon_open(&e), "the balloon is placed again");
}

#[test]
fn b_menu_3_options_writes_config_and_opens_it() {
    let mut e = Env::new("opts");
    let log = e.dir.join("xdg-open.log");
    let fake = e.dir.join("bin/xdg-open");
    std::fs::write(
        &fake,
        format!("#!/bin/sh\necho \"$1\" >> {}\n", log.display()),
    )
    .unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    e.start(&[]);
    e.advance(1000);
    let (x, y) = centre(&e);
    e.click(2, x, y);
    let s = e.snapshot();
    let item = s["menu"]
        .as_array()
        .expect("menu")
        .iter()
        .find(|i| i["label"] == "&Options...")
        .unwrap()
        .clone();
    e.click(
        1,
        item["x"].as_i64().unwrap() as i32 + 3,
        item["y"].as_i64().unwrap() as i32 + 3,
    );
    e.advance(100);
    let cfg = e.dir.join("cfg/bentwire/config.toml");
    let t = std::time::Instant::now();
    while !log.exists() {
        assert!(t.elapsed().as_secs() < 5, "xdg-open not called");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    assert!(
        std::fs::read_to_string(&cfg)
            .unwrap()
            .contains("speaking_speed")
    );
    assert_eq!(
        std::fs::read_to_string(&log).unwrap().trim(),
        cfg.display().to_string()
    );
}

#[test]
fn t_4_trace_keyboard_interactivity_and_popup() {
    let mut e = Env::new("trace2");
    let trace = e.dir.join("trace.jsonl");
    e.start(&["--trace", trace.to_str().unwrap()]);
    e.advance(1000);
    let c = e.client();
    open_ask(&e, &c, "modal");
    let (x, y) = centre(&e);
    e.click(2, x, y);
    e.advance(100);
    let lines: Vec<serde_json::Value> = std::fs::read_to_string(&trace)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert!(
        lines
            .iter()
            .any(|l| l["req"] == "keyboard_interactivity" && l["value"] == "on_demand")
    );
    let popup = lines
        .iter()
        .find(|l| l["req"] == "popup")
        .expect("popup request");
    assert!(popup["grab_serial"].as_u64().unwrap() > 0);
    assert_eq!(popup["parent"], "layer_surface");
    let buf = lines.iter().find(|l| l["req"] == "buffer").unwrap();
    assert_eq!(buf["buffer_scale"], 1);
    assert_eq!(buf["format"], "argb8888");
}

/// Regression: a description with U+0000 (as in a converted Bob actor) made the bus
/// disconnect the daemon on ListCharacters, since D-Bus strings must not contain NUL.
#[test]
fn spec_5_2_list_characters_with_nul_in_character_data() {
    let mut e = Env::new("nul");
    let pack = e.dir.join("packs/characters/nulpack");
    let ch = pack.join("nully");
    std::fs::create_dir_all(ch.join("sheets")).unwrap();
    std::fs::write(
        pack.join("pack.toml"),
        "name = \"nul test\"\nlicence = \"CC0\"\n",
    )
    .unwrap();
    let src = fixtures().join("pebble");
    std::fs::copy(src.join("sheets/a.png"), ch.join("sheets/a.png")).unwrap();
    let toml = std::fs::read_to_string(src.join("character.toml"))
        .unwrap()
        .replace(
            "description = \"Synthetic test character.\"",
            "description = \"TO: \\u0003\\u0000\\u0002 end\"",
        )
        .replace("name = \"Pebble\"", "name = \"Nu\\u0000lly\"");
    assert!(toml.contains("\\u0000"));
    std::fs::write(ch.join("character.toml"), toml).unwrap();
    let packs = e.dir.join("packs/characters");
    e.start_raw(&["--characters", packs.to_str().unwrap()]);
    e.advance(1000);
    let c = e.client();
    let list: Vec<(String, String, String)> = c
        .conn
        .call_method(
            Some("org.bentwire.Agent"),
            AGENT,
            Some(AGENT_IF),
            "ListCharacters",
            &(),
        )
        .expect("ListCharacters must not kill the daemon")
        .body()
        .deserialize()
        .unwrap();
    assert_eq!(
        list,
        vec![("nully".into(), "Nully".into(), "TO: \u{3}\u{2} end".into())]
    );
    assert_eq!(
        String::try_from(e.prop(CHAR, CHAR_IF, "Name")).unwrap(),
        "Nully"
    );
    let (code, so, se) = out(&e.cli(&["characters"]));
    assert_eq!(code, 0, "{se}");
    assert_eq!(so, "nully\tNully\t*\n");
    assert!(e.name_owned(), "daemon still on the bus");
}
