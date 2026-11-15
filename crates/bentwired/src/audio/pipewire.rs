//! Native PipeWire output through libpipewire-0.3, loaded at run time: for PipeWire
//! without pipewire-pulse.
//!
//! A `pw_thread_loop` runs one playback stream (`pw_stream_new_simple`). Its process
//! callback (on the loop's thread, not the real-time one) copies the next buffer of the
//! sound; after the last one it drains the stream, and the `drained` event ends the
//! sound. A new sound flushes what is queued and restarts the stream. Calls from other
//! threads hold the loop's lock.

use super::{Dl, Fault, Output, set_fault};
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

#[repr(C)]
struct StreamEvents {
    version: u32,
    destroy: Option<unsafe extern "C" fn()>,
    state_changed: Option<unsafe extern "C" fn(*mut c_void, c_int, c_int, *const c_char)>,
    control_info: Option<unsafe extern "C" fn()>,
    io_changed: Option<unsafe extern "C" fn()>,
    param_changed: Option<unsafe extern "C" fn()>,
    add_buffer: Option<unsafe extern "C" fn()>,
    remove_buffer: Option<unsafe extern "C" fn()>,
    process: Option<unsafe extern "C" fn(*mut c_void)>,
    drained: Option<unsafe extern "C" fn(*mut c_void)>,
    command: Option<unsafe extern "C" fn()>,
    trigger_done: Option<unsafe extern "C" fn()>,
}

/// `struct pw_buffer`.
#[repr(C)]
struct PwBuffer {
    buffer: *mut SpaBuffer,
    user_data: *mut c_void,
    size: u64,
    requested: u64,
    time: u64,
}

#[repr(C)]
struct SpaBuffer {
    n_metas: u32,
    n_datas: u32,
    metas: *mut c_void,
    datas: *mut SpaData,
}

#[repr(C)]
struct SpaData {
    type_: u32,
    flags: u32,
    fd: i64,
    mapoffset: u32,
    maxsize: u32,
    data: *mut c_void,
    chunk: *mut SpaChunk,
}

#[repr(C)]
struct SpaChunk {
    offset: u32,
    size: u32,
    stride: i32,
    flags: i32,
}

/// PW_VERSION_STREAM_EVENTS.
const EVENTS_VERSION: u32 = 2;
const STATE_ERROR: c_int = -1;
const STATE_UNCONNECTED: c_int = 0;
const STATE_STREAMING: c_int = 3;
/// How long a new stream may take to be linked to an output. A PipeWire without an
/// audio sink or session manager never links it; ALSA is tried then, within OPEN_WAIT.
const LINK_WAIT: Duration = Duration::from_millis(300);
const DIRECTION_OUTPUT: c_int = 1;
const ID_ANY: u32 = u32::MAX;
/// PW_STREAM_FLAG_AUTOCONNECT | PW_STREAM_FLAG_MAP_BUFFERS (no RT_PROCESS: the process
/// callback runs on the loop's thread and may take the feed's lock).
const FLAGS: c_int = 0x1 | 0x4;

static EVENTS: StreamEvents = StreamEvents {
    version: EVENTS_VERSION,
    destroy: None,
    state_changed: Some(on_state_changed),
    control_info: None,
    io_changed: None,
    param_changed: None,
    add_buffer: None,
    remove_buffer: None,
    process: Some(on_process),
    drained: Some(on_drained),
    command: None,
    trigger_done: None,
};

type Ptr = *mut c_void;

