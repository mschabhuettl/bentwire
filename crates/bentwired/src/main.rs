// Until the core adapter and the Wayland backend use every engine type.
#![allow(dead_code)]

mod app;
mod args;
mod audio;
mod config;
mod dbus;
mod discovery;
mod engine;
mod layout;
mod stub;

use app::{App, Backend, BackendEvent, Clock};
use calloop::signals::{Signal as UnixSignal, Signals};
use engine::{Engine, Output, Scene};
use std::collections::HashMap;
use std::process::ExitCode;
use std::time::Instant;
use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

/// Headless backend (T-4): the app traces the requests; nothing is drawn on screen.
struct Headless;

impl Backend for Headless {
    fn present(&mut self, _scene: &Scene, _outputs: &[Output]) {}
    fn poll(&mut self) -> Vec<BackendEvent> {
        Vec::new()
    }
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let args = match args::parse(std::env::args().skip(1)) {
        Ok(a) => a,
        Err(e) => {
            if !e.is_empty() {
                eprintln!("bentwired: {e}");
            }
            eprint!("{}", args::USAGE);
            return ExitCode::from(if e.is_empty() { 0 } else { 1 });
        }
    };
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("bentwired: {e}");
            ExitCode::from(1)
        }
    }
}

fn run(args: args::Args) -> Result<(), String> {
    let mut event_loop: calloop::EventLoop<App> =
        calloop::EventLoop::try_new().map_err(|e| e.to_string())?;
    // Block the signals before any thread is spawned (signalfd).
    let signals = Signals::new(&[UnixSignal::SIGTERM, UnixSignal::SIGINT, UnixSignal::SIGHUP])
        .map_err(|e| e.to_string())?;
    event_loop
        .handle()
        .insert_source(signals, |ev, _, app: &mut App| match ev.signal() {
            UnixSignal::SIGHUP => app.reload(),
            _ => app.quit = true,
        })
        .map_err(|e| e.to_string())?;

    let config = config::Config::load(&config::config_path()).unwrap_or_else(|e| {
        log::warn!("{e}; using defaults");
        config::Config::default()
    });
    let state_path = config::state_path();
    let state = config::State::load(&state_path);
    let roots = discovery::roots(&args.characters, &config.character_paths);
    let entries = discovery::discover(&roots);
    log::info!("{} characters found", entries.len());

    let conn = Connection::session().map_err(|e| format!("session bus: {e}"))?;
    let incoming = MessageIterator::from(&conn);
    match conn.request_name_with_flags(dbus::BUS_NAME, RequestNameFlags::DoNotQueue.into()) {
        Ok(RequestNameReply::PrimaryOwner) | Ok(RequestNameReply::AlreadyOwner) => {}
        Ok(_) | Err(zbus::Error::NameTaken) => return Err("already running".into()),
        Err(e) => return Err(format!("requesting {}: {e}", dbus::BUS_NAME)),
    }
    let (tx, rx) = calloop::channel::channel::<zbus::message::Message>();
    std::thread::spawn(move || {
        for m in incoming.flatten() {
            if m.header().message_type() == zbus::message::Type::MethodCall && tx.send(m).is_err() {
                break;
            }
        }
    });
    event_loop
        .handle()
        .insert_source(rx, |ev, _, app: &mut App| {
            if let calloop::channel::Event::Msg(m) = ev {
                app.handle_dbus(m);
            }
        })
        .map_err(|e| e.to_string())?;

    let outputs: Vec<Output> = args
        .outputs
        .iter()
        .enumerate()
        .map(|(i, o)| Output {
            id: i as u32,
            x: o.x,
            y: o.y,
            w: o.w,
            h: o.h,
            scale: o.scale,
        })
        .collect();
    let mut engine: Box<dyn Engine> = Box::new(stub::Stub::new());
    if let Some(d) = &args.draws {
        let text = std::fs::read_to_string(d).map_err(|e| format!("{}: {e}", d.display()))?;
        let draws = text
            .split_whitespace()
            .map(|t| {
                t.parse::<u32>()
                    .map_err(|_| format!("{}: bad draw {t:?}", d.display()))
            })
            .collect::<Result<Vec<_>, _>>()?;
        engine.set_draws(draws);
    }
    let clock = if args.headless {
        Clock::Fake(0)
    } else {
        Clock::Real(Instant::now())
    };
    engine.set_outputs(clock.now(), &outputs);

    // B-CHAR-4 startup selection.
    let candidates = discovery::startup_candidates(
        &entries,
        state.character.as_deref(),
        config.character.as_deref(),
    );
    for id in &candidates {
        let e = entries
            .iter()
            .find(|e| &e.id == id)
            .expect("candidate exists");
        match engine.select(clock.now(), Some(e)) {
            Ok(()) => break,
            Err(err) => log::warn!("character {id}: {}", err.message),
        }
    }
    let trace = match &args.trace {
        Some(p) => Some(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    if let Some(d) = &args.dump_frames {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let german = config.german();
    let mut app = App {
        engine,
        conn: conn.clone(),
        clock,
        test: args.headless || args.test_interface,
        test_full: args.headless,
        next_output_id: outputs.len() as u32,
        outputs,
        backend: Box::new(Headless),
        trace,
        dump_frames: args.dump_frames.clone(),
        dump_seq: 0,
        entries,
        roots_cli: args.characters.clone(),
        config,
        german,
        state_path,
        writer: Default::default(),
        write_at: None,
        wall: Instant::now(),
        last_scene: Scene::default(),
        last_char_props: None,
        last_agent_props: HashMap::new(),
        quit: false,
        menu_serial: 0,
    };
    app.writer.last_written = Some(state.clone());
    startup_position(&mut app, &state);
    app.flush();

    while !app.quit {
        let timeout = app.timeout();
        event_loop
            .dispatch(timeout, &mut app)
            .map_err(|e| e.to_string())?;
        app.backend_events();
        app.tick();
    }
    app.write_state(true);
    let _ = conn.release_name(dbus::BUS_NAME);
    Ok(())
}

/// B-CFG-2 / B-CFG-3: restore or choose the position, then show if visible.
fn startup_position(app: &mut App, state: &config::State) {
    let Some(p) = app.engine.props() else { return };
    let (left, top) = match (state.left, state.top) {
        (Some(l), Some(t)) => (l, t),
        _ => match app.outputs.first() {
            Some(o) => (
                o.x + o.w as i32 - 48 - p.width as i32,
                o.y + o.h as i32 - 96 - p.height as i32,
            ),
            None => (0, 0),
        },
    };
    app.engine.set_position(left, top);
    if state.visible.unwrap_or(true) {
        let now = app.clock.now();
        if let Err(e) = app.engine.call(now, "", engine::Method::Show(false)) {
            log::warn!("startup Show: {}", e.message);
        }
    }
}
