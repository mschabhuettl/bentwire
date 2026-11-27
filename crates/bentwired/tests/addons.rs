//! Add-ons end to end (spec B-ADDON-1 to B-ADDON-5): the CLI's `addons` commands, character
//! add-ons found by a running daemon, and bridge add-ons supervised by it. Bridges are small
//! shell scripts that `exec` their long-running part, so they leave no orphan.

mod common;

use common::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn addons(e: &Env) -> PathBuf {
    e.dir.join("data/bentwire/addons")
}

fn cli(e: &Env, args: &[&str]) -> (i32, String, String) {
    out(&e.cli(args))
}

/// An add-on folder `parent/<name>` with its manifest (plus `extra` lines).
fn addon(parent: &Path, name: &str, kind: &str, version: &str, extra: &str) -> PathBuf {
    let d = parent.join(name);
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(
        d.join("addon.toml"),
        format!("name = \"{name}\"\nkind = \"{kind}\"\nversion = \"{version}\"\n{extra}"),
    )
    .unwrap();
    d
}

/// A bridge whose `run.sh` (mode 0755) has `body`; started as `./run.sh hello`.
fn bridge(parent: &Path, name: &str, body: &str) -> PathBuf {
    let d = addon(
        parent,
        name,
        "bridge",
        "1",
        "command = [\"./run.sh\", \"hello\"]\n",
    );
    let s = d.join("run.sh");
    std::fs::write(&s, format!("#!/bin/sh\n{body}\n")).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&s, std::fs::Permissions::from_mode(0o755)).unwrap();
    d
}

/// `dir` packed as `<dir>.tar.gz` with its folder at the top.
fn tar_gz(dir: &Path) -> PathBuf {
    let f = dir.with_extension("tar.gz");
    let st = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&f)
        .arg("-C")
        .arg(dir.parent().unwrap())
        .arg(dir.file_name().unwrap())
        .status()
        .unwrap();
    assert!(st.success());
    f
}

fn wait_until(what: &str, secs: u64, mut f: impl FnMut() -> bool) {
    let t = Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(secs), "timed out: {what}");
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// A process that runs (zombies count as gone).
fn alive(pid: &str) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).is_ok_and(|s| {
        s.rsplit(')')
            .next()
            .is_some_and(|r| !r.trim_start().starts_with('Z'))
    })
}

fn read_pid(file: &Path) -> String {
    wait_until("bridge pid file", 10, || {
        std::fs::read_to_string(file).is_ok_and(|s| s.ends_with('\n'))
    });
    std::fs::read_to_string(file).unwrap().trim().to_string()
}

