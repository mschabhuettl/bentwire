//! The frame stepper (R-ANIM-*, B-SEQ-A, B-SW-A, B-O97-*, B-SW-O, B-ANIM-1/2).

use crate::api::{Draws, Ms, Output};
use crate::model::{Branch, CharacterModel, Exit, Family, Frame, Transition};

/// Frames consumed per timer callback before a 0 ms cycle yields.
const MAX_STEPS: u32 = 1000;

/// Logical picture of the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pic {
    Empty,
    Frame(usize, usize),
    Pose(u32),
}

/// B-SHOW-3: the picture `Show(fast)` displays, also the rest picture the character is
/// anchored by (B-CHAR-5): frame 0 of `RestPose` (office97: the rest pose), else the last
/// frame of the first Showing member, else empty.
pub fn rest_picture(m: &CharacterModel) -> Pic {
    let pic = if m.family == Family::Office97 {
        m.office97.as_ref().and_then(|o| o.rest_pose).map(Pic::Pose)
    } else {
        m.animation_index("RestPose")
            .filter(|&i| !m.animations[i].frames.is_empty())
            .map(|i| Pic::Frame(i, 0))
    };
    pic.or_else(|| {
        let members = crate::fallback::state_members(m, "Showing")?;
        let i = m.animation_index(members.first()?)?;
        let n = m.animations[i].frames.len();
        (n > 0).then(|| Pic::Frame(i, n - 1))
    })
    .unwrap_or(Pic::Empty)
}

/// Context of one player call.
pub struct Env<'a> {
    pub model: &'a CharacterModel,
    pub draws: &'a mut dyn Draws,
    /// The character is visible: frames are drawn and sounds may start (R-ANIM-9).
    pub visible: bool,
    /// Config `sound_effects`.
    pub sounds_on: bool,
    /// Local hour for `daypart`.
    pub hour: u8,
    pub out: &'a mut Vec<Output>,
}

/// B-SEQ-A `next()` (character-format 7.2). `None` = END.
pub fn agent_next(
    frames: &[Frame],
    cur: usize,
    exiting: bool,
    draws: &mut dyn Draws,
) -> Option<usize> {
    let mut cur = cur;
    let mut t = cur;
    for _ in 0..32 {
        let f = &frames[cur];
        let mut target = None;
        if exiting {
            match f.exit {
                Exit::End => return None,
                Exit::To(i) => target = Some(i),
                Exit::None => {}
            }
        }
        if target.is_none() && !f.branches.is_empty() {
            let mut r = draws.draw(100);
            for b in &f.branches {
                if let Branch::Prob { target: to, p } = *b {
                    if r < p {
                        target = Some(to);
                        break;
                    }
                    r -= p;
                }
            }
        }
        t = target.unwrap_or(cur + 1);
        if t >= frames.len() {
            return None;
        }
        if invisible(&frames[t]) {
            cur = t;
            continue;
        }
        return Some(t);
    }
    Some(t)
}

/// R-ANIM-5: an invisible branch point.
fn invisible(f: &Frame) -> bool {
    !f.has_picture && f.ms == 0
}

/// A-CORE-6: can exit-mode `next()` reach END from frame `from`? Static reachability
/// over the exit target, else the branches with p > 0 and the `cur + 1` fallthrough
/// (taken when the branch probabilities sum to less than 100).
pub fn exit_end_reachable(frames: &[Frame], from: usize) -> bool {
    let mut seen = vec![false; frames.len()];
    let mut stack = vec![from];
    while let Some(i) = stack.pop() {
        if i >= frames.len() {
            return true;
        }
        if std::mem::replace(&mut seen[i], true) {
            continue;
        }
        let f = &frames[i];
        match f.exit {
            Exit::End => return true,
            Exit::To(t) => stack.push(t),
            Exit::None => {
                let mut sum = 0;
                for b in &f.branches {
                    if let Branch::Prob { target, p } = *b
                        && p > 0
                    {
                        sum += p;
                        stack.push(target);
                    }
                }
                if sum < 100 {
                    stack.push(i + 1);
                }
            }
        }
    }
    false
}

/// Agent start frame: 0, or `next(0)` if frame 0 is invisible.
pub fn agent_start(frames: &[Frame], draws: &mut dyn Draws) -> Option<usize> {
    if frames.is_empty() {
        None
    } else if invisible(&frames[0]) {
        agent_next(frames, 0, false, draws)
    } else {
        Some(0)
    }
}

/// office97 period by local hour (act-format 4.11).
pub fn daypart(hour: u8) -> u8 {
    match hour {
        5..=9 => 0,
        10..=17 => 1,
        18..=21 => 2,
        _ => 3,
    }
}

/// B-O97-1 branch walk after frame `i` (character-format 7.3).
pub fn office_next(f: &Frame, i: usize, exiting: bool, hour: u8, draws: &mut dyn Draws) -> usize {
    for b in &f.branches {
        let taken = if exiting {
            b.target() > i
        } else {
            match *b {
                Branch::Prob { p, .. } => p >= 32768 || (p > 0 && draws.draw(32768) < p),
                Branch::Daypart { period, .. } => daypart(hour) == period,
                Branch::Host { .. } => false,
            }
        };
        if taken {
            return b.target();
        }
    }
    i + 1
}

