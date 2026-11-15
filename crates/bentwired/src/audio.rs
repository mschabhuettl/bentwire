//! Sound output. One sound at a time: starting a sound stops the previous one. Failures
//! are logged and reported to the caller, never fatal.
//!
//! The output lives on its own thread and is open only while a sound plays (and `LINGER`
//! after it), so no device or sound server stream is held between sounds: the server can
//! suspend or restart and other programs can take the card. An output that reports an
//! error is closed and the next sound opens a new one. The daemon's thread never opens,
//! closes or joins an output, so a stuck device cannot block it or its exit.
//!
//! Each sound tries, in order: the PulseAudio protocol when a Pulse server is there
//! (PulseAudio, or PipeWire with pipewire-pulse), then PipeWire natively when its socket
//! is there, then ALSA's default device through rodio. The sound servers' client
//! libraries are loaded at run time, so they are not package dependencies; without one
//! the next backend is tried.

mod pipewire;
mod pulse;

use bentwire_character::Pcm;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamBuilder, Sink};
use std::ffi::{CStr, c_void};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How long `play` waits for the audio thread to open the device.
const OPEN_WAIT: Duration = Duration::from_millis(500);
/// The stream stays open this long after its sound ended (the device's buffer drains,
/// the next sound of an animation reuses it).
const LINGER: Duration = Duration::from_secs(1);
/// How often the audio thread checks an open stream.
const POLL: Duration = Duration::from_millis(100);
/// How long dropping `Audio` waits for the audio thread to close the stream.
const CLOSE_WAIT: Duration = Duration::from_millis(200);

/// The first error a stream reported, set by its error callback.
type Fault = Arc<Mutex<Option<String>>>;

fn set_fault(fault: &Fault, e: String) {
    let mut f = fault.lock().unwrap();
    if f.is_none() {
        *f = Some(e);
    }
}

/// An open output device. Dropping it closes the device.
trait Output {
    /// Replace the current sound.
    fn play(&mut self, channels: u16, rate: u32, samples: Vec<f32>);
    /// No sound is playing.
    fn done(&self) -> bool;
}

/// Opens an output for a first sound with this many channels and this rate.
type Opener = Box<dyn FnMut(Fault, u16, u32) -> Result<Box<dyn Output>, String> + Send>;

enum Cmd {
    Play {
        channels: u16,
        rate: u32,
        samples: Vec<f32>,
        reply: Sender<bool>,
        /// `play` has given up after this.
        deadline: Instant,
    },
    Stop,
}

pub struct Audio {
    tx: Option<Sender<Cmd>>,
    /// Reply to a `play` that timed out: the audio thread is still busy with it.
    pending: Option<Receiver<bool>>,
    /// Closed when the audio thread ends.
    ended: Receiver<()>,
}

impl Audio {
    /// None when the audio thread cannot be started (the daemon runs silently). The
    /// device is opened for each sound, so one that appears later is used.
    pub fn open() -> Option<Audio> {
        let mut chooser = Chooser::default();
        // ALSA's device list is read only when the default device fails, and once
        // until a device opens: reading it opens every device, which logs noise.
        let mut scan = true;
        Audio::spawn(Box::new(move |fault, channels, rate| {
            let list = backends(
                std::env::var("PULSE_SERVER").ok().as_deref(),
                std::env::var_os("XDG_RUNTIME_DIR")
                    .map(PathBuf::from)
                    .as_deref(),
            );
            let mut log = Vec::new();
            let r = chooser.open(&list, &mut log, |b| match b {
                Backend::Pulse(server) => {
                    pulse::open(server, channels, rate, fault.clone()).map(|o| (o, "pulse".into()))
                }
                Backend::PipeWire(socket) => pipewire::open(socket, channels, rate, fault.clone())
                    .map(|o| (o, "pipewire".into())),
                Backend::Alsa => open_alsa(fault.clone(), &mut scan),
            });
            for (level, m) in log {
                log::log!(level, "{m}");
            }
            r
        }))
    }

    fn spawn(open: Opener) -> Option<Audio> {
        let (tx, rx) = mpsc::channel();
        let (ended_tx, ended) = mpsc::channel::<()>();
        std::thread::Builder::new()
            .name("bentwire-audio".into())
            .spawn(move || {
                run(rx, open);
                drop(ended_tx);
            })
            .map_err(|e| log::warn!("no audio output: {e}"))
            .ok()?;
        Some(Audio {
            tx: Some(tx),
            pending: None,
            ended,
        })
    }

