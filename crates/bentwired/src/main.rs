mod app;
mod args;
mod audio;
mod config;
mod dbus;
mod discovery;
mod library;
mod model;
#[cfg(test)]
mod oracle;
mod present;
mod tray;
mod wayland;

use app::{App, Backend, Clock};
use bentwire_core::Core;
use bentwire_core::api::{CoreInit, Event, LocaleEnv, OutputInfo, Rect, Scene, StateFile};
use calloop::signals::{Signal as UnixSignal, Signals};
use present::Rendered;
use std::cell::RefCell;
use std::collections::HashMap;
use std::process::ExitCode;
use std::rc::Rc;
use std::time::Instant;
use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::{RequestNameFlags, RequestNameReply};

/// Headless backend (T-4): the app traces the requests; nothing is drawn on screen.
struct Headless;

impl Backend for Headless {
    fn present(&mut self, _r: &Rendered, _scene: &Scene) {}
    fn poll(&mut self) -> Vec<Event> {
        Vec::new()
    }
}

fn main() -> ExitCode {
    env_logger::Builder::from_env(
        env_logger::Env::default().default_filter_or("info,zbus=error,tracing=error"),
    )
    .init();
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

/// Local hour for office97 daypart branches; `date` avoids a time-zone dependency.
fn local_hour() -> u8 {
    std::process::Command::new("date")
        .arg("+%H")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(12)
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
            // B-RUN-1: save (the core's Quit emits PersistState) and exit 0.
            _ => {
                app.event(Event::Call {
                    token: u64::MAX,
                    sender: String::new(),
                    call: bentwire_core::api::Call::Quit,
                });
                app.quit = true;
            }
        })
        .map_err(|e| e.to_string())?;

    let config = config::Config::load(&config::config_path()).unwrap_or_else(|e| {
        log::warn!("{e}; using defaults");
        config::Config::default()
    });
    let state_path = config::state_path();
    let state = config::State::load(&state_path);
    let roots = discovery::roots(&args.characters, &config.character_paths);
    let lib: library::Shared = Rc::new(RefCell::new(library::Library::new(roots)));

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
        .insert_source(rx, |ev, _, app: &mut App| match ev {
            calloop::channel::Event::Msg(m) => app.handle_dbus(m),
            // The session bus went away (logout, dbus-run-session ending): nobody can reach
            // the daemon any more, so it saves the state and exits like on SIGTERM.
            calloop::channel::Event::Closed => {
                log::info!("session bus closed; exiting");
                app.event(Event::Call {
                    token: u64::MAX,
                    sender: String::new(),
                    call: bentwire_core::api::Call::Quit,
                });
                app.quit = true;
            }
        })
        .map_err(|e| e.to_string())?;

    let mut wl = None;
    let outputs: Vec<OutputInfo> = if args.headless {
        args.outputs
            .iter()
            .enumerate()
            .map(|(i, o)| OutputInfo {
                id: i as u32,
                rect: Rect::new(o.x, o.y, o.w as i32, o.h as i32),
                scale: o.scale,
            })
            .collect()
    } else {
        let w = wayland::Wayland::connect()?;
        let fd = w.fd();
        // The Wayland socket only wakes the loop; the backend reads and dispatches in poll().
        let source = calloop::generic::Generic::new(
            unsafe { calloop::generic::FdWrapper::new(fd) },
            calloop::Interest::READ,
            calloop::Mode::Level,
        );
        event_loop
            .handle()
            .insert_source(source, |_, _, app: &mut App| {
                app.backend_events();
                Ok(calloop::PostAction::Continue)
            })
            .map_err(|e| e.to_string())?;
        let o = w.outputs();
        wl = Some(w);
        o
    };
    let draws: Box<dyn bentwire_core::api::Draws> = match (&args.draws, args.seed) {
        (Some(d), _) => {
            let text = std::fs::read_to_string(d).map_err(|e| format!("{}: {e}", d.display()))?;
            let v = text
                .split_whitespace()
                .map(|t| {
                    t.parse::<u32>()
                        .map_err(|_| format!("{}: bad draw {t:?}", d.display()))
                })
                .collect::<Result<Vec<_>, _>>()?;
            Box::new(bentwire_core::draws::ScriptedDraws::new(v))
        }
        (None, Some(seed)) => Box::new(bentwire_core::draws::SplitMix64::new(seed)),
        (None, None) => {
            let seed = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(1);
            Box::new(bentwire_core::draws::SplitMix64::new(seed))
        }
    };
    let catalog = lib.borrow().catalog();
    let env = |k: &str| std::env::var(k).ok().filter(|v| !v.is_empty());
    let state_file = state.character.clone().map(|c| StateFile {
        character: c,
        left: state.left.unwrap_or(0),
        top: state.top.unwrap_or(0),
        visible: state.visible.unwrap_or(true),
        chosen: state.chosen(),
    });
    // B-CHAR-6: once, when only the builtin placeholder is installed and it starts visible.
    let install_hint = !state.install_hint_shown
        && !catalog.is_empty()
        && catalog.iter().all(|c| c.id == config::PLACEHOLDER)
        && state_file.as_ref().is_none_or(|s| s.visible);
    let headless = args.headless;
    // A-FONT-1: headless output must not depend on the machine's fonts.
    let fonts = || {
        if headless {
            bentwire_render::font::Fonts::bundled()
        } else {
            bentwire_render::font::Fonts::with_system()
        }
    };
    let core = Core::new(CoreInit {
        config: config.to_core(),
        locale: LocaleEnv {
            lc_all: env("LC_ALL"),
            lc_messages: env("LC_MESSAGES"),
            lang: env("LANG"),
        },
        state: state_file,
        catalog,
        loader: Box::new(library::Loader(lib.clone())),
        metrics: Box::new(fonts()),
        draws,
        outputs: outputs.clone(),
        local_hour: local_hour(),
    });
    let trace = match &args.trace {
        Some(p) => Some(std::fs::File::create(p).map_err(|e| format!("{}: {e}", p.display()))?),
        None => None,
    };
    if let Some(d) = &args.dump_frames {
        std::fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let theme_dir = lib.borrow().theme.clone();
    let theme = app::load_theme(theme_dir.as_deref());
    let mut app = App {
        core,
        lib,
        conn: conn.clone(),
        clock: if headless {
            Clock::Fake(0)
        } else {
            Clock::Real(Instant::now())
        },
        test: headless || args.test_interface,
        test_full: headless,
        next_output_id: outputs.len() as u32,
        outputs,
        backend: match wl {
            Some(w) => Box::new(w),
            None => Box::new(Headless),
        },
        audio: if headless || args.no_audio {
            None
        } else {
            audio::Audio::open()
        },
        silent: headless || args.no_audio,
        tray: if headless || args.no_tray {
            None
        } else {
            let (tx, rx) = calloop::channel::channel();
            event_loop
                .handle()
                .insert_source(rx, |ev, _, app: &mut App| {
                    if let calloop::channel::Event::Msg(t) = ev {
                        app.event(Event::Tray(t));
                    }
                })
                .map_err(|e| e.to_string())?;
            tray::Tray::spawn(tx)
        },
        fonts: fonts(),
        theme,
        theme_dir,
        trace,
        dump_frames: args.dump_frames.clone(),
        dump_seq: 0,
        roots_cli: args.characters.clone(),
        config_paths: config.character_paths.clone(),
        state_path: state_path.clone(),
        install_hint_shown: state.install_hint_shown || install_hint,
        next_token: 0,
        pending: HashMap::new(),
        last_scene: None,
        rendered: Rendered::default(),
        last_props: None,
        quit: false,
        menu_serial: 0,
        grab: None,
        deferred: Vec::new(),
    };
    let now = app.clock.now();
    let out = app.core.start(now);
    app.process(out);
    if install_hint {
        log::info!(
            "{}",
            bentwire_core::l10n::install_hint(bentwire_core::api::Lang::En)
        );
        let st = config::State {
            install_hint_shown: true,
            ..state
        };
        if let Err(e) = st.save(&state_path) {
            log::warn!("writing {}: {e}", state_path.display());
        }
        app.event(Event::InstallHint);
    }
    if !headless {
        // B-O97-1 daypart: follow the local hour (the fake clock keeps the start hour).
        let mut hour = local_hour();
        event_loop
            .handle()
            .insert_source(
                calloop::timer::Timer::from_duration(std::time::Duration::from_secs(60)),
                move |_, _, app: &mut App| {
                    let h = local_hour();
                    if h != hour {
                        hour = h;
                        app.event(Event::LocalHour(h));
                    }
                    calloop::timer::TimeoutAction::ToDuration(std::time::Duration::from_secs(60))
                },
            )
            .map_err(|e| e.to_string())?;
    }

    while !app.quit {
        app.backend.flush();
        let timeout = app.timeout();
        event_loop
            .dispatch(timeout, &mut app)
            .map_err(|e| e.to_string())?;
        app.backend_events();
        app.tick();
    }
    let _ = conn.release_name(dbus::BUS_NAME);
    Ok(())
}
