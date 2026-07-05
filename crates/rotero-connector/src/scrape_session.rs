//! Resumable `/api/scrape` sessions for the browser-proxied fetch protocol.
//!
//! A gated publisher's translator fetches its citation data from an endpoint
//! that needs the user's session cookies. The connector can't supply those, but
//! the browser can: it replays the fetch in the authenticated tab. Because the
//! translator engine runs synchronously and blocks on each follow-up fetch, a
//! single request/response can't express "run the translator, but pause to let
//! the browser perform a fetch, then continue." So the scrape becomes a resumable
//! exchange:
//!
//! 1. `POST /api/scrape` starts the translation. It either finishes with no
//!    follow-up (returning the metadata immediately) or parks on the first fetch,
//!    returning `{ done: false, run_id, fetch }`.
//! 2. The extension performs `fetch` in the page context and
//!    `POST /api/scrape/continue { run_id, response }`. The connector feeds the
//!    body to the parked engine and drives to the next park or completion.
//! 3. Repeat until `{ done: true, metadata }`.
//!
//! A session holds the parked engine task and the receiving end of its fetch
//! queue between requests, keyed by an opaque `run_id`.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use rotero_translate::ZoteroItem;
use rotero_translate::engine::{BrokeredFetch, FetchResponse};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::task::JoinHandle;

/// A follow-up fetch the extension must perform in the authenticated tab.
#[derive(Debug, serde::Serialize)]
pub struct FetchInstruction {
    pub method: String,
    pub url: String,
    pub body: String,
    pub content_type: String,
    pub headers: Vec<(String, String)>,
}

/// The extension's reply carrying the fetched response.
#[derive(Debug, serde::Deserialize)]
pub struct FetchOutcome {
    pub ok: bool,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub error: Option<String>,
}

/// One driven step of a parked translation: either it needs another browser
/// fetch, or it finished with the (optional) extracted items.
pub enum Step {
    /// The engine parked on a follow-up fetch; the extension must perform it.
    Fetch(FetchInstruction),
    /// The translation completed.
    Done(Option<Vec<ZoteroItem>>),
}

/// Cap on browser-proxied follow-up fetches per scrape. A well-behaved
/// translator makes one or two; this bounds a buggy or hostile one that would
/// keep parking on new fetches, and matches the extension's own client-side cap.
const MAX_HOPS: u32 = 20;

/// A parked translation between protocol round-trips.
pub struct Session {
    join: JoinHandle<Option<Vec<ZoteroItem>>>,
    queue_rx: UnboundedReceiver<BrokeredFetch>,
    /// The reply channel for the fetch the extension is currently performing.
    /// Filled when we hand out a [`Step::Fetch`]; taken when the extension
    /// returns its outcome.
    pending: Option<tokio::sync::oneshot::Sender<FetchResponse>>,
    /// Number of follow-up fetches handed to the extension so far.
    hops: u32,
    /// When the session was last driven — used by the reaper to drop sessions the
    /// extension abandoned (popup closed mid-fetch).
    last_touched: std::time::Instant,
}

impl Session {
    /// Wrap a freshly spawned translation task and its fetch queue.
    pub fn new(
        join: JoinHandle<Option<Vec<ZoteroItem>>>,
        queue_rx: UnboundedReceiver<BrokeredFetch>,
    ) -> Self {
        Self {
            join,
            queue_rx,
            pending: None,
            hops: 0,
            last_touched: std::time::Instant::now(),
        }
    }

    /// Drive the parked engine to its next park or to completion: wait for either
    /// the translation task to finish or the next follow-up fetch to arrive. On a
    /// fetch, the reply channel is retained in [`pending`](Self::pending) for the
    /// matching continue call.
    pub async fn drive(&mut self) -> Step {
        self.last_touched = std::time::Instant::now();
        tokio::select! {
            // Bias toward draining queued fetches so a request enqueued just
            // before completion isn't skipped.
            biased;

            maybe_fetch = self.queue_rx.recv() => match maybe_fetch {
                Some(fetch) => {
                    let BrokeredFetch { req, reply } = fetch;
                    // Enforce the hop cap: past the limit, drop the reply (which
                    // unwinds the parked engine) and complete with whatever items
                    // reached the sink, rather than parking again.
                    if self.hops >= MAX_HOPS {
                        drop(reply);
                        tracing::warn!("scrape hop cap ({MAX_HOPS}) reached for {}", req.url);
                        return Step::Done(self.await_join().await);
                    }
                    self.hops += 1;
                    self.pending = Some(reply);
                    Step::Fetch(FetchInstruction {
                        method: req.method,
                        url: req.url,
                        body: req.body,
                        content_type: req.content_type,
                        headers: req.headers,
                    })
                }
                // Queue closed with no pending fetch: the engine is wrapping up.
                None => Step::Done(self.await_join().await),
            },

            joined = &mut self.join => Step::Done(joined.unwrap_or(None)),
        }
    }