struct Lib {
    loop_new: unsafe extern "C" fn(*const c_char, *const c_void) -> Ptr,
    loop_get_loop: unsafe extern "C" fn(Ptr) -> Ptr,
    loop_start: unsafe extern "C" fn(Ptr) -> c_int,
    loop_stop: unsafe extern "C" fn(Ptr),
    loop_destroy: unsafe extern "C" fn(Ptr),
    loop_lock: unsafe extern "C" fn(Ptr),
    loop_unlock: unsafe extern "C" fn(Ptr),
    properties_new_string: unsafe extern "C" fn(*const c_char) -> Ptr,
    stream_new_simple:
        unsafe extern "C" fn(Ptr, *const c_char, Ptr, *const StreamEvents, *mut c_void) -> Ptr,
    stream_connect: unsafe extern "C" fn(Ptr, c_int, u32, c_int, *const *const u32, u32) -> c_int,
    stream_destroy: unsafe extern "C" fn(Ptr),
    stream_dequeue_buffer: unsafe extern "C" fn(Ptr) -> *mut PwBuffer,
    stream_queue_buffer: unsafe extern "C" fn(Ptr, *mut PwBuffer) -> c_int,
    stream_flush: unsafe extern "C" fn(Ptr, bool) -> c_int,
    stream_set_active: unsafe extern "C" fn(Ptr, bool) -> c_int,
}

fn lib() -> Result<&'static Lib, String> {
    static LIB: OnceLock<Result<Lib, String>> = OnceLock::new();
    LIB.get_or_init(|| {
        let dl = Dl::open(c"libpipewire-0.3.so.0")?;
        // SAFETY: the types are the C prototypes of <pipewire/*.h>.
        unsafe {
            let init: unsafe extern "C" fn(*mut c_int, *mut c_void) = dl.sym(c"pw_init")?;
            let lib = Lib {
                loop_new: dl.sym(c"pw_thread_loop_new")?,
                loop_get_loop: dl.sym(c"pw_thread_loop_get_loop")?,
                loop_start: dl.sym(c"pw_thread_loop_start")?,
                loop_stop: dl.sym(c"pw_thread_loop_stop")?,
                loop_destroy: dl.sym(c"pw_thread_loop_destroy")?,
                loop_lock: dl.sym(c"pw_thread_loop_lock")?,
                loop_unlock: dl.sym(c"pw_thread_loop_unlock")?,
                properties_new_string: dl.sym(c"pw_properties_new_string")?,
                stream_new_simple: dl.sym(c"pw_stream_new_simple")?,
                stream_connect: dl.sym(c"pw_stream_connect")?,
                stream_destroy: dl.sym(c"pw_stream_destroy")?,
                stream_dequeue_buffer: dl.sym(c"pw_stream_dequeue_buffer")?,
                stream_queue_buffer: dl.sym(c"pw_stream_queue_buffer")?,
                stream_flush: dl.sym(c"pw_stream_flush")?,
                stream_set_active: dl.sym(c"pw_stream_set_active")?,
            };
            init(std::ptr::null_mut(), std::ptr::null_mut());
            Ok(lib)
        }
    })
    .as_ref()
    .map_err(Clone::clone)
}

