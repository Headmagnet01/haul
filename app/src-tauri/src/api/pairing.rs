//! One pairing request at a time, answered Allow or Deny from Haul's window.

use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::oneshot;

#[derive(Default)]
struct Inner {
    next: u64,
    /// The waiting request's number and where its answer goes.
    waiting: Option<(u64, oneshot::Sender<bool>)>,
}

#[derive(Clone)]
pub struct PairGate {
    inner: Arc<Mutex<Inner>>,
    timeout: Duration,
}

/// Another pairing request is already waiting for an answer.
#[derive(Debug)]
pub struct PairBusy;

/// A waiting pairing request. Dropping it (the browser went away) frees the gate.
pub struct PairTicket {
    n: u64,
    rx: Option<oneshot::Receiver<bool>>,
    inner: Arc<Mutex<Inner>>,
    timeout: Duration,
}

fn lock(m: &Mutex<Inner>) -> std::sync::MutexGuard<'_, Inner> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

impl PairGate {
    pub fn new(timeout: Duration) -> PairGate {
        PairGate { inner: Arc::default(), timeout }
    }

    pub fn request(&self) -> Result<PairTicket, PairBusy> {
        let mut g = lock(&self.inner);
        if g.waiting.as_ref().is_some_and(|(_, tx)| !tx.is_closed()) {
            return Err(PairBusy);
        }
        g.next += 1;
        let n = g.next;
        let (tx, rx) = oneshot::channel();
        g.waiting = Some((n, tx));
        Ok(PairTicket { n, rx: Some(rx), inner: self.inner.clone(), timeout: self.timeout })
    }

    /// Answers request `n`, if it is the one waiting. A late answer, or one meant for another request, does nothing.
    pub fn answer(&self, n: u64, allow: bool) {
        let mut g = lock(&self.inner);
        if g.waiting.as_ref().is_some_and(|(w, _)| *w == n)
            && let Some((_, tx)) = g.waiting.take()
        {
            let _ = tx.send(allow);
        }
    }

    /// Refuses whatever request is waiting (Haul is shutting down).
    pub fn deny_waiting(&self) {
        if let Some((_, tx)) = lock(&self.inner).waiting.take() {
            let _ = tx.send(false);
        }
    }
}

impl PairTicket {
    /// The request's number: the window answers by it.
    pub fn number(&self) -> u64 {
        self.n
    }

    /// True only for Allow; Deny, the timeout and a closed dialog are all false.
    pub async fn wait(mut self) -> bool {
        let Some(rx) = self.rx.take() else { return false };
        matches!(tokio::time::timeout(self.timeout, rx).await, Ok(Ok(true)))
    }
}

impl Drop for PairTicket {
    fn drop(&mut self) {
        let mut g = lock(&self.inner);
        if g.waiting.as_ref().is_some_and(|(n, _)| *n == self.n) {
            g.waiting = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    const T: Duration = Duration::from_secs(60);

    #[tokio::test(start_paused = true)]
    async fn allow_resolves_true() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        g.answer(t.number(), true);
        assert!(t.wait().await);
    }

    #[tokio::test(start_paused = true)]
    async fn deny_resolves_false() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        g.answer(t.number(), false);
        assert!(!t.wait().await);
    }

    #[tokio::test(start_paused = true)]
    async fn timeout_resolves_false() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        let w = tokio::spawn(t.wait());
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!w.await.unwrap());
    }

    #[tokio::test(start_paused = true)]
    async fn second_request_is_busy() {
        let g = PairGate::new(T);
        let _t = g.request().unwrap();
        assert!(g.request().is_err());
    }

    #[tokio::test(start_paused = true)]
    async fn gate_frees_after_answer() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        g.answer(t.number(), false);
        t.wait().await;
        assert!(g.request().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn dropped_ticket_frees_gate() {
        let g = PairGate::new(T);
        drop(g.request().unwrap());
        assert!(g.request().is_ok());
    }

    #[tokio::test(start_paused = true)]
    async fn late_answer_is_ignored() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        let old = t.number();
        let w = tokio::spawn(t.wait());
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!w.await.unwrap());
        g.answer(old, true);
        let w = tokio::spawn(g.request().unwrap().wait());
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!w.await.unwrap(), "a late Allow must not approve the next request");
    }

    #[tokio::test(start_paused = true)]
    async fn answer_for_another_request_is_ignored() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        let n = t.number();
        let w = tokio::spawn(t.wait());
        g.answer(n + 1, true);
        tokio::time::advance(Duration::from_secs(61)).await;
        assert!(!w.await.unwrap(), "an Allow meant for another request approves nothing");
    }

    #[tokio::test(start_paused = true)]
    async fn deny_waiting_refuses_whatever_waits() {
        let g = PairGate::new(T);
        let t = g.request().unwrap();
        g.deny_waiting();
        assert!(!t.wait().await);
    }
}
