//! Runs an unmodified upstream Zotero translator's JavaScript in an embedded
//! `boa` interpreter, backed by Rust host functions for the `Zotero.*` / `ZU.*`
//! surface the translators call.
//!
//! Supports web translators that read the DOM via `ZU.xpath`/`ZU.xpathText` /
//! `doc.querySelector*`, emit items via `Zotero.Item` and `.complete()`, and
//! delegate to built-in hub translators (Embedded Metadata) via a synchronous
//! `Zotero.loadTranslator("web")` bridge. Not yet implemented: the full `ZU`
//! utility set, search/import delegation (needs async HTTP), and export IO.

mod http;
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
        if let Err(e) = ctx.eval(Source::from_bytes(driver.as_bytes())) {
            return Err(format!("translator run error: {}", error_detail(&e, &mut ctx)));
        }

        // Drain the job queue so promises settle: `async doWeb` and
        // `await requestText(...)` (as arXiv uses) only complete once their
        // microtasks run. HTTP host functions are blocking, so this converges.
        //
        // A job error is non-fatal: a translator may run several async scrapers
        // (e.g. Nature tries Embedded Metadata *and* an RIS fetch) and one path
        // throwing must not discard items another path already emitted. Log and
        // keep whatever reached the sink.
        if let Err(e) = ctx.run_jobs() {
            let detail = error_detail(&e, &mut ctx);
            tracing::debug!("translator job error (keeping emitted items): {detail}");
        }
        Ok::<(), String>(())
    })();

    let items = SINK.with(|s| std::mem::take(&mut *s.borrow_mut()));
    DOM.with(|d| *d.borrow_mut() = None);
    PAGE.with(|p| *p.borrow_mut() = Page::empty());
    result?;
    Ok(items)
}