/// The "previous animation" (R-ANIM-6).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Prev {
    anim: usize,
    frame: usize,
    ended: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    /// Exit, return animation or office97 transition clip before the target.
    LeadIn,
    Target,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Run {
    anim: usize,
    /// Agent: the current frame. office97: the last consumed frame.
    frame: usize,
    /// office97: next frame to consume.
    next: usize,
    exiting: bool,
    deadline: Ms,
    /// office97 due time.
    due: Ms,
    role: Role,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    /// Start the target `anim` when the lead-in ends.
    Target(usize),
    /// office97: run the transition check, then the target.
    O97Check(usize, u8),
}

/// One frame stepper per character (R-ANIM-10).
#[derive(Clone, Debug)]
pub struct Player {
    prev: Option<Prev>,
    run: Option<Run>,
    pending: Option<Pending>,
    /// office97 logical pose (`None` = blank or unknown).
    last_pose: Option<u32>,
    pic: Pic,
    /// End of the sound currently playing, if known; `Some(None)`: unknown end.
    sound: Option<Option<Ms>>,
    /// Whether the current sound was started by the target animation.
    sound_is_target: bool,
    /// Completion is waiting for the target's sound (B-ANIM-2).
    waiting_sound: bool,
    /// Frames shown by the current exit-mode run (A-CORE-6).
    exit_seen: Vec<usize>,
}

impl Default for Player {
    fn default() -> Self {
        Self::new()
    }
}

impl Player {
    pub fn new() -> Self {
        Player {
            prev: None,
            run: None,
            pending: None,
            last_pose: None,
            pic: Pic::Empty,
            sound: None,
            sound_is_target: false,
            waiting_sound: false,
            exit_seen: Vec::new(),
        }
    }

    /// Forget everything (character switch).
    pub fn reset(&mut self) {
        *self = Player::new();
    }

    pub fn picture(&self) -> Pic {
        self.pic
    }

    pub fn set_picture(&mut self, p: Pic) {
        self.pic = p;
    }

    /// Something is stepping or waiting for completion.
    pub fn busy(&self) -> bool {
        self.run.is_some() || self.pending.is_some() || self.waiting_sound
    }

    /// The current agent frame `(anim, frame)` of the previous/current animation.
    pub fn current(&self) -> Option<(usize, usize)> {
        if let Some(r) = self.run {
            return Some((r.anim, r.frame));
        }
        self.prev.map(|p| (p.anim, p.frame))
    }

    pub fn deadline(&self) -> Option<Ms> {
        if self.waiting_sound {
            return self.sound.flatten();
        }
        self.run.map(|r| r.deadline)
    }

    fn start_sound(&mut self, now: Ms, f: &Frame, env: &mut Env, target: bool) {
        let Some(id) = &f.sound else { return };
        if !(env.visible && env.sounds_on) {
            return;
        }
        let Some(s) = env.model.sound(id) else { return };
        env.out.push(Output::SoundStart { sound: id.clone() });
        self.sound = Some(s.duration_ms.map(|d| now + d));
        self.sound_is_target = target;
    }

    fn sound_playing(&self, now: Ms) -> bool {
        match self.sound {
            Some(Some(end)) => end > now,
            Some(None) => true,
            None => false,
        }
    }

    /// R-ANIM-7 / B-ANIM-1: stop cuts. Kills the timer and the sound, keeps the picture;
    /// the stopped animation stays the previous animation.
    pub fn stop(&mut self, now: Ms, env: &mut Env) {
        if let Some(r) = self.run.take() {
            self.prev = Some(Prev {
                anim: r.anim,
                frame: r.frame,
                ended: false,
            });
        }
        self.pending = None;
        self.waiting_sound = false;
        if self.sound_playing(now) {
            env.out.push(Output::SoundStop);
        }
        self.sound = None;
    }

    /// The audio device failed: completion does not wait (R-ANIM-8). Returns true if
    /// the target completed.
    pub fn sound_failed(&mut self) -> bool {
        self.sound = None;
        if self.waiting_sound {
            self.waiting_sound = false;
            return true;
        }
        false
    }

    /// Request animation `x` through the switch rule (R-ANIM-6). With `drop_prev`
    /// (Showing) no exit, return or transition plays. Returns true if `x` completed
    /// at once (no frames).
    pub fn play(&mut self, now: Ms, env: &mut Env, x: usize, drop_prev: bool) -> bool {
        self.stop_timer_only();
        if drop_prev {
            self.prev = None;
            self.last_pose = None;
            return self.start(now, env, x, Role::Target);
        }
        match env.model.family {
            Family::Agent => self.switch_agent(now, env, x),
            Family::Office97 => self.switch_office(now, env, x),
        }
    }

    fn stop_timer_only(&mut self) {
        if let Some(r) = self.run.take() {
            self.prev = Some(Prev {
                anim: r.anim,
                frame: r.frame,
                ended: false,
            });
        }
        self.pending = None;
        self.waiting_sound = false;
    }