/// B-ADDON-1 to B-ADDON-3 without a daemon: install from a folder, a tar.gz and a file URL,
/// list, enable, disable, remove; a bridge is never enabled by an install; bad sources and
/// manifests fail and leave nothing behind.
#[test]
fn b_addon_2_3_cli_install_list_enable_disable_remove() {
    let e = Env::new("addcli");
    let src = e.dir.join("src");
    let dest = addons(&e);
    let wire = addon(&src, "wire", "character", "1.0", "");
    let (code, so, se) = cli(&e, &["addons", "install", wire.to_str().unwrap()]);
    assert_eq!(code, 0, "{se}");
    assert_eq!(
        so,
        format!(
            "installed wire 1.0 (character) in {}\n",
            dest.join("wire").display()
        )
    );
    assert_eq!(
        cli(&e, &["addons", "list"]).1,
        "wire\tcharacter\t1.0\tenabled\n"
    );
    // A bridge from a tar.gz with a top-level folder: installed disabled, exec bit kept.
    let notify = bridge(&src, "notify", "exec sleep 600");
    let tgz = tar_gz(&notify);
    let (code, so, se) = cli(&e, &["addons", "install", tgz.to_str().unwrap()]);
    assert_eq!(code, 0, "{se}");
    assert_eq!(
        so,
        format!(
            "installed notify 1 (bridge) in {}\n\
             a bridge runs only once enabled: bentwire addons enable notify\n",
            dest.join("notify").display()
        )
    );
    use std::os::unix::fs::PermissionsExt;
    let mode = std::fs::metadata(dest.join("notify/run.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755);
    let list = || cli(&e, &["addons"]).1;
    assert_eq!(
        list(),
        "notify\tbridge\t1\tdisabled\nwire\tcharacter\t1.0\tenabled\n"
    );
    assert_eq!(cli(&e, &["addons", "enable", "notify"]).0, 0);
    assert_eq!(
        list(),
        "notify\tbridge\t1\tenabled\nwire\tcharacter\t1.0\tenabled\n"
    );
    let state = std::fs::read_to_string(e.dir.join("cfg/bentwire/addons.toml")).unwrap();
    assert!(
        state.contains("enabled = [\"notify\", \"wire\"]"),
        "{state}"
    );
    // Reinstalling an enabled bridge, here from a file URL with its checksum, disables it.
    let sum = std::process::Command::new("sha256sum")
        .arg(&tgz)
        .output()
        .unwrap();
    let sum = String::from_utf8_lossy(&sum.stdout)[..64].to_string();
    let url = format!("file://{}", tgz.display());
    let (code, _, se) = cli(&e, &["addons", "install", &url, "--sha256", &sum]);
    assert_eq!(code, 0, "{se}");
    assert!(list().starts_with("notify\tbridge\t1\tdisabled\n"));
    let (code, _, se) = cli(
        &e,
        &["addons", "install", &url, "--sha256", &"0".repeat(64)],
    );
    assert_eq!(
        (code, se.contains("SHA-256 does not match")),
        (1, true),
        "{se}"
    );
    let (code, _, se) = cli(&e, &["addons", "install", "http://example.org/a.tar.gz"]);
    assert_eq!((code, se.contains("only https://")), (1, true), "{se}");
    // disable, remove.
    assert_eq!(cli(&e, &["addons", "disable", "wire"]).0, 0);
    assert!(list().ends_with("wire\tcharacter\t1.0\tdisabled\n"));
    assert_eq!(cli(&e, &["addons", "enable", "notify"]).0, 0);
    assert_eq!(cli(&e, &["addons", "remove", "notify"]).0, 0);
    assert!(!dest.join("notify").exists());
    assert_eq!(list(), "wire\tcharacter\t1.0\tdisabled\n");
    let state = std::fs::read_to_string(e.dir.join("cfg/bentwire/addons.toml")).unwrap();
    assert!(!state.contains("notify"), "{state}");
    for (args, want) in [
        (&["addons", "enable", "notify"][..], "no add-on \"notify\""),
        (&["addons", "remove", "../cfg"][..], "no add-on"),
        (&["addons", "frob"][..], "expected list"),
    ] {
        let (code, _, se) = cli(&e, args);
        assert_eq!((code, se.contains(want)), (1, true), "{args:?}: {se}");
    }
    // A bad manifest, a missing one, and an archive that leaves its folder fail.
    let bad = addon(&src, "bad", "plugin", "1", "");
    let (code, _, se) = cli(&e, &["addons", "install", bad.to_str().unwrap()]);
    assert_eq!((code, se.contains("unknown kind")), (1, true), "{se}");
    let (code, _, se) = cli(&e, &["addons", "install", src.to_str().unwrap()]);
    assert_eq!((code, se.contains("no addon.toml")), (1, true), "{se}");
    let evil = e.dir.join("evil.tar.gz");
    let py = "import io, sys, tarfile\n\
              t = tarfile.open(sys.argv[1], 'w:gz')\n\
              for n, d in [('addon.toml', b'name = \"evil\"\\nkind = \"character\"\\nversion = \"1\"\\n'), ('../../escaped', b'x')]:\n\
              \x20   i = tarfile.TarInfo(n); i.size = len(d); t.addfile(i, io.BytesIO(d))\n\
              t.close()\n";
    let st = std::process::Command::new("python3")
        .args(["-I", "-c", py])
        .arg(&evil)
        .status()
        .unwrap();
    assert!(st.success());
    let (code, _, se) = cli(&e, &["addons", "install", evil.to_str().unwrap()]);
    assert_eq!(
        (code, se.contains("leaves the add-on folder")),
        (1, true),
        "{se}"
    );
    assert!(!e.dir.join("data/bentwire/escaped").exists());
    assert!(!e.dir.join("data/escaped").exists());
    let left: Vec<String> = std::fs::read_dir(&dest)
        .unwrap()
        .map(|d| d.unwrap().file_name().into_string().unwrap())
        .collect();
    assert_eq!(left, ["wire"], "no temp folder or evil add-on left");
}

/// B-ADDON-3, B-ADDON-4: a running daemon is told by the CLI and lists a character add-on
/// after install and enable, not after disable; a reinstall with a changed manifest of the
/// same size is read again (the discovery cache keys on modification time and size).
#[test]
fn b_addon_4_character_addon_with_a_running_daemon() {
    let mut e = Env::new("addchar");
    e.seed_state("install_hint_shown = true\n");
    e.start(&[]);
    let src = e.dir.join("src");
    let pack = addon(&src, "grit-pack", "character", "1", "");
    copy_dir(&fixtures().join("pebble"), &pack.join("grit"));
    let chars = || cli(&e, &["characters"]).1;
    assert!(!chars().contains("grit\t"));
    let (code, _, se) = cli(&e, &["addons", "install", pack.to_str().unwrap()]);
    assert_eq!(code, 0, "{se}");
    wait_until("grit listed", 10, || chars().contains("grit\tPebble"));
    assert_eq!(cli(&e, &["addons", "disable", "grit-pack"]).0, 0);
    wait_until("grit gone", 10, || !chars().contains("grit\t"));
    assert_eq!(cli(&e, &["addons", "enable", "grit-pack"]).0, 0);
    wait_until("grit back", 10, || chars().contains("grit\tPebble"));
    let m = pack.join("grit/character.toml");
    let text = std::fs::read_to_string(&m).unwrap();
    std::fs::write(&m, text.replace("\"Pebble\"", "\"Gravel\"")).unwrap();
    assert_eq!(cli(&e, &["addons", "install", pack.to_str().unwrap()]).0, 0);
    wait_until("new name listed", 10, || chars().contains("grit\tGravel"));
    assert_eq!(cli(&e, &["addons", "remove", "grit-pack"]).0, 0);
    wait_until("grit removed", 10, || !chars().contains("grit\t"));
}

/// B-ADDON-5: a bridge that exits is restarted after 1 s, then 2 s (backoff); a disabled one
/// is not restarted.
#[test]
fn b_addon_5_crashing_bridge_restarts_with_backoff() {
    let mut e = Env::new("addcrash");
    let src = e.dir.join("src");
    let b = bridge(&src, "crashy", "date +%s.%N >> starts\nexit 1");
    assert_eq!(cli(&e, &["addons", "install", b.to_str().unwrap()]).0, 0);
    assert_eq!(cli(&e, &["addons", "enable", "crashy"]).0, 0);
    e.start(&[]);
    let starts = addons(&e).join("crashy/starts");
    let times = || -> Vec<f64> {
        std::fs::read_to_string(&starts)
            .unwrap_or_default()
            .lines()
            .map(|l| l.parse().unwrap())
            .collect()
    };
    wait_until("three starts", 20, || times().len() >= 3);
    let t = times();
    assert!(t[1] - t[0] >= 0.9, "first gap {:?}", t);
    assert!(t[2] - t[1] >= 1.8, "second gap {:?}", t);
    assert_eq!(cli(&e, &["addons", "disable", "crashy"]).0, 0);
    // The next restart would come 4 s after the third start.
    std::thread::sleep(Duration::from_millis(500));
    let n = times().len();
    std::thread::sleep(Duration::from_secs(5));
    assert_eq!(times().len(), n, "a disabled bridge is not restarted");
}

/// B-ADDON-5: an enabled bridge runs with its argv in its folder; it stops on disable, and
/// with the daemon on Quit, on SIGTERM and when the daemon is killed (PR_SET_PDEATHSIG).
#[test]
fn b_addon_5_bridges_stop_with_the_daemon() {
    let mut e = Env::new("addstop");
    let src = e.dir.join("src");
    let b = bridge(
        &src,
        "long",
        "echo \"$1\" > arg\necho $$ > pid\nexec sleep 600",
    );
    let tgz = tar_gz(&b);
    assert_eq!(cli(&e, &["addons", "install", tgz.to_str().unwrap()]).0, 0);
    let dir = addons(&e).join("long");
    // Installed but not enabled: never started.
    e.start(&[]);
    std::thread::sleep(Duration::from_millis(500));
    assert!(!dir.join("pid").exists());
    assert_eq!(cli(&e, &["addons", "enable", "long"]).0, 0);
    let pid = read_pid(&dir.join("pid"));
    assert!(alive(&pid));
    assert_eq!(std::fs::read_to_string(dir.join("arg")).unwrap(), "hello\n");
    assert_eq!(cli(&e, &["addons", "disable", "long"]).0, 0);
    wait_until("stopped on disable", 10, || !alive(&pid));
    let _ = std::fs::remove_file(dir.join("pid"));
    assert_eq!(cli(&e, &["addons", "enable", "long"]).0, 0);
    let stop = |e: &mut Env, how: &str| {
        if !e.name_owned() {
            let _ = std::fs::remove_file(dir.join("pid"));
            e.start(&[]);
        }
        let pid = read_pid(&dir.join("pid"));
        assert!(alive(&pid), "{how}");
        let d = e.daemon.as_ref().unwrap().id().to_string();
        match how {
            "quit" => {
                e.client().call_unit(AGENT, AGENT_IF, "Quit", &()).unwrap();
            }
            sig => {
                std::process::Command::new("kill")
                    .args([sig, &d])
                    .status()
                    .unwrap();
            }
        }
        let code = e.wait_exit(10);
        if how != "-KILL" {
            assert_eq!(code, Some(0), "{how}");
        }
        wait_until(how, 10, || !alive(&pid));
    };
    stop(&mut e, "quit");
    stop(&mut e, "-TERM");
    stop(&mut e, "-KILL");
}
