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
fn b_run_1_session_bus_gone_saves_state_and_exits_0() {
    let mut e = ready("run1bus");
    let c = e.client();
    c.call_id("MoveTo", &(10i32, 20i32, 0u32)).unwrap();
    e.advance(1000);
    e.kill_bus();
    assert_eq!(e.wait_exit(10), Some(0));
    let st = std::fs::read_to_string(e.dir.join("state/bentwire/state.toml")).unwrap();
    assert!(st.contains("left = 10") && st.contains("top = 20"), "{st}");
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
    // The hint has its own test (b_char_6_install_hint_once_with_only_builtin).
    e.seed_state("install_hint_shown = true\n");
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
    let stand = e.snapshot()["rest_opaque"].clone();
    c.call_unit(AGENT, AGENT_IF, "Select", &(other,)).unwrap();
    e.advance(3000);
    assert_eq!(
        ask_complete(&mut c, ask),
        (0, 3, "org.bentwire.Error.CharacterChanged".into())
    );
    assert_eq!(e.character(), other);
    // B-CHAR-5: the new character stands on the old one's bottom centre.
    let bc = |o: &serde_json::Value| {
        let v = |k: &str| o[k].as_i64().unwrap();
        ((2 * v("x") + v("w")) / 2, v("y") + v("h"))
    };
    let now = e.snapshot()["rest_opaque"].clone();
    assert_eq!(bc(&now), bc(&stand), "{stand} -> {now}");
    assert_ne!(now["h"], stand["h"], "characters of different size");
}