    /// Returns false if the sound cannot be played.
    pub fn play(&mut self, pcm: &Pcm) -> bool {
        if pcm.channels == 0 || pcm.rate == 0 {
            self.stop();
            return false;
        }
        if let Some(p) = &self.pending {
            match p.try_recv() {
                Err(mpsc::TryRecvError::Empty) => {
                    self.stop();
                    return false;
                }
                _ => self.pending = None,
            }
        }
        let samples: Vec<f32> = pcm.to_i16().iter().map(|s| *s as f32 / 32768.0).collect();
        let (reply, rx) = mpsc::channel();
        let cmd = Cmd::Play {
            channels: pcm.channels,
            rate: pcm.rate,
            samples,
            reply,
            deadline: Instant::now() + OPEN_WAIT,
        };
        if self.tx.as_ref().is_none_or(|tx| tx.send(cmd).is_err()) {
            return false;
        }
        match rx.recv_timeout(OPEN_WAIT) {
            Ok(ok) => ok,
            Err(RecvTimeoutError::Timeout) => {
                log::warn!("audio device did not open within {OPEN_WAIT:?}");
                self.pending = Some(rx);
                false
            }
            Err(RecvTimeoutError::Disconnected) => false,
        }
    }

    pub fn stop(&mut self) {
        if let Some(tx) = &self.tx {
            let _ = tx.send(Cmd::Stop);
        }
    }
}

impl Drop for Audio {
    /// The audio thread closes the stream; a close that hangs does not hold up the exit.
    fn drop(&mut self) {
        self.tx = None;
        let _ = self.ended.recv_timeout(CLOSE_WAIT);
    }
}

