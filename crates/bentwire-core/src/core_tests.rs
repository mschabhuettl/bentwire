//! Engine tests (fake clock, scripted draws, synthetic characters).

use std::collections::HashMap;
use std::sync::Arc;

use crate::Core;
use crate::api::*;
use crate::draws::ScriptedDraws;
use crate::model::*;
use crate::speak::tests::Fixed;

pub fn fr(ms: u32) -> Frame {
    Frame {
        ms,
        has_picture: true,
        ..Default::default()
    }
}

pub fn anim(name: &str, frames: Vec<Frame>) -> Animation {
    Animation {
        name: name.into(),
        frames,
        available: true,
        ..Default::default()
    }
}

/// A complete synthetic agent character: every state has one animation `<State>A` of
/// `ms` ms (one frame), plus `extra` animations.
pub fn character(id: &str, ms: u32) -> CharacterModel {
    let mut m = CharacterModel::new(id, Family::Agent, (100, 100));
    m.info.push(InfoEntry {
        lang: "en".into(),
        name: format!("Name {id}"),
        description: format!("Desc {id}"),
    });
    for st in [
        "Showing",
        "Hiding",
        "Speaking",
        "IdlingLevel1",
        "IdlingLevel2",
        "IdlingLevel3",
        "MovingLeft",
        "MovingRight",
        "MovingUp",
        "MovingDown",
        "GesturingLeft",
        "GesturingRight",
        "GesturingUp",
        "GesturingDown",
    ] {
        let a = format!("{st}A");
        m.animations.push(anim(&a, vec![fr(ms)]));
        m.states.push((st.into(), vec![a]));
    }
    for a in ["Wave", "Explain", "Greeting", "RestPose"] {
        m.animations.push(anim(a, vec![fr(ms)]));
    }
    m
}

struct Loader(HashMap<String, Arc<CharacterModel>>);

impl CharacterLoader for Loader {
    fn load(&mut self, id: &str) -> Result<Arc<CharacterModel>, String> {
        self.0
            .get(id)
            .cloned()
            .ok_or_else(|| format!("cannot load {id}"))
    }
}

pub struct T {
    pub core: Core,
    pub now: Ms,
    pub log: Vec<(Ms, Output)>,
    token: u64,
}

pub fn output(w: i32, h: i32) -> OutputInfo {
    OutputInfo {
        id: 1,
        rect: Rect::new(0, 0, w, h),
        scale: 1.0,
    }
}

impl T {
    pub fn with(chars: Vec<CharacterModel>, state: Option<StateFile>, draws: Vec<u32>) -> T {
        Self::with_config(chars, state, draws, Config::default())
    }

    pub fn with_config(
        chars: Vec<CharacterModel>,
        state: Option<StateFile>,
        draws: Vec<u32>,
        config: Config,
    ) -> T {
        let catalog = chars
            .iter()
            .map(|c| CatalogEntry {
                id: c.id.clone(),
                info: c.info.clone(),
            })
            .collect();
        let map = chars
            .into_iter()
            .map(|c| (c.id.clone(), Arc::new(c)))
            .collect();
        let core = Core::new(CoreInit {
            config,
            locale: LocaleEnv::default(),
            state,
            catalog,
            loader: Box::new(Loader(map)),
            metrics: Box::new(Fixed { cw: 7, lh: 13 }),
            draws: Box::new(ScriptedDraws::new(draws)),
            outputs: vec![output(1920, 1080)],
            local_hour: 12,
        });
        let mut t = T {
            core,
            now: 0,
            log: Vec::new(),
            token: 0,
        };
        let o = t.core.start(0);
        t.record(o);
        t
    }

    /// Started, visible character at (500, 500) with Show done, log cleared.
    pub fn visible(m: CharacterModel) -> T {
        let st = StateFile {
            character: m.id.clone(),
            left: 500,
            top: 500,
            visible: true,
        };
        let mut t = T::with(vec![m], Some(st), vec![]);
        t.run_until_idle_armed();
        t.log.clear();
        t
    }

    fn record(&mut self, o: Vec<Output>) {
        for x in o {
            self.log.push((self.now, x));
        }
    }

    /// Fake clock: visit each deadline in turn (recording outputs at their time).
    pub fn advance(&mut self, to: Ms) {
        while let Some(d) = self.core.next_deadline() {
            if d > to {
                break;
            }
            let d = d.max(self.now);
            let o = self.core.tick(d);
            self.now = d;
            self.record(o);
        }
        let o = self.core.tick(to);
        self.now = to;
        self.record(o);
    }

    /// Advance until the queue is idle (idle timer armed) or `limit`.
    pub fn run_until_idle_armed(&mut self) {
        // Show at start takes one animation; advance past it without firing idle.
        let target = self.now + 1000;
        while let Some(d) = self.core.next_deadline() {
            if d > target
                || self
                    .log
                    .iter()
                    .any(|(_, o)| is_sig(o, |s| matches!(s, Signal::IdleStart)))
            {
                break;
            }
            if d >= self.now + 4000 {
                break;
            }
            self.advance(d);
        }
    }

    pub fn ev(&mut self, e: Event) {
        let o = self.core.handle(self.now, e);
        self.record(o);
    }

    pub fn call_as(&mut self, sender: &str, c: Call) -> Result<ReplyValue, MethodError> {
        self.token += 1;
        let token = self.token;
        let start = self.log.len();
        self.ev(Event::Call {
            token,
            sender: sender.into(),
            call: c,
        });
        self.log[start..]
            .iter()
            .find_map(|(_, o)| match o {
                Output::Reply { token: t, result } if *t == token => Some(result.clone()),
                _ => None,
            })
            .expect("reply")
    }

    pub fn call(&mut self, c: Call) -> Result<ReplyValue, MethodError> {
        self.call_as(":1.1", c)
    }

    pub fn req(&mut self, c: Call) -> u32 {
        match self.call(c) {
            Ok(ReplyValue::RequestId(id)) => id,
            r => panic!("expected request id, got {r:?}"),
        }
    }

    pub fn signals(&self) -> Vec<(Ms, Signal)> {
        self.log
            .iter()
            .filter_map(|(t, o)| match o {
                Output::Signal { signal, .. } => Some((*t, signal.clone())),
                _ => None,
            })
            .collect()
    }

    pub fn complete_of(&self, id: u32) -> Option<(Ms, u32, String)> {
        self.signals().into_iter().find_map(|(t, s)| match s {
            Signal::RequestComplete {
                id: i,
                status,
                reason,
            } if i == id => Some((t, status, reason)),
            _ => None,
        })
    }

    pub fn count(&self, f: impl Fn(&Signal) -> bool) -> usize {
        self.signals().iter().filter(|(_, s)| f(s)).count()
    }

    pub fn press(&mut self, kind: PointerKind, surface: Surface, button: u32, x: i32, y: i32) {
        self.ev(Event::Pointer(PointerEvent {
            kind,
            surface,
            button,
            modifiers: 0,
            x,
            y,
        }));
    }

    pub fn pos(&self) -> (i32, i32) {
        let p = self.core.properties();
        (p.left, p.top)
    }
}

fn is_sig(o: &Output, f: impl Fn(&Signal) -> bool) -> bool {
    matches!(o, Output::Signal { signal, .. } if f(signal))
}

// ---------- startup, show, hide ----------

#[test]
fn b_cfg_3_first_run_and_b_char_4() {
    let t = T::with(
        vec![character("zeta", 100), character("clippit", 100)],
        None,
        vec![],
    );
    let p = t.core.properties();
    assert_eq!(p.character, "clippit");
    // Right edge 48 px and bottom edge 96 px inside the first output.
    assert_eq!((p.left, p.top), (1920 - 48 - 100, 1080 - 96 - 100));
    // Visible: an animated Show with cause 4.
    assert!(t.signals().contains(&(0, Signal::Shown { cause: 4 })));
    assert!(p.visible);
}

#[test]
fn b_char_4_state_then_config_then_sorted() {
    let st = StateFile {
        character: "beta".into(),
        left: 1,
        top: 2,
        visible: false,
    };
    let t = T::with(
        vec![character("alpha", 1), character("beta", 1)],
        Some(st),
        vec![],
    );
    let p = t.core.properties();
    assert_eq!(
        (p.character.as_str(), p.left, p.top, p.visible),
        ("beta", 1, 2, false)
    );
    let cfg = Config {
        character: Some("beta".into()),
        ..Config::default()
    };
    let t = T::with_config(
        vec![character("alpha", 1), character("beta", 1)],
        None,
        vec![],
        cfg,
    );
    assert_eq!(t.core.properties().character, "beta");
    let t = T::with(vec![character("b", 1), character("a", 1)], None, vec![]);
    assert_eq!(t.core.properties().character, "a");
    // None loads: no character, methods fail with NoCharacter.
    let mut t = T::with(vec![], None, vec![]);
    let e = t.call(Call::Play("Wave".into())).unwrap_err();
    assert_eq!(e.name, errors::NO_CHARACTER);
    assert_eq!(t.core.scene().tray.icon_character, None);
}

