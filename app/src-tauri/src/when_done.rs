//! "When done": sleep or shut the PC down once the queue and post-processing are idle.
//!
//! The decision (`should_fire`) and the countdown are pure; the power call goes through
//! `PowerAction`, so tests use a recording fake and never sleep or shut down a real PC.

use serde::{Deserialize, Serialize};
use std::io;
use std::time::{Duration, Instant};

/// Seconds between the queue going idle and the power action.
pub const COUNTDOWN_SECS: u32 = 60;
/// Idle must hold this long before the countdown starts, so a post-processing run that is
/// created a moment after the last download completes is seen first.
pub const SETTLE: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    Nothing,
    Sleep,
    Shutdown,
}

/// What the queue and post-processing are doing right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Idle {
    pub running_or_queued: usize,
    /// A run is queued, running, or waiting on a re-download.
    pub postproc_busy: bool,
    /// A run needs an archive password.
    pub postproc_waiting_on_user: bool,
    /// The schedule's window is closed and it paused downloads.
    pub held_by_schedule: bool,
}

impl Idle {
    pub fn busy(&self) -> bool {
        self.running_or_queued > 0 || self.postproc_busy
    }
}

/// `armed` becomes true once anything has run after the mode was chosen.
pub fn should_fire(mode: Mode, armed: bool, idle: &Idle) -> bool {
    mode != Mode::Nothing && armed && !idle.busy() && !idle.postproc_waiting_on_user && !idle.held_by_schedule
}

/// The 60 s countdown.
#[derive(Clone, Copy, Debug, Default)]
pub struct Countdown {
    started: Option<Instant>,
}

impl Countdown {
    pub fn start(&mut self, now: Instant) {
        self.started = Some(now);
    }

    /// Whole seconds left (60 at the start, 0 when it is time), or None when not counting.
    pub fn remaining(&self, now: Instant) -> Option<u32> {
        let s = self.started?;
        let gone = now.saturating_duration_since(s).as_secs();
        Some(COUNTDOWN_SECS.saturating_sub(u32::try_from(gone).unwrap_or(u32::MAX)))
    }

    pub fn cancel(&mut self) {
        self.started = None;
    }

    pub fn active(&self) -> bool {
        self.started.is_some()
    }
}

/// The payload of `haul:when-done`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Status {
    pub mode: Mode,
    pub countdown: Option<u32>,
}

/// What a step decided.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    Nothing,
    /// The countdown just started: show the window.
    Started,
    /// Time is up: carry out this mode (already reset to Nothing).
    Fire(Mode),
}

/// The mode, whether anything has run since it was chosen, and the countdown. Never saved.
#[derive(Debug, Default)]
pub struct WhenDone {
    mode: Mode,
    armed: bool,
    idle_since: Option<Instant>,
    countdown: Countdown,
}

impl WhenDone {
    pub fn status(&self, now: Instant) -> Status {
        Status { mode: self.mode, countdown: self.countdown.remaining(now) }
    }

    /// Choosing a mode never fires at once: something must run and finish first.
    pub fn set_mode(&mut self, mode: Mode) {
        self.mode = mode;
        self.armed = false;
        self.idle_since = None;
        self.countdown.cancel();
    }

    /// A download started (seen as an event, so a short one is never missed).
    pub fn on_started(&mut self) {
        if self.mode != Mode::Nothing {
            self.armed = true;
        }
        self.idle_since = None;
        self.countdown.cancel();
    }

    /// Cancel: back to Nothing.
    pub fn cancel(&mut self) {
        self.set_mode(Mode::Nothing);
    }

    /// Do it now: the mode to carry out, if one is chosen; resets to Nothing.
    pub fn take(&mut self) -> Option<Mode> {
        let m = self.mode;
        self.set_mode(Mode::Nothing);
        (m != Mode::Nothing).then_some(m)
    }

    /// Runs once a second (and on events) with what the queue is doing.
    /// New work during the countdown stops it; the mode stays, so it starts again once idle.
    pub fn step(&mut self, idle: &Idle, now: Instant) -> Step {
        if self.mode == Mode::Nothing {
            return Step::Nothing;
        }
        if idle.busy() {
            self.on_started();
            return Step::Nothing;
        }
        if self.countdown.active() {
            if idle.postproc_waiting_on_user || idle.held_by_schedule {
                self.countdown.cancel();
                self.idle_since = None;
                return Step::Nothing;
            }
            if self.countdown.remaining(now) == Some(0) {
                return self.take().map_or(Step::Nothing, Step::Fire);
            }
            return Step::Nothing;
        }
        if !should_fire(self.mode, self.armed, idle) {
            self.idle_since = None;
            return Step::Nothing;
        }
        let since = *self.idle_since.get_or_insert(now);
        if now.saturating_duration_since(since) < SETTLE {
            return Step::Nothing;
        }
        self.countdown.start(now);
        Step::Started
    }
}

/// Sleep and shut down. Tests use a fake.
pub trait PowerAction: Send + Sync {
    fn sleep(&self) -> io::Result<()>;
    fn shutdown(&self) -> io::Result<()>;
}

/// The real Windows calls.
pub struct WindowsPower;

impl PowerAction for WindowsPower {
    fn sleep(&self) -> io::Result<()> {
        #[cfg(windows)]
        {
            // SAFETY: plain Win32 call with no pointers; FALSE, FALSE, FALSE = sleep (not hibernate), wake events allowed.
            let ok = unsafe { windows_sys::Win32::System::Power::SetSuspendState(false, false, false) };
            if ok { Ok(()) } else { Err(io::Error::last_os_error()) }
        }
        #[cfg(not(windows))]
        Err(io::Error::other("sleep is only supported on Windows"))
    }

