//! A tiny stand-in for the behaviour core, used by `--stub-core` to test the D-Bus,
//! headless and Wayland plumbing on their own. Each request runs for 100 ms.

use crate::discovery::Entry;
use crate::engine::*;
use std::collections::VecDeque;

struct Req {
    id: u32,
    client: String,
    m: Method,
}

pub struct Stub {
    character: Option<Entry>,
    next_id: u32,
    queue: VecDeque<Req>,
    running: Option<(Req, u64)>,
    ask: Option<Req>,
    out: Vec<Effect>,
    props: CharacterProps,
}

impl Stub {
    pub fn new() -> Stub {
        Stub {
            character: None,
            next_id: 1,
            queue: VecDeque::new(),
            running: None,
            ask: None,
            out: Vec::new(),
            props: CharacterProps {
                width: 64,
                height: 64,
                idle_on: true,
                balloon_enabled: true,
                left: 100,
                top: 100,
                ..Default::default()
            },
        }
    }

    fn sig(&mut self, dest: Option<&str>, s: Signal) {
        self.out.push(Effect::Signal(dest.map(String::from), s));
    }

    fn pump(&mut self, now: u64) {
        loop {
            if let Some((_, end)) = &self.running {
                if *end > now {
                    return;
                }
                let (r, _) = self.running.take().unwrap();
                match r.m {
                    Method::Ask(_) => {
                        self.sig(None, Signal::BalloonShown);
                        self.ask = Some(r);
                        continue;
                    }
                    Method::Show(_) => {
                        self.props.visible = true;
                        self.props.visibility_cause = 4;
                        self.sig(None, Signal::Shown(4));
                    }
                    Method::Hide(_) => {
                        self.props.visible = false;
                        self.props.visibility_cause = 3;
                        self.sig(None, Signal::Hidden(3));
                    }
                    Method::MoveTo(x, y, _) => {
                        self.props.left = x;
                        self.props.top = y;
                        self.props.move_cause = 2;
                        self.sig(None, Signal::Move(x, y, 2));
                        self.out.push(Effect::Persist);
                    }
                    _ => {}
                }
                self.sig(
                    Some(&r.client.clone()),
                    Signal::RequestComplete(r.id, 0, String::new()),
                );
            }
            let Some(r) = self.queue.pop_front() else {
                return;
            };
            self.sig(Some(&r.client.clone()), Signal::RequestStart(r.id));
            self.running = Some((r, now + 100));
        }
    }
}

impl Engine for Stub {
    fn call(&mut self, now: u64, client: &str, m: Method) -> Result<u32, CallError> {
        if self.character.is_none() {
            return Err(CallError::new(E_NO_CHARACTER, "no character loaded"));
        }
        match m {
            Method::Stop(_) | Method::StopAll(_) => {
                for r in self
                    .queue
                    .drain(..)
                    .chain(self.running.take().map(|r| r.0))
                    .collect::<Vec<_>>()
                {
                    self.sig(
                        Some(&r.client),
                        Signal::RequestComplete(r.id, 3, "org.bentwire.Error.Interrupted".into()),
                    );
                }
                Ok(0)
            }
            Method::SetIdleOn(b) => {
                self.props.idle_on = b;
                Ok(0)
            }
            m => {
                let id = self.next_id;
                self.next_id += 1;
                self.queue.push_back(Req {
                    id,
                    client: client.into(),
                    m,
                });
                self.pump(now);
                Ok(id)
            }
        }
    }

    fn select(&mut self, _now: u64, entry: Option<&Entry>) -> Result<(), CallError> {
        self.character = entry.cloned();
        self.props.id = entry.map(|e| e.id.clone()).unwrap_or_default();
        self.props.name = entry
            .map(|e| crate::discovery::display_names(&e.info, "en").0)
            .unwrap_or_default();
        self.out.push(Effect::Persist);
        Ok(())
    }

    fn input(&mut self, _now: u64, i: Input) {
        if let Input::Pointer {
            kind: PointerKind::Release,
            button,
            x,
            y,
            modifiers,
        } = i
        {
            self.sig(None, Signal::Click(button, modifiers, x, y, false));
        }
    }

    fn set_outputs(&mut self, _now: u64, _outputs: &[Output]) {}
    fn set_draws(&mut self, _draws: Vec<u32>) {}
    fn set_position(&mut self, left: i32, top: i32) {
        self.props.left = left;
        self.props.top = top;
    }

    fn advance(&mut self, now: u64) {
        self.pump(now);
    }

    fn next_deadline(&self) -> Option<u64> {
        self.running.as_ref().map(|r| r.1)
    }

    fn drain(&mut self) -> Vec<Effect> {
        std::mem::take(&mut self.out)
    }

    fn props(&self) -> Option<CharacterProps> {
        self.character.as_ref().map(|_| self.props.clone())
    }

    fn scene(&self) -> Scene {
        let p = &self.props;
        let mut s = Scene::default();
        if p.visible {
            let n = (p.width * p.height) as usize;
            s.surfaces.push(Surface {
                kind: SurfaceKind::Character,
                x: p.left,
                y: p.top,
                w: p.width,
                h: p.height,
                scale: 1,
                rgba: [255u8, 0, 0, 255].repeat(n),
                input: vec![(0, 0, p.width, p.height)],
            });
        }
        s.keyboard = self.ask.is_some();
        s
    }

    fn answer(&mut self, now: u64, result: i32) {
        if let Some(r) = self.ask.take() {
            self.sig(None, Signal::BalloonHidden);
            self.sig(
                Some(&r.client.clone()),
                Signal::AskComplete(r.id, result, vec![]),
            );
            self.sig(
                Some(&r.client.clone()),
                Signal::RequestComplete(r.id, 0, String::new()),
            );
            self.pump(now);
        }
    }
}