#[test]
fn r_show_1_and_r_show_2() {
    let mut t = T::visible(character("c", 200));
    let id = t.req(Call::Show { fast: false });
    // Already visible: completes, no Shown signal.
    assert_eq!(t.complete_of(id).unwrap().1, 0);
    assert_eq!(t.count(|s| matches!(s, Signal::Shown { .. })), 0);
    let id = t.req(Call::Hide { fast: false });
    // Hiding plays first (200 ms), then unmap and Hidden(3).
    assert!(t.core.scene().character.unwrap().mapped);
    t.advance(t.now + 200);
    assert_eq!(t.complete_of(id).unwrap().0, t.now);
    assert!(!t.core.scene().character.unwrap().mapped);
    assert!(
        t.signals()
            .iter()
            .any(|(_, s)| *s == Signal::Hidden { cause: 3 })
    );
    assert_eq!(t.core.properties().visibility_cause, 3);
    let id = t.req(Call::Hide { fast: true });
    assert_eq!(t.complete_of(id).unwrap().1, 0);
    assert_eq!(t.count(|s| matches!(s, Signal::Hidden { .. })), 1);
    let now = t.now;
    let id = t.req(Call::Show { fast: false });
    assert!(t.signals().contains(&(now, Signal::Shown { cause: 4 })));
    t.advance(now + 200);
    assert_eq!(t.complete_of(id).unwrap().0, now + 200);
}

#[test]
fn b_show_3_fast_show_picture() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    match t.core.scene().character.unwrap().picture {
        Picture::Frame {
            animation, frame, ..
        } => assert_eq!((animation.as_str(), frame), ("RestPose", 0)),
        p => panic!("{p:?}"),
    }
    // Without RestPose: last frame of the first Showing member.
    let mut m = character("d", 100);
    m.animations.retain(|a| a.name != "RestPose");
    m.animations[0].frames.push(fr(100));
    let mut t = T::visible(m);
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    match t.core.scene().character.unwrap().picture {
        Picture::Frame {
            animation, frame, ..
        } => assert_eq!((animation.as_str(), frame), ("ShowingA", 1)),
        p => panic!("{p:?}"),
    }
    // office97: the rest pose.
    let mut o = CharacterModel::new("o", Family::Office97, (50, 50));
    o.office97.as_mut().unwrap().rest_pose = Some(7);
    let mut t = T::visible(o);
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    assert_eq!(t.core.scene().character.unwrap().picture, Picture::Pose(7));
    // Nothing: empty.
    let mut t = T::visible(CharacterModel::new("e", Family::Agent, (5, 5)));
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    assert_eq!(t.core.scene().character.unwrap().picture, Picture::Empty);
}

#[test]
fn r_anim_9_hidden_play_fails() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Hide { fast: true });
    let id = t.req(Call::Play("Wave".into()));
    let (_, st, reason) = t.complete_of(id).unwrap();
    assert_eq!((st, reason.as_str()), (1, errors::HIDDEN));
    // GestureAt runs invisibly and completes with Hidden.
    let id = t.req(Call::GestureAt { x: 0, y: 550 });
    assert!(t.complete_of(id).is_none());
    t.advance(t.now + 100);
    let (_, st, reason) = t.complete_of(id).unwrap();
    assert_eq!((st, reason.as_str()), (1, errors::HIDDEN));
}

// ---------- queue ----------

#[test]
fn r_q_1_b_q_1_ids_and_reply_before_signals() {
    let mut t = T::visible(character("c", 100));
    let start = t.log.len();
    let a = t.req(Call::Play("Wave".into()));
    let b = t.req(Call::Play("Wave".into()));
    assert!(a >= 1 && b == a + 1);
    // The reply precedes RequestStart of the same request.
    let first_reply = t.log[start..]
        .iter()
        .position(|(_, o)| matches!(o, Output::Reply { .. }))
        .unwrap();
    let first_start = t.log[start..]
        .iter()
        .position(|(_, o)| is_sig(o, |s| matches!(s, Signal::RequestStart { .. })))
        .unwrap();
    assert!(first_reply < first_start);
    t.advance(t.now + 1000);
    let ca = t.complete_of(a).unwrap();
    let cb = t.complete_of(b).unwrap();
    assert!(ca.0 < cb.0, "strict FIFO");
}

#[test]
fn b_q_2_r_q_5_unicast_and_events() {
    let mut t = T::visible(character("c", 100));
    let id = t.req(Call::Play("Wave".into()));
    t.advance(t.now + 100);
    let targeted: Vec<_> = t
        .log
        .iter()
        .filter_map(|(_, o)| match o {
            Output::Signal { target, signal } => Some((target.clone(), signal.clone())),
            _ => None,
        })
        .filter(|(_, s)| {
            matches!(
                s,
                Signal::RequestStart { .. } | Signal::RequestComplete { .. }
            )
        })
        .collect();
    assert_eq!(
        targeted,
        vec![
            (Some(":1.1".into()), Signal::RequestStart { id }),
            (
                Some(":1.1".into()),
                Signal::RequestComplete {
                    id,
                    status: 0,
                    reason: String::new()
                }
            ),
        ]
    );
    // Removed before start: only RequestComplete with status 3.
    let a = t.req(Call::Play("Wave".into()));
    let b = t.req(Call::Play("Wave".into()));
    t.call(Call::Stop(b)).unwrap();
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: b }), 0);
    assert_eq!(t.complete_of(b).unwrap().1, 3);
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: a }), 1);
}

fn looping(m: &mut CharacterModel) {
    let mut f1 = fr(100);
    f1.branches = vec![Branch::Prob { target: 0, p: 100 }];
    let mut f0 = fr(100);
    f0.exit = Exit::To(2);
    f1.exit = Exit::To(2);
    m.animations.push(anim("Loop", vec![f0, f1, fr(50)]));
}

#[test]
fn r_q_6_loop_blocks_until_stop() {
    let mut m = character("c", 100);
    looping(&mut m);
    let mut t = T::visible(m);
    let a = t.req(Call::Play("Loop".into()));
    let b = t.req(Call::Play("Wave".into()));
    t.advance(t.now + 5000);
    assert!(t.complete_of(a).is_none());
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: b }), 0);
    t.call(Call::Stop(a)).unwrap();
    assert_eq!(t.complete_of(a).unwrap().1, 3);
    // The next request runs after the loop's exit (frame 2, 50 ms).
    let now = t.now;
    t.advance(now + 1000);
    assert_eq!(t.complete_of(b).unwrap().0, now + 50 + 100);
}

#[test]
fn r_q_4_b_q_4_stop_zero() {
    let mut m = character("c", 100);
    looping(&mut m);
    let mut t = T::visible(m);
    let a = t.req(Call::Play("Loop".into()));
    let b = t.req(Call::Play("Wave".into()));
    t.call(Call::Stop(0)).unwrap();
    assert_eq!(t.complete_of(a).unwrap().2, errors::INTERRUPTED);
    assert_eq!(t.complete_of(b).unwrap().1, 3);
    // Running Show/Hide continue, queued successors removed.
    let h = t.req(Call::Hide { fast: false });
    let p = t.req(Call::Play("Wave".into()));
    t.call(Call::Stop(0)).unwrap();
    assert!(t.complete_of(h).is_none());
    assert_eq!(t.complete_of(p).unwrap().1, 3);
    // The loop's exit (50 ms) runs before Hiding (100 ms).
    t.advance(t.now + 150);
    assert_eq!(t.complete_of(h).unwrap().1, 0);
    // Unknown id does nothing.
    t.call(Call::Stop(9999)).unwrap();
}

#[test]
fn b_q_5_stop_all() {
    let mut t = T::visible(character("c", 100));
    let e = t.call(Call::StopAll(vec!["bogus".into()])).unwrap_err();
    assert_eq!(e.name, errors::INVALID_ARGS);
    let h = t.req(Call::Hide { fast: false });
    let p = t.req(Call::Play("Wave".into()));
    let m = t.req(Call::MoveTo {
        x: 0,
        y: 0,
        speed: 0,
    });
    t.call(Call::StopAll(vec!["move".into()])).unwrap();
    assert_eq!(t.complete_of(m).unwrap().1, 3);
    assert!(t.complete_of(p).is_none() && t.complete_of(h).is_none());
    t.call(Call::StopAll(vec!["play".into()])).unwrap();
    assert_eq!(t.complete_of(p).unwrap().1, 3);
    assert!(t.complete_of(h).is_none());
    // Empty list stops everything, including a running Hide.
    t.call(Call::StopAll(vec![])).unwrap();
    assert_eq!(t.complete_of(h).unwrap().1, 3);
}

#[test]
fn b_q_3_hidden_moveto_r_move_4() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Hide { fast: true });
    let id = t.req(Call::MoveTo {
        x: 10,
        y: 20,
        speed: 1000,
    });
    // Phase 1 fails, the frame still moves instantly, the request fails.
    assert_eq!(t.pos(), (10, 20));
    let (_, st, reason) = t.complete_of(id).unwrap();
    assert_eq!((st, reason.as_str()), (1, errors::HIDDEN));
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::Move {
            x: 10,
            y: 20,
            cause: 2
        }));
}

// ---------- moving ----------