/// An EnumFormat pod for interleaved native-endian f32 at this rate (as
/// `spa_format_audio_raw_build`): an object of (key, flags, value) properties, each
/// value padded to 8 bytes.
fn format_pod(channels: u16, rate: u32) -> Vec<u32> {
    const ID: u32 = 3;
    const INT: u32 = 4;
    const ARRAY: u32 = 13;
    const OBJECT: u32 = 15;
    #[cfg(target_endian = "little")]
    const F32: u32 = 0x11b;
    #[cfg(target_endian = "big")]
    const F32: u32 = 0x11c;
    // SPA_TYPE_OBJECT_Format, SPA_PARAM_EnumFormat.
    let mut body = vec![0x40003, 3];
    // mediaType audio, mediaSubtype raw, format, rate, channels.
    body.extend([1, 0, 4, ID, 1, 0]);
    body.extend([2, 0, 4, ID, 1, 0]);
    body.extend([0x10001, 0, 4, ID, F32, 0]);
    body.extend([0x10003, 0, 4, INT, rate, 0]);
    body.extend([0x10004, 0, 4, INT, channels as u32, 0]);
    // position: MONO, or FL FR; other counts stay unpositioned.
    let pos: &[u32] = match channels {
        1 => &[2],
        2 => &[3, 4],
        _ => &[],
    };
    if !pos.is_empty() {
        body.extend([0x10005, 0, 8 + 4 * pos.len() as u32, ARRAY, 4, ID]);
        body.extend(pos);
        if pos.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut pod = vec![4 * body.len() as u32, OBJECT];
    pod.extend(body);
    pod
}

#[derive(Default)]
struct Feed {
    channels: u16,
    rate: u32,
    sound: Vec<f32>,
    pos: usize,
    /// A sound is queued or not yet drained.
    busy: bool,
    /// The stream drains the end of the sound.
    draining: bool,
    /// The stream's last `pw_stream_state`.
    state: c_int,
}

/// What the callbacks see. Boxed: its address is the callbacks' data.
struct Shared {
    lib: &'static Lib,
    stream: Mutex<Ptr>,
    feed: Mutex<Feed>,
    fault: Fault,
}

struct PipeWire {
    lib: &'static Lib,
    thread_loop: Ptr,
    shared: Box<Shared>,
}

// SAFETY: the loop and stream are used under the loop's lock only.
unsafe impl Send for PipeWire {}

/// Connects a stream for this format to the PipeWire server at `socket`.
pub(super) fn open(
    socket: &std::path::Path,
    channels: u16,
    rate: u32,
    fault: Fault,
) -> Result<Box<dyn Output>, String> {
    // As for Pulse: a socket left behind refuses at once, before the library is loaded.
    std::os::unix::net::UnixStream::connect(socket)
        .map_err(|e| format!("{}: {e}", socket.display()))?;
    let lib = lib()?;
    // SAFETY: a NUL-terminated name and no properties.
    let thread_loop = unsafe { (lib.loop_new)(c"bentwire-pipewire".as_ptr(), std::ptr::null()) };
    if thread_loop.is_null() {
        return Err("pw_thread_loop_new failed".into());
    }
    let pw = PipeWire {
        lib,
        thread_loop,
        shared: Box::new(Shared {
            lib,
            stream: Mutex::new(std::ptr::null_mut()),
            feed: Mutex::default(),
            fault,
        }),
    };
    // The loop is not running yet: no lock needed.
    pw.connect(channels, rate)?;
    // SAFETY: a live loop.
    if unsafe { (lib.loop_start)(thread_loop) } < 0 {
        return Err("pw_thread_loop_start failed".into());
    }
    let t = Instant::now();
    loop {
        let state = pw.shared.feed.lock().unwrap().state;
        match state {
            STATE_STREAMING => return Ok(Box::new(pw)),
            STATE_ERROR => break,
            _ if t.elapsed() >= LINK_WAIT => {
                return Err(format!("no audio output linked within {LINK_WAIT:?}"));
            }
            _ => {}
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // The error callback recorded why.
    Err(pw
        .shared
        .fault
        .lock()
        .unwrap()
        .take()
        .unwrap_or_else(|| "stream error".into()))
}

impl PipeWire {
    /// Replaces the stream by one for this format. With the loop locked or not running.
    fn connect(&self, channels: u16, rate: u32) -> Result<(), String> {
        let lib = self.lib;
        self.destroy_stream();
        {
            let mut f = self.shared.feed.lock().unwrap();
            f.channels = channels;
            f.rate = rate;
        }
        let props = CString::new(format!(
            "media.type=Audio media.category=Playback application.name=bentwire \
             node.latency={}/{rate}",
            (rate / 40).max(1)
        ))
        .unwrap();
        // SAFETY: valid arguments; the events are static and the data outlives the
        // stream (it is destroyed first). The properties are taken over by the stream.
        let stream = unsafe {
            let p = (lib.properties_new_string)(props.as_ptr());
            (lib.stream_new_simple)(
                (lib.loop_get_loop)(self.thread_loop),
                c"bentwire".as_ptr(),
                p,
                &EVENTS,
                &*self.shared as *const Shared as *mut c_void,
            )
        };
        if stream.is_null() {
            return Err(format!(
                "pw_stream_new_simple: {}",
                std::io::Error::last_os_error()
            ));
        }
        *self.shared.stream.lock().unwrap() = stream;
        let pod = format_pod(channels, rate);
        let params = [pod.as_ptr()];
        // SAFETY: a new stream; the pod is copied by the call.
        let r = unsafe {
            (lib.stream_connect)(stream, DIRECTION_OUTPUT, ID_ANY, FLAGS, params.as_ptr(), 1)
        };
        if r < 0 {
            self.destroy_stream();
            return Err(format!(
                "pw_stream_connect: {}",
                std::io::Error::from_raw_os_error(-r)
            ));
        }
        Ok(())
    }

    fn destroy_stream(&self) {
        let s = std::mem::replace(
            &mut *self.shared.stream.lock().unwrap(),
            std::ptr::null_mut(),
        );
        if !s.is_null() {
            // SAFETY: a live stream, not used after this.
            unsafe { (self.lib.stream_destroy)(s) }
        }
    }
}

impl Output for PipeWire {
    fn play(&mut self, channels: u16, rate: u32, samples: Vec<f32>) {
        let lib = self.lib;
        // SAFETY: a running loop; unlocked below.
        unsafe { (lib.loop_lock)(self.thread_loop) };
        let same = {
            let f = self.shared.feed.lock().unwrap();
            (f.channels, f.rate) == (channels, rate)
        };
        let ok = if same {
            let s = *self.shared.stream.lock().unwrap();
            if !s.is_null() {
                // Drop what is queued of the previous sound; restart after a drain.
                // SAFETY: a live stream, the loop locked.
                unsafe {
                    (lib.stream_flush)(s, false);
                    (lib.stream_set_active)(s, true);
                }
            }
            !s.is_null()
        } else {
            match self.connect(channels, rate) {
                Ok(()) => true,
                Err(e) => {
                    set_fault(&self.shared.fault, e);
                    false
                }
            }
        };
        {
            let mut f = self.shared.feed.lock().unwrap();
            f.busy = ok && !samples.is_empty();
            f.sound = samples;
            f.pos = 0;
            f.draining = false;
        }
        // SAFETY: locked above.
        unsafe { (lib.loop_unlock)(self.thread_loop) };
    }

    fn done(&self) -> bool {
        !self.shared.feed.lock().unwrap().busy
    }
}

impl Drop for PipeWire {
    fn drop(&mut self) {
        // SAFETY: the loop is live; the stream goes first, then the loop's thread.
        unsafe {
            (self.lib.loop_lock)(self.thread_loop);
            self.destroy_stream();
            (self.lib.loop_unlock)(self.thread_loop);
            (self.lib.loop_stop)(self.thread_loop);
            (self.lib.loop_destroy)(self.thread_loop);
        }
    }
}

unsafe extern "C" fn on_state_changed(
    data: *mut c_void,
    old: c_int,
    state: c_int,
    error: *const c_char,
) {
    // SAFETY: the data is the stream's `Shared`.
    let sh = unsafe { &*(data as *const Shared) };
    // A stream being destroyed (its pointer already cleared) reports UNCONNECTED too.
    let current = !sh.stream.lock().unwrap().is_null();
    if current {
        sh.feed.lock().unwrap().state = state;
    }
    if current && (state == STATE_ERROR || (state == STATE_UNCONNECTED && old != STATE_UNCONNECTED))
    {
        let e = if error.is_null() {
            "stream disconnected".into()
        } else {
            // SAFETY: a NUL-terminated string for the duration of the call.
            unsafe { CStr::from_ptr(error) }
                .to_string_lossy()
                .into_owned()
        };
        set_fault(&sh.fault, e);
    }
}

unsafe extern "C" fn on_process(data: *mut c_void) {
    // SAFETY: the data is the stream's `Shared`; the callback runs on the loop's thread
    // with the loop locked, the stream alive.
    unsafe {
        let sh = &*(data as *const Shared);
        let stream = *sh.stream.lock().unwrap();
        if stream.is_null() {
            return;
        }
        let b = (sh.lib.stream_dequeue_buffer)(stream);
        if b.is_null() {
            return;
        }
        let buf = &*(*b).buffer;
        let d = &mut *buf.datas;
        if buf.n_datas == 0 || d.data.is_null() {
            (sh.lib.stream_queue_buffer)(stream, b);
            return;
        }
        let mut f = sh.feed.lock().unwrap();
        let ch = f.channels.max(1) as usize;
        let mut frames = d.maxsize as usize / (4 * ch);
        if (*b).requested != 0 {
            frames = frames.min((*b).requested as usize);
        }
        let n = (f.sound.len() - f.pos).min(frames * ch);
        std::ptr::copy_nonoverlapping(f.sound[f.pos..].as_ptr(), d.data.cast::<f32>(), n);
        f.pos += n;
        let chunk = &mut *d.chunk;
        chunk.offset = 0;
        chunk.stride = (4 * ch) as i32;
        chunk.size = (4 * n) as u32;
        let end = f.busy && !f.draining && f.pos == f.sound.len();
        f.draining |= end;
        drop(f);
        (sh.lib.stream_queue_buffer)(stream, b);
        if end {
            (sh.lib.stream_flush)(stream, true);
        }
    }
}

unsafe extern "C" fn on_drained(data: *mut c_void) {
    // SAFETY: the data is the stream's `Shared`.
    let sh = unsafe { &*(data as *const Shared) };
    let mut f = sh.feed.lock().unwrap();
    if f.draining {
        f.draining = false;
        f.busy = false;
    }
}

#[cfg(all(test, target_endian = "little"))]
mod tests {
    use super::*;

    /// The words of `spa_format_audio_raw_build(SPA_PARAM_EnumFormat, F32, ...)` as
    /// built by PipeWire 1.6.11's headers.
    #[test]
    #[rustfmt::skip]
    fn format_pod_matches_spa() {
        assert_eq!(format_pod(1, 22050), [
            0xa0, 0xf, 0x40003, 0x3, 0x1, 0x0, 0x4, 0x3, 0x1, 0x0, 0x2, 0x0, 0x4, 0x3, 0x1,
            0x0, 0x10001, 0x0, 0x4, 0x3, 0x11b, 0x0, 0x10003, 0x0, 0x4, 0x4, 0x5622, 0x0,
            0x10004, 0x0, 0x4, 0x4, 0x1, 0x0, 0x10005, 0x0, 0xc, 0xd, 0x4, 0x3, 0x2, 0x0,
        ]);
        assert_eq!(format_pod(2, 44100), [
            0xa0, 0xf, 0x40003, 0x3, 0x1, 0x0, 0x4, 0x3, 0x1, 0x0, 0x2, 0x0, 0x4, 0x3, 0x1,
            0x0, 0x10001, 0x0, 0x4, 0x3, 0x11b, 0x0, 0x10003, 0x0, 0x4, 0x4, 0xac44, 0x0,
            0x10004, 0x0, 0x4, 0x4, 0x2, 0x0, 0x10005, 0x0, 0x10, 0xd, 0x4, 0x3, 0x3, 0x4,
        ]);
        assert_eq!(format_pod(3, 8000), [
            0x80, 0xf, 0x40003, 0x3, 0x1, 0x0, 0x4, 0x3, 0x1, 0x0, 0x2, 0x0, 0x4, 0x3, 0x1,
            0x0, 0x10001, 0x0, 0x4, 0x3, 0x11b, 0x0, 0x10003, 0x0, 0x4, 0x4, 0x1f40, 0x0,
            0x10004, 0x0, 0x4, 0x4, 0x3, 0x0,
        ]);
    }
}
