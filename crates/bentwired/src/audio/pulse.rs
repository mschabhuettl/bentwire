//! PulseAudio protocol output through libpulse-simple, loaded at run time. Serves
//! PulseAudio and PipeWire with pipewire-pulse.
//!
//! `pa_simple` calls block, so a feeder thread owns the connection and writes the sound
//! in 10 ms chunks: a new sound or a stop is noticed within a chunk, and the server's
//! buffer (about `TARGET` long) is flushed. At the end of a sound the feeder drains the
//! stream, which also starts a sound shorter than the prebuffer.

use super::{Dl, Fault, Output, set_fault};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::Duration;

/// The server's buffer for the stream (`tlength`): the latency of a sound.
const TARGET: Duration = Duration::from_millis(50);
/// Samples per write, as a fraction of a second.
const CHUNKS_PER_SECOND: u32 = 100;

#[repr(C)]
struct SampleSpec {
    format: c_int,
    rate: u32,
    channels: u8,
}

#[repr(C)]
struct BufferAttr {
    maxlength: u32,
    tlength: u32,
    prebuf: u32,
    minreq: u32,
    fragsize: u32,
}

#[cfg(target_endian = "little")]
const FLOAT32NE: c_int = 5;
#[cfg(target_endian = "big")]
const FLOAT32NE: c_int = 6;
const PLAYBACK: c_int = 1;

type NewFn = unsafe extern "C" fn(
    *const c_char,
    *const c_char,
    c_int,
    *const c_char,
    *const c_char,
    *const SampleSpec,
    *const c_void,
    *const BufferAttr,
    *mut c_int,
) -> *mut c_void;
type IoFn = unsafe extern "C" fn(*mut c_void, *mut c_int) -> c_int;