#[test]
fn r_move_1_r_move_2_r_move_3_phases() {
    let mut t = T::visible(character("c", 100));
    let now = t.now;
    // From (500,500) to (650,500): the target is screen-right of the frame, so
    // MovingLeft (100 ms); slide 25 px per 40 ms (6 ticks); then Speaking (100 ms)
    // and Move(..., 2).
    let id = t.req(Call::MoveTo {
        x: 650,
        y: 500,
        speed: 1000,
    });
    match t.core.scene().character.unwrap().picture {
        Picture::Frame { animation, .. } => assert_eq!(animation, "MovingLeftA"),
        p => panic!("{p:?}"),
    }
    t.advance(now + 100 + 40);
    assert_eq!(t.pos(), (525, 500));
    t.advance(now + 100 + 240);
    assert_eq!(t.pos(), (650, 500));
    assert!(t.complete_of(id).is_none());
    t.advance(now + 440);
    assert_eq!(t.complete_of(id).unwrap(), (now + 440, 0, String::new()));
    let moves: Vec<_> = t
        .signals()
        .into_iter()
        .filter(|(_, s)| matches!(s, Signal::Move { .. }))
        .collect();
    assert_eq!(
        moves,
        vec![(
            now + 440,
            Signal::Move {
                x: 650,
                y: 500,
                cause: 2
            }
        )]
    );
    assert_eq!(t.core.properties().move_cause, 2);
    // Speed 0: placed at once, the slide emits Move itself (once).
    let id = t.req(Call::MoveTo {
        x: 0,
        y: 0,
        speed: 0,
    });
    t.advance(t.now + 1000);
    assert_eq!(t.complete_of(id).unwrap().1, 0);
    assert_eq!(
        t.count(|s| *s
            == Signal::Move {
                x: 0,
                y: 0,
                cause: 2
            }),
        1
    );
}

#[test]
fn r_move_2_drag_cancels_slide() {
    let mut t = T::visible(character("c", 0));
    let id = t.req(Call::MoveTo {
        x: 1500,
        y: 500,
        speed: 1000,
    });
    t.advance(t.now + 80);
    let (x, y) = t.pos();
    assert!(x > 500 && x < 1500);
    t.press(PointerKind::Press, Surface::Character, 1, x + 5, y + 5);
    t.press(PointerKind::Motion, Surface::Character, 1, x + 15, y + 5);
    t.advance(t.now + 2000);
    let (_, st, reason) = t.complete_of(id).unwrap();
    assert_eq!((st, reason.as_str()), (1, errors::DRAGGED));
}

#[test]
fn r_gest_1_via_core() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::GestureAt { x: 0, y: 550 });
    match t.core.scene().character.unwrap().picture {
        Picture::Frame { animation, .. } => assert_eq!(animation, "GesturingRightA"),
        p => panic!("{p:?}"),
    }
}

// ---------- idle ----------

