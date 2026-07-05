// The JS-side sandbox shim, injected before a translator runs. Defines the
// `Zotero`, `ZU`/`Zotero.Utilities`, and `doc` globals in terms of the Rust
// host functions (__saveItem, __xpathText, __xpathAll, __debug, __cleanAuthor).
//
// This is intentionally a JS shim rather than more Rust: the pure string-logic
// ZU functions (cleanDOI, trimInternal, …) are cheaper and clearer as JS, and
// it mirrors how the upstream engine layers utilities.js over the host. Host
// functions are used only where Rust must be involved (DOM/XPath, item output,
// name-particle handling).

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
Zotero.done = function () {};
Zotero.wait = function () {};
Zotero.setProgress = function () {};
Zotero.getOption = function () { return undefined; };
Zotero.getHiddenPref = function () { return undefined; };

// --- loadTranslator delegation ---
// A site translator delegates the actual extraction to a hub translator (most
// often Embedded Metadata) via:
//   var t = Zotero.loadTranslator("web");
//   t.setTranslator(uuid); t.setDocument(doc);
//   t.setHandler("itemDone", function (obj, item) { /* enrich */ item.complete(); });
//   t.translate();
// The Rust host runs the built-in hub for `uuid` against the current page and
// returns its items; we replay them through the itemDone handler as live
// Zotero.Item objects so the handler can enrich and complete them. If no
// itemDone handler is set, items are completed as-is.
Zotero.loadTranslator = function (type) {
    var handlers = {};
    var self = {
        _uuid: "",
        setTranslator: function (uuid) { self._uuid = String(uuid); },
        setDocument: function () {},
        setSearch: function () {},
        setString: function () {},
        setHandler: function (name, fn) { handlers[name] = fn; },
        getTranslatorObject: function (cb) { if (cb) cb({}); },
        translate: function () {
            var items = [];
            if (type === "web") {
                try { items = JSON.parse(__loadTranslator(self._uuid)); } catch (e) { items = []; }
            }
            // else: search/import/export delegation not bridged yet → no items.
            for (var i = 0; i < items.length; i++) {
                var item = __reviveItem(items[i]);
                if (handlers.itemDone) {
                    handlers.itemDone(self, item);
                } else {
                    item.complete();
                }
            }
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

// --- Utilities (ZU) ---
var ZU = {};
Zotero.Utilities = ZU;

// DOM/XPath → host functions. `doc`/`node` args are ignored (single-document
// engine); the expression is evaluated against the current page.
ZU.xpathText = function (node, xpath) { var t = __xpathText(xpath); return t === "" ? null : t; };
ZU.xpath = function (node, xpath) {
    // Return an array of pseudo-nodes exposing textContent, which is what most
    // translators read after an xpath() call.
    var vals = JSON.parse(__xpathAll(xpath));
    return vals.map(function (s) { return { textContent: s, innerText: s, nodeValue: s }; });
};

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
    n.querySelector = function (sel) { return __querySelector(handle, sel); };
    n.querySelectorAll = function (sel) { return __querySelectorAll(handle, sel); };
    return n;
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

// String helpers (pure JS ports of the common ZU functions).
ZU.trim = function (s) { return s == null ? "" : String(s).replace(/^\s+|\s+$/g, ""); };
ZU.trimInternal = function (s) { return s == null ? "" : String(s).replace(/\s+/g, " ").replace(/^\s+|\s+$/g, ""); };
ZU.superCleanString = function (s) { return ZU.trim(String(s).replace(/^[\s .,;:!?()\[\]{}]+|[\s .,;:!?()\[\]{}]+$/g, "")); };
ZU.cleanDOI = function (s) { if (!s) return null; var m = String(s).match(/10(?:\.[0-9]{4,})?\/[^\s]*[^\s.,]/); return m ? m[0] : null; };
// Delegates to the Rust ZU port (correct surname-particle handling).
ZU.cleanAuthor = function (name, type, useComma) {
    return JSON.parse(__cleanAuthor(String(name || ""), String(type || "author"), !!useComma));
};
ZU.capitalizeTitle = function (s) { return s == null ? "" : String(s); };
ZU.getPageRange = function (s) {
    var m = String(s || "").match(/^\s*([0-9]+)\s*[-–]\s*([0-9]+)\s*$/);
    return m ? [m[1], m[2]] : [s, s];
};

// The `doc` global. XPath host functions ignore the node arg (the engine holds
// the real DOM), but CSS queries route through the shared node table with the
// document root as scope 0. `location`/`title` are patched per-run by the engine
// driver so `doc.location.href` / `doc.location.pathname` reflect the real URL.
var doc = {
    location: { href: "", pathname: "", search: "", hash: "" },
    documentElement: {},
    title: "",
    querySelector: function (sel) { return __querySelector(0, sel); },
    querySelectorAll: function (sel) { return __querySelectorAll(0, sel); },
    getElementById: function (id) { return __querySelector(0, "#" + id); },
    evaluate: undefined
};
