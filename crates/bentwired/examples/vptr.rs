//! Test helper for the desktop scenario: a zwlr_virtual_pointer_v1 client that drives a
//! real compositor pointer (sway headless). Reads commands from stdin, one per line:
//!   move X Y        absolute position in the output layout
//!   press B         B = 1 left, 2 right
//!   release B
//!   sleep MS
//! usage: vptr LAYOUT_WIDTH LAYOUT_HEIGHT < commands

use std::io::BufRead;
use wayland_client::globals::{GlobalListContents, registry_queue_init};
use wayland_client::protocol::wl_pointer::ButtonState;
use wayland_client::protocol::wl_registry;
use wayland_client::{Connection, Dispatch, QueueHandle};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
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
impl Dispatch<ZwlrVirtualPointerManagerV1, ()> for S {
    fn event(
        _: &mut S,
        _: &ZwlrVirtualPointerManagerV1,
        _: <ZwlrVirtualPointerManagerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
    }
}
impl Dispatch<ZwlrVirtualPointerV1, ()> for S {
    fn event(
        _: &mut S,
        _: &ZwlrVirtualPointerV1,
        _: <ZwlrVirtualPointerV1 as wayland_client::Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<S>,
    ) {
    }
}

fn main() {
    let a: Vec<u32> = std::env::args()
        .skip(1)
        .map(|s| s.parse().expect("number"))
        .collect();
    let (w, h) = (a[0], a[1]);
    let conn = Connection::connect_to_env().expect("wayland");
    let (globals, mut q) = registry_queue_init::<S>(&conn).expect("registry");
    let qh = q.handle();
    let mgr: ZwlrVirtualPointerManagerV1 = globals
        .bind(&qh, 1..=2, ())
        .expect("zwlr_virtual_pointer_manager_v1");
    let p = mgr.create_virtual_pointer(None, &qh, ());
    q.roundtrip(&mut S).unwrap();
    let t0 = std::time::Instant::now();
    let now = || t0.elapsed().as_millis() as u32;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let f: Vec<&str> = line.split_whitespace().collect();
        match f.as_slice() {
            ["move", x, y] => {
                p.motion_absolute(now(), x.parse().unwrap(), y.parse().unwrap(), w, h)
            }
            ["press" | "release", b] => {
                let code = if *b == "2" { 0x111 } else { 0x110 };
                let st = if f[0] == "press" {
                    ButtonState::Pressed
                } else {
                    ButtonState::Released
                };
                p.button(now(), code, st);
            }
            ["sleep", ms] => {
                std::thread::sleep(std::time::Duration::from_millis(ms.parse().unwrap()));
                continue;
            }
            [] => continue,
            _ => panic!("bad command {line:?}"),
        }
        p.frame();
        q.roundtrip(&mut S).unwrap();
    }
}