/// The audio thread: opens the device for a sound, closes it when idle or broken.
fn run(rx: Receiver<Cmd>, mut open: Opener) {
    let mut out: Option<(Box<dyn Output>, Fault)> = None;
    let mut idle_since: Option<Instant> = None;
    let mut failing = false;
    loop {
        let cmd = if out.is_some() {
            rx.recv_timeout(POLL)
        } else {
            rx.recv().map_err(|_| RecvTimeoutError::Disconnected)
        };
        if let Some((o, fault)) = &out {
            let err = fault.lock().unwrap().take();
            if let Some(e) = err {
                log::warn!("audio stream error: {e}; closing the stream");
                out = None;
            } else if !o.done() {
                idle_since = None;
            } else if idle_since.get_or_insert_with(Instant::now).elapsed() >= LINGER {
                out = None;
            }
        }
        match cmd {
            Ok(Cmd::Play {
                channels,
                rate,
                samples,
                reply,
                deadline,
            }) => {
                if out.is_none() && Instant::now() <= deadline {
                    let fault = Fault::default();
                    match open(fault.clone(), channels, rate) {
                        Ok(o) => {
                            if failing {
                                log::info!("audio output available again");
                            }
                            failing = false;
                            out = Some((o, fault));
                        }
                        Err(e) => {
                            if !failing {
                                log::warn!("no audio output: {e}");
                            }
                            failing = true;
                        }
                    }
                }
                let ok = out.is_some() && Instant::now() <= deadline;
                if ok && let Some((o, _)) = &mut out {
                    o.play(channels, rate, samples);
                    idle_since = None;
                }
                let _ = reply.send(ok);
            }
            Ok(Cmd::Stop) => out = None,
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// A way to the speakers, tried in the order of `backends`.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Backend {
    /// The PulseAudio protocol at this libpulse server string.
    Pulse(String),
    /// PipeWire's own protocol at this socket.
    PipeWire(PathBuf),
    /// ALSA's default device: without an ALSA plugin of the sound server, the card itself,
    /// which a running server holds.
    Alsa,
}

impl Backend {
    fn name(&self) -> &'static str {
        match self {
            Backend::Pulse(_) => "pulse",
            Backend::PipeWire(_) => "pipewire",
            Backend::Alsa => "alsa",
        }
    }
}

/// The backends to try for a sound, given `$PULSE_SERVER` and `$XDG_RUNTIME_DIR`. The
/// explicit server string also keeps libpulse from starting a server of its own.
fn backends(pulse_server: Option<&str>, runtime: Option<&Path>) -> Vec<Backend> {
    let mut list = Vec::new();
    let socket = runtime
        .map(|d| d.join("pulse/native"))
        .filter(|p| p.exists());
    match (pulse_server.filter(|s| !s.is_empty()), socket) {
        (Some(s), _) => list.push(Backend::Pulse(s.to_string())),
        (None, Some(p)) => list.push(Backend::Pulse(format!("unix:{}", p.display()))),
        (None, None) => {}
    }
    if let Some(p) = runtime.map(|d| d.join("pipewire-0")).filter(|p| p.exists()) {
        list.push(Backend::PipeWire(p));
    }
    list.push(Backend::Alsa);
    list
}

/// Opens the first backend that works. Says which one is used when that changes, and
/// that a backend failed once until it works again.
#[derive(Default)]
struct Chooser {
    used: Option<String>,
    failed: Vec<&'static str>,
}

type Opened = Result<(Box<dyn Output>, String), String>;

impl Chooser {
    /// `try_open` returns the output and how to name it in the log.
    fn open(
        &mut self,
        list: &[Backend],
        log: &mut Vec<(log::Level, String)>,
        mut try_open: impl FnMut(&Backend) -> Opened,
    ) -> Result<Box<dyn Output>, String> {
        let mut last = String::from("no audio backend");
        for (i, b) in list.iter().enumerate() {
            match try_open(b) {
                Ok((o, label)) => {
                    self.failed.retain(|n| *n != b.name());
                    if self.used.as_ref() != Some(&label) {
                        log.push((log::Level::Info, format!("audio: {label}")));
                        self.used = Some(label);
                    }
                    return Ok(o);
                }
                Err(e) => {
                    if let Some(next) = list.get(i + 1)
                        && !self.failed.contains(&b.name())
                    {
                        self.failed.push(b.name());
                        let m = format!("audio: {} failed: {e}; trying {}", b.name(), next.name());
                        log.push((log::Level::Warn, m));
                    }
                    last = format!("{}: {e}", b.name());
                }
            }
        }
        Err(last)
    }
}

/// A shared library loaded at run time. It is never unloaded: sound server client
/// libraries do not support that.
struct Dl(*mut c_void);

impl Dl {
    fn open(name: &CStr) -> Result<Dl, String> {
        // SAFETY: dlopen with a NUL-terminated name.
        let h = unsafe { libc::dlopen(name.as_ptr(), libc::RTLD_NOW | libc::RTLD_LOCAL) };
        if h.is_null() {
            // SAFETY: dlerror returns NULL or a NUL-terminated string.
            let e = unsafe { libc::dlerror() };
            return Err(if e.is_null() {
                format!("{}: cannot load", name.to_string_lossy())
            } else {
                // SAFETY: see above.
                unsafe { CStr::from_ptr(e) }.to_string_lossy().into_owned()
            });
        }
        Ok(Dl(h))
    }

    /// The function `name` of this library (or of the libraries it loaded).
    ///
    /// # Safety
    /// `T` must be the function pointer type of that C function.
    unsafe fn sym<T: Copy>(&self, name: &CStr) -> Result<T, String> {
        assert_eq!(size_of::<T>(), size_of::<*mut c_void>());
        // SAFETY: a live handle and a NUL-terminated name.
        let p = unsafe { libc::dlsym(self.0, name.as_ptr()) };
        if p.is_null() {
            return Err(format!("{} not found", name.to_string_lossy()));
        }
        // SAFETY: the caller names the type; sizes are equal.
        Ok(unsafe { std::mem::transmute_copy(&p) })
    }
}

/// rodio's stream with its sink. The sink is dropped before the stream.
struct Device {
    sink: Sink,
    stream: OutputStream,
}

impl Output for Device {
    fn play(&mut self, channels: u16, rate: u32, samples: Vec<f32>) {
        self.sink.stop();
        self.sink = Sink::connect_new(self.stream.mixer());
        self.sink
            .append(SamplesBuffer::new(channels, rate, samples));
    }

    fn done(&self) -> bool {
        self.sink.empty()
    }
}

/// ALSA's default device, else (if `scan`) the first that opens, as rodio's
/// `open_default_stream`. `scan` is cleared when the list was read in vain and set when
/// a device opens. The error callback only records the error: rodio's default one prints
/// every error, and a broken ALSA stream reports errors in a tight loop.
fn open_alsa(fault: Fault, scan: &mut bool) -> Opened {
    let on_error = move |e: rodio::cpal::StreamError| {
        set_fault(&fault, e.to_string());
        // A broken stream calls this in a loop until it is closed.
        std::thread::sleep(Duration::from_millis(20));
    };
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let opened = |mut stream: OutputStream, label: String| -> Opened {
        stream.log_on_drop(false);
        Ok((
            Box::new(Device {
                sink: Sink::connect_new(stream.mixer()),
                stream,
            }),
            label,
        ))
    };
    let first = match OutputStreamBuilder::from_default_device()
        .and_then(|b| b.with_error_callback(on_error.clone()).open_stream())
    {
        Ok(s) => {
            *scan = true;
            return opened(s, "alsa default".into());
        }
        Err(e) => e.to_string(),
    };
    if !std::mem::replace(scan, false) {
        return Err(first);
    }
    let devices = rodio::cpal::default_host()
        .output_devices()
        .map_err(|_| first.clone())?;
    for d in devices {
        let name = d.name().unwrap_or_default();
        if let Ok(s) = OutputStreamBuilder::from_device(d).and_then(|b| {
            b.with_error_callback(on_error.clone())
                .open_stream_or_fallback()
        }) {
            *scan = true;
            return opened(s, format!("alsa {name}"));
        }
    }
    Err(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct Counts {
        opens: AtomicUsize,
        open: AtomicUsize,
        /// The next open fails.
        fail: AtomicBool,
        /// Closing blocks forever.
        hang: AtomicBool,
        /// Closing blocks until this is cleared.
        hold: AtomicBool,
        plays: AtomicUsize,
        faults: Mutex<Vec<Fault>>,
    }

    /// Plays for `ms` milliseconds of wall time.
    struct Fake {
        c: Arc<Counts>,
        until: Option<Instant>,
    }

    impl Output for Fake {
        fn play(&mut self, _: u16, rate: u32, samples: Vec<f32>) {
            self.c.plays.fetch_add(1, Ordering::SeqCst);
            let ms = samples.len() as u64 * 1000 / rate as u64;
            self.until = Some(Instant::now() + Duration::from_millis(ms));
        }
        fn done(&self) -> bool {
            self.until.is_none_or(|t| Instant::now() >= t)
        }
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.c.open.fetch_sub(1, Ordering::SeqCst);
            while self.c.hold.load(Ordering::SeqCst) {
                std::thread::sleep(Duration::from_millis(10));
            }
            if self.c.hang.load(Ordering::SeqCst) {
                loop {
                    std::thread::park();
                }
            }
        }
    }

    fn fake() -> (Audio, Arc<Counts>) {
        let c = Arc::new(Counts::default());
        let c2 = c.clone();
        let audio = Audio::spawn(Box::new(move |fault, _, _| {
            if c2.fail.swap(false, Ordering::SeqCst) {
                return Err("no device".into());
            }
            c2.opens.fetch_add(1, Ordering::SeqCst);
            c2.open.fetch_add(1, Ordering::SeqCst);
            c2.faults.lock().unwrap().push(fault);
            Ok(Box::new(Fake {
                c: c2.clone(),
                until: None,
            }))
        }))
        .unwrap();
        (audio, c)
    }

    /// `ms` milliseconds of mono 8-bit PCM at 1000 Hz.
    fn pcm(ms: usize) -> Pcm {
        Pcm {
            channels: 1,
            rate: 1000,
            bits: 8,
            data: vec![128; ms],
        }
    }

    fn wait_for(what: &str, f: impl Fn() -> bool) {
        let t = Instant::now();
        while !f() {
            assert!(t.elapsed() < Duration::from_secs(5), "timed out: {what}");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn closed_when_idle() {
        let (mut a, c) = fake();
        assert_eq!(
            c.open.load(Ordering::SeqCst),
            0,
            "nothing open before a sound"
        );
        assert!(a.play(&pcm(100)));
        assert!(a.play(&pcm(100)), "a second sound reuses the stream");
        assert_eq!(c.opens.load(Ordering::SeqCst), 1);
        wait_for("closed after the sound", || {
            c.open.load(Ordering::SeqCst) == 0
        });
        assert!(a.play(&pcm(100)), "the next sound opens it again");
        assert_eq!(c.opens.load(Ordering::SeqCst), 2);
        a.stop();
        wait_for("closed on stop", || c.open.load(Ordering::SeqCst) == 0);
    }

    #[test]
    fn reopened_after_error() {
        let (mut a, c) = fake();
        assert!(a.play(&pcm(10_000)));
        let fault = c.faults.lock().unwrap()[0].clone();
        *fault.lock().unwrap() = Some("device gone".into());
        wait_for("broken stream closed", || {
            c.open.load(Ordering::SeqCst) == 0
        });
        assert!(a.play(&pcm(100)));
        assert_eq!(c.opens.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn failed_open_is_retried() {
        let (mut a, c) = fake();
        c.fail.store(true, Ordering::SeqCst);
        assert!(!a.play(&pcm(100)), "no device: the sound fails");
        assert!(a.play(&pcm(100)), "the device appeared");
    }

    #[test]
    fn hanging_close_does_not_block() {
        let (mut a, c) = fake();
        assert!(a.play(&pcm(10_000)));
        c.hang.store(true, Ordering::SeqCst);
        a.stop();
        wait_for("close started", || c.open.load(Ordering::SeqCst) == 0);
        // The audio thread hangs in the close: sounds fail within OPEN_WAIT, once.
        let t = Instant::now();
        assert!(!a.play(&pcm(100)));
        assert!(!a.play(&pcm(100)));
        assert!(t.elapsed() < OPEN_WAIT * 2, "{:?}", t.elapsed());
        let t = Instant::now();
        drop(a);
        assert!(t.elapsed() < CLOSE_WAIT * 2, "{:?}", t.elapsed());
    }

    #[test]
    fn sound_given_up_does_not_start_later() {
        let (mut a, c) = fake();
        assert!(a.play(&pcm(10_000)));
        c.hold.store(true, Ordering::SeqCst);
        a.stop();
        wait_for("close started", || c.open.load(Ordering::SeqCst) == 0);
        assert!(!a.play(&pcm(100)), "the close takes longer than OPEN_WAIT");
        c.hold.store(false, Ordering::SeqCst);
        assert_eq!(a.pending.take().unwrap().recv(), Ok(false));
        assert_eq!(c.plays.load(Ordering::SeqCst), 1);
        assert!(a.play(&pcm(100)), "the thread is free again");
    }

    #[test]
    fn drop_closes_the_stream() {
        let (mut a, c) = fake();
        assert!(a.play(&pcm(10_000)));
        drop(a);
        assert_eq!(c.open.load(Ordering::SeqCst), 0);
    }

    /// A temporary `$XDG_RUNTIME_DIR` with these (empty) files.
    fn runtime(files: &[&str]) -> PathBuf {
        let d = crate::config::tests::tempdir();
        for f in files {
            let p = d.join(f);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(&p, "").unwrap();
        }
        d
    }

    #[test]
    fn backends_from_environment() {
        use Backend::*;
        // The owner's case: PulseAudio's socket, PipeWire's beside it.
        let both = runtime(&["pulse/native", "pipewire-0"]);
        let native = format!("unix:{}", both.join("pulse/native").display());
        let pw = PipeWire(both.join("pipewire-0"));
        let owner = [Pulse(native.clone()), pw.clone(), Alsa];
        assert_eq!(backends(None, Some(&both)), owner);
        assert_eq!(backends(Some(""), Some(&both)), owner);
        let tcp = Pulse("tcp:host".into());
        assert_eq!(
            backends(Some("tcp:host"), Some(&both)),
            [tcp.clone(), pw, Alsa]
        );
        assert_eq!(backends(Some("tcp:host"), None), [tcp, Alsa]);
        // PipeWire without pipewire-pulse.
        let only = runtime(&["pipewire-0"]);
        assert_eq!(
            backends(None, Some(&only)),
            [PipeWire(only.join("pipewire-0")), Alsa]
        );
        assert_eq!(backends(None, Some(&runtime(&[]))), [Alsa]);
        assert_eq!(backends(None, None), [Alsa]);
    }

    struct Silent;

    impl Output for Silent {
        fn play(&mut self, _: u16, _: u32, _: Vec<f32>) {}
        fn done(&self) -> bool {
            true
        }
    }

    /// Opens with fake backends, of which `works` open. Returns the backends tried, the
    /// log and whether one opened.
    fn choose(
        c: &mut Chooser,
        list: &[Backend],
        works: &[&str],
    ) -> (Vec<&'static str>, Vec<String>, bool) {
        let mut tried = Vec::new();
        let mut log = Vec::new();
        let r = c.open(list, &mut log, |b| {
            tried.push(b.name());
            if works.contains(&b.name()) {
                Ok((Box::new(Silent) as Box<dyn Output>, b.name().to_string()))
            } else {
                Err(format!("no {}", b.name()))
            }
        });
        (tried, log.into_iter().map(|(_, m)| m).collect(), r.is_ok())
    }

    #[test]
    fn pulse_first_alsa_untouched() {
        // The owner's case: the server holds the card, so ALSA's default would fail.
        let list = backends(None, Some(&runtime(&["pulse/native", "pipewire-0"])));
        let mut c = Chooser::default();
        let r = choose(&mut c, &list, &["pulse"]);
        assert_eq!(r, (vec!["pulse"], vec!["audio: pulse".into()], true));
        let r = choose(&mut c, &list, &["pulse"]);
        assert_eq!(r, (vec!["pulse"], vec![], true), "logged once");
    }

    #[test]
    fn failed_backend_falls_through_logged_once() {
        let list = [Backend::Pulse("unix:/x".into()), Backend::Alsa];
        let mut c = Chooser::default();
        // No libpulse: ALSA.
        let (tried, log, ok) = choose(&mut c, &list, &["alsa"]);
        assert_eq!(tried, ["pulse", "alsa"]);
        assert_eq!(
            log,
            ["audio: pulse failed: no pulse; trying alsa", "audio: alsa"]
        );
        assert!(ok);
        assert_eq!(choose(&mut c, &list, &["alsa"]).1, Vec::<String>::new());
        // The server is back, then gone again.
        assert_eq!(
            choose(&mut c, &list, &["pulse", "alsa"]).1,
            ["audio: pulse"]
        );
        assert_eq!(
            choose(&mut c, &list, &["alsa"]).1,
            ["audio: pulse failed: no pulse; trying alsa", "audio: alsa"]
        );
        // Nothing works: the last error, for the audio thread's own log line.
        let mut log = Vec::new();
        let r = Chooser::default().open(&list, &mut log, |b| Err(format!("no {}", b.name())));
        assert_eq!(r.err().as_deref(), Some("alsa: no alsa"));
        assert_eq!(log.len(), 1);
    }

    #[test]
    fn stale_pulse_socket_fails_at_once() {
        let p = crate::config::tests::tempdir().join("native");
        drop(std::os::unix::net::UnixListener::bind(&p).unwrap());
        assert!(p.exists(), "the socket file stays");
        let t = Instant::now();
        let r = pulse::open(&format!("unix:{}", p.display()), 1, 22050, Fault::default());
        assert!(r.is_err_and(|e| e.contains("refused")));
        // A $PULSE_SERVER list or a TCP server is libpulse's to try.
        let list = format!("unix:{} unix:/run/other/native", p.display());
        assert_eq!(pulse::probe(&list), Ok(()));
        assert_eq!(pulse::probe("tcp:localhost:4713"), Ok(()));
        assert!(
            t.elapsed() < Duration::from_millis(100),
            "{:?}",
            t.elapsed()
        );
    }

    #[test]
    fn pipewire_after_pulse() {
        let list = backends(None, Some(&runtime(&["pulse/native", "pipewire-0"])));
        let mut c = Chooser::default();
        // A stale pulse socket (or no libpulse), PipeWire running.
        let (tried, log, ok) = choose(&mut c, &list, &["pipewire", "alsa"]);
        assert_eq!(tried, ["pulse", "pipewire"]);
        assert_eq!(
            log,
            [
                "audio: pulse failed: no pulse; trying pipewire",
                "audio: pipewire"
            ]
        );
        assert!(ok);
        // No libpipewire either.
        let (tried, log, _) = choose(&mut c, &list, &["alsa"]);
        assert_eq!(tried, ["pulse", "pipewire", "alsa"]);
        assert_eq!(
            log,
            [
                "audio: pipewire failed: no pipewire; trying alsa",
                "audio: alsa"
            ]
        );
    }

    #[test]
    fn stale_pipewire_socket_fails_at_once() {
        let p = crate::config::tests::tempdir().join("pipewire-0");
        drop(std::os::unix::net::UnixListener::bind(&p).unwrap());
        let t = Instant::now();
        let r = pipewire::open(&p, 1, 22050, Fault::default());
        assert!(r.is_err_and(|e| e.contains("refused")));
        assert!(t.elapsed() < Duration::from_millis(100));
    }
}