    fn switch_agent(&mut self, now: Ms, env: &mut Env, x: usize) -> bool {
        let Some(p) = self.prev else {
            return self.start(now, env, x, Role::Target);
        };
        let a = &env.model.animations[p.anim];
        let valid = p.frame < a.frames.len();
        if valid
            && a.frames[p.frame].exit != Exit::End
            && (!p.ended || a.transition == Transition::Exit)
        {
            // B-SW-A case 1: exit mode, first step at once.
            self.pending = Some(Pending::Target(x));
            self.exit_seen = vec![p.frame];
            match agent_next(&a.frames, p.frame, true, env.draws)
                .filter(|t| *t != p.frame || exit_end_reachable(&a.frames, p.frame))
            {
                None => {
                    self.pending = None;
                    self.prev = Some(Prev { ended: true, ..p });
                    self.start(now, env, x, Role::Target)
                }
                Some(t) => {
                    self.exit_seen.push(t);
                    self.run = Some(Run {
                        anim: p.anim,
                        frame: t,
                        next: 0,
                        exiting: true,
                        deadline: now,
                        due: now,
                        role: Role::LeadIn,
                    });
                    self.show_agent(now, now, env, false);
                    self.after_show(now, env)
                }
            }
        } else if p.ended
            && let Some(rname) = &a.return_animation
            && let Some(r) = env.model.animation_index(rname)
            && r != x
        {
            // Case 2: the return animation first.
            self.pending = Some(Pending::Target(x));
            if self.start(now, env, r, Role::LeadIn) {
                return true;
            }
            false
        } else {
            self.start(now, env, x, Role::Target)
        }
    }

    fn switch_office(&mut self, now: Ms, env: &mut Env, x: usize) -> bool {
        if let Some(p) = self.prev
            && !p.ended
        {
            // B-SW-O step 1: exit mode from the current frame, first step 100 ms later.
            let a = &env.model.animations[p.anim];
            if p.frame < a.frames.len() {
                let next = office_next(&a.frames[p.frame], p.frame, true, env.hour, env.draws);
                self.pending = Some(Pending::O97Check(x, 0));
                self.run = Some(Run {
                    anim: p.anim,
                    frame: p.frame,
                    next,
                    exiting: true,
                    deadline: now + 100,
                    due: now + 100,
                    role: Role::LeadIn,
                });
                return false;
            }
        }
        self.office_check(now, env, x, 0)
    }

    /// B-SW-O steps 2 to 4.
    fn office_check(&mut self, now: Ms, env: &mut Env, x: usize, depth: u8) -> bool {
        let m = env.model;
        let start_pose = start_pose(&m.animations[x].frames);
        if depth < 8
            && let Some(last) = self.last_pose
            && Some(last) != start_pose
            && let Some(o) = &m.office97
        {
            let found = o.transitions.iter().find(|(from, to, _)| {
                *from == last && (Some(*to) == start_pose || Some(*to) == o.rest_pose)
            });
            if let Some((_, _, state)) = found
                && let Some(clip) = pick_member(m, state, env.draws)
            {
                self.pending = Some(Pending::O97Check(x, depth + 1));
                if self.start(now, env, clip, Role::LeadIn) {
                    return true;
                }
                return false;
            }
        }
        self.start(now, env, x, Role::Target)
    }

    /// Start animation `anim` at `now`. Returns true if a target completed at once.
    fn start(&mut self, now: Ms, env: &mut Env, anim: usize, role: Role) -> bool {
        if role == Role::Target {
            self.sound_is_target = false;
        }
        let frames = &env.model.animations[anim].frames;
        match env.model.family {
            Family::Agent => match agent_start(frames, env.draws) {
                None => {
                    self.prev = Some(Prev {
                        anim,
                        frame: 0,
                        ended: true,
                    });
                    if role == Role::LeadIn {
                        self.lead_in_done(now, env)
                    } else {
                        self.segment_end(now, env)
                    }
                }
                Some(f) => {
                    self.run = Some(Run {
                        anim,
                        frame: f,
                        next: 0,
                        exiting: false,
                        deadline: now,
                        due: now,
                        role,
                    });
                    self.show_agent(now, now, env, true);
                    self.after_show(now, env)
                }
            },
            Family::Office97 => {
                self.run = Some(Run {
                    anim,
                    frame: 0,
                    next: 0,
                    exiting: false,
                    deadline: now,
                    due: now,
                    role,
                });
                self.tick(now, env)
            }
        }
    }

    /// Show the agent run's current frame at `now`; `base` is the previous deadline.
    fn show_agent(&mut self, now: Ms, base: Ms, env: &mut Env, _first: bool) {
        let mut r = self.run.expect("run");
        let f = &env.model.animations[r.anim].frames[r.frame];
        if env.visible {
            self.pic = Pic::Frame(r.anim, r.frame);
        }
        let ms = Ms::from(f.ms);
        let d = base + ms;
        r.deadline = if d < now { now + ms } else { d };
        self.run = Some(r);
        let target = r.role == Role::Target;
        let f = f.clone();
        self.start_sound(now, &f, env, target);
    }

    /// Process frames that are due at `now` (several for `ms = 0` frames).
    fn after_show(&mut self, now: Ms, env: &mut Env) -> bool {
        let mut done = false;
        let mut steps = 0;
        while let Some(r) = self.run {
            if r.deadline > now {
                break;
            }
            steps += 1;
            if steps > MAX_STEPS {
                // A cycle of 0 ms frames: yield instead of spinning (A-CORE-4).
                if let Some(r) = &mut self.run {
                    r.deadline = now + 10;
                }
                break;
            }
            done |= self.step(now, env);
            if self.waiting_sound {
                break;
            }
        }
        done
    }

