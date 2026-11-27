//! Speech output through espeak-ng (spec B-TTS-6). `libespeak-ng.so.1` is loaded at run
//! time (at start, [`load`]), so it is not a package dependency; without it every utterance
//! fails and the core paces the text as before.
//!
//! espeak-ng runs on its own thread in synchronous mode: `espeak_Synth` hands the PCM and
//! the events to a callback on that thread and plays nothing. The few declarations below
//! are written from espeak-ng's documented API (`speak_lib.h`, API revision 12).

use crate::audio::Dl;
use bentwire_character::Pcm;
use bentwire_core::api::{Ms, SpeechMark, VoiceSpec};
use std::cell::RefCell;
use std::ffi::{CString, c_char, c_int, c_uint, c_void};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

/// An utterance for the engine (the core's `SpeechStart`).
pub struct Job {
    pub serial: u64,
    pub text: String,
    pub voice: VoiceSpec,
}

/// The engine's answer: the PCM and its timeline (B-TTS-2), or why there is none.
pub struct Spoken {
    pub serial: u64,
    pub pcm: Pcm,
    pub marks: Vec<(Ms, SpeechMark)>,
}

pub type Answer = Result<Spoken, u64>;

/// The speech thread. Jobs are synthesised in order; a cancelled one is cut short.
pub struct Speech {
    tx: mpsc::Sender<Job>,
    /// The serial of the utterance still wanted, 0 for none.
    wanted: Arc<AtomicU64>,
}

/// B-TTS-6: load and initialise espeak-ng. `espeak_Initialize` sets the process's LC_CTYPE
/// (glibc's setlocale is not thread-safe), so this runs on the main thread before any other
/// thread is spawned.
pub fn load() -> Result<Engine, String> {
    Engine::load(c"libespeak-ng.so.1")
}

impl Speech {
    /// `engine` is [`load`]'s result; a failure is logged with the first utterance.
    pub fn spawn(
        engine: Result<Engine, String>,
        answer: impl Fn(Answer) + Send + 'static,
    ) -> Speech {
        let (tx, rx) = mpsc::channel::<Job>();
        let wanted = Arc::new(AtomicU64::new(0));
        let w = wanted.clone();
        std::thread::Builder::new()
            .name("bentwire-speech".into())
            .spawn(move || {
                let mut logged = false;
                for job in rx {
                    if w.load(Ordering::SeqCst) != job.serial {
                        continue;
                    }
                    let r = match &engine {
                        Ok(e) => e.synth(&job, &w).map_err(|err| {
                            if w.load(Ordering::SeqCst) == job.serial {
                                log::warn!("speech: {err}");
                            }
                        }),
                        Err(e) => {
                            if !std::mem::replace(&mut logged, true) {
                                log::warn!("speech: espeak-ng not available: {e}");
                            }
                            Err(())
                        }
                    };
                    answer(r.map_err(|_| job.serial));
                }
            })
            .expect("speech thread");
        Speech { tx, wanted }
    }

    /// False when the speech thread is gone (no answer will come).
    pub fn say(&self, job: Job) -> bool {
        self.wanted.store(job.serial, Ordering::SeqCst);
        self.tx.send(job).is_ok()
    }

    /// The current utterance is no longer wanted.
    pub fn cancel(&self) {
        self.wanted.store(0, Ordering::SeqCst);
    }
}

/// `espeak_EVENT`. `id` is a union of an int, a pointer and `char[8]`.
#[repr(C)]
struct Event {
    kind: c_int,
    _unique_identifier: c_uint,
    text_position: c_int,
    _length: c_int,
    audio_position: c_int,
    _sample: c_int,
    _user_data: *mut c_void,
    id: [u8; 8],
}

/// `espeak_VOICE`.
#[repr(C)]
struct Voice {
    name: *const c_char,
    languages: *const c_char,
    identifier: *const c_char,
    gender: u8,
    age: u8,
    variant: u8,
    xx1: u8,
    score: c_int,
    spare: *mut c_void,
}

type Callback = extern "C" fn(*mut i16, c_int, *mut Event) -> c_int;

const AUDIO_OUTPUT_SYNCHRONOUS: c_int = 2;
const INITIALIZE_PHONEME_EVENTS: c_int = 0x0001;
const INITIALIZE_PHONEME_IPA: c_int = 0x0002;
const INITIALIZE_DONT_EXIT: c_int = 0x8000;
const EVENT_LIST_TERMINATED: c_int = 0;
const EVENT_WORD: c_int = 1;
const EVENT_PHONEME: c_int = 7;
const POS_CHARACTER: c_int = 1;
const CHARS_UTF8: c_uint = 1;
const RATE: c_int = 1;
/// B-TTS-6: longer texts are paced, not spoken (a few minutes of audio, held in memory).
const MAX_SPOKEN_CHARS: usize = 4000;
const PITCH: c_int = 3;
const RATE_MINIMUM: u32 = 80;

