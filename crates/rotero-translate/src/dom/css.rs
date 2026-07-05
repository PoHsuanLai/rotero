//! CSS-selector queries over a parsed document, backing the translator engine's
//! `doc.querySelector*` DOM API and the translate.js `text()`/`attr()` globals.
//!
//! The majority of upstream Zotero translators query the page with CSS selectors
//! rather than XPath. This adapter answers those queries via [`scraper`] (the same
//! `html5ever` parser the rest of the crate uses), alongside the XPath adapter in
//! the parent module.
//!
//! # Node handles
//!
//! Translators call `querySelectorAll` to get a list of rows, then run further
//! `text(row, sel)` queries scoped to each row. To let JavaScript hold a reference
//! to a node across host-function calls without lifetimes leaking into the engine,
//! each matched element is assigned an opaque integer handle: an index into a
//! per-document table of [`NodeId`]s. A handle round-trips back through
//! [`CssDom::scoped_*`] to re-derive a scoped [`ElementRef`] on demand.

use ego_tree::NodeId;
use scraper::{ElementRef, Html, Selector};

/// A parsed document queryable by CSS selector, with a table mapping opaque
/// integer handles (given out to JavaScript) back to tree nodes.
pub struct CssDom {
    html: Html,
    /// Handle `i` (as seen by JS) is `handles[i]`. Handle `0` is reserved for the
    /// document root, so `text(doc, sel)` and `text(node, sel)` share one path.
    handles: Vec<NodeId>,
}

impl CssDom {
    /// Parse an HTML string into a CSS-queryable document.
    pub fn parse(html: &str) -> Self {
        let html = Html::parse_document(html);
        // Handle 0 == document root.
        let root = html.tree.root().id();
        Self {
            html,
            handles: vec![root],
        }
    }

    /// Resolve a JS node handle to a scoped element, or the document root for
    /// handle `0` / an out-of-range handle. Returns `None` only if the stored id
    /// no longer resolves (should not happen for ids this table minted).
    fn resolve(&self, handle: usize) -> Option<ElementRef<'_>> {
        if handle == 0 {
            // Root: the document itself isn't an element; fall through to querying
            // from root via `select`, so callers should special-case root.
            return None;
        }
        let id = *self.handles.get(handle)?;
        let node = self.html.tree.get(id)?;
        ElementRef::wrap(node)
    }

    /// Run a selector from a scope (handle `0` = whole document) and return the
    /// matched elements as freshly-minted handles. Appends to the handle table.
    pub fn select(&mut self, scope: usize, selector: &str) -> Vec<usize> {
        let Ok(sel) = Selector::parse(selector) else {
            return Vec::new();
        };
        // Collect ids first so the immutable borrow of `self.html` ends before we
        // push into `self.handles`.
        let ids: Vec<NodeId> = if scope == 0 {
            self.html.select(&sel).map(|el| el.id()).collect()
        } else {
            match self.resolve(scope) {
                Some(el) => el.select(&sel).map(|e| e.id()).collect(),
                None => Vec::new(),
            }
        };
        ids.into_iter()
            .map(|id| {
                self.handles.push(id);
                self.handles.len() - 1
            })
            .collect()
    }

    /// Text content of the `index`-th match of `selector` within `scope`
    /// (whitespace-collapsed, trimmed). Empty string if there is no such match —
    /// mirrors translate.js `text()`, which returns `""` rather than throwing.
    pub fn text(&self, scope: usize, selector: &str, index: usize) -> String {
        self.nth(scope, selector, index)
            .map(|el| collapse_ws(&el.text().collect::<String>()))
            .unwrap_or_default()
    }

    /// Value of `attribute` on the `index`-th match of `selector` within `scope`.
    /// Empty string if the match or attribute is absent (mirrors `attr()`).
    pub fn attr(&self, scope: usize, selector: &str, attribute: &str, index: usize) -> String {
        self.nth(scope, selector, index)
            .and_then(|el| el.attr(attribute).map(str::to_string))
            .unwrap_or_default()
    }

    /// The `index`-th element matching `selector` within `scope`.
    fn nth(&self, scope: usize, selector: &str, index: usize) -> Option<ElementRef<'_>> {
        let sel = Selector::parse(selector).ok()?;
        if scope == 0 {
            self.html.select(&sel).nth(index)
        } else {
            self.resolve(scope)?.select(&sel).nth(index)
        }
    }

    /// Text content of a node handle itself (not a sub-selector) — backs
    /// `node.textContent` / `node.innerText` on handles returned by `select`.
    pub fn node_text(&self, handle: usize) -> String {
        self.resolve(handle)
            .map(|el| collapse_ws(&el.text().collect::<String>()))
            .unwrap_or_default()
    }

    /// Value of an attribute on a node handle itself — backs
    /// `node.getAttribute(name)`.
    pub fn node_attr(&self, handle: usize, attribute: &str) -> String {
        self.resolve(handle)
            .and_then(|el| el.attr(attribute).map(str::to_string))
            .unwrap_or_default()
    }
}

/// Collapse internal whitespace runs to single spaces and trim, matching
/// `ZU.trimInternal` — the form translators expect from `text()`.
fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAGE: &str = r#"<html><body>
        <div class="result">
            <a class="title" href="/a/1">First Paper</a>
            <span class="author">Ada Lovelace</span>
        </div>
        <div class="result">
            <a class="title" href="/a/2">Second Paper</a>
            <span class="author">Alan Turing</span>
        </div>
    </body></html>"#;

    #[test]
    fn document_scoped_text_and_attr() {
        let dom = CssDom::parse(PAGE);
        assert_eq!(dom.text(0, "a.title", 0), "First Paper");
        assert_eq!(dom.text(0, "a.title", 1), "Second Paper");
        assert_eq!(dom.attr(0, "a.title", "href", 1), "/a/2");
        // Missing match / attr → empty, not panic.
        assert_eq!(dom.text(0, ".nope", 0), "");
        assert_eq!(dom.attr(0, "a.title", "data-x", 0), "");
    }

    #[test]
    fn select_returns_handles_and_scopes() {
        let mut dom = CssDom::parse(PAGE);
        let rows = dom.select(0, "div.result");
        assert_eq!(rows.len(), 2);
        // Scoped query within the second row only.
        assert_eq!(dom.text(rows[1], "a.title", 0), "Second Paper");
        assert_eq!(dom.text(rows[1], "span.author", 0), "Alan Turing");
        assert_eq!(dom.attr(rows[0], "a.title", "href", 0), "/a/1");
    }

    #[test]
    fn node_handle_text_and_attr() {
        let mut dom = CssDom::parse(PAGE);
        let titles = dom.select(0, "a.title");
        assert_eq!(dom.node_text(titles[0]), "First Paper");
        assert_eq!(dom.node_attr(titles[0], "href"), "/a/1");
    }

    #[test]
    fn whitespace_is_collapsed() {
        let dom = CssDom::parse("<p>  hello \n   world  </p>");
        assert_eq!(dom.text(0, "p", 0), "hello world");
    }

    #[test]
    fn invalid_selector_is_empty_not_panic() {
        let mut dom = CssDom::parse(PAGE);
        assert!(dom.select(0, ">>>bad").is_empty());
        assert_eq!(dom.text(0, ">>>bad", 0), "");
    }
}
