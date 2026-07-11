use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct SharedRateLimiter {
    inner: Arc<Mutex<RateLimiterState>>,
}

#[derive(Debug)]
struct RateLimiterState {
    limit: Option<u64>,
    available: f64,
    last_refill: Instant,
}

impl Default for RateLimiterState {
    fn default() -> Self {
        Self {
            limit: None,
            available: 0.0,
            last_refill: Instant::now(),
        }
    }
}

fn normalize(bytes_per_second: Option<u64>) -> Option<u64> {
    // 0 means "unlimited" (matches the UI, where an empty field clears the
    // limit); a literal zero would otherwise freeze every acquire forever.
    bytes_per_second.filter(|&bps| bps > 0)
}

impl SharedRateLimiter {
    /// Construct with an initial limit, synchronously — callers can hand the
    /// limiter to workers without racing an async `update_limit`.
    pub fn with_limit(bytes_per_second: Option<u64>) -> Self {
        let limit = normalize(bytes_per_second);
        Self {
            inner: Arc::new(Mutex::new(RateLimiterState {
                limit,
                available: limit.unwrap_or(0) as f64,
                last_refill: Instant::now(),
            })),
        }
    }

    pub async fn update_limit(&self, bytes_per_second: Option<u64>) {
        let mut state = self.inner.lock().await;
        state.refill();
        let limit = normalize(bytes_per_second);
        state.limit = limit;
        state.available = limit.unwrap_or(u64::MAX) as f64;
    }

    /// Token-bucket acquire with debt: a request larger than the bucket
    /// capacity is granted immediately and drives `available` negative, so
    /// subsequent acquires wait out the deficit. This keeps the average rate
    /// at the limit without ever livelocking on oversized network chunks.
    pub async fn acquire(&self, bytes: u64) {
        loop {
            let sleep_for = {
                let mut state = self.inner.lock().await;
                match state.limit {
                    None => return,
                    Some(limit) => {
                        state.refill();
                        if state.available > 0.0 {
                            state.available -= bytes as f64;
                            return;
                        }
                        let deficit = -state.available;
                        Duration::from_secs_f64((deficit / limit as f64).clamp(0.01, 1.0))
                    }
                }
            };
            tokio::time::sleep(sleep_for).await;
        }
    }
}

impl RateLimiterState {
    fn refill(&mut self) {
        let Some(limit) = self.limit else {
            self.available = u64::MAX as f64;
            self.last_refill = Instant::now();
            return;
        };
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.available = (self.available + elapsed * limit as f64).min(limit as f64);
        self.last_refill = now;
    }
}

#[derive(Clone, Default)]
pub struct Limiters {
    pub global_download: SharedRateLimiter,
    pub global_upload: SharedRateLimiter,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn unlimited_returns_immediately() {
        let limiter = SharedRateLimiter::default();
        limiter.acquire(10 * 1024 * 1024).await;
    }

    #[tokio::test]
    async fn zero_limit_is_unlimited() {
        let limiter = SharedRateLimiter::with_limit(Some(0));
        // Must not hang.
        limiter.acquire(1024).await;
    }

    #[tokio::test]
    async fn oversized_request_does_not_livelock() {
        let limiter = SharedRateLimiter::with_limit(Some(8 * 1024));
        // 64 KiB chunk against an 8 KiB/s limit: first acquire is granted on
        // the initial bucket, the second waits out the debt but completes.
        limiter.acquire(64 * 1024).await;
        tokio::time::timeout(Duration::from_secs(12), limiter.acquire(1))
            .await
            .expect("acquire should complete once the deficit is repaid");
    }

    #[tokio::test]
    async fn paces_to_limit() {
        let limiter = SharedRateLimiter::with_limit(Some(100 * 1024));
        let start = Instant::now();
        // Bucket starts full (100 KiB); acquiring 300 KiB total should need
        // roughly 2 extra seconds of refill.
        for _ in 0..30 {
            limiter.acquire(10 * 1024).await;
        }
        let elapsed = start.elapsed();
        assert!(
            elapsed >= Duration::from_millis(1500),
            "elapsed {elapsed:?}"
        );
        assert!(elapsed < Duration::from_secs(6), "elapsed {elapsed:?}");
    }

    #[tokio::test]
    async fn update_limit_lifts_block() {
        let limiter = SharedRateLimiter::with_limit(Some(1));
        limiter.acquire(1024).await; // grants immediately, deep debt
        let l2 = limiter.clone();
        let waiter = tokio::spawn(async move { l2.acquire(1024).await });
        tokio::time::sleep(Duration::from_millis(50)).await;
        limiter.update_limit(None).await;
        tokio::time::timeout(Duration::from_secs(2), waiter)
            .await
            .expect("waiter should finish after limit removed")
            .unwrap();
    }
}