struct Lib {
    new: NewFn,
    write: unsafe extern "C" fn(*mut c_void, *const c_void, usize, *mut c_int) -> c_int,
    drain: IoFn,
    flush: IoFn,
    free: unsafe extern "C" fn(*mut c_void),
    strerror: unsafe extern "C" fn(c_int) -> *const c_char,
}

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| {
        let dl = Dl::open(c"libpulse-simple.so.0")?;
        // SAFETY: the types are the C prototypes of <pulse/simple.h> and <pulse/error.h>.
        unsafe {
            Ok(Lib {
                new: dl.sym(c"pa_simple_new")?,
                write: dl.sym(c"pa_simple_write")?,
                drain: dl.sym(c"pa_simple_drain")?,
                flush: dl.sym(c"pa_simple_flush")?,
                free: dl.sym(c"pa_simple_free")?,
                strerror: dl.sym(c"pa_strerror")?,
            })
        }
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// One `pa_simple` playback stream. Used only by the feeder thread.
struct Stream {
    lib: &'static Lib,
    s: *mut c_void,
    channels: u16,
    rate: u32,
}

// SAFETY: a pa_simple may be used from any thread, one at a time; only the feeder does.
unsafe impl Send for Stream {}

impl Stream {
    fn new(lib: &'static Lib, server: &CStr, channels: u16, rate: u32) -> Result<Stream, String> {
        let spec = SampleSpec {
            format: FLOAT32NE,
            rate,
            channels: u8::try_from(channels).map_err(|_| format!("{channels} channels"))?,
        };
        let bytes = |d: Duration| {
            (d.as_micros() as u64 * rate as u64 / 1_000_000) as u32 * 4 * channels as u32
        };
        let attr = BufferAttr {
            maxlength: u32::MAX,
            tlength: bytes(TARGET),
            prebuf: u32::MAX,
            minreq: u32::MAX,
            fragsize: u32::MAX,
        };
        let mut err = 0;
        // SAFETY: all pointers are valid for the call; NULL device and channel map mean
        // the defaults.
        let s = unsafe {
            (lib.new)(
                server.as_ptr(),
                c"bentwire".as_ptr(),
                PLAYBACK,
                std::ptr::null(),
                c"Sound effects".as_ptr(),
                &spec,
                std::ptr::null(),
                &attr,
                &mut err,
            )
        };
        if s.is_null() {
            return Err(lib.error(err));
        }
        Ok(Stream {
            lib,
            s,
            channels,
            rate,
        })
    }

    fn write(&mut self, samples: &[f32]) -> Result<(), String> {
        let mut err = 0;
        // SAFETY: the buffer is valid for its length in bytes.
        let r = unsafe {
            (self.lib.write)(
                self.s,
                samples.as_ptr().cast(),
                size_of_val(samples),
                &mut err,
            )
        };
        if r < 0 {
            Err(self.lib.error(err))
        } else {
            Ok(())
        }
    }

    fn call(&mut self, f: IoFn) -> Result<(), String> {
        let mut err = 0;
        // SAFETY: `s` is a live pa_simple.
        if unsafe { f(self.s, &mut err) } < 0 {
            Err(self.lib.error(err))
        } else {
            Ok(())
        }
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        // SAFETY: `s` is live and not used after this.
        unsafe { (self.lib.free)(self.s) }
    }
}

impl Lib {
    fn error(&self, err: c_int) -> String {
        // SAFETY: pa_strerror returns a static string or NULL.
        let p = unsafe { (self.strerror)(err) };
        if p.is_null() {
            format!("error {err}")
        } else {
            // SAFETY: a NUL-terminated static string.
            unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
        }
    }
}

#[derive(Default)]
struct Feed {
    /// The sound being written and how far.
    sound: Option<(u16, u32, Vec<f32>, usize)>,
    /// Counts `play` calls: the feeder flushes when it changes.
    serial: u64,
    /// A sound is written or not yet drained.
    busy: bool,
    quit: bool,
}

#[derive(Default)]
struct Shared {
    feed: Mutex<Feed>,
    wake: Condvar,
}

struct Pulse {
    shared: Arc<Shared>,
    feeder: Option<std::thread::JoinHandle<()>>,
}

/// Connects to `server` (a libpulse server string) with a stream for this format.
pub(super) fn open(
    server: &str,
    channels: u16,
    rate: u32,
    fault: Fault,
) -> Result<Box<dyn Output>, String> {
    // A socket left behind by a server that is gone refuses at once; checked before
    // loading the library, so that a missing server costs nothing.
    if let Some(path) = server.strip_prefix("unix:") {
        std::os::unix::net::UnixStream::connect(path).map_err(|e| format!("{path}: {e}"))?;
    }
    let lib = lib()?;
    let server = CString::new(server).map_err(|e| e.to_string())?;
    let stream = Stream::new(lib, &server, channels, rate)?;
    let shared = Arc::new(Shared::default());
    let s2 = shared.clone();
    let feeder = std::thread::Builder::new()
        .name("bentwire-pulse".into())
        .spawn(move || {
            if let Err(e) = feed(&s2, stream, &server) {
                set_fault(&fault, e);
                let mut f = s2.feed.lock().unwrap();
                while !f.quit {
                    f = s2.wake.wait(f).unwrap();
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Box::new(Pulse {
        shared,
        feeder: Some(feeder),
    }))
}

/// The feeder thread: writes the current sound chunk by chunk until told to quit.
fn feed(shared: &Shared, mut stream: Stream, server: &CStr) -> Result<(), String> {
    let mut serial = 0;
    let mut chunk = Vec::new();
    loop {
        let (channels, rate, last, replaced) = {
            let mut f = shared.feed.lock().unwrap();
            while f.sound.is_none() && !f.quit {
                f = shared.wake.wait(f).unwrap();
            }
            if f.quit {
                return Ok(());
            }
            let replaced = f.serial != serial;
            serial = f.serial;
            let (channels, rate, samples, pos) = f.sound.as_mut().unwrap();
            let n = (*rate / CHUNKS_PER_SECOND).max(1) as usize * *channels as usize;
            let end = samples.len().min(*pos + n);
            chunk.clear();
            chunk.extend_from_slice(&samples[*pos..end]);
            *pos = end;
            let r = (*channels, *rate, end == samples.len(), replaced);
            if r.2 {
                f.sound = None;
            }
            r
        };
        if (channels, rate) != (stream.channels, stream.rate) {
            let lib = stream.lib;
            drop(stream);
            stream = Stream::new(lib, server, channels, rate)?;
        } else if replaced {
            stream.call(stream.lib.flush)?;
        }
        stream.write(&chunk)?;
        if last {
            stream.call(stream.lib.drain)?;
            let mut f = shared.feed.lock().unwrap();
            if f.serial == serial {
                f.busy = false;
            }
        }
    }
}

impl Output for Pulse {
    fn play(&mut self, channels: u16, rate: u32, samples: Vec<f32>) {
        let mut f = self.shared.feed.lock().unwrap();
        f.serial += 1;
        f.busy = !samples.is_empty();
        f.sound = (!samples.is_empty()).then_some((channels, rate, samples, 0));
        self.shared.wake.notify_all();
    }

    fn done(&self) -> bool {
        !self.shared.feed.lock().unwrap().busy
    }
}

impl Drop for Pulse {
    /// The feeder finishes its chunk and closes the stream; the server drops what is
    /// still buffered.
    fn drop(&mut self) {
        self.shared.feed.lock().unwrap().quit = true;
        self.shared.wake.notify_all();
        if let Some(t) = self.feeder.take() {
            let _ = t.join();
        }
    }
}