/// Render a thrown JS error with its `message` and `stack` (if present) for
/// diagnostics — boa's default `Display` gives only the type and message, which
/// isn't enough to locate a failure inside a large translator.
fn error_detail(err: &boa_engine::JsError, ctx: &mut Context) -> String {
    let opaque = err.to_opaque(ctx);
    let read = |key: &str, ctx: &mut Context| -> Option<String> {
        opaque
            .as_object()
            .and_then(|o| o.get(js_string!(key), ctx).ok())
            .filter(|v| !v.is_undefined() && !v.is_null())
            .and_then(|v| v.to_string(ctx).ok())
            .map(|s| s.to_std_string_escaped())
    };
    match read("stack", ctx) {
        Some(stack) if !stack.is_empty() => format!("{err} | stack: {stack}"),
        _ => err.to_string(),
    }
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

/// Run a built-in *import* translator over a string for `loadTranslator('import')`
/// delegation (e.g. a site translator that fetches an RIS/BibTeX citation and
/// replays it through an import translator's `itemDone`). Returns the parsed
/// items as a JSON array, or `[]` if the target UUID has no in-process bridge.
fn delegate_import(uuid: &str, input: &str) -> String {
    // RIS — the dominant import delegation target (Nature and many others fetch
    // an RIS export and hand it to this translator).
    const RIS: &str = "32d59d2d-b65a-4da4-b0a3-bdd3cfb979e7";
    // BibTeX.
    const BIBTEX: &str = "9cb70025-a888-4a29-a210-93ec52da40d4";

    let papers = match uuid {
        RIS => rotero_bib::import_ris::import_ris(input),
        BIBTEX => rotero_bib::import_bibtex::import_bibtex(input)
            .map(|imported| imported.into_iter().map(|p| p.paper).collect::<Vec<_>>()),
        _ => {
            tracing::debug!("loadTranslator(import): no in-process bridge for {uuid}");
            Ok(Vec::new())
        }
    };
    let items: Vec<ZoteroItem> = match papers {
        Ok(papers) => papers
            .into_iter()
            .map(ZoteroItem::from_paper)
            .filter(|it| !it.title.is_empty())
            .collect(),
        Err(e) => {
            tracing::debug!("loadTranslator(import) parse failed: {e}");
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
            Ok(mut item) => {
                // Translators often emit relative attachment URLs (e.g.
                // "paper.pdf"); resolve them against the page URL so downstream
                // consumers get a fetchable absolute URL. Real Zotero resolves
                // attachment URLs against the document — this mirrors that.
                let base = PAGE.with(|p| p.borrow().url.clone());
                resolve_item_urls(&mut item, &base);
                SINK.with(|s| s.borrow_mut().push(item));
            }
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

    // __xpathCount(expr): number of nodes matching an XPath (for doc.evaluate
    // iteration bounds).
    let xpath_count = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let expr = arg_string(args, 0, ctx)?;
        let n = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .map(|dom| dom.xpath_count(&expr))
                .unwrap_or(0)
        });
        Ok(JsValue::from(n as i32))
    });
    ctx.register_global_callable(js_string!("__xpathCount"), 1, xpath_count)
        .map_err(|e| format!("register __xpathCount: {e}"))?;

    // __xpathNodeAttr(expr, index, attr): attribute of the index-th XPath match.
    let xpath_node_attr = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let expr = arg_string(args, 0, ctx)?;
        let index = arg_usize(args, 1, ctx)?;
        let attr = arg_string(args, 2, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .and_then(|dom| dom.xpath_node_attr(&expr, index, &attr))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__xpathNodeAttr"), 3, xpath_node_attr)
        .map_err(|e| format!("register __xpathNodeAttr: {e}"))?;

    // __xpathNodeText(expr, index): text of the index-th XPath match.
    let xpath_node_text = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let expr = arg_string(args, 0, ctx)?;
        let index = arg_usize(args, 1, ctx)?;
        let out = DOM.with(|d| {
            d.borrow()
                .as_ref()
                .and_then(|dom| dom.xpath_node_text(&expr, index))
                .unwrap_or_default()
        });
        Ok(JsValue::from(js_string!(out)))
    });
    ctx.register_global_callable(js_string!("__xpathNodeText"), 2, xpath_node_text)
        .map_err(|e| format!("register __xpathNodeText: {e}"))?;

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

    // __loadImportTranslator(uuid, string): parse `string` with the built-in
    // import translator for `uuid` (RIS/BibTeX) and return items as a JSON array
    // (for Zotero.loadTranslator('import') + setString delegation). "[]" if
    // there's no bridge.
    let load_import = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let uuid = arg_string(args, 0, ctx)?;
        let input = arg_string(args, 1, ctx)?;
        let json = delegate_import(&uuid, &input);
        Ok(JsValue::from(js_string!(json)))
    });
    ctx.register_global_callable(js_string!("__loadImportTranslator"), 2, load_import)
        .map_err(|e| format!("register __loadImportTranslator: {e}"))?;

    // __setActiveDom(html, url): re-point the active DOM/page at a freshly
    // fetched document, so a processDocuments/requestDocument callback that
    // queries `doc` sees the fetched page. Returns "1" on success, "" on parse
    // failure. The caller restores the original DOM afterward via the same fn.
    let set_active_dom = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let html = arg_string(args, 0, ctx)?;
        let url = arg_string(args, 1, ctx)?;
        let ok = match ParsedDom::parse(&html) {
            Ok(dom) => {
                DOM.with(|d| *d.borrow_mut() = Some(dom));
                PAGE.with(|p| *p.borrow_mut() = Page { html, url });
                true
            }
            Err(e) => {
                tracing::debug!("__setActiveDom parse failed: {e}");
                false
            }
        };
        Ok(JsValue::from(js_string!(if ok { "1" } else { "" })))
    });
    ctx.register_global_callable(js_string!("__setActiveDom"), 2, set_active_dom)
        .map_err(|e| format!("register __setActiveDom: {e}"))?;

    // __currentHtml(): the active page's HTML, so processDocuments can save and
    // restore the original document around fetched ones.
    let current_html = NativeFunction::from_copy_closure(|_this, _args, _ctx| {
        let html = PAGE.with(|p| p.borrow().html.clone());
        Ok(JsValue::from(js_string!(html)))
    });
    ctx.register_global_callable(js_string!("__currentHtml"), 0, current_html)
        .map_err(|e| format!("register __currentHtml: {e}"))?;

    // __debug(msg): route translator debug output to tracing.
    let debug = NativeFunction::from_copy_closure(|_this, args, ctx| {
        if let Some(v) = args.first() {
            let s = v.to_string(ctx)?.to_std_string_escaped();
            if std::env::var("ROTERO_ENGINE_DEBUG").is_ok() {
                eprintln!("[engine] {s}");
            }
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

    // __strToDate(s): parse a date string via the Rust ZU date port. Returns
    // JSON { year, month, day } with null for absent components (month/day
    // 1-based). Backs ZU.strToDate / ZU.strToISO in the sandbox.
    let str_to_date = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let s = arg_string(args, 0, ctx)?;
        let d = crate::zu::str_to_date(&s);
        let json = serde_json::json!({
            "year": d.year,
            "month": d.month,
            "day": d.day,
        });
        Ok(JsValue::from(js_string!(json.to_string())))
    });
    ctx.register_global_callable(js_string!("__strToDate"), 1, str_to_date)
        .map_err(|e| format!("register __strToDate: {e}"))?;

    // __httpGet(url): blocking GET. Returns a JSON { ok, body|error } so the JS
    // shim can invoke the translator's success/failure callback appropriately.
    let http_get = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = arg_string(args, 0, ctx)?;
        Ok(JsValue::from(js_string!(http_result(http::get(&url)))))
    });
    ctx.register_global_callable(js_string!("__httpGet"), 1, http_get)
        .map_err(|e| format!("register __httpGet: {e}"))?;

    // __httpPost(url, body, contentType): blocking POST, same result shape.
    let http_post = NativeFunction::from_copy_closure(|_this, args, ctx| {
        let url = arg_string(args, 0, ctx)?;
        let body = arg_string(args, 1, ctx)?;
        let content_type = {
            let c = arg_string(args, 2, ctx)?;
            if c.is_empty() {
                "application/x-www-form-urlencoded".to_string()
            } else {
                c
            }
        };
        Ok(JsValue::from(js_string!(http_result(http::post(
            &url,
            &body,
            &content_type
        )))))
    });
    ctx.register_global_callable(js_string!("__httpPost"), 3, http_post)
        .map_err(|e| format!("register __httpPost: {e}"))?;

    Ok(())
}

