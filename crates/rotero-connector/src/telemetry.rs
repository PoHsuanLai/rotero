//! Scrape-tier telemetry: tracks which tier of the `/api/scrape` fallback chain
//! served each request. This is the coverage meter that informs when the Node
//! translation server can be retired by default — a high native-hit-rate means
//! the in-process Rust translators cover real-world usage.

use std::sync::atomic::{AtomicU64, Ordering};

/// Which tier of the scrape chain produced a result.
#[derive(Debug, Clone, Copy)]
pub enum Tier {
    /// In-process Rust translators (registry).
    Native,
    /// Zotero Node translation server.
    Node,
    /// Generic meta-tag scraper fallback.
    Scrape,
    /// Nothing produced a usable result.
    Miss,
}

static NATIVE: AtomicU64 = AtomicU64::new(0);
static NODE: AtomicU64 = AtomicU64::new(0);
static SCRAPE: AtomicU64 = AtomicU64::new(0);
static MISS: AtomicU64 = AtomicU64::new(0);

/// Record a tier hit for a URL and log a running hit-rate summary.
pub fn record(tier: Tier, url: &str) {
    let counter = match tier {
        Tier::Native => &NATIVE,
        Tier::Node => &NODE,
        Tier::Scrape => &SCRAPE,
        Tier::Miss => &MISS,
    };
    counter.fetch_add(1, Ordering::Relaxed);
    tracing::info!("scrape tier={tier:?} url={url}");

    let s = snapshot();
    if s.total() > 0 {
        tracing::info!(
            "scrape coverage: native={} node={} scrape={} miss={} (native+node parity={:.0}%)",
            s.native,
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
    pub native: u64,
    pub node: u64,
    pub scrape: u64,
    pub miss: u64,
}

impl Snapshot {
    /// Total recorded scrape requests.
    pub fn total(&self) -> u64 {
        self.native + self.node + self.scrape + self.miss
    }

    /// Fraction of requests served with structured metadata by the native or
    /// Node tier (i.e. not the generic scraper and not a miss). This is the
    /// flip metric: when it clears the threshold, Node can default off.
    pub fn parity_pct(&self) -> f64 {
        let total = self.total();
        if total == 0 {
            return 0.0;
        }
        (self.native + self.node) as f64 / total as f64 * 100.0
    }
}

/// Read the current tier counters.
pub fn snapshot() -> Snapshot {
    Snapshot {
        native: NATIVE.load(Ordering::Relaxed),
        node: NODE.load(Ordering::Relaxed),
        scrape: SCRAPE.load(Ordering::Relaxed),
        miss: MISS.load(Ordering::Relaxed),
    }
}