pub struct Engine {
    rate: u32,
    synth: unsafe extern "C" fn(
        *const c_void,
        usize,
        c_uint,
        c_int,
        c_uint,
        c_uint,
        *mut c_uint,
        *mut c_void,
    ) -> c_int,
    set_parameter: unsafe extern "C" fn(c_int, c_int, c_int) -> c_int,
    set_voice: unsafe extern "C" fn(*mut Voice) -> c_int,
}

/// What the callback collects during one `espeak_Synth`, on the speech thread.
struct Collect {
    samples: Vec<i16>,
    marks: Vec<(Ms, SpeechMark)>,
    serial: u64,
    wanted: Arc<AtomicU64>,
}

thread_local! {
    static COLLECT: RefCell<Option<Collect>> = const { RefCell::new(None) };
}

/// B-TTS-6: PCM, word events (1-based character positions) and IPA phoneme events (an
/// empty name is a pause). Returning 1 aborts a cancelled utterance.
extern "C" fn callback(wav: *mut i16, n: c_int, events: *mut Event) -> c_int {
    COLLECT.with(|c| {
        let mut c = c.borrow_mut();
        let Some(c) = c.as_mut() else { return 1 };
        if !wav.is_null() && n > 0 {
            // SAFETY: espeak-ng passes `n` samples at `wav`.
            c.samples
                .extend_from_slice(unsafe { std::slice::from_raw_parts(wav, n as usize) });
        }
        let mut e = events;
        // SAFETY: the list ends with an EVENT_LIST_TERMINATED entry.
        while !e.is_null() && unsafe { (*e).kind } != EVENT_LIST_TERMINATED {
            let ev = unsafe { &*e };
            let t = ev.audio_position.max(0) as Ms;
            match ev.kind {
                EVENT_WORD => {
                    let o = (ev.text_position - 1).max(0) as usize;
                    c.marks.push((t, SpeechMark::Word(o)));
                }
                EVENT_PHONEME => c.marks.push((t, SpeechMark::Phoneme(phoneme(&ev.id)))),
                _ => {}
            }
            // SAFETY: still inside the list.
            e = unsafe { e.add(1) };
        }
        (c.wanted.load(Ordering::SeqCst) != c.serial) as c_int
    })
}

/// A phoneme name: UTF-8, NUL-terminated unless it takes all 8 bytes; empty = pause.
fn phoneme(id: &[u8; 8]) -> String {
    let n = id.iter().position(|&b| b == 0).unwrap_or(8);
    match String::from_utf8_lossy(&id[..n]).into_owned() {
        s if s.is_empty() => "_".into(),
        s => s,
    }
}

/// B-TTS-3: espeak-ng's pitch (0 to 100, 50 normal) for a base pitch in Hz.
fn espeak_pitch(v: &VoiceSpec) -> c_int {
    let base = if v.gender == Some(1) { 200 } else { 100 };
    v.pitch.map_or(50, |hz| (50 * hz / base).min(100)) as c_int
}

/// B-TTS-3: SAPI 4 gender numbers (1 female, 2 male) to espeak-ng's (1 male, 2 female).
fn espeak_gender(g: Option<u32>) -> u8 {
    match g {
        Some(1) => 2,
        Some(2) => 1,
        _ => 0,
    }
}

impl Engine {
    fn load(name: &std::ffi::CStr) -> Result<Engine, String> {
        let dl = Dl::open(name)?;
        // SAFETY: the types are the C prototypes of espeak-ng's speak_lib.h.
        unsafe {
            let init: unsafe extern "C" fn(c_int, c_int, *const c_char, c_int) -> c_int =
                dl.sym(c"espeak_Initialize")?;
            let set_callback: unsafe extern "C" fn(Callback) =
                dl.sym(c"espeak_SetSynthCallback")?;
            let e = Engine {
                rate: 0,
                synth: dl.sym(c"espeak_Synth")?,
                set_parameter: dl.sym(c"espeak_SetParameter")?,
                set_voice: dl.sym(c"espeak_SetVoiceByProperties")?,
            };
            let options = INITIALIZE_PHONEME_EVENTS | INITIALIZE_PHONEME_IPA | INITIALIZE_DONT_EXIT;
            let rate = init(AUDIO_OUTPUT_SYNCHRONOUS, 0, std::ptr::null(), options);
            if rate <= 0 {
                return Err("espeak_Initialize failed (no espeak-ng-data?)".into());
            }
            set_callback(callback);
            Ok(Engine {
                rate: rate as u32,
                ..e
            })
        }
    }

