//! Batches engine events into one `Update` per period.

use crate::dto::{ProgressView, StateView, StealView, Update};
use haul_engine::{Event, EventBus};
use std::time::Duration;
use tokio::sync::broadcast::error::RecvError;

/// Latest progress per id (first-seen order), every state change and steal in order, and a resync flag.
#[derive(Default)]
pub struct Batcher {
    progress: Vec<ProgressView>,
    states: Vec<StateView>,
    steals: Vec<StealView>,
    resync: bool,
}

impl Batcher {
    pub fn push(&mut self, e: &Event) {
        match e {
            Event::Progress { id, written, size, speed_bps, segments } => {
                let v = ProgressView {
                    id: *id,
                    written: *written,
                    size: *size,
                    speed_bps: *speed_bps,
                    segments: segments.iter().map(|&(s, p, e)| [s, p, e]).collect(),
                };
                match self.progress.iter_mut().find(|p| p.id == *id) {
                    Some(slot) => *slot = v,
                    None => self.progress.push(v),
                }
            }
            Event::StateChanged { id, state, error } => {
                self.states.push(StateView { id: *id, state: state.as_str().to_string(), error: error.clone() })
            }
            Event::Steal { id, bytes } => self.steals.push(StealView { id: *id, bytes: *bytes }),
            Event::AllDone => {}
        }
    }

    /// Events were dropped: the UI must reload a full snapshot.
    pub fn mark_lagged(&mut self) {
        self.resync = true;
    }

    pub fn take(&mut self) -> Option<Update> {
        if self.progress.is_empty() && self.states.is_empty() && self.steals.is_empty() && !self.resync {
            return None;
        }
        let b = std::mem::take(self);
        Some(Update { progress: b.progress, states: b.states, steals: b.steals, resync: b.resync })
    }
}

/// Subscribes now (so no event sent after this call is missed), then emits at most one `Update` per `period`.
pub fn spawn_forwarder(
    events: EventBus,
    period: Duration,
    emit: impl Fn(Update) + Send + 'static,
) -> tokio::task::JoinHandle<()> {
    let mut rx = events.subscribe();
    tokio::spawn(async move {
        let mut batch = Batcher::default();
        let mut tick = tokio::time::interval(period);
        tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                r = rx.recv() => match r {
                    Ok(e) => batch.push(&e),
                    Err(RecvError::Lagged(_)) => batch.mark_lagged(),
                    Err(RecvError::Closed) => break,
                },
                _ = tick.tick() => {
                    if let Some(u) = batch.take() {
                        emit(u);
                    }
                }
            }
        }
        if let Some(u) = batch.take() {
            emit(u);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use haul_engine::{DownloadState, Event, EventBus};
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    fn prog(id: i64, written: u64) -> Event {
        Event::Progress { id, written, size: Some(100), speed_bps: written, segments: vec![(0, written, 100)] }
    }

    #[test]
    fn progress_coalesces() {
        let mut b = Batcher::default();
        b.push(&prog(1, 10));
        b.push(&prog(1, 20));
        b.push(&prog(2, 5));
        b.push(&prog(1, 30));
        let u = b.take().unwrap();
        assert_eq!(u.progress.len(), 2);
        let one = u.progress.iter().find(|p| p.id == 1).unwrap();
        assert_eq!(one.written, 30);
        assert_eq!(one.segments, vec![[0, 30, 100]]);
        assert!(!u.resync);
    }

    #[test]
    fn states_and_steals_keep_order() {
        let mut b = Batcher::default();
        b.push(&Event::StateChanged { id: 1, state: DownloadState::Running, error: None });
        b.push(&Event::Steal { id: 1, bytes: 7 });
        b.push(&Event::StateChanged { id: 1, state: DownloadState::Paused, error: Some("x".into()) });
        b.push(&Event::Steal { id: 2, bytes: 9 });
        let u = b.take().unwrap();
        let states: Vec<_> = u.states.iter().map(|s| (s.id, s.state.as_str(), s.error.clone())).collect();
        assert_eq!(states, vec![(1, "running", None), (1, "paused", Some("x".to_string()))]);
        let steals: Vec<_> = u.steals.iter().map(|s| (s.id, s.bytes)).collect();
        assert_eq!(steals, vec![(1, 7), (2, 9)]);
    }

    #[test]
    fn empty_take_is_none() {
        let mut b = Batcher::default();
        assert!(b.take().is_none());
        b.push(&Event::AllDone);
        assert!(b.take().is_none());
        b.push(&prog(1, 1));
        assert!(b.take().is_some());
        assert!(b.take().is_none(), "take resets");
    }

    #[tokio::test]
    async fn lagged_receiver_requests_resync() {
        let bus = EventBus::new();
        let got: Arc<Mutex<Vec<Update>>> = Arc::default();
        let sink = got.clone();
        let h = spawn_forwarder(bus.clone(), Duration::from_millis(20), move |u| sink.lock().unwrap().push(u));
        for i in 0..1100 {
            bus.send(prog(i % 20, i as u64));
        }
        let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
        while got.lock().unwrap().is_empty() {
            assert!(tokio::time::Instant::now() < deadline, "no update emitted");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        assert!(got.lock().unwrap()[0].resync);
        h.abort();
    }
}
