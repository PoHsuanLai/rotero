// The JS-side sandbox shim, injected before a translator runs. Defines the
// `Zotero`, `ZU`/`Zotero.Utilities`, and `doc` globals in terms of the Rust
// host functions (__saveItem, __xpathText, __xpathAll, __debug, __cleanAuthor).
//
// This is intentionally a JS shim rather than more Rust: the pure string-logic
// ZU functions (cleanDOI, trimInternal, …) are cheaper and clearer as JS, and
// it mirrors how the upstream engine layers utilities.js over the host. Host
// functions are used only where Rust must be involved (DOM/XPath, item output,
// name-particle handling).

// --- Polyfills for methods boa doesn't implement ---
// boa (our JS engine) omits the legacy `String.prototype.substr`. Many upstream
// translators (and a couple of ZU ports below) still use it, so provide a
// spec-faithful implementation. Without this, `"...".substr(...)` throws
// "TypeError: not a callable function" and aborts the whole translator run.
if (typeof String.prototype.substr !== "function") {
    String.prototype.substr = function (start, length) {
        var str = String(this);
        var size = str.length;
        start = start === undefined ? 0 : Number(start);
        if (isNaN(start)) start = 0;
        if (start < 0) start = Math.max(size + start, 0);
        var len = length === undefined ? Infinity : Number(length);
        if (isNaN(len) || len < 0) len = 0;
        var end = Math.min(start + len, size);
        return start >= size || start >= end ? "" : str.slice(start, end);
    };
}

var Zotero = {};

// --- Item ---
Zotero.Item = function (itemType) {
    this.itemType = itemType || "";
    this.creators = [];
    this.tags = [];
    this.attachments = [];
    this.notes = [];
};
Zotero.Item.prototype.complete = function () {
    // Serialize to the ZoteroItem shape the Rust side deserializes. Field names
    // match serde(rename_all = camelCase): itemType, title, creators, DOI, etc.
    var out = {};
    for (var k in this) {
        if (!this.hasOwnProperty(k)) continue;
        var v = this[k];
        if (v === undefined || v === null) continue;
        out[k] = v;
    }
    __saveItem(JSON.stringify(out));
};

Zotero.debug = function (msg) { try { __debug(String(msg)); } catch (e) {} };
// upstream utilities.js reads Zotero.Prefs.get('capitalizeTitles'); default on.
Zotero.Prefs = { get: function () { return true; } };
Zotero.done = function () {};

// --- Import input (Zotero.read line pump) ---
// Import translators (RIS/BibTeX/…) consume their input a line at a time via
// `Zotero.read()`, which returns the next line (without its terminator) and
// `false` at EOF. `__setImportInput` seeds the buffer before an import run; the
// pump splits on CR/LF so both `\n` and `\r\n` sources read one logical line at a
// time, mirroring Zotero's line reader.
var __importLines = [];
var __importPos = 0;
function __setImportInput(s) {
    var str = String(s == null ? "" : s);
    __importLines = str.length ? str.split(/\r\n|\r|\n/) : [];
    // A trailing newline yields a final empty element; drop it so EOF is reached
    // right after the last real line, as Zotero's reader does.
    if (__importLines.length && __importLines[__importLines.length - 1] === "") {
        __importLines.pop();
    }
    __importPos = 0;
}
Zotero.read = function () {
    if (__importPos >= __importLines.length) return false;
    return __importLines[__importPos++];
};
Zotero.getString = function () { return ""; };
Zotero.setCharacterSet = function () {};
Zotero.wait = function () {};
Zotero.setProgress = function () {};
Zotero.getOption = function () { return undefined; };
Zotero.getHiddenPref = function () { return undefined; };
// Multi-item selection isn't supported in this single-shot engine: return no
// selection so the "multiple" path yields nothing (the single-article path,
// which is what scraping a specific URL hits, doesn't call this).
Zotero.selectItems = function (items, callback) { if (callback) callback(null); return null; };