    fn set_voice(&self, lang: &str, gender: u8) -> c_int {
        // espeak-ng 1.52 copies the language into a 20-byte buffer unchecked.
        if lang.len() >= 20 {
            return -1;
        }
        let l = CString::new(lang).unwrap_or_default();
        let mut v = Voice {
            name: std::ptr::null(),
            languages: l.as_ptr(),
            identifier: std::ptr::null(),
            gender,
            age: 0,
            variant: 0,
            xx1: 0,
            score: 0,
            spare: std::ptr::null_mut(),
        };
        // SAFETY: a valid voice spec whose strings outlive the call.
        unsafe { (self.set_voice)(&mut v) }
    }

    fn synth(&self, job: &Job, wanted: &Arc<AtomicU64>) -> Result<Spoken, String> {
        if !speakable(&job.text) {
            return Err(format!("text longer than {MAX_SPOKEN_CHARS} characters"));
        }
        let v = &job.voice;
        let lang = v.language.to_ascii_lowercase();
        let gender = espeak_gender(v.gender);
        // An unknown dialect falls back to its language (`de-at` to `de`).
        let primary = lang.split('-').next().unwrap_or("");
        if self.set_voice(&lang, gender) != 0 && self.set_voice(primary, gender) != 0 {
            return Err(format!("no espeak-ng voice for {:?}", v.language));
        }
        // SAFETY: plain value parameters.
        unsafe {
            (self.set_parameter)(RATE, v.words_per_minute.max(RATE_MINIMUM) as c_int, 0);
            (self.set_parameter)(PITCH, espeak_pitch(v), 0);
        }
        let text = CString::new(job.text.replace('\0', " ")).expect("no NUL");
        COLLECT.with(|c| {
            *c.borrow_mut() = Some(Collect {
                samples: Vec::new(),
                marks: Vec::new(),
                serial: job.serial,
                wanted: wanted.clone(),
            })
        });
        let len = text.as_bytes_with_nul().len();
        // SAFETY: a NUL-terminated UTF-8 text; synchronous mode returns when done.
        let r = unsafe {
            (self.synth)(
                text.as_ptr().cast(),
                len,
                0,
                POS_CHARACTER,
                0,
                CHARS_UTF8,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        let c = COLLECT.with(|c| c.borrow_mut().take()).expect("collected");
        if r != 0 {
            return Err(format!("espeak_Synth failed ({r})"));
        }
        if wanted.load(Ordering::SeqCst) != job.serial {
            return Err("cancelled".into());
        }
        let mut marks = c.marks;
        marks.push((
            c.samples.len() as Ms * 1000 / self.rate as Ms,
            SpeechMark::End,
        ));
        Ok(Spoken {
            serial: job.serial,
            pcm: Pcm {
                channels: 1,
                rate: self.rate,
                bits: 16,
                data: c.samples.iter().flat_map(|s| s.to_le_bytes()).collect(),
            },
            marks,
        })
    }
}

fn speakable(text: &str) -> bool {
    text.chars().count() <= MAX_SPOKEN_CHARS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_tts_6_long_texts_are_not_spoken() {
        assert!(speakable(&"a".repeat(MAX_SPOKEN_CHARS)));
        assert!(!speakable(&"a".repeat(MAX_SPOKEN_CHARS + 1)));
    }

    #[test]
    fn b_tts_6_missing_library_fails_every_utterance() {
        let e = Engine::load(c"libbentwire-no-such-espeak.so.1");
        assert!(e.is_err());
        let (tx, rx) = mpsc::channel();
        let s = Speech::spawn(e, move |a| tx.send(a.err()).unwrap());
        for serial in [7, 8] {
            s.say(Job {
                serial,
                text: "hi".into(),
                voice: VoiceSpec {
                    language: "en".into(),
                    gender: None,
                    pitch: None,
                    words_per_minute: 170,
                },
            });
            assert_eq!(rx.recv().unwrap(), Some(serial));
        }
    }

    #[test]
    fn b_tts_3_b_tts_6_espeak_values() {
        let v = |gender, pitch| VoiceSpec {
            language: "en-US".into(),
            gender,
            pitch,
            words_per_minute: 170,
        };
        assert_eq!(espeak_pitch(&v(None, None)), 50);
        assert_eq!(espeak_pitch(&v(Some(2), Some(50))), 25);
        assert_eq!(espeak_pitch(&v(Some(2), Some(130))), 65);
        assert_eq!(espeak_pitch(&v(Some(1), Some(152))), 38);
        assert_eq!(espeak_pitch(&v(None, Some(262))), 100);
        assert_eq!(espeak_gender(Some(1)), 2);
        assert_eq!(espeak_gender(Some(2)), 1);
        assert_eq!(espeak_gender(None), 0);
        assert_eq!(phoneme(b"\xcb\x88a\xc9\xaa\0\0\0"), "ˈaɪ");
        assert_eq!(phoneme(&[0; 8]), "_");
        assert_eq!(phoneme(b"abcdefgh"), "abcdefgh");
    }
}
