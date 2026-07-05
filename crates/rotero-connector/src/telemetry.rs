//! Tracks which tier of the `/api/scrape` fallback chain served each request.
//! The in-process-versus-Node hit rate indicates how much of real-world usage
//! the built-in translators cover, and hence when the Node translation server
//! can be disabled by default.

use std::sync::atomic::{AtomicU64, Ordering};

/// Which tier of the scrape chain produced a result.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    /// In-process translators (registry).
    Builtin,
    /// Zotero Node translation server.
    Node,
    /// Generic meta-tag scraper fallback.
    Scrape,
    /// Nothing produced a usable result.
    Miss,
}

static BUILTIN: AtomicU64 = AtomicU64::new(0);
static NODE: AtomicU64 = AtomicU64::new(0);
static SCRAPE: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);

/// Record a tier hit for a URL and log a running hit-rate summary.
pub fn record(tier: Tier, url: &str) {
    let counter = match tier {
        Tier::Builtin => &BUILTIN,
        Tier::Node => &NODE,
        Tier::Scrape => &SCRAPE,
        Tier::Miss => &MISS,
    };
    counter.fetch_add(1, Ordering::Relaxed);
    tracing::info!("scrape tier={tier:?} url={url}");

    let s = snapshot();
    if s.total() > 0 {
        tracing::info!(
            "scrape coverage: builtin={} node={} scrape={} miss={} (builtin+node parity={:.0}%)",
            s.builtin,
            s.node,
            s.scrape,
            s.miss,
            s.parity_pct(),
        );
    }
}

/// A point-in-time snapshot of the tier counters.
#[derive(Debug, Clone, Copy)]
pub struct Snapshot {
    pub builtin: u64,
    pub node: u64,
    pub scrape: u64,
    pub miss: u64,
}

impl Snapshot {
    /// Total recorded scrape requests.
    pub fn total(&self) -> u64 {
        self.builtin + self.node + self.scrape + self.miss
    }

    /// Percentage of requests served with structured metadata by the in-process
    /// or Node tier, excluding generic-scraper hits and misses.
    pub fn parity_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.builtin + self.node) as f64 / total as f64 * 100.0
    }
}

/// Read the current tier counters.
pub fn snapshot() -> Snapshot {
    Snapshot {
        builtin: BUILTIN.load(Ordering::Relaxed),
        node: NODE.load(Ordering::Relaxed),
        scrape: SCRAPE.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
    }
}
