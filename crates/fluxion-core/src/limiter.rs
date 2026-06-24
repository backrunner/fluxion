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

impl SharedRateLimiter {
    pub async fn update_limit(&self, bytes_per_second: Option<u64>) {
        let mut state = self.inner.lock().await;
        state.refill();
        state.limit = bytes_per_second;
        state.available = bytes_per_second.unwrap_or(u64::MAX) as f64;
    }

    pub async fn acquire(&self, bytes: u64) {
        loop {
            let sleep_for = {
                let mut state = self.inner.lock().await;
                match state.limit {
                    None => return,
                    Some(0) => Duration::from_millis(100),
                    Some(limit) => {
                        state.refill();
                        if state.available >= bytes as f64 {
                            state.available -= bytes as f64;
                            return;
                        }
                        let missing = bytes as f64 - state.available;
                        Duration::from_secs_f64((missing / limit as f64).min(1.0))
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