#[test]
fn r_speak_1_builtin_placeholder_say_wait_completes() {
    // Only the builtin pack: its Speaking animation must end, or phase 1 never does.
    let mut e = Env::new("bisay");
    // The balloon checked here is the say's, not the B-CHAR-6 hint's.
    e.seed_state("install_hint_shown = true\n");
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

/// B-CHAR-1 on use: characters installed while the daemon runs are listed and selectable
/// without a restart; an unknown id gets close ids and the install command as hints.
#[test]
fn b_char_1_new_characters_without_restart_and_select_hints() {
    let e = ready("rescan");
    let user = e.dir.join("data/bentwire/characters");
    copy_dir(&fixtures().join("pebble"), &user.join("quartz"));
    let (code, so, se) = out(&e.cli(&["characters"]));
    assert_eq!(code, 0, "{se}");
    assert_eq!(so, "pebble\tPebble\t*\nquartz\tPebble\n");
    let (code, _, se) = out(&e.cli(&["select", "quarz"]));
    assert_eq!(code, 1);
    assert_eq!(
        se,
        "org.bentwire.Error.NoSuchCharacter: no character \"quarz\"\n\
         bentwire: did you mean quartz?\n\
         bentwire: more characters: bentwire characters install classics\n"
    );
    // Select sees a character installed after the last listing.
    copy_dir(&fixtures().join("pebble"), &user.join("slate"));
    let (code, _, se) = out(&e.cli(&["select", "slate"]));
    assert_eq!(code, 0, "{se}");
    assert_eq!(e.character(), "slate");
}

/// A fake bentwire-classics release (no Microsoft data): `classics/` holds a pack.toml, the
/// builtin placeholder under the id `clippit` and `marker` = `version`. Returns the
/// `file://` base URL for BENTWIRE_CLASSICS_URL.
fn fake_release(dir: &std::path::Path, version: &str) -> String {
    let stage = dir.join(format!("stage-{version}"));
    let pack = stage.join("classics");
    copy_dir(
        &repo().join("characters/builtin/bentwire-placeholder"),
        &pack.join("clippit"),
    );
    std::fs::write(
        pack.join("pack.toml"),
        "name = \"classics\"\nlicence = \"CC0\"\n",
    )
    .unwrap();
    std::fs::write(pack.join("marker"), version).unwrap();
    let rel = dir.join(format!("release-{version}"));
    std::fs::create_dir_all(&rel).unwrap();
    let name = format!("bentwire-classics-{version}.tar.gz");
    let st = std::process::Command::new("tar")
        .arg("-czf")
        .arg(rel.join(&name))
        .arg("-C")
        .arg(&stage)
        .arg("classics")
        .status()
        .unwrap();
    assert!(st.success());
    let sum = std::process::Command::new("sha256sum")
        .arg(&name)
        .current_dir(&rel)
        .output()
        .unwrap();
    std::fs::write(rel.join("sha256sums.txt"), sum.stdout).unwrap();
    format!("file://{}", rel.display())
}

/// spec 5.5 `characters install classics`, B-CHAR-4 default: the running daemon picks the
/// pack up and switches to clippit while the user never chose; a newer release replaces the
/// copy; a bad checksum or a git checkout leaves the installed copy alone.
#[test]
fn spec_5_5_characters_install_classics() {
    let mut e = Env::new("install");
    e.seed_state("install_hint_shown = true\n");
    let builtin = repo().join("characters/builtin");
    e.start_raw(&["--characters", builtin.to_str().unwrap()]);
    e.advance(3000);
    assert_eq!(e.character(), "bentwire-placeholder");
    let dest = e.dir.join("data/bentwire/characters/classics");
    let url = fake_release(&e.dir, "9.9.1");
    let install = |url: &str| {
        out(&e.cli_env(
            &["characters", "install", "classics"],
            &[("BENTWIRE_CLASSICS_URL", url.to_string())],
        ))
    };
    let (code, so, se) = install(&url);
    assert_eq!(code, 0, "{se}");
    assert_eq!(
        so,
        format!(
            "installed bentwire-classics 9.9.1 in {}\nbentwired now has 2 characters\n",
            dest.display()
        )
    );
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "9.9.1"
    );
    // Never chosen: the rescan made clippit the character, without a restart.
    assert_eq!(e.character(), "clippit");
    // A newer release replaces the old copy as a whole.
    std::fs::write(dest.join("stale"), "x").unwrap();
    let url2 = fake_release(&e.dir, "9.9.2");
    let (code, _, se) = install(&url2);
    assert_eq!(code, 0, "{se}");
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "9.9.2"
    );
    assert!(!dest.join("stale").exists());
    let left: Vec<_> = std::fs::read_dir(dest.parent().unwrap())
        .unwrap()
        .map(|d| d.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, ["classics"], "no temp directory left");
    // A wrong checksum fails and keeps the installed copy.
    let rel3 = url2.trim_start_matches("file://").to_string();
    std::fs::write(
        format!("{rel3}/sha256sums.txt"),
        format!("{}  bentwire-classics-9.9.2.tar.gz\n", "0".repeat(64)),
    )
    .unwrap();
    let (code, _, se) = install(&url2);
    assert_eq!(code, 1);
    assert!(se.contains("SHA-256 does not match"), "{se}");
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "9.9.2"
    );
    // Unreachable source: fails, copy kept.
    let (code, _, se) = install(&format!("file://{}/missing", e.dir.display()));
    assert_eq!(code, 1);
    assert!(se.contains("downloading"), "{se}");
    assert!(dest.join("pack.toml").is_file());
    // A git checkout is left alone.
    std::fs::create_dir(dest.join(".git")).unwrap();
    let (code, _, se) = install(&url);
    assert_eq!(code, 0, "{se}");
    assert!(se.contains("left alone"), "{se}");
    assert_eq!(
        std::fs::read_to_string(dest.join("marker")).unwrap(),
        "9.9.2"
    );
}

