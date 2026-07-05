//! Runs an unmodified upstream Zotero translator's JavaScript in an embedded
//! `boa` interpreter, backed by Rust host functions for the `Zotero.*` / `ZU.*`
//! surface the translators call.
//!
//! Supports web translators that read the DOM via `ZU.xpath`/`ZU.xpathText` /
//! `doc.querySelector*`, emit items via `Zotero.Item` and `.complete()`, and
//! delegate to built-in hub translators (Embedded Metadata) via a synchronous
//! `Zotero.loadTranslator("web")` bridge. Not yet implemented: the full `ZU`
//! utility set, search/import delegation (needs async HTTP), and export IO.

mod sandbox;

use std::cell::RefCell;

use boa_engine::{Context, JsValue, NativeFunction, Source, js_string};

use crate::dom::ParsedDom;
use crate::item::ZoteroItem;

thread_local! {
    /// Items emitted by the running translator via `Zotero.Item.complete()`.
    static SINK: RefCell<Vec<ZoteroItem>> = const { RefCell::new(Vec::new()) };
    /// The DOM the current run's `ZU.xpath` host functions query.
    static DOM: RefCell<Option<ParsedDom>> = const { RefCell::new(None) };
    /// The current run's raw HTML and URL, so `loadTranslator` delegation can
    /// re-run a built-in hub translator (e.g. Embedded Metadata) over the page.
    static PAGE: RefCell<Page> = const { RefCell::new(Page::empty()) };
}

/// The page context a delegated built-in translator needs.
struct Page {
    html: String,
    url: String,
}

impl Page {
    const fn empty() -> Self {
        Self {
            html: String::new(),
            url: String::new(),
        }
    }
}

/// Run a translator's JavaScript against a document and collect emitted items.
///
/// `source` is the translator `.js` (unmodified upstream). `html` is the page,
/// `url` its address. Runs `detectWeb(doc, url)`; if it returns truthy, runs
/// `doWeb(doc, url)` and returns whatever items `.complete()` emitted.
pub fn run_web_translator(source: &str, html: &str, url: &str) -> Result<Vec<ZoteroItem>, String> {
    let dom = ParsedDom::parse(html)?;
    DOM.with(|d| *d.borrow_mut() = Some(dom));
    SINK.with(|s| s.borrow_mut().clear());
    PAGE.with(|p| {
        *p.borrow_mut() = Page {
            html: html.to_string(),
            url: url.to_string(),
        }
    });

    let result = (|| {
        let mut ctx = Context::default();
        register_host_functions(&mut ctx)?;

        // Inject the sandbox shim (defines Zotero, ZU, doc) then the translator.
        ctx.eval(Source::from_bytes(sandbox::SHIM))
            .map_err(|e| format!("sandbox shim error: {e}"))?;
        ctx.eval(Source::from_bytes(source))
            .map_err(|e| format!("translator load error: {e}"))?;

        // Populate doc.location from the real URL, then detectWeb(doc, url) →
        // run doWeb only if truthy. Translators read doc.location.pathname etc.
        let (path, search, hash) = split_url(url);
        let driver = format!(
            r#"
            (function() {{
                var __url = {url};
                doc.location.href = __url;
                doc.location.pathname = {path};
                doc.location.search = {search};
                doc.location.hash = {hash};
                var __detected = (typeof detectWeb === 'function') ? detectWeb(doc, __url) : false;
                if (__detected) {{
                    if (typeof doWeb === 'function') doWeb(doc, __url);
                }}
                return __detected ? String(__detected) : "";
            }})()
            "#,
            url = js_str_literal(url),
            path = js_str_literal(&path),
            search = js_str_literal(&search),
            hash = js_str_literal(&hash),
        );
        ctx.eval(Source::from_bytes(driver.as_bytes()))
            .map_err(|e| format!("translator run error: {e}"))?;
        Ok::<(), String>(())
    })();

    let items = SINK.with(|s| std::mem::take(&mut *s.borrow_mut()));
    DOM.with(|d| *d.borrow_mut() = None);
    PAGE.with(|p| *p.borrow_mut() = Page::empty());
    result?;
    Ok(items)
}

