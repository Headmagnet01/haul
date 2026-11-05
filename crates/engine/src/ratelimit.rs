//! Speed limits (token bucket) and the per-host connection cap.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio::time::Instant;

struct Bucket {
    rate: Option<u64>,
    tokens: f64,
    last: Instant,
}

/// A token bucket shared by many connections. Burst = rate / 4.
/// Callers take their bytes at once and then sleep off any debt, so a
/// slow consumer never blocks the others while it waits.
pub struct RateLimiter {
    bucket: Mutex<Bucket>,
}

impl RateLimiter {
    pub fn new(bps: Option<u64>) -> Self {
        let bps = bps.filter(|&b| b > 0);
        RateLimiter {
            bucket: Mutex::new(Bucket { rate: bps, tokens: bps.map_or(0.0, |b| b as f64 / 4.0), last: Instant::now() }),
        }
    }

    pub fn set_rate(&self, bps: Option<u64>) {
        let bps = bps.filter(|&b| b > 0);
        let mut b = self.bucket.lock().unwrap();
        if b.rate != bps {
            b.rate = bps;
            b.tokens = bps.map_or(0.0, |r| r as f64 / 4.0);
            b.last = Instant::now();
        }
    }

    pub fn rate(&self) -> Option<u64> {
        self.bucket.lock().unwrap().rate
    }

    pub async fn acquire(&self, bytes: u64) {
        let wait = {
            let mut b = self.bucket.lock().unwrap();
            let Some(rate) = b.rate else { return };
            let rate = rate as f64;
            let now = Instant::now();
            let burst = rate / 4.0;
            b.tokens = (b.tokens + now.duration_since(b.last).as_secs_f64() * rate).min(burst);
            b.last = now;
            b.tokens -= bytes as f64;
            if b.tokens >= 0.0 { None } else { Some(Duration::from_secs_f64(-b.tokens / rate)) }
        };
        if let Some(w) = wait {
            tokio::time::sleep(w).await;
        }
    }
}

/// At most `per_host` connections to one host, across all downloads.
pub struct HostLimiter {
    per_host: usize,
    hosts: Mutex<HashMap<String, Arc<Semaphore>>>,
}

impl HostLimiter {
    pub fn new(per_host: usize) -> Self {
        HostLimiter { per_host: per_host.max(1), hosts: Mutex::new(HashMap::new()) }
    }

    pub async fn acquire(&self, host: &str) -> OwnedSemaphorePermit {
        let sem = {
            let mut m = self.hosts.lock().unwrap();
            m.entry(host.to_ascii_lowercase()).or_insert_with(|| Arc::new(Semaphore::new(self.per_host))).clone()
        };
        sem.acquire_owned().await.expect("host semaphore is never closed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(start_paused = true)]
    async fn ratelimit_unit() {
        let rl = RateLimiter::new(Some(1_000_000));
        let t0 = tokio::time::Instant::now();
        let mut left: u64 = 3_000_000;
        while left > 0 {
            let n = left.min(64 * 1024);
            rl.acquire(n).await;
            left -= n;
        }
        let secs = t0.elapsed().as_secs_f64();
        assert!((2.7..=3.3).contains(&secs), "took {secs:.3} s");

        let free = RateLimiter::new(None);
        let t1 = tokio::time::Instant::now();
        free.acquire(10_000_000).await;
        assert_eq!(t1.elapsed().as_millis(), 0);
    }

    #[tokio::test]
    async fn host_limiter_caps_per_host() {
        let h = HostLimiter::new(2);
        let _a = h.acquire("x.org").await;
        let _b = h.acquire("x.org").await;
        let other = tokio::time::timeout(std::time::Duration::from_millis(50), h.acquire("y.org")).await;
        assert!(other.is_ok(), "a different host must not wait");
        let third = tokio::time::timeout(std::time::Duration::from_millis(50), h.acquire("x.org")).await;
        assert!(third.is_err(), "a third connection to x.org must wait");
    }
}