/// B-CHAR-6: with only the builtin placeholder, the first start says how to get the
/// classics, once (the flag is in the state file), and an explicit choice is kept.
#[test]
fn b_char_6_install_hint_once_with_only_builtin() {
    let mut e = Env::new("hint");
    let builtin = repo().join("characters/builtin");
    e.start_raw(&["--characters", builtin.to_str().unwrap()]);
    let mut seen = String::new();
    for _ in 0..100 {
        e.advance(100);
        if let Some(l) = e.snapshot()["agent_balloon_lines"].as_array() {
            seen = l
                .iter()
                .map(|x| x.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(" ");
        }
    }
    assert!(
        seen.ends_with("bentwire characters install classics"),
        "{seen:?}"
    );
    assert!(
        e.state().contains("install_hint_shown = true"),
        "{}",
        e.state()
    );
    let c = e.client();
    c.call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
    let st = e.state();
    assert!(
        st.contains("install_hint_shown = true") && st.contains("chosen = false"),
        "{st}"
    );
    // Second start: no hint.
    e.start_raw(&["--characters", builtin.to_str().unwrap()]);
    for _ in 0..100 {
        e.advance(100);
        assert!(
            e.snapshot()["agent_balloon_lines"].is_null(),
            "hint shown twice"
        );
    }
    // Not shown when other characters are installed.
    let mut e = Env::new("hint2");
    e.start(&["--characters", builtin.to_str().unwrap()]);
    for _ in 0..50 {
        e.advance(100);
        assert!(
            e.snapshot()["agent_balloon_lines"].is_null(),
            "hint with pebble"
        );
    }
}

/// Bottom centre of an `{x, y, w, h}` box of the snapshot.
fn bottom_centre(o: &serde_json::Value) -> Option<(i64, i64)> {
    let v = |k: &str| o[k].as_i64();
    Some(((2 * v("x")? + v("w")?) / 2, v("y")? + v("h")?))
}

/// The displayed character's opaque box (`None`: nothing drawn).
fn displayed(s: &serde_json::Value) -> Option<(i64, i64)> {
    let c = s["surfaces"]
        .as_array()?
        .iter()
        .find(|x| x["kind"] == "character")?;
    bottom_centre(&c["opaque"])
}

/// B-CHAR-5, B-GAL-1 on real data: a gallery walk with Next through every character under
/// $BENTWIRE_PACKS (builtin and classics in `scripts/oracle-check.sh library`). The
/// bottom centre of each character's rest picture (Test1.Snapshot `rest_opaque`, the
/// renderer's alpha shape at the character's position) stays within 2 px of the first
/// character's. The walk starts mid-screen, so nothing is clamped and nothing is exempt.
/// The displayed picture after the Wave preview is printed for information: some
/// characters end their preview in another pose than their rest picture.
#[test]
#[ignore = "needs BENTWIRE_PACKS=<characters dir>"]
fn library_gallery_walk_keeps_the_bottom_centre() {
    let Some(packs) = std::env::var_os("BENTWIRE_PACKS") else {
        panic!("NOT RUN: set BENTWIRE_PACKS to a characters/ directory with packs");
    };
    let mut e = Env::new("walk");
    std::fs::create_dir_all(e.dir.join("cfg/bentwire")).unwrap();
    std::fs::write(e.dir.join("cfg/bentwire/config.toml"), "idle = false\n").unwrap();
    e.seed_state("install_hint_shown = true\n");
    let root = std::path::PathBuf::from(packs);
    e.start_raw(&["--characters", root.to_str().unwrap()]);
    e.advance(5000);
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
        .unwrap()
        .body()
        .deserialize()
        .unwrap();
    let ids: Vec<String> = list.into_iter().map(|x| x.0).collect();
    assert!(ids.len() > 1, "{ids:?}");
    c.call_unit(AGENT, AGENT_IF, "Select", &(ids[0].as_str(),))
        .unwrap();
    c.call_id("MoveTo", &(800i32, 450i32, 0u32)).unwrap();
    e.advance(10_000);
    let (x, y) = centre(&e);
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
    e.advance(10_000);
    let s = e.snapshot();
    assert!(!s["office_balloon"].is_null(), "gallery open");
    let first = bottom_centre(&s["rest_opaque"]).unwrap_or_else(|| panic!("{s}"));
    let mut bad = Vec::new();
    let mut other_pose = Vec::new();
    let (mut worst, mut kept) = (0, 0);
    let balloon = |s: &serde_json::Value| {
        s["surfaces"]
            .as_array()
            .and_then(|v| v.iter().find(|x| x["kind"] == "office_balloon").cloned())
            .map(|b| {
                let v = |k: &str| b[k].as_i64().unwrap_or(0);
                (v("x"), v("x") + v("w"), v("y") + v("h"))
            })
    };
    let mut prev_balloon = balloon(&s);
    for id in &ids[1..] {
        e.key("n");
        e.advance(10_000);
        let s = e.snapshot();
        let b = balloon(&s);
        // Kept: same bottom edge and same left or right edge (it grows from its corner).
        if let (Some(p), Some(n)) = (prev_balloon, b)
            && n.2 == p.2
            && (n.0 == p.0 || n.1 == p.1)
        {
            kept += 1;
        }
        prev_balloon = b;
        assert_eq!(s["character"], id.as_str());
        assert!(!s["office_balloon"].is_null(), "gallery open at {id}");
        let rest = bottom_centre(&s["rest_opaque"]).unwrap_or_else(|| panic!("{id}: {s}"));
        let (dx, dy) = (rest.0 - first.0, rest.1 - first.1);
        worst = worst.max(dx.abs()).max(dy.abs());
        if dx.abs() > 2 || dy.abs() > 2 {
            bad.push(format!(
                "{id}: rest picture at {rest:?}, first at {first:?}"
            ));
        }
        match displayed(&s) {
            Some(d) if d != rest => {
                other_pose.push(format!("{id} {:+} {:+}", d.0 - rest.0, d.1 - rest.1))
            }
            Some(_) => {}
            None => other_pose.push(format!("{id} (nothing drawn)")),
        }
    }
    eprintln!(
        "{} characters walked; rest picture bottom centre at most {worst} px off; the balloon \
         stayed {kept} times; after the preview {} show another picture than their rest \
         picture: {}",
        ids.len(),
        other_pose.len(),
        other_pose.join(", ")
    );
    assert!(bad.is_empty(), "{bad:#?}");
}

// ---------- size factor (B-SCALE-1) ----------

fn set_scale(e: &Env, n: u32) -> Result<(), String> {
    e.conn
        .call_method(
            Some("org.bentwire.Agent"),
            AGENT,
            Some("org.freedesktop.DBus.Properties"),
            "Set",
            &(AGENT_IF, "Scale", Value::from(n)),
        )
        .map(|_| ())
        .map_err(|err| match err {
            zbus::Error::MethodError(name, _, _) => name.to_string(),
            other => other.to_string(),
        })
}

fn scale_of(e: &Env) -> u32 {
    u32::try_from(e.prop(AGENT, AGENT_IF, "Scale")).unwrap()
}

fn surface(s: &serde_json::Value, kind: &str) -> serde_json::Value {
    s["surfaces"]
        .as_array()
        .unwrap()
        .iter()
        .find(|x| x["kind"] == kind)
        .unwrap_or_else(|| panic!("no {kind} in {s}"))
        .clone()
}

fn int(v: &serde_json::Value) -> i64 {
    v.as_i64().unwrap()
}

/// The newest `--dump-frames` PNG as (width, RGBA).
fn last_frame(dir: &std::path::Path) -> (u32, Vec<u8>) {
    let mut names: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|d| d.unwrap().path())
        .collect();
    names.sort();
    let f = std::fs::File::open(names.last().unwrap()).unwrap();
    let mut r = png::Decoder::new(std::io::BufReader::new(f))
        .read_info()
        .unwrap();
    let mut buf = vec![0; r.output_buffer_size()];
    let info = r.next_frame(&mut buf).unwrap();
    buf.truncate(info.buffer_size());
    (info.width, buf)
}

