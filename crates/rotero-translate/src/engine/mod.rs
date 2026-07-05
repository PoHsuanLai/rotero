//! Runs an unmodified upstream Zotero translator's JavaScript in an embedded
//! `boa` interpreter, backed by Rust host functions for the `Zotero.*` / `ZU.*`
//! surface the translators call.
//!
//! Supports web translators that read the DOM via `ZU.xpath`/`ZU.xpathText` and
//! emit items via `Zotero.Item` and `.complete()`. Not yet implemented: the
//! full `ZU` utility set, `loadTranslator` delegation, and import/export IO.

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

    let result = (|| {
        let mut ctx = Context::default();
        register_host_functions(&mut ctx)?;

        // Inject the sandbox shim (defines Zotero, ZU, doc) then the translator.
        ctx.eval(Source::from_bytes(sandbox::SHIM))
            .map_err(|e| format!("sandbox shim error: {e}"))?;
        ctx.eval(Source::from_bytes(source))
            .map_err(|e| format!("translator load error: {e}"))?;

        // detectWeb(doc, url) → run doWeb only if truthy.
        let driver = format!(
            r#"
            (function() {{
                var __url = {url};
                var __detected = (typeof detectWeb === 'function') ? detectWeb(doc, __url) : false;
                if (__detected) {{
                    if (typeof doWeb === 'function') doWeb(doc, __url);
                }}
                return __detected ? String(__detected) : "";
            }})()
            "#,
            url = js_str_literal(url),
        );
        ctx.eval(Source::from_bytes(driver.as_bytes()))
            .map_err(|e| format!("translator run error: {e}"))?;
        Ok::<(), String>(())
    })();

    let items = SINK.with(|s| std::mem::take(&mut *s.borrow_mut()));
    DOM.with(|d| *d.borrow_mut() = None);
    result?;
    Ok(items)
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

/// Render a Rust string as a JS string literal (JSON-encoding handles escaping).
fn js_str_literal(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".to_string())
}