    fn shutdown(&self) -> io::Result<()> {
        let status = std::process::Command::new("shutdown").args(["/s", "/t", "0"]).status()?;
        if status.success() { Ok(()) } else { Err(io::Error::other(format!("shutdown exited with {status}"))) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idle() -> Idle {
        Idle::default()
    }

    fn busy() -> Idle {
        Idle { running_or_queued: 1, ..Idle::default() }
    }

    /// Runs `w` from `t0` through `secs` one-second steps of `state`; returns the steps.
    fn run(w: &mut WhenDone, t0: Instant, secs: u64, state: Idle) -> Vec<Step> {
        (0..=secs).map(|s| w.step(&state, t0 + Duration::from_secs(s))).collect()
    }

    #[test]
    fn not_armed_does_not_fire() {
        assert!(!should_fire(Mode::Shutdown, false, &idle()));
        let mut w = WhenDone::default();
        w.set_mode(Mode::Shutdown);
        let t0 = Instant::now();
        assert!(run(&mut w, t0, 120, idle()).iter().all(|s| *s == Step::Nothing));
        assert_eq!(w.status(t0).mode, Mode::Shutdown);
    }

    #[test]
    fn fires_when_idle_and_armed() {
        assert!(should_fire(Mode::Sleep, true, &idle()));
        assert!(!should_fire(Mode::Nothing, true, &idle()));
        assert!(!should_fire(Mode::Sleep, true, &busy()));
        let mut w = WhenDone::default();
        w.set_mode(Mode::Sleep);
        let t0 = Instant::now();
        w.step(&busy(), t0);
        let steps = run(&mut w, t0 + Duration::from_secs(1), 70, idle());
        assert_eq!(steps.iter().filter(|s| **s == Step::Started).count(), 1);
        assert_eq!(steps.iter().filter(|s| **s == Step::Fire(Mode::Sleep)).count(), 1);
        assert_eq!(w.status(t0).mode, Mode::Nothing);
    }

    #[test]
    fn fires_after_failure() {
        // The last download failed: nothing running or queued, so it still fires.
        let after_fail = Idle { running_or_queued: 0, ..Idle::default() };
        assert!(should_fire(Mode::Shutdown, true, &after_fail));
    }

    #[test]
    fn password_wait_blocks() {
        let waiting = Idle { postproc_waiting_on_user: true, ..Idle::default() };
        assert!(!should_fire(Mode::Shutdown, true, &waiting));
        let mut w = WhenDone::default();
        w.set_mode(Mode::Shutdown);
        w.on_started();
        assert!(run(&mut w, Instant::now(), 120, waiting).iter().all(|s| *s == Step::Nothing));
    }

    #[test]
    fn schedule_hold_blocks() {
        let held = Idle { held_by_schedule: true, ..Idle::default() };
        assert!(!should_fire(Mode::Sleep, true, &held));
    }

    #[test]
    fn countdown_counts_from_60() {
        let mut c = Countdown::default();
        let t0 = Instant::now();
        assert_eq!(c.remaining(t0), None);
        c.start(t0);
        assert_eq!(c.remaining(t0), Some(60));
        assert_eq!(c.remaining(t0 + Duration::from_millis(1500)), Some(59));
        assert_eq!(c.remaining(t0 + Duration::from_secs(60)), Some(0));
        assert_eq!(c.remaining(t0 + Duration::from_secs(90)), Some(0));
    }

    #[test]
    fn cancel_resets_to_nothing() {
        let mut w = WhenDone::default();
        w.set_mode(Mode::Shutdown);
        w.on_started();
        let t0 = Instant::now();
        assert!(run(&mut w, t0, 3, idle()).contains(&Step::Started));
        w.cancel();
        assert_eq!(w.status(t0), Status { mode: Mode::Nothing, countdown: None });
        assert!(run(&mut w, t0, 120, idle()).iter().all(|s| *s == Step::Nothing));
    }

    #[test]
    fn new_work_cancels_countdown() {
        let mut w = WhenDone::default();
        w.set_mode(Mode::Shutdown);
        w.on_started();
        let t0 = Instant::now();
        assert!(run(&mut w, t0, 3, idle()).contains(&Step::Started));
        // A download starts 10 s into the countdown.
        assert_eq!(w.step(&busy(), t0 + Duration::from_secs(13)), Step::Nothing);
        assert_eq!(w.status(t0).countdown, None);
        assert_eq!(w.status(t0).mode, Mode::Shutdown);
        // It finishes; the countdown starts over from 60.
        let t1 = t0 + Duration::from_secs(20);
        assert!(run(&mut w, t1, 3, idle()).contains(&Step::Started));
        assert_eq!(w.status(t1 + Duration::from_secs(2)).countdown, Some(60));
    }

    #[test]
    fn take_returns_mode_once() {
        let mut w = WhenDone::default();
        assert_eq!(w.take(), None);
        w.set_mode(Mode::Sleep);
        assert_eq!(w.take(), Some(Mode::Sleep));
        assert_eq!(w.take(), None);
    }

    #[test]
    fn mode_serialises_lowercase() {
        assert_eq!(serde_json::to_string(&Mode::Shutdown).unwrap(), "\"shutdown\"");
        let s = Status { mode: Mode::Sleep, countdown: Some(42) };
        assert_eq!(serde_json::to_value(s).unwrap(), serde_json::json!({ "mode": "sleep", "countdown": 42 }));
    }
}
