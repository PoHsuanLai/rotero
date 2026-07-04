//! The JS-side sandbox shim, injected before a translator runs. It defines the
//! `Zotero`, `ZU`/`Zotero.Utilities`, and `doc` globals in terms of the Rust
//! host functions (`__saveItem`, `__xpathText`, `__xpathAll`, `__debug`).
//!
//! This is intentionally a JS shim rather than more Rust: the ported ZU utility
//! functions that are pure string logic (cleanDOI, trimInternal, …) are cheaper
//! and clearer to keep as JS here, and it mirrors how the upstream engine layers
//! `utilities.js` over the host. Host functions are only used where Rust must be
//! involved (the DOM/XPath, item output).

pub const SHIM: &str = r#"
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

// String helpers (pure JS ports of the common ZU functions).
ZU.trim = function (s) { return s == null ? "" : String(s).replace(/^\s+|\s+$/g, ""); };
ZU.trimInternal = function (s) { return s == null ? "" : String(s).replace(/\s+/g, " ").replace(/^\s+|\s+$/g, ""); };
ZU.superCleanString = function (s) { return ZU.trim(String(s).replace(/^[\s .,;:!?()\[\]{}]+|[\s .,;:!?()\[\]{}]+$/g, "")); };
ZU.cleanDOI = function (s) { if (!s) return null; var m = String(s).match(/10(?:\.[0-9]{4,})?\/[^\s]*[^\s.,]/); return m ? m[0] : null; };
ZU.cleanAuthor = function (name, type, useComma) {
    name = ZU.trimInternal(String(name || ""));
    var first = "", last = name;
    if (useComma && name.indexOf(",") !== -1) {
        var parts = name.split(",");
        last = ZU.trim(parts[0]);
        first = ZU.trim(parts.slice(1).join(","));
    } else {
        var idx = name.lastIndexOf(" ");
        if (idx !== -1) { first = name.slice(0, idx); last = name.slice(idx + 1); }
    }
    return { firstName: first, lastName: last, creatorType: type || "author" };
};
ZU.capitalizeTitle = function (s) { return s == null ? "" : String(s); };
ZU.getPageRange = function (s) {
    var m = String(s || "").match(/^\s*([0-9]+)\s*[-–]\s*([0-9]+)\s*$/);
    return m ? [m[1], m[2]] : [s, s];
};

// A minimal `doc` placeholder — translators pass it back into ZU.xpath*, which
// ignore it (the engine holds the real DOM). Present so `doc.location` etc. and
// truthiness checks don't throw.
var doc = { location: { href: "" }, documentElement: {}, title: "" };
"#;