#[test]
fn b_scale_1_test1_snapshot_and_pixels_at_1x_and_2x() {
    let mut e = Env::new("scale12");
    let frames = e.dir.join("frames");
    e.start(&[
        "--dump-frames",
        frames.to_str().unwrap(),
        "--outputs",
        "800x600+0+0",
    ]);
    e.advance(1000);
    assert_eq!(scale_of(&e), 1);
    let s1 = e.snapshot();
    let (w, img1) = last_frame(&frames);
    set_scale(&e, 2).unwrap();
    e.advance(10);
    let s2 = e.snapshot();
    let (_, img2) = last_frame(&frames);
    let (c1, c2) = (surface(&s1, "character"), surface(&s2, "character"));
    assert_eq!((int(&c1["w"]), int(&c1["h"])), (32, 32));
    assert_eq!((int(&c2["w"]), int(&c2["h"])), (64, 64));
    assert_eq!((int(&c2["buffer_scale"]), int(&s2["scale"])), (1, 2));
    assert_eq!(u32::try_from(e.prop(CHAR, CHAR_IF, "Width")).unwrap(), 64);
    // The input region's box is doubled and keeps its bottom centre (B-CHAR-5 anchor).
    let (o1, o2) = (&c1["opaque"], &c2["opaque"]);
    assert_eq!(int(&o2["w"]), 2 * int(&o1["w"]));
    assert_eq!(int(&o2["h"]), 2 * int(&o1["h"]));
    assert_eq!(
        int(&o2["x"]) + int(&o2["w"]) / 2,
        int(&o1["x"]) + int(&o1["w"]) / 2
    );
    assert_eq!(int(&o2["y"]) + int(&o2["h"]), int(&o1["y"]) + int(&o1["h"]));
    assert_eq!(s2["rest_opaque"], *o2);
    // Every pixel of the 2x surface repeats its 1x pixel (nearest neighbour, bitmap).
    let px = |img: &[u8], x: i64, y: i64| {
        let o = ((y * w as i64 + x) * 4) as usize;
        img[o..o + 4].to_vec()
    };
    let (x1, y1, x2, y2) = (int(&c1["x"]), int(&c1["y"]), int(&c2["x"]), int(&c2["y"]));
    let mut opaque = 0;
    for y in 0..64 {
        for x in 0..64 {
            let p = px(&img2, x2 + x, y2 + y);
            assert_eq!(p, px(&img1, x1 + x / 2, y1 + y / 2), "pixel {x},{y}");
            opaque += usize::from(p[3] == 255);
        }
    }
    assert_eq!(opaque, 4 * 24 * 24);
}