// --- loadTranslator delegation ---
// A site translator delegates the actual extraction to a hub translator (most
// often Embedded Metadata). Two call styles are used upstream:
//
//   // (a) translate():
//   var t = Zotero.loadTranslator("web");
//   t.setTranslator(uuid); t.setDocument(doc);
//   t.setHandler("itemDone", function (obj, item) { /* enrich */ item.complete(); });
//   t.translate();
//
//   // (b) getTranslatorObject(): the caller drives the delegate's doWeb itself
//   t.setHandler("itemDone", ...);
//   t.getTranslatorObject(function (trans) { trans.doWeb(doc, url); });
//
// Both resolve to the same thing here: the Rust host runs the built-in hub for
// `uuid` against the current page and returns its items; we replay them through
// the itemDone handler as live Zotero.Item objects so the handler can enrich and
// complete them. If no itemDone handler is set, items are completed as-is.
Zotero.loadTranslator = function (type) {
    var handlers = {};
    var self = {
        _uuid: "",
        setTranslator: function (uuid) { self._uuid = String(uuid); },
        setDocument: function () {},
        setSearch: function () {},
        setString: function (s) { self._string = String(s == null ? "" : s); },
        setHandler: function (name, fn) { handlers[name] = fn; },
        translate: function () { self._run(); },
        // Hand back a translator-shaped object whose detectWeb/doWeb run the
        // delegate. The caller's `trans.doWeb(doc, url)` therefore replays items
        // through the itemDone handler exactly as translate() would.
        //
        // Upstream supports two call forms: the callback form
        // `t.getTranslatorObject(function (trans) { ... })` and the
        // promise form `let trans = await t.getTranslatorObject()`. Both must
        // yield the proxy: return it *and* invoke the callback if present, so a
        // translator that `await`s the result (e.g. Frontiers, whose EM
        // delegation drops all fields otherwise) gets a real object rather than
        // `undefined`.
        getTranslatorObject: function (cb) {
            var proxy = {
                detectWeb: function () { return "journalArticle"; },
                doWeb: function () { return self._run(); }
            };
            if (cb) cb(proxy);
            return Promise.resolve(proxy);
        },
        _run: function () {
            var items = [];
            if (type === "web") {
                try { items = JSON.parse(__loadTranslator(self._uuid)); } catch (e) { items = []; }
            } else if (type === "import") {
                // A site translator hands us a fetched RIS/BibTeX string via
                // setString and drives its import translator; parse it host-side.
                try { items = JSON.parse(__loadImportTranslator(self._uuid, self._string || "")); } catch (e) { items = []; }
            }
            // else: search/export delegation not bridged yet → no items.
            for (var i = 0; i < items.length; i++) {
                var item = __reviveItem(items[i]);
                if (handlers.itemDone) {
                    handlers.itemDone(self, item);
                } else {
                    item.complete();
                }
            }
            // If a delegated translator produced nothing, a caller that guards
            // with an "error" handler (e.g. Nature's scrapeRIS) expects it to
            // fire so its continuation still runs. Fire it once, then done.
            if (!items.length && handlers.error) handlers.error();
            if (handlers.done) handlers.done();
        }
    };
    return self;
};

// Rebuild a plain parsed item object into a live Zotero.Item so handlers can
// mutate fields and call .complete().
function __reviveItem(obj) {
    var item = new Zotero.Item(obj.itemType || "");
    for (var k in obj) {
        if (obj.hasOwnProperty(k)) item[k] = obj[k];
    }
    return item;
}

// `Z` is the conventional upstream alias for `Zotero`.
var Z = Zotero;

// --- Utilities (ZU) ---
var ZU = {};
Zotero.Utilities = ZU;

// DOM/XPath → host functions. `doc`/`node` args are ignored (single-document
// engine); the expression is evaluated against the current page. Installed onto
// ZU by __applyZUOverrides after upstream utilities loads.
function xpathNodes(node, xpath) {
    // Return an array of pseudo-nodes exposing textContent, which is what most
    // translators read after an xpath() call.
    var vals = JSON.parse(__xpathAll(xpath));
    return vals.map(function (s) { return { textContent: s, innerText: s, nodeValue: s }; });
}

