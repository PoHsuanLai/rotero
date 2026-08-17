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
    // Host only. The full URL made the log a plaintext record of every page the
    // user scraped, and publisher URLs from an authenticated session routinely
    // carry tokens in the query string. The host is what the hit-rate numbers
    // are actually about.
    tracing::info!("scrape outcome={outcome:?} host={}", host_of(url));

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

/// The scheme and host of a URL, dropping path, query, and fragment.
///
/// Enough to tell which publishers the translators handle badly, without
/// recording what the user was reading or the session tokens that publisher
/// URLs carry in their query strings.
fn host_of(url: &str) -> String {
    let after_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or_default();
    // Credentials can appear before the host as user:pass@host.
    let host = host.rsplit('@').next().unwrap_or(host);
    if host.is_empty() {
        "unknown".to_string()
    } else {
        host.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::host_of;

    #[test]
    fn only_the_host_survives() {
        assert_eq!(
            host_of("https://example.com/article/123?session=SECRET#frag"),
            "example.com"
        );
        assert_eq!(host_of("http://sub.example.org/x"), "sub.example.org");
        assert_eq!(host_of("example.com/path"), "example.com");
    }

    #[test]
    fn credentials_are_not_logged() {
        assert_eq!(
            host_of("https://user:pass@example.com/x?t=1"),
            "example.com"
        );
    }

    #[test]
    fn something_unparseable_is_not_leaked_verbatim() {
        assert_eq!(host_of(""), "unknown");
        assert_eq!(host_of("://"), "unknown");
    }
}