    /// Timer callback. Returns true if the target animation completed.
    pub fn tick(&mut self, now: Ms, env: &mut Env) -> bool {
        if self.waiting_sound {
            if !self.sound_playing(now) {
                self.waiting_sound = false;
                self.sound = None;
                return true;
            }
            return false;
        }
        let Some(r) = self.run else { return false };
        if r.deadline > now {
            return false;
        }
        match env.model.family {
            Family::Agent => self.after_show(now, env),
            Family::Office97 => self.office_tick(now, env),
        }
    }

    /// Agent: one deadline reached.
    fn step(&mut self, now: Ms, env: &mut Env) -> bool {
        let r = self.run.expect("run");
        let frames = &env.model.animations[r.anim].frames;
        let mut next = agent_next(frames, r.frame, r.exiting, env.draws);
        if r.exiting
            && let Some(t) = next
        {
            // A-CORE-6: an exit run that revisits a frame ends only when END is
            // unreachable (a loop without exits must not block the queue forever;
            // random revisits on a path to END terminate as in the original).
            if self.exit_seen.contains(&t) {
                if !exit_end_reachable(frames, r.frame) {
                    next = None;
                }
            } else {
                self.exit_seen.push(t);
            }
        }
        match next {
            None => {
                self.run = None;
                self.prev = Some(Prev {
                    anim: r.anim,
                    frame: r.frame,
                    ended: true,
                });
                if r.role == Role::LeadIn {
                    self.lead_in_done(now, env)
                } else {
                    self.segment_end(now, env)
                }
            }
            Some(t) => {
                self.run = Some(Run { frame: t, ..r });
                self.show_agent(now, r.deadline, env, false);
                false
            }
        }
    }

    fn lead_in_done(&mut self, now: Ms, env: &mut Env) -> bool {
        match self.pending.take() {
            Some(Pending::Target(x)) => self.start(now, env, x, Role::Target),
            Some(Pending::O97Check(x, d)) => self.office_check(now, env, x, d),
            None => false,
        }
    }

    /// The target's last frame expired: complete now or after its sound (B-ANIM-2).
    fn segment_end(&mut self, now: Ms, _env: &mut Env) -> bool {
        if self.sound_is_target && self.sound_playing(now) {
            self.waiting_sound = true;
            if self.sound == Some(None) {
                // Unknown end: do not wait.
                self.waiting_sound = false;
                return true;
            }
            return false;
        }
        true
    }

    /// B-O97-3 tick.
    fn office_tick(&mut self, now: Ms, env: &mut Env) -> bool {
        let mut r = self.run.expect("run");
        if r.due < now {
            r.due = now;
        }
        let frames = &env.model.animations[r.anim].frames;
        let mut drawn: Option<usize> = None;
        let target = r.role == Role::Target;
        let mut steps = 0;
        loop {
            if r.next >= frames.len() {
                if r.due <= now {
                    if let Some(d) = drawn
                        && env.visible
                    {
                        self.pic = Pic::Frame(r.anim, d);
                    }
                    self.run = None;
                    self.prev = Some(Prev {
                        anim: r.anim,
                        frame: r.frame,
                        ended: true,
                    });
                    return if r.role == Role::LeadIn {
                        self.lead_in_done(now, env)
                    } else {
                        self.segment_end(now, env)
                    };
                }
                break;
            }
            steps += 1;
            if r.due > now + 14 || steps > MAX_STEPS {
                break;
            }
            let i = r.next;
            let f = &frames[i];
            self.start_sound(now, &f.clone(), env, target);
            let mut ms = Ms::from(f.ms);
            if r.exiting && ms > 99 {
                ms = 100;
            }
            if f.ms == 0 {
                if f.pose.is_some() && env.visible {
                    self.last_pose = f.pose;
                }
            } else {
                drawn = Some(i);
                self.last_pose = f.pose;
            }
            r.due += ms;
            r.frame = i;
            r.next = office_next(f, i, r.exiting, env.hour, env.draws);
        }
        if let Some(d) = drawn
            && env.visible
        {
            self.pic = Pic::Frame(r.anim, d);
        }
        r.deadline = now + r.due.saturating_sub(now).max(14);
        self.run = Some(r);
        false
    }
}

/// B-SW-O step 2: X's start pose.
pub fn start_pose(frames: &[Frame]) -> Option<u32> {
    let f0 = frames.first()?;
    if f0.ms == 0 && frames.get(1).is_some_and(|f| f.pose.is_none()) {
        return None;
    }
    f0.pose
}

