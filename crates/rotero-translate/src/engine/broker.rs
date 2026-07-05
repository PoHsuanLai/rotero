//! Follow-up HTTP for the engine's `ZU.doGet` / `requestJSON` / `processDocuments`
//! host functions, routed through a pluggable [`FetchBroker`].
//!
//! A translator often makes a follow-up request after landing on the article
//! page — to a citation/export endpoint that, for gated publishers, requires the
//! user's logged-in session cookies. Those cookies are `HttpOnly`, so the only
//! way to authenticate is to run the fetch in the user's browser tab. The broker
//! abstracts *where* a follow-up fetch executes:
//!
//! - [`DirectBroker`] issues the request itself via the anonymous blocking
//!   [`reqwest`](super::http) client. This is the default and the fallback: it
//!   serves the tab-less callers (the in-app Find-PDF path) and any request that
//!   can't be proxied.
//! - A channel-backed broker (installed by the connector) hands the request to
//!   the browser extension, which fetches it in the authenticated page context
//!   and returns the body.
//!
//! The engine runs synchronously on a `spawn_blocking` thread, so [`FetchBroker`]
//! is a *blocking* interface. A channel broker blocks the engine thread on a
//! reply channel while an async task performs the browser round-trip — the engine
//! thread simply parks, holding all its state in memory, and resumes when the
//! reply arrives.

use std::cell::RefCell;

use super::http;

/// A follow-up HTTP request a running translator wants performed.
#[derive(Debug, Clone)]
pub struct FetchRequest {
    /// `"GET"` or `"POST"`.
    pub method: String,
    /// Absolute request URL (already resolved against the page).
    pub url: String,
    /// Request body (POST only; empty for GET).
    pub body: String,
    /// `Content-Type` for a POST body.
    pub content_type: String,
    /// Extra request headers (e.g. `Referer`).
    pub headers: Vec<(String, String)>,
}

/// The outcome of a [`FetchRequest`]: the response body, or an error message the
/// engine surfaces to the translator as a failed request.
pub type FetchResponse = Result<String, String>;

/// Performs a translator's follow-up HTTP requests. Blocking, because the engine
/// runs synchronously; a channel implementation parks the engine thread on a
/// reply channel during the async round-trip.
pub trait FetchBroker: Send {
    /// Perform `req`, blocking until the response (or an error) is available.
    fn fetch(&self, req: FetchRequest) -> FetchResponse;
}

/// A broker that issues requests directly via the anonymous blocking client.
/// The default when no browser proxy is installed, and the fallback for requests
/// that can't be proxied.
pub struct DirectBroker;

impl FetchBroker for DirectBroker {
    fn fetch(&self, req: FetchRequest) -> FetchResponse {
        if req.method.eq_ignore_ascii_case("POST") {
            http::post(&req.url, &req.body, &req.content_type, &req.headers)
        } else {
            http::get(&req.url, &req.headers)
        }
    }
}

/// One parked follow-up fetch: the request, and the channel its response is
/// delivered on. The async driver owns the receiving end of the request queue
/// and sends the response back through `reply`.
pub struct BrokeredFetch {
    pub req: FetchRequest,
    pub reply: tokio::sync::oneshot::Sender<FetchResponse>,
}

/// A broker that hands each follow-up fetch to an async driver (the connector's
/// browser-proxy loop) and parks the engine thread until the driver replies.
///
/// `fetch` sends the request plus a fresh reply channel onto the queue, then
/// blocks the engine thread on that channel. The driver performs the request
/// (typically a round-trip to the browser extension) and sends the body back.
/// If the queue or reply channel is dropped — the driver gave up, or the run was
/// reaped — the engine sees an error and the translator's request fails, which
/// unwinds the run cleanly.
pub struct ChannelBroker {
    queue: tokio::sync::mpsc::UnboundedSender<BrokeredFetch>,
}

impl ChannelBroker {
    /// Build a broker over an existing request queue.
    pub fn new(queue: tokio::sync::mpsc::UnboundedSender<BrokeredFetch>) -> Self {
        Self { queue }
    }
}

impl FetchBroker for ChannelBroker {
    fn fetch(&self, req: FetchRequest) -> FetchResponse {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .queue
            .send(BrokeredFetch {
                req,
                reply: reply_tx,
            })
            .is_err()
        {
            return Err("fetch broker closed".to_string());
        }
        // Park the engine thread until the driver delivers the response.
        reply_rx
            .blocking_recv()
            .unwrap_or_else(|_| Err("fetch broker dropped the reply".to_string()))
    }
}

thread_local! {
    /// The broker the current run's follow-up fetches route through. `None` means
    /// use [`DirectBroker`] — the anonymous, single-shot path.
    static BROKER: RefCell<Option<Box<dyn FetchBroker>>> = const { RefCell::new(None) };
}

/// Install `broker` for the current thread's run, returning a guard that clears
/// it on drop so a later run on the same pooled thread starts clean.
pub fn install(broker: Box<dyn FetchBroker>) -> BrokerGuard {
    BROKER.with(|b| *b.borrow_mut() = Some(broker));
    BrokerGuard
}

/// Restores the thread-local broker to `None` when dropped.
pub struct BrokerGuard;

impl Drop for BrokerGuard {
    fn drop(&mut self) {
        BROKER.with(|b| *b.borrow_mut() = None);
    }
}

/// Perform a follow-up GET through the installed broker (or [`DirectBroker`]).
pub fn get(url: &str, headers: &[(String, String)]) -> FetchResponse {
    dispatch(FetchRequest {
        method: "GET".to_string(),
        url: url.to_string(),
        body: String::new(),
        content_type: String::new(),
        headers: headers.to_vec(),
    })
}

/// Perform a follow-up POST through the installed broker (or [`DirectBroker`]).
pub fn post(
    url: &str,
    body: &str,
    content_type: &str,
    headers: &[(String, String)],
) -> FetchResponse {
    dispatch(FetchRequest {
        method: "POST".to_string(),
        url: url.to_string(),
        body: body.to_string(),
        content_type: content_type.to_string(),
        headers: headers.to_vec(),
    })
}

/// Route `req` to the installed broker, falling back to [`DirectBroker`].
fn dispatch(req: FetchRequest) -> FetchResponse {
    BROKER.with(|b| match b.borrow().as_ref() {
        Some(broker) => broker.fetch(req),
        None => DirectBroker.fetch(req),
    })
}