#[test]
fn b_scale_1_drag_and_balloon_at_3x() {
    let mut e = Env::new("scale3");
    e.start(&["--outputs", "800x600+0+0"]);
    e.advance(1000);
    let c1 = surface(&e.snapshot(), "character");
    let (_, so, _) = out(&e.cli(&["scale", "3"]));
    assert_eq!(so, "");
    e.advance(10);
    let s = e.snapshot();
    let c = surface(&s, "character");
    assert_eq!((int(&c["w"]), int(&c["h"])), (96, 96));
    // A press on a pixel that only the 3x picture covers (right of the 1x frame) drags.
    let o = &c["opaque"];
    let (px, py) = (
        int(&o["x"]) + int(&o["w"]) - 3,
        int(&o["y"]) + int(&o["h"]) / 2,
    );
    assert!(px >= int(&c1["x"]) + 32, "{c1} {c}");
    let mut cl = e.client();
    let (px, py) = (px as i32, py as i32);
    e.pointer("press", 1, px, py);
    e.pointer("motion", 1, px - 300, py + 5);
    e.pointer("release", 1, px - 300, py + 5);
    let (x, y) = (int(&c["x"]) - 300, int(&c["y"]) + 5);
    cl.wait_for("Move after the drag", |r| {
        r.member == "Move" && r.args == json!([x, y, 1])
    });
    // The Agent balloon sits above the 96 x 96 frame, centred on it; its size is the
    // normal one.
    cl.call_id("Speak", &("Hi there",)).unwrap();
    e.advance(500);
    let b = surface(&e.snapshot(), "agent_balloon");
    assert_eq!(int(&b["y"]) + int(&b["h"]), y);
    assert_eq!(int(&b["x"]), x + (96 - int(&b["w"])) / 2);
    e.cli(&["scale", "1"]);
    e.advance(10);
    let b1 = surface(&e.snapshot(), "agent_balloon");
    assert_eq!((&b1["w"], &b1["h"]), (&b["w"], &b["h"]));
}

#[test]
fn b_scale_1_cli_dbus_round_trip_and_persistence() {
    let mut e = Env::new("scalecli");
    e.start(&[]);
    e.advance(1000);
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
    assert_eq!(out(&e.cli(&["scale"])), (0, "1\n".into(), String::new()));
    assert_eq!(out(&e.cli(&["scale", "2"])).0, 0);
    assert_eq!(out(&e.cli(&["scale"])).1, "2\n");
    let (code, _, se) = out(&e.cli(&["scale", "5"]));
    assert_eq!(code, 1);
    assert!(
        se.contains("org.freedesktop.DBus.Error.InvalidArgs"),
        "{se}"
    );
    assert_eq!(out(&e.cli(&["scale", "two"])).0, 1);
    assert_eq!(
        set_scale(&e, 0),
        Err("org.freedesktop.DBus.Error.InvalidArgs".into())
    );
    set_scale(&e, 3).unwrap();
    assert_eq!(scale_of(&e), 3);
    assert_eq!(u32::try_from(e.prop(CHAR, CHAR_IF, "Width")).unwrap(), 96);
    // PropertiesChanged: Agent1 Scale and Character1 Width.
    let (mut scale, mut width) = (false, false);
    for m in it.flatten().take(20) {
        if m.header().member().map(|x| x.as_str()) != Some("PropertiesChanged") {
            continue;
        }
        let (iface, changed, _): (
            String,
            std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
            Vec<String>,
        ) = m.body().deserialize().unwrap();
        if iface == AGENT_IF && changed.contains_key("Scale") {
            scale |= u32::try_from(&changed["Scale"]).unwrap() == 2;
        }
        if iface == CHAR_IF && changed.contains_key("Width") {
            width |= u32::try_from(&changed["Width"]).unwrap() == 64;
        }
        if scale && width {
            break;
        }
    }
    assert!(scale && width);
    // Persisted (B-CFG-2) and chosen: it wins over the config key after a restart.
    let s = e.snapshot();
    e.client().call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
    assert_eq!(e.wait_exit(10), Some(0));
    let st = e.state();
    assert!(
        st.contains("scale = 3") && st.contains("scale_chosen = true"),
        "{st}"
    );
    std::fs::create_dir_all(e.dir.join("cfg/bentwire")).unwrap();
    std::fs::write(e.dir.join("cfg/bentwire/config.toml"), "scale = 2\n").unwrap();
    e.start(&[]);
    e.advance(1000);
    assert_eq!(scale_of(&e), 3);
    let s2 = e.snapshot();
    assert_eq!((&s2["left"], &s2["top"]), (&s["left"], &s["top"]));
}

