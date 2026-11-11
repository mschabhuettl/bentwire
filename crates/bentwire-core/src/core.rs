//! The request engine: queue, phases, idle, input, menu, gallery (spec section 3).

use crate::api::*;

/// The deterministic core. See [`crate::api`] for the driving contract.
pub struct Core {
    now: Ms,
    init: CoreInit,
}

impl Core {
    /// Creates the core. Nothing happens until [`Core::start`].
    pub fn new(init: CoreInit) -> Self {
        Core { now: 0, init }
    }

    /// Startup: B-CHAR-4 selection, B-CFG-3 first-run position and the initial Show.
    pub fn start(&mut self, now: Ms) -> Vec<Output> {
        self.now = now;
        Vec::new()
    }

    /// Processes one input at time `now` (after processing every deadline <= `now`).
    pub fn handle(&mut self, now: Ms, event: Event) -> Vec<Output> {
        let _ = event;
        self.tick(now)
    }

    /// Processes every deadline that is due at `now` (late ones are processed late).
    pub fn tick(&mut self, now: Ms) -> Vec<Output> {
        self.now = self.now.max(now);
        Vec::new()
    }

    /// Fake-clock stepping: visits each deadline up to `target` in turn, then sets the
    /// clock to `target`.
    pub fn advance_to(&mut self, target: Ms) -> Vec<Output> {
        let mut out = Vec::new();
        while let Some(d) = self.next_deadline() {
            if d > target {
                break;
            }
            out.extend(self.tick(d));
        }
        out.extend(self.tick(target));
        out
    }

    /// The earliest pending timer, if any.
    pub fn next_deadline(&self) -> Option<Ms> {
        None
    }

    /// Replaces the random source (`Test1.SetDraws`, `--seed`).
    pub fn set_draws(&mut self, draws: Box<dyn Draws>) {
        self.init.draws = draws;
    }

    /// Current scene.
    pub fn scene(&self) -> Scene {
        Scene {
            character: None,
            agent_balloon: None,
            office_balloon: None,
            menu: None,
            tray: TrayScene {
                icon_character: None,
                title: String::new(),
                tooltip: String::new(),
                menu: Vec::new(),
            },
            outputs: self.init.outputs.clone(),
        }
    }

    /// Current properties.
    pub fn properties(&self) -> Properties {
        Properties {
            character: String::new(),
            name: String::new(),
            description: String::new(),
            visible: false,
            left: 0,
            top: 0,
            width: 0,
            height: 0,
            idle_on: self.init.config.idle,
            move_cause: 0,
            visibility_cause: 0,
            balloon_enabled: false,
            animation_names: Vec::new(),
        }
    }
}