    /// How long since the session was last driven.
    pub fn idle_for(&self) -> std::time::Duration {
        self.last_touched.elapsed()
    }

    /// Deliver the extension's fetch outcome to the parked engine, unblocking it.
    /// Returns `false` if there was no fetch awaiting a reply (a stray continue).
    pub fn resume(&mut self, outcome: FetchOutcome) -> bool {
        let Some(reply) = self.pending.take() else {
            return false;
        };
        let response: FetchResponse = if outcome.ok {
            Ok(outcome.body)
        } else {
            Err(outcome
                .error
                .unwrap_or_else(|| "browser fetch failed".to_string()))
        };
        // If the engine already went away, the send fails harmlessly.
        let _ = reply.send(response);
        true
    }

    /// Await the translation task to completion (used when the fetch queue closes
    /// without a pending fetch).
    async fn await_join(&mut self) -> Option<Vec<ZoteroItem>> {
        (&mut self.join).await.unwrap_or(None)
    }
}

/// The connector's live scrape sessions, keyed by `run_id`.
#[derive(Default)]
pub struct SessionStore {
    sessions: Mutex<HashMap<u64, Session>>,
    next_id: AtomicU64,
}

impl SessionStore {
    /// Register a parked session, returning its fresh `run_id`.
    pub fn insert(&self, session: Session) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.sessions.lock().unwrap().insert(id, session);
        id
    }

    /// Remove and return a session (for the continue handler to drive it).
    pub fn take(&self, run_id: u64) -> Option<Session> {
        self.sessions.lock().unwrap().remove(&run_id)
    }

    /// Re-insert a session under its existing `run_id` after driving it (still
    /// parked on another fetch).
    pub fn put_back(&self, run_id: u64, session: Session) {
        self.sessions.lock().unwrap().insert(run_id, session);
    }

    /// Drop sessions idle longer than `ttl`. Dropping a parked session closes its
    /// reply channel, which unwinds the blocked engine thread and frees it.
    /// Returns the number reaped.
    pub fn reap_idle(&self, ttl: std::time::Duration) -> usize {
        let mut sessions = self.sessions.lock().unwrap();
        let before = sessions.len();
        sessions.retain(|_, s| s.idle_for() < ttl);
        before - sessions.len()
    }

    /// Number of live sessions (for diagnostics/tests).
    pub fn len(&self) -> usize {
        self.sessions.lock().unwrap().len()
    }

    /// Whether there are no live sessions.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// How long a parked session may sit untouched before the reaper drops it. The
/// extension drives continuations back-to-back, so a session idle this long has
/// been abandoned (popup closed mid-fetch).
pub const SESSION_TTL: std::time::Duration = std::time::Duration::from_secs(120);

