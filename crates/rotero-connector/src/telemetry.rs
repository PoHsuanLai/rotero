//! Tracks the `/api/scrape` outcome for each request — whether the in-process
//! translators extracted structured metadata or missed — to show how much of
//! real-world usage they cover.

use std::sync::atomic::{AtomicU64, Ordering};

/// The outcome of a scrape request.
#[derive(Debug, Clone, Copy)]
pub enum Outcome {
    /// The translators (the corpus JS engine + Rust hubs) extracted an item.
    Hit,
    /// Nothing produced a usable result.
    Miss,
}

static HIT: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);

/// Record an outcome for a URL and log a running hit-rate summary.
pub fn record(outcome: Outcome, url: &str) {
    let counter = match outcome {
        Outcome::Hit => &HIT,
        Outcome::Miss => &MISS,
    };
    counter.fetch_add(1, Ordering::Relaxed);
    tracing::info!("scrape outcome={outcome:?} url={url}");

    let s = snapshot();
    if s.total() > 0 {
        tracing::info!(
            "scrape coverage: hit={} miss={} (hit rate={:.0}%)",
            s.hit,
            s.miss,
            s.hit_rate_pct(),
        );
    }
}

/// A point-in-time snapshot of the outcome counters.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub hit: u64,
    pub miss: u64,
}

impl Snapshot {
    /// Total recorded scrape requests.
    pub fn total(&self) -> u64 {
        self.hit + self.miss
    }

    /// Percentage of requests for which the translators extracted metadata.
    pub fn hit_rate_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.hit as f64 / total as f64 * 100.0
    }
}

/// Read the current outcome counters.
pub fn snapshot() -> Snapshot {
    Snapshot {
        hit: HIT.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
    }
}
