//! Sound output through rodio. One sound at a time: starting a sound stops the previous
//! one. Failures are logged and reported to the caller, never fatal.
//!
//! The output stream lives on its own thread and is open only while a sound plays (and
//! `LINGER` after it), so no device or PipeWire stream is held between sounds: PipeWire
//! can suspend or restart and other programs can take the card. A stream that reports an
//! error is closed and the next sound opens a new one. The daemon's thread never opens,
//! closes or joins a stream, so a stuck device cannot block it or its exit.

use bentwire_character::Pcm;
use rodio::buffer::SamplesBuffer;
use rodio::{OutputStream, OutputStreamBuilder, Sink};
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

/// An open output device. Dropping it closes the device.
trait Output {
    /// Replace the current sound.
    fn play(&mut self, channels: u16, rate: u32, samples: Vec<f32>);
    /// No sound is playing.
    fn done(&self) -> bool;
}

type Opener = Box<dyn FnMut(Fault) -> Result<Box<dyn Output>, String> + Send>;

enum Cmd {
    Play {
        channels: u16,
        rate: u32,
        samples: Vec<f32>,
        reply: Sender<bool>,
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
        Audio::spawn(Box::new(open_device))
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
            }) => {
                let t = Instant::now();
                if out.is_none() {
                    let fault = Fault::default();
                    match open(fault.clone()) {
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
                let ok = out.is_some() && t.elapsed() <= OPEN_WAIT;
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

/// The default device, else the first that opens (as rodio's `open_default_stream`).
/// The error callback only records the error: rodio's default one prints every error,
/// and a broken ALSA stream reports errors in a tight loop.
fn open_device(fault: Fault) -> Result<Box<dyn Output>, String> {
    let on_error = move |e: rodio::cpal::StreamError| {
        let mut f = fault.lock().unwrap();
        if f.is_none() {
            *f = Some(e.to_string());
        }
        drop(f);
        // A broken stream calls this in a loop until it is closed.
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stream = OutputStreamBuilder::from_default_device()
        .and_then(|b| b.with_error_callback(on_error.clone()).open_stream())
        .or_else(|first| {
            use rodio::cpal::traits::HostTrait;
            let devices = rodio::cpal::default_host()
                .output_devices()
                .map_err(|_| first.to_string())?;
            for d in devices {
                if let Ok(s) = OutputStreamBuilder::from_device(d).and_then(|b| {
                    b.with_error_callback(on_error.clone())
                        .open_stream_or_fallback()
                }) {
                    return Ok(s);
                }
            }
            Err(first.to_string())
        })?;
    stream.log_on_drop(false);
    Ok(Box::new(Device {
        sink: Sink::connect_new(stream.mixer()),
        stream,
    }))
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
        faults: Mutex<Vec<Fault>>,
    }

    /// Plays for `ms` milliseconds of wall time.
    struct Fake {
        c: Arc<Counts>,
        until: Option<Instant>,
    }

    impl Output for Fake {
        fn play(&mut self, _: u16, rate: u32, samples: Vec<f32>) {
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
        let audio = Audio::spawn(Box::new(move |fault| {
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
    fn drop_closes_the_stream() {
        let (mut a, c) = fake();
        assert!(a.play(&pcm(10_000)));
        drop(a);
        assert_eq!(c.open.load(Ordering::SeqCst), 0);
    }
}