/// Periodically reap abandoned sessions from `store`, forever. Spawn once at
/// startup. The interval is a fraction of [`SESSION_TTL`] so an abandoned session
/// is freed reasonably promptly.
pub async fn run_reaper(store: std::sync::Arc<SessionStore>) {
    let mut ticker = tokio::time::interval(SESSION_TTL / 4);
    loop {
        ticker.tick().await;
        let reaped = store.reap_idle(SESSION_TTL);
        if reaped > 0 {
            tracing::debug!("reaped {reaped} abandoned scrape session(s)");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rotero_translate::engine::{ChannelBroker, run_web_translator_with_broker};

    /// Spawn a brokered engine run for `src` and wrap it in a [`Session`]. The
    /// translator's follow-up fetches surface through the returned session's
    /// [`drive`](Session::drive), exactly as a real scrape would.
    fn spawn_session(src: &'static str, url: &'static str) -> Session {
        let (queue_tx, queue_rx) = tokio::sync::mpsc::unbounded_channel();
        let join = tokio::task::spawn_blocking(move || {
            run_web_translator_with_broker(
                src,
                "<html><body></body></html>",
                None,
                url,
                Box::new(ChannelBroker::new(queue_tx)),
            )
            .ok()
        });
        Session::new(join, queue_rx)
    }

    #[tokio::test]
    async fn parks_on_fetch_then_completes_on_resume() {
        // Parks on a follow-up fetch, then builds its item from the response body.
        let src = r#"
        function detectWeb(doc, url) { return "journalArticle"; }
        async function doWeb(doc, url) {
            var data = await requestJSON("/rest/cite/1");
            var item = new Zotero.Item("journalArticle");
            item.title = data.title;
            item.complete();
        }
        "#;
        let mut session = spawn_session(src, "https://gated.example.org/document/1");

        // First drive parks on the citation fetch.
        let fetch = match session.drive().await {
            Step::Fetch(f) => f,
            Step::Done(_) => panic!("expected a follow-up fetch"),
        };
        assert_eq!(fetch.url, "https://gated.example.org/rest/cite/1");

        // Deliver the browser's fetched body; the run then completes.
        assert!(session.resume(FetchOutcome {
            ok: true,
            body: r#"{"title":"Recovered Via Browser"}"#.to_string(),
            error: None,
        }));

        match session.drive().await {
            Step::Done(items) => {
                let items = items.expect("items");
                assert_eq!(items[0].title, "Recovered Via Browser");
            }
            Step::Fetch(_) => panic!("expected completion after the single fetch"),
        }
    }

    #[tokio::test]
    async fn completes_without_any_fetch() {
        // No follow-up: the first drive returns Done directly (the one-shot path).
        let src = r#"
        function detectWeb(doc, url) { return "journalArticle"; }
        function doWeb(doc, url) {
            var item = new Zotero.Item("journalArticle");
            item.title = "No Fetch Needed";
            item.complete();
        }
        "#;
        let mut session = spawn_session(src, "https://open.example.org/a");
        match session.drive().await {
            Step::Done(items) => assert_eq!(items.expect("items")[0].title, "No Fetch Needed"),
            Step::Fetch(_) => panic!("no fetch expected"),
        }
    }

    #[tokio::test]
    async fn resume_without_pending_fetch_is_rejected() {
        let src = r#"
        function detectWeb(doc, url) { return "journalArticle"; }
        function doWeb(doc, url) {
            var item = new Zotero.Item("journalArticle");
            item.title = "x";
            item.complete();
        }
        "#;
        let mut session = spawn_session(src, "https://open.example.org/a");
        // Nothing has been driven, so no fetch awaits a reply.
        assert!(!session.resume(FetchOutcome {
            ok: true,
            body: String::new(),
            error: None,
        }));
    }

    #[tokio::test]
    async fn session_store_insert_take_roundtrip() {
        let store = SessionStore::default();
        let session = spawn_session(
            r#"function detectWeb(){return "journalArticle";}
               function doWeb(){var i=new Zotero.Item("journalArticle");i.title="t";i.complete();}"#,
            "https://open.example.org/a",
        );
        let id = store.insert(session);
        assert!(store.take(id).is_some(), "inserted session is retrievable");
        assert!(store.take(id).is_none(), "a taken session is gone");
    }

    #[tokio::test]
    async fn hop_cap_completes_the_run() {
        // A translator that would fetch forever (each response triggers another
        // request). The session must stop parking once the cap is hit and complete
        // with whatever it has, rather than looping without bound.
        let src = r#"
        function detectWeb(doc, url) { return "journalArticle"; }
        async function doWeb(doc, url) {
            var item = new Zotero.Item("journalArticle");
            item.title = "Loops Forever";
            for (var i = 0; i < 1000; i++) {
                try { await requestText("/rest/next/" + i); } catch (e) { break; }
            }
            item.complete();
        }
        "#;
        let mut session = spawn_session(src, "https://gated.example.org/document/1");

        // Feed exactly MAX_HOPS fetches, then the cap should force completion.
        let mut fetch_count = 0;
        loop {
            match session.drive().await {
                Step::Fetch(_) => {
                    fetch_count += 1;
                    assert!(
                        session.resume(FetchOutcome {
                            ok: true,
                            body: "ok".to_string(),
                            error: None,
                        }),
                        "each dispatched fetch has a pending reply"
                    );
                }
                Step::Done(items) => {
                    assert_eq!(items.expect("items")[0].title, "Loops Forever");
                    break;
                }
            }
        }
        assert_eq!(
            fetch_count, MAX_HOPS,
            "capped at MAX_HOPS follow-up fetches"
        );
    }

    #[tokio::test]
    async fn reaper_drops_abandoned_parked_session() {
        // A translator that parks on a fetch and is never continued.
        let src = r#"
        function detectWeb(doc, url) { return "journalArticle"; }
        async function doWeb(doc, url) {
            await requestText("/rest/cite/1");
            var item = new Zotero.Item("journalArticle");
            item.title = "never reached";
            item.complete();
        }
        "#;
        let store = SessionStore::default();
        let mut session = spawn_session(src, "https://gated.example.org/document/1");

        // Park it on the fetch, then register it and walk away.
        match session.drive().await {
            Step::Fetch(_) => {}
            Step::Done(_) => panic!("expected a park"),
        }
        store.insert(session);
        assert_eq!(store.len(), 1);

        // A zero TTL reaps it immediately; dropping the session unwinds the parked
        // engine thread (its reply channel closes).
        let reaped = store.reap_idle(std::time::Duration::ZERO);
        assert_eq!(reaped, 1);
        assert!(store.is_empty(), "abandoned session was dropped");
    }
}
