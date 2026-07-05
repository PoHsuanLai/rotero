//! Runs an unmodified upstream Zotero translator (`.js`) as a [`Translator`],
//! bridging the boa engine (see [`crate::engine`]) into the registry.
//!
//! Each upstream translator file begins with a JSON metadata header (label,
//! `target` URL regex, type) followed by the JavaScript body. [`JsTranslator`]
//! parses the header once at construction, compiles `target` into a [`Regex`]
//! for [`matches_url`](Translator::matches_url), and runs the body via
//! [`run_web_translator`](crate::engine::run_web_translator) in
//! [`translate`](Translator::translate).

use async_trait::async_trait;
use regex::Regex;

use crate::engine::run_web_translator_raw;
use crate::item::ZoteroItem;

use super::{TranslationContext, Translator};

/// One upstream web translator, ready to run against a page.
pub struct JsTranslator {
    label: String,
    /// Compiled `target` regex; `None` if the header had no target (matches
    /// nothing, since a web translator without a target can't be dispatched).
    target: Option<Regex>,
    /// The JavaScript body (header stripped), owned so the struct is `'static`.
    source: String,
}

impl JsTranslator {
    /// Build a translator from a full upstream `.js` file (JSON header + body).
    ///
    /// Returns `Err` if the header doesn't parse. A missing or invalid `target`
    /// regex is tolerated (the translator simply matches no URL) so one bad
    /// pattern in a bulk load doesn't abort the batch.
    pub fn from_source(full: &str) -> Result<Self, String> {
        let (header, body) = split_header(full)?;
        let meta: serde_json::Value =
            serde_json::from_str(header).map_err(|e| format!("translator header JSON: {e}"))?;

        let label = meta
            .get("label")
            .and_then(|v| v.as_str())
            .unwrap_or("Unknown")
            .to_string();

        let target = meta
            .get("target")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .and_then(|pat| match Regex::new(pat) {
                Ok(re) => Some(re),
                Err(e) => {
                    tracing::debug!("translator {label:?} target regex rejected: {e}");
                    None
                }
            });

        Ok(Self {
            label,
            target,
            source: body.to_string(),
        })
    }

    /// Whether this translator has a usable `target` URL regex. A web translator
    /// without one can never be dispatched, so the loader drops it.
    pub fn has_target(&self) -> bool {
        self.target.is_some()
    }
}

#[async_trait]
impl Translator for JsTranslator {
    fn id(&self) -> &'static str {
        // The trait wants a &'static str for cheap logging, but a JsTranslator's
        // label is loaded at runtime. Leak it once: the set of translators is
        // fixed for the process lifetime, so this is a bounded, one-time leak.
        Box::leak(self.label.clone().into_boxed_str())
    }

    /// Site translators outrank the generic Embedded Metadata hub (priority 50).
    fn priority(&self) -> i32 {
        100
    }

    /// Match the page URL against the translator's `target` regex.
    fn matches_url(&self, url: &str) -> bool {
        self.target.as_ref().is_some_and(|re| re.is_match(url))
    }

    async fn translate(
        &self,
        ctx: &TranslationContext,
    ) -> Result<Vec<ZoteroItem>, crate::TranslateError> {
        // boa's Context is !Send; run the synchronous engine on a blocking thread
        // so the returned future stays Send (required by the async trait). When
        // the context carries a fetch queue, follow-up requests route through a
        // channel broker to the browser-proxy driver; otherwise they fetch
        // directly (anonymous).
        let source = self.source.clone();
        let body = ctx.body.to_string();
        let raw_body = ctx.raw_body.as_ref().map(|r| r.to_string());
        let url = ctx.url.clone();
        let broker: Option<Box<dyn crate::engine::FetchBroker>> = ctx
            .fetch_queue
            .clone()
            .map(|q| Box::new(crate::engine::ChannelBroker::new(q)) as Box<_>);
        let result = tokio::task::spawn_blocking(move || match broker {
            Some(broker) => crate::engine::run_web_translator_with_broker(
                &source,
                &body,
                raw_body.as_deref(),
                &url,
                broker,
            ),
            None => run_web_translator_raw(&source, &body, raw_body.as_deref(), &url),
        })
        .await
        .map_err(|e| crate::TranslateError::Translation(format!("engine task panicked: {e}")))?;

        match result {
            Ok(items) if items.is_empty() => Err(crate::TranslateError::NotApplicable),
            Ok(items) => Ok(items),
            Err(e) => Err(crate::TranslateError::Translation(e)),
        }
    }
}

/// Split an upstream translator file into its leading JSON metadata header and
/// the JavaScript body. The header is the first top-level `{...}` object;
/// everything after its closing brace is the body.
fn split_header(full: &str) -> Result<(&str, &str), String> {
    let start = full.find('{').ok_or("translator file has no JSON header")?;
    let bytes = full.as_bytes();
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;

    for i in start..bytes.len() {
        let c = bytes[i];
        if in_string {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_string = false;
            }
            continue;
        }
        match c {
            b'"' => in_string = true,
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    let header = &full[start..=i];
                    let body = &full[i + 1..];
                    return Ok((header, body));
                }
            }
            _ => {}
        }
    }
    Err("translator JSON header is unterminated".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
    "translatorID": "abc",
    "label": "Example Journal",
    "target": "^https?://example\\.org/article/",
    "translatorType": 4
}

function detectWeb(doc, url) { return "journalArticle"; }
function doWeb(doc, url) {
    var item = new Zotero.Item("journalArticle");
    item.title = text(doc, "h1");
    item.complete();
}
"#;

    #[test]
    fn parses_header_and_matches_target() {
        let t = JsTranslator::from_source(SAMPLE).expect("parse");
        assert_eq!(t.label, "Example Journal");
        assert!(t.matches_url("https://example.org/article/42"));
        assert!(!t.matches_url("https://other.com/x"));
    }

    #[test]
    fn body_excludes_header() {
        let (_h, body) = split_header(SAMPLE).unwrap();
        assert!(!body.contains("translatorID"));
        assert!(body.contains("function doWeb"));
    }

    #[test]
    fn header_with_brace_in_target_string() {
        // A `}` inside a JSON string must not end the header early.
        let src = r#"{ "label": "X", "target": "a}b", "translatorType": 4 }
function doWeb() {}"#;
        let (header, body) = split_header(src).unwrap();
        assert!(header.contains("a}b"));
        assert!(body.contains("function doWeb"));
    }

    #[tokio::test]
    async fn runs_against_a_page() {
        let t = JsTranslator::from_source(SAMPLE).expect("parse");
        let ctx = TranslationContext::new(
            "https://example.org/article/42".to_string(),
            Some("text/html".to_string()),
            std::sync::Arc::from("<html><body><h1>Hello</h1></body></html>"),
            None,
        );
        let items = t.translate(&ctx).await.expect("translate");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Hello");
    }
}