// --- CSS-selector DOM API ---
// A node is `{ __h: <handle> }` plus lazy textContent/getAttribute. Handle 0 is
// the document root, so `doc` and returned nodes share one query path. Handles
// index a Rust-side node table (see engine/dom).
function __scopeHandle(node) {
    if (node === doc || node == null) return 0;
    return (typeof node.__h === "number") ? node.__h : 0;
}
function __wrapNode(handle) {
    var n = { __h: handle };
    Object.defineProperty(n, "textContent", { get: function () { return __nodeText(handle); } });
    Object.defineProperty(n, "innerText", { get: function () { return __nodeText(handle); } });
    n.getAttribute = function (name) { var v = __nodeAttr(handle, String(name)); return v === "" ? null : v; };
    // Live DOM elements expose an `.href` (resolved absolute) on <a>/<link>; many
    // translators read it directly (e.g. `node.href`). Mirror that by resolving
    // the raw href attribute against the page URL.
    Object.defineProperty(n, "href", { get: function () {
        var raw = __nodeAttr(handle, "href");
        return raw === "" ? "" : __resolveUrl(raw);
    } });
    n.querySelector = function (sel) { return __querySelector(handle, sel); };
    n.querySelectorAll = function (sel) { return __querySelectorAll(handle, sel); };
    return n;
}
// Resolve a possibly-relative URL against the current page URL. Falls back to
// the raw value if resolution isn't possible.
function __resolveUrl(raw) {
    if (raw == null || raw === "") return raw || "";
    var s = String(raw);
    if (/^[a-zA-Z][a-zA-Z0-9+.-]*:/.test(s) || s.indexOf("//") === 0) return s;
    var base = (doc && doc.location && doc.location.href) || "";
    if (!base) return s;
    try {
        if (s.charAt(0) === "/") {
            var m = base.match(/^([a-zA-Z][\w+.-]*:\/\/[^\/]+)/);
            return m ? m[1] + s : s;
        }
        var cut = base.replace(/[?#].*$/, "");
        cut = cut.replace(/\/[^\/]*$/, "/");
        return cut + s;
    } catch (e) { return s; }
}
function __querySelectorAll(scope, sel) {
    var handles = JSON.parse(__cssSelect(scope, String(sel)));
    return handles.map(__wrapNode);
}
function __querySelector(scope, sel) {
    var all = __querySelectorAll(scope, sel);
    return all.length ? all[0] : null;
}

// translate.js globals: text(node, sel[, index]) and attr(node, sel, attr[, index]).
function text(node, sel, index) {
    return __cssText(__scopeHandle(node), String(sel), index ? index : 0);
}
function attr(node, sel, attribute, index) {
    return __cssAttr(__scopeHandle(node), String(sel), String(attribute), index ? index : 0);
}
ZU.text = text;
ZU.attr = attr;

// The pure-JS string utilities (trim, capitalizeTitle, cleanTags, cleanISBN,
// removeDiacritics, …) come from the vendored upstream `utilities.js`, loaded
// after this shim. Only the functions that must bridge to the Rust host — the
// DOM (xpath/CSS), the blocking HTTP helpers, the Rust author/date ports, and the
// schema stubs we can't answer without the item schema — are (re)installed onto
// `Zotero.Utilities` by `__applyZUOverrides`, run once utilities.js has assigned
// `Zotero.Utilities`. Keeping these as overrides means upstream's versions (which
// assume a real DOM / app internals) never shadow the host-backed ones.
function __applyZUOverrides() {
    ZU = Zotero.Utilities;

    // DOM access — upstream's xpath needs a real document; ours queries the
    // engine's active DOM via host functions.
    ZU.xpathText = function (node, xpath) { var t = __xpathText(xpath); return t === "" ? null : t; };
    ZU.xpath = xpathNodes;
    ZU.text = text;
    ZU.attr = attr;

    // Author/date parsing routes to the Rust ports (surname particles; the
    // multi-format date parser). strToDate returns month/day 1-based (upstream is
    // 0-based); translators mostly consume via strToISO, which compensates.
    ZU.cleanAuthor = function (name, type, useComma) {
        return JSON.parse(__cleanAuthor(String(name || ""), String(type || "author"), !!useComma));
    };
    ZU.strToDate = function (s) {
        var d = JSON.parse(__strToDate(String(s)));
        return { year: d.year, month: d.month, day: d.day };
    };
    ZU.strToISO = function (s) {
        if (!s) return "";
        var str = String(s);
        var iso = str.match(/\b(\d{4})-(\d{2})-(\d{2})\b/);
        if (iso) return iso[0];
        var d = ZU.strToDate(str);
        if (!d.year) return "";
        var out = ZU.lpad(d.year, "0", 4);
        if (d.month != null) out += "-" + ZU.lpad(d.month, "0", 2);
        if (d.day != null) out += "-" + ZU.lpad(d.day, "0", 2);
        return out;
    };

    // Schema guards: without the item schema we accept optimistically, so a
    // translator's optional-field checks take the common path.
    ZU.fieldIsValidForType = function () { return true; };
    ZU.itemTypeExists = function () { return true; };

    // Blocking HTTP helpers (relative-URL resolution + header forwarding + the
    // browser-proxy broker) — installed below as they depend on host functions.
    __applyZUHttp();
}

// --- HTTP (blocking, via host functions) ---
// Translators fetch over the network with callback-style helpers. The host does
// a synchronous request (we run on a blocking thread) and we invoke the callback
// with the body. Errors invoke the optional failure/`done` callback with "".
//
// Every URL is resolved against the page URL via `__resolveUrl` first: gated
// publishers build *relative* follow-up URLs (IEEE `/rest/search/citation/...`,
// Atypon `/action/downloadCitation`, JSTOR `/citation/ris/...`) and the host's
// scheme guard rejects a bare path. Request `headers` (esp. `Referer`, which
// several publishers require) are serialized to a JSON string and threaded to
// the host GET/POST.

// Serialize a headers object to a JSON string the host fns parse, or "" if none.
function __headersJson(h) {
    if (!h || typeof h !== "object") return "";
    try {
        var out = {};
        var any = false;
        for (var k in h) {
            if (Object.prototype.hasOwnProperty.call(h, k) && h[k] != null) {
                out[k] = String(h[k]);
                any = true;
            }
        }
        return any ? JSON.stringify(out) : "";
    } catch (e) { return ""; }
}

// doGet(urls, processor, done, responseCharset, headers)
function doGet(urls, processor, done, responseCharset, headers) {
    var list = Array.isArray(urls) ? urls : [urls];
    var hj = __headersJson(headers);
    for (var i = 0; i < list.length; i++) {
        var u = __resolveUrl(String(list[i]));
        var r = JSON.parse(__httpGet(u, hj));
        if (r.ok && processor) processor(r.body, {}, u);
        else if (!r.ok) __debug("doGet failed: " + (r.error || ""));
    }
    if (done) done();
}

// doPost(url, body, onDone, headers)
function doPost(url, body, onDone, headers) {
    var ct = (headers && (headers["Content-Type"] || headers["content-type"])) || "";
    var u = __resolveUrl(String(url));
    var r = JSON.parse(__httpPost(u, String(body == null ? "" : body), String(ct), __headersJson(headers)));
    if (r.ok && onDone) onDone(r.body, {});
    else if (!r.ok) { __debug("doPost failed: " + (r.error || "")); if (onDone) onDone("", {}); }
}

// processDocuments(urls, processor, done): fetch each URL, make it the active
// document, and hand a `doc` to the processor. Restores the original document
// afterward so the outer translator's later queries are unaffected.
function processDocuments(urls, processor, done) {
    var list = Array.isArray(urls) ? urls : [urls];
    var savedHtml = __currentHtml();
    var savedUrl = doc.location.href;
    for (var i = 0; i < list.length; i++) {
        var u = __resolveUrl(String(list[i]));
        var r = JSON.parse(__httpGet(u, ""));
        if (!r.ok) { __debug("processDocuments failed: " + (r.error || "")); continue; }
        if (__setActiveDom(r.body, u) && processor) {
            var fetched = __makeDoc(u);
            processor(fetched, u);
        }
    }
    // Restore the original active document.
    if (savedHtml !== "") __setActiveDom(savedHtml, savedUrl);
    if (done) done();
}

// Promise-based request API (arXiv-style: `await requestText(url)`). Backed by
// the same blocking host GET; resolves synchronously so the engine's microtask
// drain completes the await. The optional `opts` mirrors Zotero's
// `request(url, { headers, ... })` — we thread `opts.headers` (e.g. IEEE's
// `{ headers: { Referer: url } }`) to the host.
function requestText(url, opts) {
    var hj = __headersJson(opts && opts.headers);
    var r = JSON.parse(__httpGet(__resolveUrl(String(url)), hj));
    if (r.ok) return Promise.resolve(r.body);
    return Promise.reject(new Error(r.error || "request failed"));
}
function requestJSON(url, opts) {
    return requestText(url, opts).then(function (t) { return JSON.parse(t); });
}
function requestDocument(url, opts) {
    var hj = __headersJson(opts && opts.headers);
    var u = __resolveUrl(String(url));
    var r = JSON.parse(__httpGet(u, hj));
    if (!r.ok) return Promise.reject(new Error(r.error || "request failed"));
    __setActiveDom(r.body, u);
    return Promise.resolve(__makeDoc(u));
}
// Install the host-backed HTTP helpers onto ZU (= Zotero.Utilities). Called by
// __applyZUOverrides after upstream utilities loads, so these win over any
// upstream network helpers (which assume Zotero's app HTTP stack).
function __applyZUHttp() {
    ZU.doGet = doGet;
    ZU.doPost = doPost;
    ZU.processDocuments = processDocuments;
    ZU.requestText = requestText;
    ZU.requestJSON = requestJSON;
    ZU.requestDocument = requestDocument;
}

// XPathResult ordering constants translators pass to doc.evaluate (ignored; we
// always iterate in document order).
var XPathResult = { ANY_TYPE: 0, ORDERED_NODE_ITERATOR_TYPE: 5, FIRST_ORDERED_NODE_TYPE: 9 };

// A node returned by doc.evaluate: exposes textContent and attribute reads for
// the (expr, index) it represents. `.href` is the common one translators read.
function __xpathNode(expr, index) {
    var n = {};
    Object.defineProperty(n, "textContent", { get: function () { return __xpathNodeText(expr, index); } });
    Object.defineProperty(n, "innerText", { get: function () { return __xpathNodeText(expr, index); } });
    Object.defineProperty(n, "href", { get: function () { return __xpathNodeAttr(expr, index, "href"); } });
    n.getAttribute = function (name) { var v = __xpathNodeAttr(expr, index, String(name)); return v === "" ? null : v; };
    return n;
}

// doc.evaluate(expr, context, resolver, type, result) → an XPathResult with
// iterateNext()/singleNodeValue over the matched nodes. Context/resolver/type
// are ignored (single-document engine, always document-order).
function __evaluate(expr) {
    var count = __xpathCount(String(expr));
    var i = 0;
    return {
        iterateNext: function () { return i < count ? __xpathNode(String(expr), i++) : null; },
        get singleNodeValue() { return count > 0 ? __xpathNode(String(expr), 0) : null; },
        snapshotLength: count,
        snapshotItem: function (k) { return k < count ? __xpathNode(String(expr), k) : null; }
    };
}

// A `doc`-shaped handle over whatever is currently the active DOM. The main
// `doc` and every fetched document share this surface. (The active DOM is
// swapped by __setActiveDom before a fetched doc is built.)
function __makeDoc(url) {
    return {
        location: { href: url || "", pathname: "", search: "", hash: "" },
        documentElement: {},
        title: "",
        querySelector: function (sel) { return __querySelector(0, sel); },
        querySelectorAll: function (sel) { return __querySelectorAll(0, sel); },
        getElementById: function (id) { return __querySelector(0, "#" + id); },
        evaluate: function (expr) { return __evaluate(expr); }
    };
}

// A minimal DOMParser: parseFromString swaps the active DOM to the given markup
// and returns a doc handle over it. Enough for translators that fetch XML/HTML
// and immediately query it (e.g. arXiv's Atom feed).
function DOMParser() {}
DOMParser.prototype.parseFromString = function (str, contentType) {
    __setActiveDom(String(str), "");
    return __makeDoc("");
};

// The `doc` global. XPath/CSS host functions query the active DOM; location/
// title are patched per-run by the engine driver so doc.location.pathname etc.
// reflect the real URL.
var doc = __makeDoc("");