#[test]
fn b_idle_t_r_idle_1_r_idle_2_r_idle_3_vector() {
    let st = StateFile {
        character: "c".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut m = character("c", 1000);
    // Fast Show so the queue is empty at t = 0.
    m.states.retain(|s| s.0 != "Showing");
    let mut t = T::with(vec![m], Some(st), vec![]);
    t.advance(30_000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    // Picks at 4000, 9000, 14000 ...: observe the picture changes.
    let mut t2 = T::with(
        vec![character_idle()],
        Some(StateFile {
            character: "i".into(),
            left: 0,
            top: 0,
            visible: true,
        }),
        vec![],
    );
    let mut levels = Vec::new();
    let mut last = None;
    let mut tm = 0;
    while tm < 120_000 {
        tm += 500;
        t2.advance(tm);
        if let Some(c) = t2.core.scene().character
            && let Picture::Frame { animation, .. } = c.picture
            && animation.starts_with("Idle")
            && Some(tm) != last
        {
            if levels.last().map(|(_, a): &(Ms, String)| a) != Some(&animation) || tm % 5000 == 4000
            {
                levels.push((tm, animation.clone()));
            }
            last = Some(tm);
        }
    }
    let starts: Vec<(Ms, String)> = levels
        .into_iter()
        .filter(|(t, _)| t % 5000 == 4000)
        .collect();
    assert_eq!(starts[0], (4000, "Idle1".to_string()));
    assert_eq!(starts[1].0, 9000);
    assert_eq!(starts[9], (49_000, "Idle1".to_string()));
    assert_eq!(starts[10], (54_000, "Idle2".to_string()));
    assert_eq!(starts[19], (99_000, "Idle2".to_string()));
    assert_eq!(starts[20], (104_000, "Idle3".to_string()));
}

/// Idle levels with distinct names, 1000 ms each, no Showing.
fn character_idle() -> CharacterModel {
    let mut m = CharacterModel::new("i", Family::Agent, (10, 10));
    for (st, a) in [
        ("IdlingLevel1", "Idle1"),
        ("IdlingLevel2", "Idle2"),
        ("IdlingLevel3", "Idle3"),
    ] {
        m.animations.push(anim(a, vec![fr(1000)]));
        m.states.push((st.into(), vec![a.into()]));
    }
    m.animations.push(anim("Wave", vec![fr(100)]));
    m
}

#[test]
fn b_idle_t_r_idle_4_play_interrupts_idle() {
    let st = StateFile {
        character: "i".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut t = T::with(vec![character_idle()], Some(st), vec![]);
    t.advance(9500);
    assert_eq!(
        t.signals()
            .iter()
            .filter(|(_, s)| *s == Signal::IdleStart)
            .count(),
        1
    );
    let id = t.req(Call::Play("Wave".into()));
    // Stops the idle animation and emits IdleComplete.
    assert!(t.signals().contains(&(9500, Signal::IdleComplete)));
    t.advance(9600);
    assert_eq!(t.complete_of(id).unwrap().0, 9600);
    // Next pick is level 1 at completion + 4000.
    t.advance(13_599);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    t.advance(13_600);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 2);
    match t.core.scene().character.unwrap().picture {
        Picture::Frame { animation, .. } => assert_eq!(animation, "Idle1"),
        p => panic!("{p:?}"),
    }
}

#[test]
fn r_idle_4_idle_on_false_and_r_idle_1_conditions() {
    let st = StateFile {
        character: "i".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut t = T::with(vec![character_idle()], Some(st), vec![]);
    t.advance(4500);
    t.call(Call::SetIdleOn(false)).unwrap();
    assert!(t.signals().contains(&(4500, Signal::IdleComplete)));
    t.advance(20_000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    t.call(Call::SetIdleOn(true)).unwrap();
    t.advance(24_000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 2);
    // Hidden: no idle.
    t.req(Call::Hide { fast: true });
    let n = t.count(|s| *s == Signal::IdleStart);
    t.advance(60_000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), n);
}

#[test]
fn r_idle_5_missing_level() {
    let mut m = CharacterModel::new("n", Family::Agent, (10, 10));
    m.animations.push(anim("Idle2", vec![fr(1000)]));
    m.states.push(("IdlingLevel2".into(), vec!["Idle2".into()]));
    let st = StateFile {
        character: "n".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut t = T::with(vec![m], Some(st), vec![]);
    // Level 1 missing at 4000: n becomes 1, re-armed (retry at 8000); n stays 1.
    t.advance(4000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    t.advance(50_000);
    match t.core.scene().character.unwrap().picture {
        Picture::Empty => {}
        p => panic!("level 2 never reached: {p:?}"),
    }
}

#[test]
fn b_idle_o_office97() {
    let mut m = CharacterModel::new("o", Family::Office97, (10, 10));
    m.animations.push(anim(
        "Idle.1",
        vec![Frame {
            ms: 300,
            has_picture: true,
            pose: Some(1),
            ..Default::default()
        }],
    ));
    m.animations.push(anim(
        "Idle.2",
        vec![Frame {
            ms: 0,
            has_picture: true,
            pose: Some(2),
            ..Default::default()
        }],
    ));
    m.states
        .push(("Idle".into(), vec!["Idle.1".into(), "Idle.2".into()]));
    m.office97.as_mut().unwrap().idle = "Idle".into();
    let st = StateFile {
        character: "o".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    // Draws: variant 0 (300 ms), then 1 (0 ms) repeatedly.
    let mut t = T::with(vec![m], Some(st), vec![0, 1, 1, 1, 1]);
    t.advance(0);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    assert_eq!(
        t.core.scene().character.unwrap().picture,
        Picture::Frame {
            animation: "Idle.1".into(),
            frame: 0,
            mouth: None
        }
    );
    t.advance(300);
    // Replayed at once (variant 2: 0 ms frames); it cannot spin: >= 100 ms apart.
    t.advance(1000);
    assert!(t.core.next_deadline().unwrap() > 1000);
    assert_eq!(t.count(|s| *s == Signal::IdleStart), 1);
    // Missing idle state: 4000 ms retry.
    let mut m = CharacterModel::new("p", Family::Office97, (10, 10));
    m.office97.as_mut().unwrap().idle = "Idle".into();
    let st = StateFile {
        character: "p".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut t = T::with(vec![m], Some(st), vec![]);
    t.advance(0);
    assert_eq!(t.core.next_deadline(), Some(4000));
}

// ---------- speak ----------

#[test]
fn r_bal_8_r_bal_7_b_speak_2_r_speak_4_pacing_vector() {
    let mut m = character("c", 0);
    m.states.retain(|s| s.0 != "Speaking");
    let mut t = T::visible(m);
    let now = t.now;
    let id = t.req(Call::Speak("Hello big world".into()));
    assert!(t.signals().contains(&(now, Signal::BalloonShown)));
    assert_eq!(
        t.core.scene().agent_balloon.unwrap().lines,
        vec!["Hello".to_string()]
    );
    t.advance(now + 352);
    assert_eq!(
        t.core.scene().agent_balloon.unwrap().lines,
        vec!["Hello big".to_string()]
    );
    t.advance(now + 704);
    assert_eq!(
        t.core.scene().agent_balloon.unwrap().lines,
        vec!["Hello big world".to_string()]
    );
    t.advance(now + 1055);
    assert!(t.complete_of(id).is_none());
    t.advance(now + 1056);
    assert_eq!(t.complete_of(id).unwrap(), (now + 1056, 0, String::new()));
    t.advance(now + 2555);
    assert!(t.core.scene().agent_balloon.is_some());
    t.advance(now + 2556);
    assert!(t.core.scene().agent_balloon.is_none());
    assert!(t.signals().contains(&(now + 2556, Signal::BalloonHidden)));
}

#[test]
fn r_speak_1_speakable_then_output_and_bookmarks() {
    let mut t = T::visible(character("c", 100));
    let now = t.now;
    let id = t
        .call_as(":1.9", Call::Speak("\\Mrk=7\\a b \\Mrk=8\\".into()))
        .unwrap();
    let ReplyValue::RequestId(id) = id else {
        panic!()
    };
    // Phase 1: Speaking (100 ms) before the balloon.
    assert!(t.core.scene().agent_balloon.is_none());
    t.advance(now + 100);
    assert!(t.core.scene().agent_balloon.is_some());
    let marks: Vec<_> = t
        .log
        .iter()
        .filter_map(|(tm, o)| match o {
            Output::Signal {
                target,
                signal: Signal::Bookmark { n },
            } => Some((*tm, target.clone(), *n)),
            _ => None,
        })
        .collect();
    assert_eq!(marks, vec![(now + 100, Some(":1.9".into()), 7)]);
    t.advance(now + 100 + 704);
    assert!(t.log.iter().any(|(tm, o)| *tm == now + 804
        && matches!(
            o,
            Output::Signal {
                signal: Signal::Bookmark { n: 8 },
                ..
            }
        )));
    assert_eq!(t.complete_of(id).unwrap().1, 0);
}

#[test]
fn r_speak_2_empty_and_hidden() {
    let mut t = T::visible(character("c", 100));
    let id = t.req(Call::Speak(String::new()));
    assert_eq!(t.complete_of(id).unwrap().1, 0);
    t.req(Call::Hide { fast: true });
    let id = t.req(Call::Speak("hi".into()));
    t.advance(t.now + 500);
    let (_, st, r) = t.complete_of(id).unwrap();
    assert_eq!((st, r.as_str()), (1, errors::HIDDEN));
    let e = t.call(Call::Speak("\\Mrk=0\\".into())).unwrap_err();
    assert_eq!(e.name, errors::INVALID_ARGS);
}

#[test]
fn r_speak_3_alternative_draw() {
    let mut m = character("c", 0);
    m.states.retain(|s| s.0 != "Speaking");
    let st = StateFile {
        character: "c".into(),
        left: 500,
        top: 500,
        visible: true,
    };
    let mut t = T::with(vec![m], Some(st), vec![0; 64]);
    t.run_until_idle_armed();
    t.core.set_draws(Box::new(ScriptedDraws::new([2])));
    t.req(Call::Speak("one|two|three".into()));
    assert_eq!(
        t.core.scene().agent_balloon.unwrap().lines,
        vec!["three".to_string()]
    );
}

#[test]
fn b_bal_1_no_balloon() {
    let mut m = character("c", 0);
    m.balloon.enabled = false;
    let mut t = T::visible(m);
    let now = t.now;
    let id = t.req(Call::Speak("a b".into()));
    t.advance(now + 2000);
    assert_eq!(t.count(|s| *s == Signal::BalloonShown), 0);
    assert_eq!(t.complete_of(id).unwrap().0, now + 704);
    assert!(!t.core.properties().balloon_enabled);
}

#[test]
fn r_bal_10_no_auto_hide_and_click() {
    let mut m = character("c", 0);
    m.balloon.auto_hide = false;
    let mut t = T::visible(m);
    t.req(Call::Speak("a".into()));
    t.advance(t.now + 10_000);
    assert!(t.core.scene().agent_balloon.is_some());
    // A click on the (no longer speaking) character hides it.
    t.press(PointerKind::Press, Surface::Character, 1, 510, 510);
    t.press(PointerKind::Release, Surface::Character, 1, 510, 510);
    assert!(t.core.scene().agent_balloon.is_none());
    // A MoveTo that moves hides it at once.
    t.req(Call::Speak("a".into()));
    t.advance(t.now + 10_000);
    t.req(Call::MoveTo {
        x: 0,
        y: 0,
        speed: 0,
    });
    assert!(t.core.scene().agent_balloon.is_none());
}

#[test]
fn r_bal_9_scroll() {
    let mut m = character("c", 0);
    m.states.retain(|s| s.0 != "Speaking");
    m.balloon.chars_per_line = 4; // 28 px wide, 8 px of text: one char per line.
    m.balloon.lines = 2;
    let mut t = T::visible(m);
    let now = t.now;
    t.req(Call::Speak("a b c".into()));
    t.advance(now + 704);
    let b = t.core.scene().agent_balloon.unwrap();
    assert_eq!(b.lines, vec!["b".to_string(), "c".to_string()]);
    assert!(b.scroll_px > 0);
    t.advance(now + 704 + 352);
    assert_eq!(t.core.scene().agent_balloon.unwrap().scroll_px, 0);
}

// ---------- ask ----------

fn ask(mode: &str) -> AskSpec {
    AskSpec {
        text: Some("Question?".into()),
        buttons: Some(AskButtons::Set("yes-no".into())),
        checkboxes: Some(vec!["&remember".into()]),
        mode: Some(mode.into()),
        animation: Some(String::new()),
        ..Default::default()
    }
}

fn ask_complete(t: &T, id: u32) -> Option<(i32, Vec<bool>)> {
    t.signals().into_iter().find_map(|(_, s)| match s {
        Signal::AskComplete {
            id: i,
            result,
            checked,
        } if i == id => Some((result, checked)),
        _ => None,
    })
}

#[test]
fn b_ask_4_b_wl_8_phases_and_answer() {
    let mut t = T::visible(character("c", 100));
    let now = t.now;
    let mut spec = ask("modal");
    spec.animation = None; // default Explain, 100 ms
    let id = t.req(Call::Ask(spec));
    assert!(t.core.scene().office_balloon.is_none());
    t.advance(now + 100);
    let s = t.core.scene();
    assert!(s.office_balloon.is_some());
    assert!(s.character.unwrap().keyboard_on_demand);
    // The queue continues: a Play runs while the ask is open.
    let p = t.req(Call::Play("Wave".into()));
    t.advance(now + 200);
    assert_eq!(t.complete_of(p).unwrap().1, 0);
    // Check the box with its mnemonic, answer with the No button.
    t.ev(Event::Key {
        key: Key::Char('r'),
        modifiers: 0,
    });
    t.ev(Event::Key {
        key: Key::Char('n'),
        modifiers: 4,
    });
    assert_eq!(ask_complete(&t, id), Some((-4, vec![true])));
    let sig = t.signals();
    let ia = sig
        .iter()
        .position(|(_, s)| matches!(s, Signal::AskComplete { .. }))
        .unwrap();
    let ic = sig
        .iter()
        .position(|(_, s)| {
            *s == Signal::RequestComplete {
                id,
                status: 0,
                reason: String::new(),
            }
        })
        .unwrap();
    assert!(ia < ic);
    assert!(t.core.scene().office_balloon.is_none());
}

#[test]
fn b_ask_4_hidden_and_too_big() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Hide { fast: true });
    let id = t.req(Call::Ask(ask("modal")));
    assert_eq!(t.complete_of(id).unwrap().2, errors::HIDDEN);
    t.req(Call::Show { fast: true });
    t.ev(Event::Outputs(vec![output(60, 60)]));
    let id = t.req(Call::Ask(ask("modal")));
    assert_eq!(t.complete_of(id).unwrap().2, errors::BALLOON_TOO_BIG);
}

#[test]
fn b_ask_2_keyboard_and_clicks() {
    let mut t = T::visible(character("c", 100));
    let mut spec = ask("modal");
    spec.buttons = Some(AskButtons::Set("ok-cancel".into()));
    let id = t.req(Call::Ask(spec));
    // Esc activates Cancel.
    t.ev(Event::Key {
        key: Key::Escape,
        modifiers: 0,
    });
    assert_eq!(ask_complete(&t, id), Some((-2, vec![false])));
    // Enter activates the focused control (first button); Tab cycles; Space toggles.
    let id = t.req(Call::Ask(ask("modal")));
    let s = t.core.scene().office_balloon.unwrap();
    assert!(s.buttons[0].focused);
    t.ev(Event::Key {
        key: Key::Tab,
        modifiers: 0,
    }); // -> No
    t.ev(Event::Key {
        key: Key::Tab,
        modifiers: 0,
    }); // -> checkbox
    t.ev(Event::Key {
        key: Key::Space,
        modifiers: 0,
    });
    t.ev(Event::Key {
        key: Key::Tab,
        modifiers: 1,
    }); // back -> No
    t.ev(Event::Key {
        key: Key::Enter,
        modifiers: 0,
    });
    assert_eq!(ask_complete(&t, id), Some((-4, vec![true])));
    // Modal without Cancel/Close: Esc does nothing.
    let id = t.req(Call::Ask(ask("modal")));
    t.ev(Event::Key {
        key: Key::Escape,
        modifiers: 0,
    });
    assert_eq!(ask_complete(&t, id), None);
    // Click on Yes.
    let b = t.core.scene().office_balloon.unwrap().buttons[0].rect;
    t.press(
        PointerKind::Press,
        Surface::OfficeBalloon,
        1,
        b.x + 2,
        b.y + 2,
    );
    assert_eq!(
        t.core.scene().office_balloon.unwrap().buttons[0].state,
        ControlState::Pressed
    );
    t.press(
        PointerKind::Release,
        Surface::OfficeBalloon,
        1,
        b.x + 2,
        b.y + 2,
    );
    assert_eq!(ask_complete(&t, id), Some((-3, vec![false])));
    // A clicked label returns its 1-based index.
    let id = t.req(Call::Ask(AskSpec {
        labels: Some(vec!["one".into(), "two".into()]),
        animation: Some(String::new()),
        ..Default::default()
    }));
    let l = t.core.scene().office_balloon.unwrap().labels[1].rect;
    t.press(
        PointerKind::Press,
        Surface::OfficeBalloon,
        1,
        l.x + 2,
        l.y + 2,
    );
    t.press(
        PointerKind::Release,
        Surface::OfficeBalloon,
        1,
        l.x + 2,
        l.y + 2,
    );
    assert_eq!(ask_complete(&t, id), Some((2, vec![])));
}

#[test]
fn b_ask_6_autodown() {
    let mut t = T::visible(character("c", 100));
    let id = t.req(Call::Ask(ask("autodown")));
    // Click on the character dismisses with result 0, status 0.
    t.press(PointerKind::Press, Surface::Character, 1, 510, 510);
    assert_eq!(ask_complete(&t, id), Some((0, vec![false])));
    assert_eq!(t.complete_of(id).unwrap().1, 0);
    t.press(PointerKind::Release, Surface::Character, 1, 510, 510);
    // Esc dismisses (no Cancel in yes-no).
    let id = t.req(Call::Ask(ask("autodown")));
    t.ev(Event::Key {
        key: Key::Escape,
        modifiers: 0,
    });
    assert_eq!(ask_complete(&t, id).unwrap().0, 0);
    // A click on the balloon outside its controls.
    let id = t.req(Call::Ask(ask("autodown")));
    let b = t.core.scene().office_balloon.unwrap().body;
    t.press(
        PointerKind::Press,
        Surface::OfficeBalloon,
        1,
        b.x + 1,
        b.y + 1,
    );
    assert_eq!(ask_complete(&t, id).unwrap().0, 0);
}

#[test]
fn b_ask_7_matrix_modal() {
    let mut t = T::visible(character("c", 100));
    // Play: runs, the balloon stays.
    let id = t.req(Call::Ask(ask("modal")));
    t.req(Call::Play("Wave".into()));
    t.advance(t.now + 200);
    assert!(t.core.scene().office_balloon.is_some());
    // Speak waits at the head until the ask closes.
    let sp = t.req(Call::Speak("x".into()));
    t.advance(t.now + 2000);
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: sp }), 0);
    t.ev(Event::Key {
        key: Key::Char('y'),
        modifiers: 0,
    });
    assert_eq!(ask_complete(&t, id).unwrap().0, -3);
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: sp }), 1);
    t.advance(t.now + 2000);
    // MoveTo: hides when the slide moves, placed again at the end of the slide.
    let id = t.req(Call::Ask(ask("modal")));
    let before = t.core.scene().office_balloon.unwrap().window;
    let now = t.now;
    t.req(Call::MoveTo {
        x: 300,
        y: 500,
        speed: 1000,
    });
    t.advance(now + 100 + 40);
    assert!(t.core.scene().office_balloon.is_none());
    t.advance(now + 2000);
    let after = t.core.scene().office_balloon.unwrap().window;
    assert_eq!(after.x, before.x - 200);
    // Drag: hides at drag start, placed again at drag end.
    t.press(PointerKind::Press, Surface::Character, 1, 310, 510);
    t.press(PointerKind::Motion, Surface::Character, 1, 330, 510);
    assert!(t.core.scene().office_balloon.is_none());
    t.press(PointerKind::Release, Surface::Character, 1, 330, 510);
    assert!(t.core.scene().office_balloon.is_some());
    // StopAll without ask: no effect.
    t.call(Call::StopAll(vec!["play".into()])).unwrap();
    assert!(ask_complete(&t, id).is_none());
    // Stop(ask id): result 0, status 3, Interrupted.
    t.call(Call::Stop(id)).unwrap();
    assert_eq!(ask_complete(&t, id).unwrap().0, 0);
    assert_eq!(t.complete_of(id).unwrap().1, 3);
    // Hide reaching execution: status 3, Hidden.
    let id = t.req(Call::Ask(ask("modal")));
    t.req(Call::Hide { fast: true });
    assert_eq!(
        t.complete_of(id).unwrap(),
        (t.now, 3, errors::HIDDEN.to_string())
    );
    t.req(Call::Show { fast: true });
    // StopAll(["ask"]).
    let id = t.req(Call::Ask(ask("modal")));
    t.call(Call::StopAll(vec!["ask".into()])).unwrap();
    assert_eq!(t.complete_of(id).unwrap().2, errors::INTERRUPTED);
    // Output change: placed again.
    let id = t.req(Call::Ask(ask("modal")));
    t.ev(Event::Outputs(vec![output(800, 600)]));
    assert!(t.core.scene().office_balloon.is_some());
    // Idle runs while the balloon stays.
    t.advance(t.now + 6000);
    assert!(t.count(|s| *s == Signal::IdleStart) >= 1);
    assert!(t.core.scene().office_balloon.is_some());
    // Character switch closes it: CharacterChanged.
    let _ = id;
}

