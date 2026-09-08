use std::time::{Duration, Instant};

/// A countdown timer for one focus session.
///
/// `clock` is injected rather than calling `Instant::now()` directly, so
/// tests can drive time forward deterministically with
/// [`new_with_fake_clock`](Timer::new_with_fake_clock).
pub struct Timer<F: Fn() -> Instant> {
    pub clock: F,
    pub state: SessionState,
    pub focus_time: Duration,
}

/// Whether a [`Timer`] is counting down or stopped, and the data needed to
/// compute [`Timer::remaining`] in either case.
pub enum SessionState {
    Running {
        started_at: Instant,
        remaining_time_at_start: Duration,
    },
    Idle {
        remaining_time: Duration,
    },
}

impl Timer<fn() -> Instant> {
    /// Creates an idle timer with `focus_time_in_minutes` of time remaining,
    /// using the real system clock.
    pub fn new(focus_time_in_minutes: u32) -> Self {
        Self {
            clock: Instant::now,
            state: SessionState::Idle {
                remaining_time: Duration::from_mins(focus_time_in_minutes.into()),
            },
            focus_time: Duration::from_mins(focus_time_in_minutes.into()),
        }
    }
}

impl Default for Timer<fn() -> Instant> {
    fn default() -> Self {
        Self {
            clock: std::time::Instant::now,
            state: crate::timer::SessionState::Idle {
                remaining_time: Duration::from_mins(30),
            },
            focus_time: Duration::from_mins(30),
        }
    }
}

impl<F: Fn() -> Instant> Timer<F> {
    /// Creates an idle timer with `focus_time` remaining, using `clock`
    /// instead of the real system clock.
    ///
    /// Intended for tests: pass a closure over a shared, manually-advanced
    /// `Instant` so time can be moved forward deterministically.
    pub fn new_with_fake_clock(focus_time: Duration, clock: F) -> Self {
        Self {
            clock,
            state: SessionState::Idle {
                remaining_time: focus_time,
            },
            focus_time,
        }
    }

    /// Moves the timer from `SessionState::Idle` to `SessionState::Running`,
    /// resuming from whatever time was remaining.
    ///
    /// # Panics
    ///
    /// Panics if the timer is already running.
    pub fn start(&mut self) {
        match self.state {
            SessionState::Idle { remaining_time } => {
                self.state = SessionState::Running {
                    started_at: (self.clock)(),
                    remaining_time_at_start: remaining_time,
                };
            }
            SessionState::Running { .. } => panic!("Cannot start running timer!"),
        }
    }

    /// Moves the timer from `SessionState::Running` to `SessionState::Idle`,
    /// freezing the remaining time so [`start`](Self::start) can resume it later.
    ///
    /// # Panics
    ///
    /// Panics if the timer is already idle.
    pub fn pause(&mut self) {
        match self.state {
            SessionState::Running { .. } => {
                self.state = SessionState::Idle {
                    remaining_time: self.remaining(),
                }
            }
            SessionState::Idle { .. } => {
                panic!("Cannot pause paused timer!")
            }
        }
    }

    /// Moves the timer to `SessionState::Idle` with the full `focus_time`
    /// remaining, discarding any progress made in the current session.
    pub fn reset(&mut self) {
        self.state = SessionState::Idle {
            remaining_time: self.focus_time,
        };
    }

    /// Returns the time left in the current session.
    ///
    /// While running, this is computed from `clock()` and the time the
    /// session started; while idle, it's the frozen remaining time.
    pub fn remaining(&self) -> Duration {
        match self.state {
            SessionState::Running {
                started_at,
                remaining_time_at_start,
            } => remaining_time_at_start.saturating_sub((self.clock)() - started_at),
            SessionState::Idle { remaining_time } => remaining_time,
        }
    }

    /// Returns how far through the session we are, as a value in `0.0..=1.0`,
    /// where `0.0` is just started and `1.0` is time up.
    ///
    /// # Panics
    ///
    /// Panics if the timer is idle, since progress is meaningless outside of
    /// a running session.
    pub fn progress(&self) -> f32 {
        match self.state {
            SessionState::Idle { .. } => {
                panic!("remaining_as_ratio should not be called from Idle state!")
            }
            SessionState::Running { .. } => {
                let raw_progress: f32 = (self.focus_time.as_secs_f32()
                    - self.remaining().as_secs_f32())
                    / self.focus_time.as_secs_f32();
                raw_progress.clamp(0.0, 1.0)
            }
        }
    }