/// Run a built-in hub translator against the current page for `loadTranslator`
/// delegation. Returns the extracted items as a JSON array, or `[]` if the
/// target UUID isn't one with an in-process bridge (search/import delegates need
/// async HTTP and aren't bridged yet).
fn delegate_builtin(uuid: &str) -> String {
    // Embedded Metadata — the dominant web delegation target. Its core extractor
    // is synchronous, so it runs inline without the async registry path.
    const EMBEDDED_METADATA: &str = "951c027d-74ac-47d4-a107-9c3069ab7b48";

    let items: Vec<ZoteroItem> = match uuid {
        EMBEDDED_METADATA => PAGE.with(|p| {
            let page = p.borrow();
            let item = crate::html_meta::extract_zotero_item(&page.html);
            if item.title.is_empty() {
                Vec::new()
            } else {
                let item = ZoteroItem {
                    url: if item.url.is_empty() {
                        page.url.clone()
                    } else {
                        item.url.clone()
                    },
                    ..item
                };
                vec![item]
            }
        }),
        _ => {
            tracing::debug!("loadTranslator: no in-process bridge for {uuid}");
            Vec::new()
        }
    };
    serde_json::to_string(&items).unwrap_or_else(|_| "[]".to_string())
}

/// Register the Rust-backed host functions the sandbox shim calls.
fn register_host_functions(ctx: &mut Context) -> Result<(), String> {
    // __saveItem(json): parse a serialized item and push it to the sink.
    let save = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let json = args
            .first()
            .cloned()
            .unwrap_or(JsValue::undefined())
            .to_string(ctx)?
            .to_std_string_escaped();
        match serde_json::from_str::<ZoteroItem>(&json) {
            Ok(item) => SINK.with(|s| s.borrow_mut().push(item)),
            Err(e) => tracing::debug!("engine: failed to parse emitted item: {e}"),
        }
        Ok(JsValue::undefined())
    });
    ctx.register_global_callable(js_string!("__saveItem"), 1, save)
        .map_err(|e| format!("register __saveItem: {e}"))?;

    // __xpathText(expr): first string value for an XPath, or "".
    let xpath_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let expr = args
            .first()
            .cloned()
            .unwrap_or(JsValue::undefined())
            .to_string(ctx)?
            .to_std_string_escaped();
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .and_then(|dom| dom.xpath_text(&expr))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__xpathText"), 1, xpath_text)
        .map_err(|e| format!("register __xpathText: {e}"))?;

    // __xpathAll(expr): JSON array of string values for an XPath.
    let xpath_all = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let expr = args
            .first()
            .cloned()
            .unwrap_or(JsValue::undefined())
            .to_string(ctx)?
            .to_std_string_escaped();
        let vals = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.xpath_strings(&expr))
                .unwrap_or_default()
        });
        let json = serde_json::to_string(&vals).unwrap_or_else(|_| "[]".to_string());
        Ok(JsValue::from(js_string!(json)))
    });
    ctx.register_global_callable(js_string!("__xpathAll"), 1, xpath_all)
        .map_err(|e| format!("register __xpathAll: {e}"))?;

    // --- CSS-selector host functions (doc.querySelector* / text() / attr()) ---
    // Handles are integer indices into the DOM's per-run node table; handle 0 is
    // the document root. text/attr take (scopeHandle, selector[, attr][, index]).

    // __cssSelect(scope, selector): JSON array of node handles matching the
    // selector within the scope node (0 = whole document).
    let css_select = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let scope = arg_usize(args, 0, ctx)?;
        let selector = arg_string(args, 1, ctx)?;
        let handles = DOM.with(|d| {
            d.borrow_mut()
                .as_mut()
                .map(|dom| dom.css_mut().select(scope, &selector))
                .unwrap_or_default()
        });
        let json = serde_json::to_string(&handles).unwrap_or_else(|_| "[]".to_string());
        Ok(JsValue::from(js_string!(json)))
    });
    ctx.register_global_callable(js_string!("__cssSelect"), 2, css_select)
        .map_err(|e| format!("register __cssSelect: {e}"))?;

    // __cssText(scope, selector, index): text of the index-th match, or "".
    let css_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let scope = arg_usize(args, 0, ctx)?;
        let selector = arg_string(args, 1, ctx)?;
        let index = arg_usize(args, 2, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.css().text(scope, &selector, index))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__cssText"), 3, css_text)
        .map_err(|e| format!("register __cssText: {e}"))?;

    // __cssAttr(scope, selector, attr, index): attribute of the index-th match.
    let css_attr = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let scope = arg_usize(args, 0, ctx)?;
        let selector = arg_string(args, 1, ctx)?;
        let attribute = arg_string(args, 2, ctx)?;
        let index = arg_usize(args, 3, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.css().attr(scope, &selector, &attribute, index))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__cssAttr"), 4, css_attr)
        .map_err(|e| format!("register __cssAttr: {e}"))?;

    // __nodeText(handle): text content of the node handle itself.
    let node_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let handle = arg_usize(args, 0, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.css().node_text(handle))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__nodeText"), 1, node_text)
        .map_err(|e| format!("register __nodeText: {e}"))?;

    // __nodeAttr(handle, attr): attribute value on the node handle itself.
    let node_attr = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let handle = arg_usize(args, 0, ctx)?;
        let attribute = arg_string(args, 1, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.css().node_attr(handle, &attribute))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__nodeAttr"), 2, node_attr)
        .map_err(|e| format!("register __nodeAttr: {e}"))?;

    // __loadTranslator(uuid): run the built-in hub translator for `uuid` against
    // the current page and return its items as a JSON array (for the JS-side
    // Zotero.loadTranslator delegation bridge). "[]" if there's no bridge.
    let load_translator = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let uuid = arg_string(args, 0, ctx)?;
        let json = delegate_builtin(&uuid);
        Ok(JsValue::from(js_string!(json)))
    });
    ctx.register_global_callable(js_string!("__loadTranslator"), 1, load_translator)
        .map_err(|e| format!("register __loadTranslator: {e}"))?;

    // __debug(msg): route translator debug output to tracing.
    let debug = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(v) = args.first() {
            let s = v.to_string(ctx)?.to_std_string_escaped();
            tracing::trace!("translator: {s}");
        }
        Ok(JsValue::undefined())
    });
    ctx.register_global_callable(js_string!("__debug"), 1, debug)
        .map_err(|e| format!("register __debug: {e}"))?;

    // __cleanAuthor(name, type, useComma): structured author via the Rust ZU
    // port (correct surname-particle handling). Returns JSON.
    let clean_author = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let name = arg_string(args, 0, ctx)?;
        let ctype = arg_string(args, 1, ctx)?;
        let use_comma = args.get(2).map(|v| v.to_boolean()).unwrap_or(false);
        let a = crate::zu::clean_author(&name, &ctype, use_comma);
        let json = serde_json::json!({
            "firstName": a.first_name,
            "lastName": a.last_name,
            "creatorType": a.creator_type,
        });
        Ok(JsValue::from(js_string!(json.to_string())))
    });
    ctx.register_global_callable(js_string!("__cleanAuthor"), 3, clean_author)
        .map_err(|e| format!("register __cleanAuthor: {e}"))?;

    Ok(())
}