#[test]
fn b_ask_7_matrix_autodown_and_switch() {
    let mut t = T::with(
        vec![character("a", 100), character("b", 100)],
        Some(StateFile {
            character: "a".into(),
            left: 500,
            top: 500,
            visible: true,
        }),
        vec![],
    );
    t.advance(1000);
    for c in [
        Call::Play("Wave".into()),
        Call::GestureAt { x: 0, y: 0 },
        Call::Speak("x".into()),
        Call::MoveTo {
            x: 0,
            y: 0,
            speed: 0,
        },
    ] {
        let id = t.req(Call::Ask(ask("autodown")));
        t.req(c);
        assert_eq!(ask_complete(&t, id).unwrap().0, 0);
        assert_eq!(t.complete_of(id).unwrap().1, 0);
        t.advance(t.now + 3000);
    }
    // A second Ask dismisses the first.
    let id = t.req(Call::Ask(ask("autodown")));
    let id2 = t.req(Call::Ask(ask("autodown")));
    assert_eq!(ask_complete(&t, id).unwrap().0, 0);
    // Select closes with CharacterChanged.
    t.call(Call::Select("b".into())).unwrap();
    assert_eq!(
        t.complete_of(id2).unwrap(),
        (t.now, 3, errors::CHARACTER_CHANGED.to_string())
    );
    assert_eq!(t.core.properties().character, "b");
}

#[test]
fn b_char_5_switch() {
    let mut m = character("a", 100);
    looping(&mut m);
    let mut t = T::with(
        vec![m, character("b", 100)],
        Some(StateFile {
            character: "a".into(),
            left: 77,
            top: 88,
            visible: true,
        }),
        vec![],
    );
    t.advance(1000);
    let a = t.req(Call::Play("Loop".into()));
    let b = t.req(Call::Play("Wave".into()));
    assert_eq!(
        t.call(Call::Select("zzz".into())).unwrap_err().name,
        errors::NO_SUCH_CHARACTER
    );
    t.call(Call::Select("b".into())).unwrap();
    assert_eq!(t.complete_of(a).unwrap().2, errors::CHARACTER_CHANGED);
    assert_eq!(t.complete_of(b).unwrap().1, 3);
    assert_eq!(t.pos(), (77, 88));
    assert!(t.signals().contains(&(t.now, Signal::Shown { cause: 4 })));
    t.advance(t.now + 1);
    assert!(
        t.log
            .iter()
            .any(|(_, o)| matches!(o, Output::PersistState(s) if s.character == "b"))
    );
}

#[test]
fn b_char_5_invalid_character() {
    let mut t = T::visible(character("a", 100));
    t.ev(Event::Catalog(vec![
        CatalogEntry {
            id: "a".into(),
            info: vec![],
        },
        CatalogEntry {
            id: "broken".into(),
            info: vec![],
        },
    ]));
    assert_eq!(
        t.call(Call::Select("broken".into())).unwrap_err().name,
        errors::INVALID_CHARACTER
    );
    let list = t.call(Call::ListCharacters).unwrap();
    assert_eq!(
        list,
        ReplyValue::Characters(vec![
            ("a".into(), String::new(), String::new()),
            ("broken".into(), String::new(), String::new())
        ])
    );
}

// ---------- user interaction ----------