#[test]
fn b_scale_1_size_submenu_through_test1_and_trace() {
    let mut e = Env::new("scalemenu");
    let trace = e.dir.join("trace.jsonl");
    e.start(&["--trace", trace.to_str().unwrap()]);
    e.advance(1000);
    let c = surface(&e.snapshot(), "character");
    let o = &c["opaque"];
    let (x, y) = (
        (int(&o["x"]) + int(&o["w"]) / 2) as i32,
        (int(&o["y"]) + int(&o["h"]) / 2) as i32,
    );
    e.click(2, x, y);
    let s = e.snapshot();
    let size = s["menu"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["label"] == "&Size")
        .unwrap_or_else(|| panic!("no Size in {s}"))
        .clone();
    assert_eq!(size["submenu"], true);
    let (sx, sy) = ((int(&size["x"]) + 5) as i32, (int(&size["y"]) + 5) as i32);
    e.pointer("motion", 0, sx, sy);
    let s = e.snapshot();
    let sub = s["submenu"].as_array().unwrap().clone();
    let labels: Vec<_> = sub
        .iter()
        .map(|i| (i["label"].clone(), i["checked"].clone()))
        .collect();
    assert_eq!(
        labels,
        vec![
            (json!("&1x"), json!(true)),
            (json!("&2x"), json!(false)),
            (json!("&3x"), json!(false)),
            (json!("&4x"), json!(false)),
        ]
    );
    // Right of the menu, its first item level with Size.
    let m = surface(&s, "menu");
    assert_eq!(
        int(&surface(&s, "submenu")["x"]),
        int(&m["x"]) + int(&m["w"])
    );
    assert_eq!(int(&sub[0]["y"]), int(&size["y"]));
    let (tx, ty) = (
        (int(&sub[2]["x"]) + 5) as i32,
        (int(&sub[2]["y"]) + 5) as i32,
    );
    e.pointer("motion", 0, tx, ty);
    e.click(1, tx, ty);
    e.advance(10);
    let s = e.snapshot();
    assert!(s["menu"].is_null() && s["submenu"].is_null(), "{s}");
    assert_eq!(scale_of(&e), 3);
    // The trace: the submenu popup is a child of the menu and is destroyed first.
    let t = std::fs::read_to_string(&trace).unwrap();
    let reqs: Vec<String> = t
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .filter(|l| l["surface"] == "menu" || l["surface"] == "submenu")
        .filter(|l| l["req"] == "popup" || l["req"] == "destroy")
        .map(|l| {
            format!(
                "{} {}",
                l["req"].as_str().unwrap(),
                l["surface"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        reqs,
        [
            "popup menu",
            "popup submenu",
            "destroy submenu",
            "destroy menu"
        ]
    );
    let popup = t
        .lines()
        .map(|l| serde_json::from_str::<serde_json::Value>(l).unwrap())
        .find(|l| l["req"] == "popup" && l["surface"] == "submenu")
        .unwrap();
    // Anchored at Size's row across the menu, first items level (3 = border + padding).
    assert_eq!(popup["anchor_rect"][2], m["w"]);
    assert_eq!(popup["offset"], json!([0, -3]));
}
