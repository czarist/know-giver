//! Shared pacing for long batch jobs. When the server answers "too many
//! requests", every worker pauses together, fewer requests run at once, and
//! the limit grows back slowly after a run of successes.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Requests in flight at the start of a job.
const START_LIMIT: usize = 4;
/// Upper bound on requests in flight.
const MAX_LIMIT: usize = 6;
/// Successful requests needed before allowing one more in flight.
const GROW_AFTER: usize = 6;
/// Pause after the first "too many requests" when the server gives no hint.
const FIRST_PAUSE: Duration = Duration::from_secs(10);
/// Longest pause between attempts.
const MAX_PAUSE: Duration = Duration::from_secs(120);
/// Attempts of one request before giving up (about 8 minutes of waiting).
pub(crate) const MAX_ATTEMPTS: u32 = 7;

#[derive(Debug)]
struct State {
    in_flight: usize,
    limit: usize,
    successes: usize,
    strikes: u32,
    paused_until: Option<Instant>,
}

/// Pacing shared by every clone of a [`crate::Client`] made with
/// [`crate::Client::paced`].
#[derive(Debug)]
pub struct Throttle {
    state: Mutex<State>,
    changed: Condvar,
    cancel: Arc<AtomicBool>,
}

/// One request slot; frees itself when dropped.
pub(crate) struct Permit<'a>(&'a Throttle);

impl Drop for Permit<'_> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.in_flight = state.in_flight.saturating_sub(1);
        drop(state);
        self.0.changed.notify_all();
    }
}

impl Throttle {
    /// `cancel` stops waiting as soon as it is set.
    pub fn new(cancel: Arc<AtomicBool>) -> Self {
        Self {
            state: Mutex::new(State {
                in_flight: 0,
                limit: START_LIMIT,
                successes: 0,
                strikes: 0,
                paused_until: None,
            }),
            changed: Condvar::new(),
            cancel,
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// How long all requests are paused because the server asked to slow down.
    pub fn paused_for(&self) -> Option<Duration> {
        let until = self.lock().paused_until?;
        until.checked_duration_since(Instant::now())
    }

    /// Requests allowed in flight right now.
    pub fn limit(&self) -> usize {
        self.lock().limit
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }

    /// Waits for a free slot outside a pause. `None` when cancelled.
    pub(crate) fn acquire(&self) -> Option<Permit<'_>> {
        let mut state = self.lock();
        loop {
            if self.cancelled() {
                return None;
            }
            let now = Instant::now();
            let paused = state.paused_until.filter(|until| *until > now);
            if paused.is_none() && state.in_flight < state.limit {
                state.in_flight += 1;
                return Some(Permit(self));
            }
            // Wake up regularly to notice cancellation.
            let wait = paused
                .map(|until| until - now)
                .unwrap_or(Duration::MAX)
                .min(Duration::from_millis(250));
            state = match self.changed.wait_timeout(state, wait) {
                Ok((guard, _)) => guard,
                Err(poisoned) => poisoned.into_inner().0,
            };
        }
    }

    pub(crate) fn succeeded(&self) {
        let mut state = self.lock();
        state.strikes = 0;
        state.successes += 1;
        if state.successes >= GROW_AFTER && state.limit < MAX_LIMIT {
            state.limit += 1;
            state.successes = 0;
        }
        drop(state);
        self.changed.notify_all();
    }

    /// The server said "too many requests": halve the requests in flight and
    /// pause everyone. Returns the pause.
    pub(crate) fn rate_limited(&self, retry_after: Option<Duration>) -> Duration {
        let mut state = self.lock();
        state.strikes += 1;
        state.successes = 0;
        state.limit = (state.limit / 2).max(1);
        let backoff = FIRST_PAUSE.saturating_mul(1 << (state.strikes - 1).min(6));
        let pause = retry_after.unwrap_or(backoff).min(MAX_PAUSE);
        let until = Instant::now() + pause;
        if state.paused_until.is_none_or(|current| current < until) {
            state.paused_until = Some(until);
        }
        tracing::info!(
            secs = pause.as_secs(),
            limit = state.limit,
            "AI server asked to slow down; pausing"
        );
        pause
    }
}

/// Reads `Retry-After` given in seconds.
pub(crate) fn retry_after(value: Option<&str>) -> Option<Duration> {
    value?.trim().parse::<u64>().ok().map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn throttle() -> Throttle {
        Throttle::new(Arc::default())
    }

    #[test]
    fn rate_limit_halves_the_limit_and_pauses() {
        let t = throttle();
        assert_eq!(t.limit(), START_LIMIT);
        let pause = t.rate_limited(None);
        assert_eq!(pause, FIRST_PAUSE);
        assert_eq!(t.limit(), START_LIMIT / 2);
        assert!(t.paused_for().is_some());
        assert_eq!(t.rate_limited(None), FIRST_PAUSE * 2);
        assert_eq!(t.limit(), 1);
        assert_eq!(t.rate_limited(None), FIRST_PAUSE * 4);
        assert_eq!(t.limit(), 1);
    }

    #[test]
    fn server_hint_wins_but_is_capped() {
        let t = throttle();
        assert_eq!(
            t.rate_limited(Some(Duration::from_secs(3))),
            Duration::from_secs(3)
        );
        assert_eq!(t.rate_limited(Some(Duration::from_secs(900))), MAX_PAUSE);
    }

    #[test]
    fn limit_grows_back_after_successes() {
        let t = throttle();
        t.rate_limited(Some(Duration::ZERO));
        assert_eq!(t.limit(), 2);
        for _ in 0..GROW_AFTER {
            t.succeeded();
        }
        assert_eq!(t.limit(), 3);
        for _ in 0..GROW_AFTER * 10 {
            t.succeeded();
        }
        assert_eq!(t.limit(), MAX_LIMIT);
    }

    #[test]
    fn slots_are_limited_and_freed() {
        let t = throttle();
        let permits: Vec<_> = (0..START_LIMIT).filter_map(|_| t.acquire()).collect();
        assert_eq!(t.lock().in_flight, START_LIMIT);
        drop(permits);
        assert_eq!(t.lock().in_flight, 0);
    }

    #[test]
    fn cancel_stops_waiting() {
        let cancel = Arc::new(AtomicBool::new(false));
        let t = Throttle::new(Arc::clone(&cancel));
        t.rate_limited(Some(Duration::from_secs(60)));
        cancel.store(true, Ordering::Relaxed);
        let started = Instant::now();
        assert!(t.acquire().is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn parses_retry_after_seconds() {
        assert_eq!(retry_after(Some(" 12 ")), Some(Duration::from_secs(12)));
        assert_eq!(retry_after(Some("Wed, 21 Oct 2015 07:28:00 GMT")), None);
        assert_eq!(retry_after(None), None);
    }
}