#[test]
fn r_ui_1_b_ui_1_click_and_double_click() {
    let mut t = T::visible(character("c", 100));
    t.press(PointerKind::Press, Surface::Character, 1, 510, 520);
    t.press(PointerKind::Release, Surface::Character, 1, 510, 520);
    assert_eq!(
        t.count(|s| *s
            == Signal::Click {
                button: 1,
                modifiers: 0,
                x: 510,
                y: 520,
                from_tray: false
            }),
        1
    );
    t.now += 400;
    t.press(PointerKind::Press, Surface::Character, 1, 590, 590);
    assert_eq!(
        t.count(|s| *s
            == Signal::DblClick {
                button: 1,
                modifiers: 0,
                x: 590,
                y: 590,
                from_tray: false
            }),
        1
    );
    t.press(PointerKind::Release, Surface::Character, 1, 590, 590);
    // The release of a double-click gives nothing.
    assert_eq!(t.count(|s| matches!(s, Signal::Click { .. })), 1);
    // Beyond double_click_ms: two clicks.
    t.now += 600;
    t.press(PointerKind::Press, Surface::Character, 1, 510, 520);
    t.press(PointerKind::Release, Surface::Character, 1, 510, 520);
    assert_eq!(t.count(|s| matches!(s, Signal::Click { .. })), 2);
}

#[test]
fn r_ui_2_r_win_4_drag() {
    let mut t = T::visible(character("c", 100));
    t.press(PointerKind::Press, Surface::Character, 1, 510, 510);
    t.press(PointerKind::Motion, Surface::Character, 1, 511, 511);
    assert_eq!(t.count(|s| matches!(s, Signal::DragStart { .. })), 0);
    t.press(PointerKind::Motion, Surface::Character, 1, 512, 510);
    assert_eq!(t.count(|s| matches!(s, Signal::DragStart { .. })), 1);
    t.press(PointerKind::Motion, Surface::Character, 1, 2100, 300);
    // 1:1, no clamping; the output is not recomputed during the drag.
    assert_eq!(t.pos(), (2090, 290));
    assert!(t.core.scene().character.unwrap().dragging);
    t.press(PointerKind::Release, Surface::Character, 1, 2100, 300);
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::DragComplete {
            button: 1,
            modifiers: 0,
            x: 2100,
            y: 300
        }));
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::Move {
            x: 2090,
            y: 290,
            cause: 1
        }));
    assert_eq!(t.count(|s| matches!(s, Signal::Click { .. })), 0);
    // Right-button drag does not open the menu.
    t.press(PointerKind::Press, Surface::Character, 2, 2100, 300);
    t.press(PointerKind::Motion, Surface::Character, 2, 2110, 300);
    t.press(PointerKind::Release, Surface::Character, 2, 2110, 300);
    assert!(t.core.scene().menu.is_none());
    // Middle button never drags.
    t.now += 1000;
    t.press(PointerKind::Press, Surface::Character, 4, 2110, 300);
    t.press(PointerKind::Motion, Surface::Character, 4, 2150, 300);
    assert_eq!(t.count(|s| matches!(s, Signal::DragStart { .. })), 2);
}

#[test]
fn b_menu_1_r_ui_3_b_menu_2_b_menu_3() {
    let mut t = T::visible(character("c", 100));
    t.press(PointerKind::Press, Surface::Character, 2, 510, 520);
    t.press(PointerKind::Release, Surface::Character, 2, 510, 520);
    assert!(t.count(|s| matches!(s, Signal::Click { button: 2, .. })) == 1);
    let m = t.core.scene().menu.unwrap();
    assert_eq!((m.x, m.y), (510, 520));
    let labels: Vec<String> = m
        .entries
        .iter()
        .map(|e| match e {
            MenuEntry::Item { label, .. } => label.clone(),
            MenuEntry::Separator => "-".into(),
        })
        .collect();
    assert_eq!(
        labels,
        vec![
            "&Hide",
            "-",
            "&Options...",
            "&Choose Assistant...",
            "&Animate!"
        ]
    );
    // Options... opens the config file after the menu closes.
    t.ev(Event::Menu(MenuInput::Activate(2)));
    assert!(t.core.scene().menu.is_none());
    assert!(t.log.iter().any(|(_, o)| *o == Output::OpenConfig));
    // German strings; keyboard navigation and mnemonics.
    let cfg = Config {
        language: LanguageSetting::De,
        ..Config::default()
    };
    let mut t = T::with_config(vec![character("c", 100)], None, vec![], cfg);
    t.advance(1000);
    let (x, y) = t.pos();
    t.press(PointerKind::Press, Surface::Character, 2, x + 1, y + 1);
    t.press(PointerKind::Release, Surface::Character, 2, x + 1, y + 1);
    let m = t.core.scene().menu.unwrap();
    assert!(
        matches!(&m.entries[3], MenuEntry::Item { label, .. } if label == "Assistenten &wählen...")
    );
    t.ev(Event::Key {
        key: Key::Down,
        modifiers: 0,
    });
    assert_eq!(t.core.scene().menu.unwrap().highlighted, Some(0));
    t.ev(Event::Key {
        key: Key::Down,
        modifiers: 0,
    });
    assert_eq!(t.core.scene().menu.unwrap().highlighted, Some(2));
    t.ev(Event::Key {
        key: Key::Escape,
        modifiers: 0,
    });
    assert!(t.core.scene().menu.is_none());
    // No Animate! item without matching animations.
    let mut m = CharacterModel::new("x", Family::Agent, (10, 10));
    m.animations.push(anim("Foo", vec![fr(1)]));
    let mut t = T::visible(m);
    t.press(PointerKind::Press, Surface::Character, 2, 501, 501);
    t.press(PointerKind::Release, Surface::Character, 2, 501, 501);
    assert_eq!(t.core.scene().menu.unwrap().entries.len(), 4);
}

#[test]
fn b_ui_4_user_hide() {
    let mut m = character("c", 100);
    looping(&mut m);
    let mut t = T::visible(m);
    let a = t.req(Call::Play("Loop".into()));
    let h = t.req(Call::Hide { fast: false });
    t.press(PointerKind::Press, Surface::Character, 2, 510, 510);
    t.press(PointerKind::Release, Surface::Character, 2, 510, 510);
    t.ev(Event::Menu(MenuInput::Activate(0)));
    assert_eq!(t.complete_of(a).unwrap().2, errors::HIDDEN_BY_USER);
    assert_eq!(t.complete_of(h).unwrap().2, errors::HIDDEN_BY_USER);
    // Animated Hide after the loop's exit (50 ms) and Hiding (100 ms).
    t.advance(t.now + 1000);
    assert!(
        t.signals()
            .iter()
            .any(|(_, s)| *s == Signal::Hidden { cause: 7 })
    );
    assert_eq!(t.core.properties().visibility_cause, 7);
    // Tray menu while hidden: only Show; Show with cause 2.
    let tray = t.core.scene().tray.menu;
    assert_eq!(
        tray,
        vec![MenuEntry::Item {
            label: "&Show".into(),
            action: MenuAction::Show
        }]
    );
    t.ev(Event::Tray(TrayInput::Menu(MenuAction::Show)));
    assert!(
        t.signals()
            .iter()
            .any(|(_, s)| *s == Signal::Shown { cause: 2 })
    );
    t.advance(t.now + 500);
    // Tray Hide: cause 1; open ask closes with HiddenByUser.
    let id = t.req(Call::Ask(ask("modal")));
    t.ev(Event::Tray(TrayInput::Menu(MenuAction::Hide)));
    assert_eq!(
        t.complete_of(id).unwrap(),
        (t.now, 3, errors::HIDDEN_BY_USER.to_string())
    );
    t.advance(t.now + 500);
    assert!(
        t.signals()
            .iter()
            .any(|(_, s)| *s == Signal::Hidden { cause: 1 })
    );
}

#[test]
fn b_tray_1_r_ui_4_activate() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Hide { fast: true });
    t.ev(Event::Tray(TrayInput::Activate { x: 5, y: 6 }));
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::Click {
            button: 1,
            modifiers: 0,
            x: 5,
            y: 6,
            from_tray: true
        }));
    assert!(
        t.signals()
            .iter()
            .any(|(_, s)| *s == Signal::Shown { cause: 2 })
    );
    t.ev(Event::Tray(TrayInput::SecondaryActivate { x: 1, y: 2 }));
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::Click {
            button: 4,
            modifiers: 0,
            x: 1,
            y: 2,
            from_tray: true
        }));
    let tr = t.core.scene().tray;
    assert_eq!(tr.title, "Name c");
    assert_eq!(tr.icon_character.as_deref(), Some("c"));
}

#[test]
fn b_menu_4_animate_shuffle() {
    let mut m = character("c", 100);
    m.animations.push(anim("Printing", vec![fr(100)]));
    let st = StateFile {
        character: "c".into(),
        left: 500,
        top: 500,
        visible: true,
    };
    // Shuffle draws at load: list = [Greeting, Explain, Wave, Print] (resolution order
    // of ANIMATE_NAMES), Fisher-Yates with draws 0, 0, 0.
    let mut t = T::with(vec![m], Some(st), vec![0, 0, 0]);
    t.advance(1000);
    let mut played = Vec::new();
    for _ in 0..4 {
        t.ev(Event::Menu(MenuInput::Dismiss));
        t.press(PointerKind::Press, Surface::Character, 2, 510, 510);
        t.press(PointerKind::Release, Surface::Character, 2, 510, 510);
        t.ev(Event::Menu(MenuInput::Activate(4)));
        if let Picture::Frame { animation, .. } = t.core.scene().character.unwrap().picture {
            played.push(animation);
        }
        t.advance(t.now + 1000);
        t.now += 1000;
    }
    played.sort();
    assert_eq!(played, vec!["Explain", "Greeting", "Printing", "Wave"]);
    // Daemon requests: no request signals.
    assert_eq!(t.count(|s| matches!(s, Signal::RequestStart { .. })), 0);
}