    /// Resets the timer to idle once a running session's time has run out.
    ///
    /// Call this once per frame; it's a no-op unless the running session has
    /// actually reached zero remaining time.
    pub fn update_state(&mut self) {
        if self.remaining().is_zero() && self.is_running() {
            self.reset()
        }
    }

    fn is_running(&self) -> bool {
        matches!(self.state, SessionState::Running { .. })
    }

    #[cfg(test)]
    fn is_idle(&self) -> bool {
        matches!(self.state, SessionState::Idle { .. })
    }

    /// [`start`](Self::start)s the timer if idle, or [`pause`](Self::pause)s
    /// it if running.
    pub fn toggle(&mut self) {
        match self.state {
            SessionState::Running { .. } => self.pause(),
            SessionState::Idle { .. } => self.start(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const THIRTY_MINUTES: Duration = Duration::from_secs(30 * 60);
    const FIFTEEN_MINUTES: Duration = Duration::from_secs(15 * 60);
    const FIVE_MINUTES: Duration = Duration::from_secs(5 * 60);

    #[test]
    fn new_timer_has_full_duration_remaining() {
        let timer = Timer::new(30);

        assert_eq!(THIRTY_MINUTES, timer.focus_time);
    }

    #[test]
    fn started_timer_is_running() {
        let mut timer = Timer::new(32);
        timer.start();

        assert!(timer.is_running());
    }

    #[test]
    fn paused_timer_is_paused() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.pause();

        assert!(timer.is_idle());
    }

    #[test]
    fn starting_paused_timer_works() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.pause();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.start();

        assert!(timer.is_running());
    }

    #[test]
    fn remaining_time_decreases_correctly() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIFTEEN_MINUTES);

        assert_eq!(timer.remaining(), FIFTEEN_MINUTES)
    }

    #[test]
    fn paused_timer_freezes_time_left() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.pause();
        let time_left_at_pause: Duration = timer.remaining();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        let time_left_after_five_minutes_paused: Duration = timer.remaining();

        assert_eq!(time_left_at_pause, time_left_after_five_minutes_paused)
    }

    #[test]
    fn running_timer_has_less_remaining_than_full_duration() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);

        assert!(timer.remaining() < THIRTY_MINUTES);
    }

    #[test]
    #[should_panic]
    fn starting_running_timer_panics() {
        let mut timer = Timer::new(30);
        timer.start();
        timer.start();
    }

    #[test]
    #[should_panic]
    fn pausing_idle_timer_panics() {
        let mut timer = Timer::new(32);
        timer.pause();
    }

    #[test]
    fn time_up_means_idle() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + THIRTY_MINUTES + FIVE_MINUTES);
        timer.update_state();

        assert!(timer.is_idle())
    }

    #[test]
    fn remaining_time_returns_full_time_at_overtime() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + THIRTY_MINUTES + FIVE_MINUTES);
        timer.update_state();

        assert!(timer.remaining() == THIRTY_MINUTES);
    }

    #[test]
    fn state_is_idle_at_overtime() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + THIRTY_MINUTES + FIVE_MINUTES);
        timer.update_state();

        assert!(timer.is_idle());
    }

    #[test]
    fn reset_resets_remaining_time_to_focus_time() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.reset();

        assert!(timer.remaining() == THIRTY_MINUTES);
    }

    #[test]
    fn timer_is_idle_after_reset() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIVE_MINUTES);
        timer.reset();

        assert!(timer.is_idle());
    }

    #[test]
    fn progress_is_zero_at_start() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();

        assert!((timer.progress() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn progress_is_zero_point_five_at_halfway() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + FIFTEEN_MINUTES);

        assert!((timer.progress() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn progress_is_one_point_zero_at_end_of_timer() {
        let fake_time = std::cell::Cell::new(Instant::now());
        let mut timer = Timer::new_with_fake_clock(THIRTY_MINUTES, || fake_time.get());
        timer.start();
        fake_time.set(fake_time.get() + THIRTY_MINUTES);

        assert!((timer.progress() - 1.0).abs() < 1e-6);
    }
}