/// Serialize an HTTP host-function result to the `{ ok, body|error }` JSON the
/// sandbox's ZU HTTP helpers consume.
fn http_result(r: Result<String, String>) -> String {
    let v = match r {
        Ok(body) => serde_json::json!({ "ok": true, "body": body }),
        Err(error) => {
            tracing::debug!("engine http: {error}");
            serde_json::json!({ "ok": false, "error": error })
        }
    };
    v.to_string()
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

/// Resolve an item's own URL and its attachment URLs against the page `base`,
/// turning translator-emitted relative URLs (e.g. `paper.pdf`) into absolute
/// ones. A `base` that doesn't parse, or already-absolute URLs, leave things
/// unchanged.
fn resolve_item_urls(item: &mut ZoteroItem, base: &str) {
    let Ok(base_url) = reqwest::Url::parse(base) else {
        return;
    };
    resolve_in_place(&mut item.url, &base_url);
    for att in &mut item.attachments {
        resolve_in_place(&mut att.url, &base_url);
    }
}

/// Resolve `url` against `base` if it's relative; no-op for empty or already
/// absolute URLs (`Url::join` returns the argument when it's absolute).
fn resolve_in_place(url: &mut String, base: &reqwest::Url) {
    if url.is_empty() {
        return;
    }
    if let Ok(joined) = base.join(url) {
        *url = joined.to_string();
    }
}