#[test]
fn b_gal_1_gallery() {
    let mut t = T::with(
        vec![
            character("a", 100),
            character("b", 100),
            character("c", 100),
        ],
        Some(StateFile {
            character: "b".into(),
            left: 500,
            top: 500,
            visible: true,
        }),
        vec![],
    );
    t.advance(1000);
    let open_menu = |t: &mut T| {
        let (x, y) = t.pos();
        t.press(PointerKind::Press, Surface::Character, 2, x + 1, y + 1);
        t.press(PointerKind::Release, Surface::Character, 2, x + 1, y + 1);
        t.ev(Event::Menu(MenuInput::Activate(3)));
    };
    // A client ask is closed first.
    let id = t.req(Call::Ask(ask("modal")));
    open_menu(&mut t);
    assert_eq!(t.complete_of(id).unwrap().2, errors::INTERRUPTED);
    let s = t.core.scene().office_balloon.unwrap();
    let caps: Vec<&str> = s.buttons.iter().map(|b| b.caption.as_str()).collect();
    assert_eq!(caps, vec!["<Back", "Next>", "OK", "Cancel"]);
    assert_eq!(s.heading[0].runs[0].text, "Name");
    // Next: switches the live character, previews Wave, balloon survives.
    t.ev(Event::Key {
        key: Key::Char('n'),
        modifiers: 0,
    });
    assert_eq!(t.core.properties().character, "c");
    let s = t.core.scene().office_balloon.unwrap();
    assert_eq!(s.buttons[1].state, ControlState::Disabled);
    // Disabled Next ignores its mnemonic.
    t.ev(Event::Key {
        key: Key::Char('n'),
        modifiers: 0,
    });
    assert_eq!(t.core.properties().character, "c");
    t.advance(t.now + 1000);
    // Client Speak waits while the gallery is open.
    let sp = t.req(Call::Speak("x".into()));
    assert_eq!(t.count(|s| *s == Signal::RequestStart { id: sp }), 0);
    // Cancel (Esc) switches back.
    t.ev(Event::Key {
        key: Key::Escape,
        modifiers: 0,
    });
    assert_eq!(t.core.properties().character, "b");
    assert!(t.core.scene().office_balloon.is_none());
    // No request signals for the gallery itself (only the client's).
    assert_eq!(t.count(|s| matches!(s, Signal::AskComplete { .. })), 1);
    // OK keeps the choice.
    t.advance(t.now + 3000);
    open_menu(&mut t);
    t.ev(Event::Key {
        key: Key::Char('b'),
        modifiers: 0,
    });
    assert_eq!(t.core.properties().character, "a");
    let ok = t.core.scene().office_balloon.unwrap().buttons[2].rect;
    t.press(
        PointerKind::Press,
        Surface::OfficeBalloon,
        1,
        ok.x + 1,
        ok.y + 1,
    );
    t.press(
        PointerKind::Release,
        Surface::OfficeBalloon,
        1,
        ok.x + 1,
        ok.y + 1,
    );
    assert_eq!(t.core.properties().character, "a");
    assert!(t.core.scene().office_balloon.is_none());
    t.advance(t.now + 2000);
    assert!(
        t.log
            .iter()
            .any(|(_, o)| matches!(o, Output::PersistState(s) if s.character == "a"))
    );
}

#[test]
fn b_wl_4_output_change_pulls_back() {
    let mut t = T::visible(character("c", 100));
    t.ev(Event::Outputs(vec![output(400, 300)]));
    assert_eq!(t.pos(), (300, 200));
    assert!(t.signals().iter().any(|(_, s)| *s
        == Signal::Move {
            x: 300,
            y: 200,
            cause: 4
        }));
    assert_eq!(t.core.properties().move_cause, 4);
}

#[test]
fn b_cfg_2_b_run_1_persist_throttle_and_quit() {
    let mut t = T::visible(character("c", 100));
    t.advance(t.now + 2000);
    t.log.clear();
    t.req(Call::MoveTo {
        x: 1,
        y: 1,
        speed: 0,
    });
    t.req(Call::MoveTo {
        x: 2,
        y: 2,
        speed: 0,
    });
    t.advance(t.now + 999);
    let n = t
        .log
        .iter()
        .filter(|(_, o)| matches!(o, Output::PersistState(_)))
        .count();
    assert_eq!(n, 1);
    t.advance(t.now + 1000);
    let w: Vec<_> = t
        .log
        .iter()
        .filter_map(|(_, o)| match o {
            Output::PersistState(s) => Some((s.left, s.top)),
            _ => None,
        })
        .collect();
    assert_eq!(*w.last().unwrap(), (2, 2));
    t.log.clear();
    t.call(Call::Quit).unwrap();
    let kinds: Vec<_> = t
        .log
        .iter()
        .map(|(_, o)| match o {
            Output::Reply { .. } => "reply",
            Output::PersistState(_) => "persist",
            Output::Quit => "quit",
            _ => "other",
        })
        .collect();
    assert_eq!(kinds, vec!["reply", "persist", "quit"]);
}

#[test]
fn b_fb_1_b_char_2_play_errors_and_unavailable() {
    let mut m = character("c", 100);
    m.animations.retain(|a| a.name != "Wave");
    m.animations.push(Animation {
        available: false,
        ..anim("Broken", vec![fr(1)])
    });
    let mut t = T::visible(m);
    let e = t.call(Call::Play("Goodbye".into())).unwrap_err();
    assert_eq!(e.name, errors::UNKNOWN_ANIMATION);
    assert!(e.message.contains("Goodbye"));
    // Goodbye -> Wave via the chain.
    let mut t2 = T::visible(character("d", 100));
    let id = t2.req(Call::Play("Goodbye".into()));
    let _ = id;
    match t2.core.scene().character.unwrap().picture {
        Picture::Frame { animation, .. } => assert_eq!(animation, "Wave"),
        p => panic!("{p:?}"),
    }
    let id = t.req(Call::Play("broken".into()));
    assert_eq!(
        t.complete_of(id).unwrap(),
        (t.now, 1, errors::ANIMATION_UNAVAILABLE.to_string())
    );
    // Missing state: success, nothing played.
    let mut m = CharacterModel::new("x", Family::Agent, (5, 5));
    m.animations.push(anim("PointUp", vec![fr(1)]));
    m.states.push(("GesturingUp".into(), vec!["Gone".into()]));
    let mut t = T::visible(m);
    let id = t.req(Call::GestureAt { x: 0, y: -1000 });
    assert_eq!(t.complete_of(id).unwrap().1, 0);
}

#[test]
fn r_anim_8_sound_commands() {
    let mut m = character("c", 100);
    m.sounds.push(Sound {
        id: "s".into(),
        duration_ms: Some(300),
    });
    let mut f = fr(100);
    f.sound = Some("s".into());
    m.animations.push(anim("Snd", vec![f]));
    let mut t = T::visible(m.clone());
    let id = t.req(Call::Play("Snd".into()));
    assert!(
        t.log
            .iter()
            .any(|(_, o)| *o == Output::SoundStart { sound: "s".into() })
    );
    let now = t.now;
    t.advance(now + 1000);
    assert_eq!(t.complete_of(id).unwrap().0, now + 300);
    // Device failure: completion does not wait.
    let id = t.req(Call::Play("Snd".into()));
    let now = t.now;
    t.advance(now + 150);
    t.ev(Event::SoundFailed);
    assert_eq!(t.complete_of(id).unwrap().0, now + 150);
    // sound_effects = false: no sound.
    let cfg = Config {
        sound_effects: false,
        ..Config::default()
    };
    let mut t = T::with_config(vec![m], None, vec![], cfg);
    t.advance(1000);
    t.req(Call::Play("Snd".into()));
    assert!(
        !t.log
            .iter()
            .any(|(_, o)| matches!(o, Output::SoundStart { .. }))
    );
}

#[test]
fn properties_and_list() {
    let t = T::visible(character("c", 100));
    let p = t.core.properties();
    assert_eq!((p.width, p.height, p.name.as_str()), (100, 100, "Name c"));
    assert!(p.animation_names.contains(&"Wave".to_string()));
}

// ---------- further rule tests ----------

fn current_anim(t: &T) -> String {
    match t.core.scene().character.unwrap().picture {
        Picture::Frame { animation, .. } => animation,
        p => panic!("{p:?}"),
    }
}