/// R-STATE-1 / R-STATE-2: a uniformly chosen available member of a state.
pub fn pick_member(m: &CharacterModel, state: &str, draws: &mut dyn Draws) -> Option<usize> {
    let members = crate::fallback::state_members(m, state)?;
    let avail: Vec<usize> = members
        .iter()
        .filter_map(|n| m.animation_index(n))
        .filter(|&i| m.animations[i].available)
        .collect();
    if avail.is_empty() {
        return None;
    }
    Some(avail[draws.draw(avail.len() as u32) as usize])
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::draws::ScriptedDraws;
    use crate::model::{Animation, Sound};

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

    struct H {
        m: CharacterModel,
        p: Player,
        d: ScriptedDraws,
        out: Vec<Output>,
        visible: bool,
    }

    impl H {
        fn new(m: CharacterModel, draws: Vec<u32>) -> H {
            H {
                m,
                p: Player::new(),
                d: ScriptedDraws::new(draws),
                out: Vec::new(),
                visible: true,
            }
        }
        fn play(&mut self, now: Ms, name: &str) -> bool {
            let x = self.m.animation_index(name).unwrap();
            let mut env = Env {
                model: &self.m,
                draws: &mut self.d,
                visible: self.visible,
                sounds_on: true,
                hour: 12,
                out: &mut self.out,
            };
            self.p.play(now, &mut env, x, false)
        }
        fn stop(&mut self, now: Ms) {
            let mut env = Env {
                model: &self.m,
                draws: &mut self.d,
                visible: self.visible,
                sounds_on: true,
                hour: 12,
                out: &mut self.out,
            };
            self.p.stop(now, &mut env)
        }
        /// Fake clock: run until completion or `limit`, recording (time, picture).
        fn run(&mut self, limit: Ms) -> (Option<Ms>, Vec<(Ms, Pic)>) {
            let mut v = vec![];
            let mut last = None;
            while let Some(d) = self.p.deadline() {
                if d > limit {
                    break;
                }
                let mut env = Env {
                    model: &self.m,
                    draws: &mut self.d,
                    visible: self.visible,
                    sounds_on: true,
                    hour: 12,
                    out: &mut self.out,
                };
                let done = self.p.tick(d, &mut env);
                if Some(self.p.picture()) != last {
                    last = Some(self.p.picture());
                    v.push((d, self.p.picture()));
                }
                if done {
                    return (Some(d), v);
                }
            }
            (None, v)
        }
    }

    fn agent(anims: Vec<Animation>) -> CharacterModel {
        let mut m = CharacterModel::new("t", Family::Agent, (10, 10));
        m.animations = anims;
        m
    }

    #[test]
    fn r_anim_2_frame_clock() {
        let mut h = H::new(
            agent(vec![anim("A", vec![fr(100), fr(200), fr(300)])]),
            vec![],
        );
        assert!(!h.play(0, "A"));
        assert_eq!(h.p.picture(), Pic::Frame(0, 0));
        let (done, v) = h.run(10_000);
        assert_eq!(done, Some(600));
        assert_eq!(v[0], (100, Pic::Frame(0, 1)));
        assert_eq!(v[1], (300, Pic::Frame(0, 2)));
    }

    #[test]
    fn r_anim_2_late_frame_no_catch_up() {
        let mut h = H::new(
            agent(vec![anim("A", vec![fr(100), fr(200), fr(300)])]),
            vec![],
        );
        h.play(0, "A");
        // Frame 2 (200 ms) is shown late at t = 800: next deadline 1000.
        let mut env = Env {
            model: &h.m,
            draws: &mut h.d,
            visible: true,
            sounds_on: true,
            hour: 12,
            out: &mut h.out,
        };
        h.p.tick(800, &mut env);
        assert_eq!(h.p.picture(), Pic::Frame(0, 1));
        assert_eq!(h.p.deadline(), Some(1000));
    }

    #[test]
    fn b_seq_a_r_anim_4_branch_vector() {
        let mut f = fr(100);
        f.branches = vec![
            Branch::Prob { target: 5, p: 30 },
            Branch::Prob { target: 7, p: 50 },
        ];
        let mut frames = vec![f];
        frames.extend((1..10).map(|_| fr(100)));
        for (r, want) in [(29, 5), (30, 7), (79, 7), (80, 1)] {
            let mut d = ScriptedDraws::new([r]);
            assert_eq!(agent_next(&frames, 0, false, &mut d), Some(want), "r={r}");
            assert_eq!(d.taken, 1);
        }
    }

    #[test]
    fn r_anim_5_invisible_frames() {
        let inv = Frame {
            branches: vec![Branch::Prob { target: 3, p: 100 }],
            sound: Some("s".into()),
            ..Default::default()
        };
        let mut m = agent(vec![anim("A", vec![inv, fr(100), fr(100), fr(50)])]);
        m.sounds.push(Sound {
            id: "s".into(),
            duration_ms: Some(10),
        });
        let mut h = H::new(m, vec![0]);
        h.play(0, "A");
        // Frame 0 is walked through, never shown, its sound not started.
        assert_eq!(h.p.picture(), Pic::Frame(0, 3));
        assert!(h.out.is_empty());
        // 32-hop limit on a cycle of invisible frames.
        let c = Frame {
            branches: vec![Branch::Prob { target: 0, p: 100 }],
            ..Default::default()
        };
        let frames = vec![c, fr(1)];
        let mut d = ScriptedDraws::new([]);
        assert_eq!(agent_next(&frames, 0, false, &mut d), Some(0));
        assert_eq!(d.taken, 32);
    }

    #[test]
    fn r_anim_5_zero_ms_frame_with_layers_is_shown() {
        let mut z = fr(0);
        z.sound = Some("s".into());
        let mut m = agent(vec![anim("A", vec![fr(100), z, fr(100)])]);
        m.sounds.push(Sound {
            id: "s".into(),
            duration_ms: Some(10),
        });
        let mut h = H::new(m, vec![]);
        h.play(0, "A");
        let (done, v) = h.run(1000);
        assert_eq!(h.out, vec![Output::SoundStart { sound: "s".into() }]);
        // Frame 1 is current at 100 (sound starts), frame 2 follows at the same deadline.
        assert_eq!(v, vec![(100, Pic::Frame(0, 2))]);
        assert_eq!(done, Some(200));
    }

    #[test]
    fn b_anim_2_r_anim_3_completion_waits_for_sound() {
        let mut f0 = fr(100);
        f0.sound = Some("s".into());
        let mut m = agent(vec![anim("A", vec![f0, fr(100)])]);
        m.sounds.push(Sound {
            id: "s".into(),
            duration_ms: Some(500),
        });
        let mut h = H::new(m.clone(), vec![]);
        h.play(0, "A");
        assert_eq!(h.run(10_000).0, Some(500));
        // R-ANIM-8: a failed device does not wait.
        let mut h = H::new(m, vec![]);
        h.play(0, "A");
        h.run(250);
        assert!(h.p.busy());
        assert!(h.p.sound_failed());
    }

    #[test]
    fn r_anim_8_sound_gating() {
        let mut f0 = fr(100);
        f0.sound = Some("s".into());
        let mut m = agent(vec![anim("A", vec![f0])]);
        m.sounds.push(Sound {
            id: "s".into(),
            duration_ms: Some(50),
        });
        let mut h = H::new(m, vec![]);
        h.visible = false;
        h.play(0, "A");
        assert!(h.out.is_empty());
        // Hidden: frames are stepped but not drawn.
        assert_eq!(h.p.picture(), Pic::Empty);
        assert_eq!(h.run(1000).0, Some(100));
    }

    fn exit_char() -> CharacterModel {
        // Loop: 0 -> 1 -> 2 -> back to 1 (100 %); exits from 1 and 2 jump to 3.
        let mut f1 = fr(100);
        f1.exit = Exit::To(3);
        let mut f2 = fr(100);
        f2.exit = Exit::To(3);
        f2.branches = vec![Branch::Prob { target: 1, p: 100 }];
        let mut f3 = fr(100);
        f3.exit = Exit::None;
        let mut f0 = fr(100);
        f0.exit = Exit::End;
        let lp = anim("Loop", vec![f0, f1, f2, f3]);
        let mut done = anim("Done", vec![fr(100)]);
        done.transition = Transition::Return;
        done.return_animation = Some("Back".into());
        let back = anim("Back", vec![fr(100)]);
        let x = anim("X", vec![fr(100)]);
        let mut ex = anim(
            "Ex",
            vec![
                {
                    let mut f = fr(100);
                    f.exit = Exit::To(1);
                    f
                },
                fr(100),
            ],
        );
        ex.transition = Transition::Exit;
        agent(vec![lp, done, back, x, ex])
    }

    #[test]
    fn b_sw_a_r_anim_7_b_anim_1_loop_stopped_resumes_through_exit() {
        let mut h = H::new(exit_char(), vec![0; 10]);
        h.play(0, "Loop");
        // R-Q-6: a 100 % backward branch never completes.
        assert_eq!(h.run(1000).0, None);
        h.stop(1000);
        let cur = h.p.current().unwrap();
        assert!(cur.1 == 1 || cur.1 == 2);
        // Next request: first exit step at once (frame 3), then X.
        h.play(1000, "X");
        assert_eq!(h.p.picture(), Pic::Frame(0, 3));
        let (done, v) = h.run(5000);
        assert_eq!(v, vec![(1100, Pic::Frame(3, 0))]);
        assert_eq!(done, Some(1200));
    }

    #[test]
    fn b_sw_a_return_after_finished() {
        let mut h = H::new(exit_char(), vec![]);
        h.play(0, "Done");
        assert_eq!(h.run(1000).0, Some(100));
        h.play(100, "X");
        // Return animation Back plays first.
        assert_eq!(h.p.picture(), Pic::Frame(2, 0));
        let (done, v) = h.run(5000);
        assert_eq!(v, vec![(200, Pic::Frame(3, 0))]);
        assert_eq!(done, Some(300));
        // Return animation equal to X: X simply plays.
        let mut h = H::new(exit_char(), vec![]);
        h.play(0, "Done");
        h.run(1000);
        h.play(100, "Back");
        assert_eq!(h.run(5000).0, Some(200));
    }

    #[test]
    fn b_sw_a_r_anim_6_transition_exit_and_showing_drops() {
        let mut h = H::new(exit_char(), vec![]);
        h.play(0, "Ex");
        // Ex frame 0 exits to 1: normal play 0 -> 1 -> END at 200.
        assert_eq!(h.run(1000).0, Some(200));
        // Finished with transition = exit: exit mode from the last frame (exit None:
        // normal sequencing -> END), then X at once.
        h.play(200, "X");
        assert_eq!(h.p.picture(), Pic::Frame(3, 0));
        // Showing drops the previous animation.
        let mut h = H::new(exit_char(), vec![0; 4]);
        h.play(0, "Loop");
        h.run(250);
        h.stop(250);
        let x = h.m.animation_index("X").unwrap();
        let mut env = Env {
            model: &h.m,
            draws: &mut h.d,
            visible: true,
            sounds_on: true,
            hour: 12,
            out: &mut h.out,
        };
        h.p.play(250, &mut env, x, true);
        assert_eq!(h.p.picture(), Pic::Frame(3, 0));
    }

    #[test]
    fn b_sw_a_agent15_exit_end_plays_return() {
        // 1.5 characters: exit = "end" on every frame; a stopped animation is not
        // exited and an unfinished one is replaced.
        let mut f = fr(100);
        f.exit = Exit::End;
        let mut a = anim("A", vec![f.clone(), f]);
        a.return_animation = Some("R".into());
        let m = agent(vec![a, anim("R", vec![fr(100)]), anim("X", vec![fr(100)])]);
        let mut h = H::new(m, vec![]);
        h.play(0, "A");
        h.run(10_000);
        h.play(200, "X");
        assert_eq!(h.p.picture(), Pic::Frame(1, 0));
    }

    fn o97(anims: Vec<Animation>) -> CharacterModel {
        let mut m = CharacterModel::new("o", Family::Office97, (10, 10));
        m.animations = anims;
        m
    }

    fn pf(ms: u32, pose: u32) -> Frame {
        Frame {
            ms,
            has_picture: true,
            pose: Some(pose),
            ..Default::default()
        }
    }

    #[test]
    fn b_o97_3_coalescing_vector() {
        let mut h = H::new(
            o97(vec![anim("A", vec![pf(5, 1), pf(5, 2), pf(100, 3)])]),
            vec![],
        );
        h.play(0, "A");
        assert_eq!(h.p.picture(), Pic::Frame(0, 2));
        assert_eq!(h.p.deadline(), Some(110));
        assert_eq!(h.run(1000).0, Some(110));
    }

    #[test]
    fn b_o97_3_minimum_tick_and_late_reset() {
        let mut h = H::new(
            o97(vec![anim("A", vec![pf(20, 1), pf(20, 2), pf(20, 3)])]),
            vec![],
        );
        h.play(0, "A");
        // Frame 0 consumed (due 20 > 14): next tick at max(20, 14) = 20.
        assert_eq!(h.p.deadline(), Some(20));
        let mut env = Env {
            model: &h.m,
            draws: &mut h.d,
            visible: true,
            sounds_on: true,
            hour: 12,
            out: &mut h.out,
        };
        // Late tick at 100: due resets to 100, frame 1 consumed, next at 120.
        h.p.tick(100, &mut env);
        assert_eq!(h.p.picture(), Pic::Frame(0, 1));
        assert_eq!(h.p.deadline(), Some(120));
    }

    #[test]
    fn zero_ms_cycles_do_not_spin() {
        let mut a = fr(0);
        a.branches = vec![Branch::Prob { target: 0, p: 100 }];
        let mut h = H::new(agent(vec![anim("A", vec![a])]), vec![0; 5000]);
        h.play(0, "A");
        assert_eq!(h.p.deadline(), Some(10));
        let mut o = pf(0, 1);
        o.branches = vec![Branch::Prob {
            target: 0,
            p: 32768,
        }];
        let mut h = H::new(o97(vec![anim("A", vec![o])]), vec![]);
        h.play(0, "A");
        assert_eq!(h.p.deadline(), Some(14));
    }

    #[test]
    fn a_core_6_random_revisit_on_exit_path_reaches_end() {
        // Exit path 1 -> 2; frame 2 branches back to 1 with 50 % or falls through to
        // END. Draws: back (revisit), back (revisit), then fall through.
        let mut f0 = fr(100);
        f0.branches = vec![Branch::Prob { target: 0, p: 100 }];
        f0.exit = Exit::To(1);
        let mut f2 = fr(100);
        f2.branches = vec![Branch::Prob { target: 1, p: 50 }];
        let m = agent(vec![
            anim("L", vec![f0, fr(100), f2]),
            anim("X", vec![fr(100)]),
        ]);
        let mut h = H::new(m, vec![0, 0, 10, 10, 90]);
        h.play(0, "L");
        h.run(250);
        h.stop(250);
        h.play(250, "X");
        let (_, v) = h.run(10_000);
        let frames: Vec<Pic> = v.iter().map(|x| x.1).collect();
        // 1 (first exit step at once), 2, 1, 2, 1, 2, then X.
        assert_eq!(
            frames,
            vec![
                Pic::Frame(0, 2),
                Pic::Frame(0, 1),
                Pic::Frame(0, 2),
                Pic::Frame(0, 1),
                Pic::Frame(0, 2),
                Pic::Frame(1, 0)
            ]
        );
        assert!(exit_end_reachable(&h.m.animations[0].frames, 1));
        // A loop without exits: END unreachable.
        let mut g = fr(100);
        g.branches = vec![Branch::Prob { target: 0, p: 100 }];
        assert!(!exit_end_reachable(&[g], 0));
    }

    #[test]
    fn b_o97_2_zero_frames_and_blank() {
        let mut z = pf(0, 7);
        z.sound = Some("s".into());
        let blank = Frame {
            ms: 50,
            ..Default::default()
        };
        let mut m = o97(vec![anim("A", vec![z, pf(30, 1), blank])]);
        m.sounds.push(Sound {
            id: "s".into(),
            duration_ms: Some(1),
        });
        let mut h = H::new(m, vec![]);
        h.play(0, "A");
        // The 0 ms frame starts its sound and is not drawn.
        assert_eq!(h.out, vec![Output::SoundStart { sound: "s".into() }]);
        assert_eq!(h.p.picture(), Pic::Frame(0, 1));
        h.run(1000);
        // A frame without pose blanks the character and the logical pose.
        assert_eq!(h.p.picture(), Pic::Frame(0, 2));
        assert_eq!(h.p.last_pose, None);
    }

    #[test]
    fn b_o97_1_branches() {
        let mut f = pf(10, 1);
        f.branches = vec![
            Branch::Prob { target: 3, p: 0 },
            Branch::Prob {
                target: 2,
                p: 16384,
            },
            Branch::Daypart {
                target: 4,
                period: 1,
            },
        ];
        let frames = [f.clone()];
        let mut d = ScriptedDraws::new([16383]);
        assert_eq!(office_next(&frames[0], 0, false, 12, &mut d), 2);
        let mut d = ScriptedDraws::new([16384]);
        // Not taken; daypart 12 h = period 1 taken.
        assert_eq!(office_next(&frames[0], 0, false, 12, &mut d), 4);
        assert_eq!(d.taken, 1);
        let mut d = ScriptedDraws::new([20000]);
        assert_eq!(office_next(&frames[0], 0, false, 3, &mut d), 1);
        // p >= 32768 needs no draw; host never taken.
        let mut g = pf(10, 1);
        g.branches = vec![
            Branch::Host {
                target: 5,
                value: 0,
            },
            Branch::Prob {
                target: 6,
                p: 32768,
            },
        ];
        let mut d = ScriptedDraws::new([]);
        assert_eq!(office_next(&g, 0, false, 12, &mut d), 6);
        assert_eq!(d.taken, 0);
        // Exit mode: forward taken (p = 0 included), backward skipped.
        let mut e = pf(10, 1);
        e.branches = vec![
            Branch::Prob {
                target: 0,
                p: 30000,
            },
            Branch::Prob { target: 9, p: 0 },
        ];
        assert_eq!(office_next(&e, 3, true, 12, &mut d), 9);
        assert_eq!(daypart(5), 0);
        assert_eq!(daypart(9), 0);
        assert_eq!(daypart(10), 1);
        assert_eq!(daypart(18), 2);
        assert_eq!(daypart(22), 3);
        assert_eq!(daypart(4), 3);
    }

    #[test]
    fn b_sw_o_exit_and_transitions() {
        // Loop 0 <-> 1 with a p = 0 exit branch to 2 on frame 1.
        let mut f1 = pf(200, 2);
        f1.branches = vec![
            Branch::Prob { target: 2, p: 0 },
            Branch::Prob {
                target: 0,
                p: 32768,
            },
        ];
        let lp = anim("Loop", vec![pf(200, 1), f1, pf(300, 5)]);
        let x = anim("X", vec![pf(100, 9)]);
        let clip = anim("Clip.1", vec![pf(100, 8)]);
        let mut m = o97(vec![lp, x, clip]);
        m.states.push(("T".into(), vec!["Clip.1".into()]));
        m.office97 = Some(crate::model::Office97 {
            rest_pose: Some(8),
            idle: "Idle".into(),
            transitions: vec![(5, 8, "T".into())],
        });
        let mut h = H::new(m, vec![]);
        h.play(0, "Loop");
        assert_eq!(h.run(300).0, None);
        h.stop(300);
        // Current: frame 1 (shown at 200). Exit step 100 ms later.
        h.play(300, "X");
        assert_eq!(h.p.deadline(), Some(400));
        let (done, v) = h.run(5000);
        // Frame 2 (pose 5) at 400 lasting 100 (capped), then clip T (5 -> rest 8) at
        // 500, then X at 600, done at 700.
        assert_eq!(
            v,
            vec![
                (400, Pic::Frame(0, 2)),
                (500, Pic::Frame(2, 0)),
                (600, Pic::Frame(1, 0))
            ]
        );
        assert_eq!(done, Some(700));
    }

    #[test]
    fn b_sw_o_start_pose() {
        let blank = Frame {
            ms: 50,
            ..Default::default()
        };
        assert_eq!(start_pose(&[pf(0, 3), blank.clone()]), None);
        assert_eq!(start_pose(&[pf(0, 3), pf(10, 4)]), Some(3));
        assert_eq!(start_pose(&[pf(10, 3), blank]), Some(3));
    }

    #[test]
    fn r_state_1_uniform_member_and_r_state_2() {
        let mut m = agent(vec![anim("A", vec![fr(1)]), anim("B", vec![fr(1)])]);
        m.states.push(("S".into(), vec!["A".into(), "B".into()]));
        m.states.push(("Empty".into(), vec![]));
        m.states.push(("Gone".into(), vec!["Missing".into()]));
        let mut d = ScriptedDraws::new([1, 0]);
        assert_eq!(pick_member(&m, "s", &mut d), Some(1));
        assert_eq!(pick_member(&m, "S", &mut d), Some(0));
        assert_eq!(pick_member(&m, "Empty", &mut d), None);
        assert_eq!(pick_member(&m, "Gone", &mut d), None);
        assert_eq!(pick_member(&m, "Nope", &mut d), None);
        m.animations[1].available = false;
        let mut d = ScriptedDraws::new([5]);
        assert_eq!(pick_member(&m, "S", &mut d), Some(0));
        assert_eq!(d.taken, 1);
    }
}
