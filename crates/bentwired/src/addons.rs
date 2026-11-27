//! Add-ons (B-ADDON-3 to B-ADDON-5): the enabled ones, and the bridges among them, which run
//! as child processes of the daemon. No add-on code runs inside the daemon.

use bentwire_character::addon::{self, Addon, Kind};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const FIRST_DELAY: Duration = Duration::from_secs(1);
const MAX_DELAY: Duration = Duration::from_secs(60);
/// How long a stopping daemon waits for its bridges after SIGTERM before SIGKILL.
const STOP_WAIT: Duration = Duration::from_secs(2);

/// The enabled add-ons in `dir` (the names in `list`) whose manifest loads, sorted by name;
/// add-ons that do not load are logged.
pub fn enabled(dir: &Path, list: &Path) -> Vec<(PathBuf, Addon)> {
    let names = addon::read_enabled(list).unwrap_or_else(|e| {
        log::warn!("{e}; no add-on enabled");
        Vec::new()
    });
    addon::installed(dir)
        .into_iter()
        .filter_map(|(p, a)| match a {
            Ok(a) => names.contains(&a.name).then_some((p, a)),
            Err(e) => {
                log::warn!("skipping add-on {e}");
                None
            }
        })
        .collect()
}

struct Bridge {
    name: String,
    dir: PathBuf,
    command: Vec<String>,
    child: Option<Child>,
    started: Instant,
    /// The wait before the next restart.
    delay: Duration,
    restart_at: Option<Instant>,
}

impl Bridge {
    /// Must run on the main thread: PR_SET_PDEATHSIG fires when the thread that forked
    /// the child ends.
    fn spawn(&mut self) {
        // A relative program path with a `/` is taken from the add-on folder, a bare name
        // from PATH.
        let prog = Path::new(&self.command[0]);
        let prog = if prog.is_relative() && self.command[0].contains('/') {
            self.dir.join(prog)
        } else {
            prog.to_path_buf()
        };
        let parent = std::process::id() as libc::pid_t;
        let mut cmd = Command::new(prog);
        cmd.args(&self.command[1..])
            .current_dir(&self.dir)
            .stdin(Stdio::null())
            .process_group(0);
        // SAFETY: only async-signal-safe calls between fork and exec.
        unsafe {
            cmd.pre_exec(move || {
                // The daemon blocks the signals it reads through signalfd; the bridge
                // starts with none blocked, and gets SIGTERM when the daemon dies.
                let mut set: libc::sigset_t = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                libc::sigprocmask(libc::SIG_SETMASK, &set, std::ptr::null_mut());
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0
                    || libc::getppid() != parent
                {
                    return Err(std::io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        self.started = Instant::now();
        self.restart_at = None;
        match cmd.spawn() {
            Ok(c) => {
                log::info!("bridge {} started (pid {})", self.name, c.id());
                self.child = Some(c);
            }
            Err(e) => {
                log::warn!("bridge {}: {e}", self.name);
                self.schedule();
            }
        }
    }

    /// After an exit: restart after the delay, which doubles up to MAX_DELAY and starts
    /// again at FIRST_DELAY when the bridge ran for MAX_DELAY or longer.
    fn schedule(&mut self) {
        if self.started.elapsed() >= MAX_DELAY {
            self.delay = FIRST_DELAY;
        }
        log::warn!(
            "bridge {} restarts in {} s",
            self.name,
            self.delay.as_secs()
        );
        self.restart_at = Some(Instant::now() + self.delay);
        self.delay = (self.delay * 2).min(MAX_DELAY);
    }
}

/// Signal the process group of a bridge (its own, `process_group(0)`).
fn signal(c: &Child, sig: libc::c_int) {
    // SAFETY: plain kill(2); the child is not reaped yet, so its group id is still ours.
    unsafe {
        libc::kill(-(c.id() as libc::pid_t), sig);
    }
}

/// The running bridges, by add-on name.
#[derive(Default)]
pub struct Bridges {
    list: Vec<Bridge>,
    /// Bridges that were disabled, after SIGTERM, until they are reaped.
    stopping: Vec<Child>,
}

impl Bridges {
    /// Start the enabled bridges that do not run and stop the ones no longer enabled.
    pub fn reconcile(&mut self, enabled: Vec<(PathBuf, Addon)>) {
        let want: Vec<(PathBuf, Addon)> = enabled
            .into_iter()
            .filter(|(_, a)| a.kind == Kind::Bridge)
            .collect();
        let same = |b: &Bridge, (dir, a): &(PathBuf, Addon)| {
            b.name == a.name && b.dir == *dir && b.command == a.command
        };
        let (keep, gone): (Vec<Bridge>, Vec<Bridge>) = std::mem::take(&mut self.list)
            .into_iter()
            .partition(|b| want.iter().any(|w| same(b, w)));
        for b in gone {
            if let Some(c) = b.child {
                log::info!("stopping bridge {}", b.name);
                signal(&c, libc::SIGTERM);
                self.stopping.push(c);
            }
        }
        self.list = keep;
        for w in want {
            if self.list.iter().any(|b| same(b, &w)) {
                continue;
            }
            let (dir, a) = w;
            let mut b = Bridge {
                name: a.name,
                dir,
                command: a.command,
                child: None,
                started: Instant::now(),
                delay: FIRST_DELAY,
                restart_at: None,
            };
            b.spawn();
            self.list.push(b);
        }
    }

    /// SIGCHLD: collect the bridges that exited and schedule their restart.
    pub fn reap(&mut self) {
        for b in &mut self.list {
            if let Some(c) = &mut b.child
                && let Ok(Some(st)) = c.try_wait()
            {
                log::warn!("bridge {} exited ({st})", b.name);
                b.child = None;
                b.schedule();
            }
        }
        self.stopping
            .retain_mut(|c| matches!(c.try_wait(), Ok(None)));
    }

    /// The earliest pending restart.
    pub fn next_restart(&self) -> Option<Instant> {
        self.list.iter().filter_map(|b| b.restart_at).min()
    }

    /// Start the bridges whose restart is due.
    pub fn restart_due(&mut self) {
        let now = Instant::now();
        for b in &mut self.list {
            if b.child.is_none() && b.restart_at.is_some_and(|t| t <= now) {
                b.spawn();
            }
        }
    }

    /// Daemon exit: SIGTERM to every bridge, SIGKILL to those still running after
    /// STOP_WAIT.
    pub fn stop_all(&mut self) {
        let mut all: Vec<Child> = self
            .list
            .drain(..)
            .filter_map(|b| b.child)
            .chain(self.stopping.drain(..))
            .collect();
        for c in &all {
            signal(c, libc::SIGTERM);
        }
        let t = Instant::now();
        loop {
            all.retain_mut(|c| matches!(c.try_wait(), Ok(None)));
            if all.is_empty() || t.elapsed() >= STOP_WAIT {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        for c in &mut all {
            signal(c, libc::SIGKILL);
            let _ = c.wait();
        }
    }
}

impl Drop for Bridges {
    /// Also on an error return of the event loop.
    fn drop(&mut self) {
        self.stop_all();
    }
}