#[test]
fn r_anim_7_b_anim_1_stop_cuts_and_no_animation_requests_keep_previous() {
    let mut m = character("c", 100);
    looping(&mut m);
    m.sounds.push(Sound {
        id: "s".into(),
        duration_ms: Some(5000),
    });
    m.animations.last_mut().unwrap().frames[0].sound = Some("s".into());
    let mut t = T::visible(m);
    let id = t.req(Call::Play("Loop".into()));
    t.advance(t.now + 150);
    let pic = t.core.scene().character.unwrap().picture;
    t.call(Call::Stop(id)).unwrap();
    // Timer killed, sound stopped, picture kept.
    assert!(t.log.iter().any(|(_, o)| *o == Output::SoundStop));
    assert_eq!(t.core.scene().character.unwrap().picture, pic);
    // Hide(fast) and Show(fast) play no animation and do not consume the previous one.
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    // The next animation request resumes the loop in exit mode (frame 2) first.
    let now = t.now;
    t.req(Call::Play("Wave".into()));
    match t.core.scene().character.unwrap().picture {
        Picture::Frame {
            animation, frame, ..
        } => assert_eq!((animation.as_str(), frame), ("Loop", 2)),
        p => panic!("{p:?}"),
    }
    t.advance(now + 50);
    assert_eq!(current_anim(&t), "Wave");
}

#[test]
fn r_q_3_immediate_calls() {
    let mut t = T::with(vec![character("a", 100), character("b", 100)], None, vec![]);
    assert_eq!(t.call(Call::Stop(0)), Ok(ReplyValue::Unit));
    assert_eq!(t.call(Call::StopAll(vec![])), Ok(ReplyValue::Unit));
    assert_eq!(t.call(Call::SetIdleOn(true)), Ok(ReplyValue::Unit));
    assert_eq!(t.call(Call::Select("b".into())), Ok(ReplyValue::Unit));
    assert_eq!(t.core.properties().character, "b");
}

#[test]
fn r_bal_7_not_shown_during_drag() {
    let mut m = character("c", 0);
    m.states.retain(|s| s.0 != "Speaking");
    let mut t = T::visible(m);
    t.press(PointerKind::Press, Surface::Character, 1, 510, 510);
    t.press(PointerKind::Motion, Surface::Character, 1, 530, 510);
    let id = t.req(Call::Speak("a b".into()));
    assert!(t.core.scene().agent_balloon.is_none());
    assert_eq!(t.count(|s| *s == Signal::BalloonShown), 0);
    t.advance(t.now + 1000);
    // Text is still paced.
    assert_eq!(t.complete_of(id).unwrap().1, 0);
}

#[test]
fn b_bal_2_font_choice() {
    let mut m = character("c", 0);
    m.states.retain(|s| s.0 != "Speaking");
    m.balloon.font = "Pebble Sans".into();
    m.balloon.font_height = -15;
    let mut t = T::visible(m.clone());
    t.req(Call::Speak("a".into()));
    let f = t.core.scene().agent_balloon.unwrap().font;
    assert_eq!(
        (f.family.as_str(), f.px, f.bold),
        ("Pebble Sans", 15, false)
    );
    let cfg = Config {
        balloon_font: "Forced".into(),
        ..Config::default()
    };
    let st = StateFile {
        character: "c".into(),
        left: 500,
        top: 500,
        visible: true,
    };
    let mut t = T::with_config(vec![m], Some(st), vec![], cfg);
    t.advance(1000);
    t.req(Call::Speak("a".into()));
    assert_eq!(t.core.scene().agent_balloon.unwrap().font.family, "Forced");
}

#[test]
fn b_bal_3_anchor_is_lowest_mouth_point() {
    let mut m = character("c", 1000);
    m.canvas = (50, 50); // display 100 x 100: anchors scale by 2
    let sp = m
        .animations
        .iter_mut()
        .find(|a| a.name == "SpeakingA")
        .unwrap();
    sp.frames[0].mouths = vec![
        Mouth {
            kind: 4,
            anchor: (40, 40),
        },
        Mouth {
            kind: 1,
            anchor: (10, 30),
        },
    ];
    let mut t = T::visible(m);
    t.req(Call::Speak("a".into()));
    t.advance(t.now + 1000);
    // Speaking frame stays shown (last frame); mouth type 1 at (10,30) -> (520, 560).
    let b = t.core.scene().agent_balloon.unwrap();
    assert_eq!(b.tail_side, TailSide::Bottom);
    assert_eq!(b.tail[0].0, 520);
}

#[test]
fn b_pick_a_speakable_keeps_animation_with_mouths() {
    let mut m = character("c", 100);
    let mut f = fr(5000);
    f.mouths = vec![Mouth {
        kind: 0,
        anchor: (1, 1),
    }];
    m.animations.push(anim("Talky", vec![f]));
    let mut t = T::visible(m);
    let p = t.req(Call::Play("Talky".into()));
    t.call(Call::Stop(p)).unwrap();
    t.req(Call::Speak("hi".into()));
    // Keeps Talky (no Speaking state played), output starts at once.
    assert_eq!(current_anim(&t), "Talky");
    assert!(t.core.scene().agent_balloon.is_some());
}

#[test]
fn b_lip_2_no_mouth_during_text_speak() {
    let mut m = character("c", 100);
    m.animations
        .iter_mut()
        .find(|a| a.name == "SpeakingA")
        .unwrap()
        .frames[0]
        .mouths = vec![Mouth {
        kind: 0,
        anchor: (1, 1),
    }];
    let mut t = T::visible(m);
    t.req(Call::Speak("one two three".into()));
    for k in 1..10 {
        t.advance(t.now + 100 * k);
        if let Picture::Frame { mouth, .. } = t.core.scene().character.unwrap().picture {
            assert_eq!(mouth, None);
        }
    }
}

#[test]
fn b_o97_4_play_group_picks_variant() {
    let mut m = CharacterModel::new("o", Family::Office97, (10, 10));
    for v in ["Greeting.1", "Greeting.2", "Greeting.3"] {
        m.animations.push(anim(
            v,
            vec![Frame {
                ms: 100,
                has_picture: true,
                pose: Some(1),
                ..Default::default()
            }],
        ));
    }
    m.states.push((
        "Greeting".into(),
        vec![
            "Greeting.1".into(),
            "Greeting.2".into(),
            "Greeting.3".into(),
        ],
    ));
    let st = StateFile {
        character: "o".into(),
        left: 0,
        top: 0,
        visible: true,
    };
    let mut t = T::with(vec![m], Some(st), vec![]);
    t.core.set_draws(Box::new(ScriptedDraws::new([2])));
    t.call(Call::SetIdleOn(false)).unwrap();
    t.req(Call::Play("greeting".into()));
    assert_eq!(current_anim(&t), "Greeting.3");
    assert!(
        t.core
            .properties()
            .animation_names
            .contains(&"Greeting".to_string())
    );
}

#[test]
fn b_cfg_1_defaults() {
    let c = Config::default();
    assert_eq!(c.character, None);
    assert_eq!(c.language, LanguageSetting::Auto);
    assert!(c.sound_effects && c.balloon && c.idle);
    assert_eq!(c.balloon_font, "");
    assert_eq!(c.speaking_speed, 1.0);
    assert_eq!(c.double_click_ms, 500);
    // `idle` is the initial IdleOn.
    let cfg = Config {
        idle: false,
        ..Config::default()
    };
    let t = T::with_config(vec![character("c", 1)], None, vec![], cfg);
    assert!(!t.core.properties().idle_on);
}

#[test]
fn b_show_4_cause_codes() {
    let mut t = T::visible(character("c", 0));
    // Program: Shown 4, Hidden 3, Move 2; user: Shown 2 (tray), Hidden 1 (tray) and 7
    // (menu), Move 1 (drag); display change: Move 4.
    t.req(Call::Hide { fast: true });
    t.req(Call::Show { fast: true });
    t.req(Call::MoveTo {
        x: 10,
        y: 10,
        speed: 0,
    });
    t.ev(Event::Tray(TrayInput::Menu(MenuAction::Hide)));
    t.advance(t.now + 100);
    t.ev(Event::Tray(TrayInput::Activate { x: 0, y: 0 }));
    t.advance(t.now + 100);
    t.press(PointerKind::Press, Surface::Character, 2, 11, 11);
    t.press(PointerKind::Release, Surface::Character, 2, 11, 11);
    t.ev(Event::Menu(MenuInput::Activate(0)));
    t.advance(t.now + 100);
    t.ev(Event::Tray(TrayInput::Activate { x: 0, y: 0 }));
    t.advance(t.now + 100);
    t.now += 1000;
    t.press(PointerKind::Press, Surface::Character, 1, 11, 11);
    t.press(PointerKind::Motion, Surface::Character, 1, 21, 11);
    t.press(PointerKind::Release, Surface::Character, 1, 21, 11);
    t.ev(Event::Outputs(vec![output(50, 50)]));
    let causes: Vec<String> = t
        .signals()
        .into_iter()
        .filter_map(|(_, s)| match s {
            Signal::Shown { cause } => Some(format!("S{cause}")),
            Signal::Hidden { cause } => Some(format!("H{cause}")),
            Signal::Move { cause, .. } => Some(format!("M{cause}")),
            _ => None,
        })
        .collect();
    assert_eq!(
        causes,
        vec!["H3", "S4", "M2", "H1", "S2", "H7", "S2", "M1", "M4"]
    );
}

#[test]
fn r_ob_2_text_colours() {
    let mut t = T::visible(character("c", 100));
    t.req(Call::Ask(AskSpec {
        text: Some("body".into()),
        labels: Some(vec!["label".into()]),
        animation: Some(String::new()),
        ..Default::default()
    }));
    let s = t.core.scene().office_balloon.unwrap();
    assert_eq!(s.text[0].runs[0].color, [0, 0, 0]);
    assert_eq!(s.labels[0].lines[0].runs[0].color, [0, 0, 0x80]);
}
