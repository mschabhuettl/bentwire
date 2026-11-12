//! Test helper for the desktop scenario under KWin (`kwin_wayland --virtual`, started with
//! KWIN_WAYLAND_NO_PERMISSION_CHECKS=1): drives the pointer through org_kde_kwin_fake_input.
//! Reads commands from stdin, one per line:
//!   move X Y        absolute position in the output layout
//!   rel DX DY       relative motion (what a mouse sends)
//!   press B         B = 1 left, 2 right
//!   release B
//!   sleep MS
//!   mark TEXT       print TEXT once the commands before it are done
//! usage: kwptr < commands

use std::io::BufRead;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_plasma::fake_input::client::org_kde_kwin_fake_input::{
    self, OrgKdeKwinFakeInput,
};

struct S;

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for S {
    fn event(
        _: &mut S,
        _: &wl_registry::WlRegistry,
        _: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
    }
}

impl Dispatch<OrgKdeKwinFakeInput, ()> for S {
    fn event(
        _: &mut S,
        _: &OrgKdeKwinFakeInput,
        _: org_kde_kwin_fake_input::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
    }
}

fn main() {
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, mut q) = registry_queue_init::<S>(&conn).expect("registry");
    let qh = q.handle();
    let fake: OrgKdeKwinFakeInput = globals
        .bind(&qh, 4..=4, ())
        .expect("org_kde_kwin_fake_input");
    fake.authenticate("bentwire scenario".into(), "desktop test".into());
    q.roundtrip(&mut S).unwrap();
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        let num = |i: usize| f[i].parse::<f64>().expect("number");
        match f.as_slice() {
            ["move", _, _] => fake.pointer_motion_absolute(num(1), num(2)),
            ["rel", _, _] => fake.pointer_motion(num(1), num(2)),
            ["press" | "release", b] => {
                let code = if *b == "2" { 0x111 } else { 0x110 };
                fake.button(code, (f[0] == "press") as u32);
            }
            ["sleep", ms] => {
                std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap()));
                continue;
            }
            ["mark", m] => {
                println!("{m}");
                continue;
            }
            [] => continue,
            _ => panic!("bad command {line:?}"),
        }
        q.roundtrip(&mut S).unwrap();
    }
}