/// Read argument `i` as a Rust string (empty if absent).
fn arg_string(args: &[JsValue], i: usize, ctx: &mut Context) -> Result<String, boa_engine::JsError> {
    Ok(args
        .get(i)
        .cloned()
        .unwrap_or(JsValue::undefined())
        .to_string(ctx)?
        .to_std_string_escaped())
}

/// Read argument `i` as a `usize` node handle / index (0 if absent or negative).
fn arg_usize(args: &[JsValue], i: usize, ctx: &mut Context) -> Result<usize, boa_engine::JsError> {
    let n = args
        .get(i)
        .cloned()
        .unwrap_or(JsValue::undefined())
        .to_number(ctx)?;
    Ok(if n.is_finite() && n >= 0.0 {
        n as usize
    } else {
        0
    })
}

/// Split a URL into `(pathname, search, hash)` for `doc.location`. `search`
/// includes the leading `?`, `hash` the leading `#`, matching the DOM `Location`
/// API. Falls back to empty components if the URL doesn't parse.
fn split_url(url: &str) -> (String, String, String) {
    match reqwest::Url::parse(url) {
        Ok(u) => {
            let path = u.path().to_string();
            let search = u.query().map(|q| format!("?{q}")).unwrap_or_default();
            let hash = u.fragment().map(|f| format!("#{f}")).unwrap_or_default();
            (path, search, hash)
        }
        Err(_) => (String::new(), String::new(), String::new()),
    }
}

/// Render a Rust string as a JS string literal (JSON-encoding handles escaping).
fn js_str_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}
