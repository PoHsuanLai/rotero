//! Tracks which tier of the `/api/scrape` chain served each request. The
//! built-in translators' hit rate (vs. the generic-scraper fallback and misses)
//! shows how much of real-world usage the structured translators cover.

use std::sync::atomic::{AtomicU64, Ordering};

/// Which tier of the scrape chain produced a result.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    /// In-process translators (the corpus JS engine + Rust hubs).
    Builtin,
    /// Generic meta-tag scraper fallback.
    Scrape,
    /// Nothing produced a usable result.
    Miss,
}

static BUILTIN: AtomicU64 = AtomicU64::new(0);
static SCRAPE: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);

/// Record a tier hit for a URL and log a running hit-rate summary.
pub fn record(tier: Tier, url: &str) {
    let counter = match tier {
        Tier::Builtin => &BUILTIN,
        Tier::Scrape => &SCRAPE,
        Tier::Miss => &MISS,
    };
    counter.fetch_add(1, Ordering::Relaxed);
    tracing::info!("scrape tier={tier:?} url={url}");

    let s = snapshot();
    if s.total() > 0 {
        tracing::info!(
            "scrape coverage: builtin={} scrape={} miss={} (structured={:.0}%)",
            s.builtin,
            s.scrape,
            s.miss,
            s.structured_pct(),
        );
    }
}

/// A point-in-time snapshot of the tier counters.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub builtin: u64,
    pub scrape: u64,
    pub miss: u64,
}

impl Snapshot {
    /// Total recorded scrape requests.
    pub fn total(&self) -> u64 {
        self.builtin + self.scrape + self.miss
    }

    /// Percentage of requests served with structured metadata by the built-in
    /// translators, excluding generic-scraper hits and misses.
    pub fn structured_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        self.builtin as f64 / total as f64 * 100.0
    }
}

/// Read the current tier counters.
pub fn snapshot() -> Snapshot {
    Snapshot {
        builtin: BUILTIN.load(Ordering::Relaxed),
        scrape: SCRAPE.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
    }
}
