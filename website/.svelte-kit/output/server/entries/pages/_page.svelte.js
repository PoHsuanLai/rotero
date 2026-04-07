import { a2 as attr_class, a3 as store_get, a4 as unsubscribe_stores, a5 as ensure_array_like, e as escape_html, a6 as stringify, a7 as head } from "../../chunks/renderer.js";
import { w as writable } from "../../chunks/index.js";
import "clsx";
function getInitialTheme() {
  return "dark";
}
const theme = writable(getInitialTheme());
function Nav($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    var $$store_subs;
    let scrolled = false;
    let mobileOpen = false;
    $$renderer2.push(`<nav${attr_class("nav svelte-1h32yp1", void 0, { "scrolled": scrolled })}><div class="nav-inner container svelte-1h32yp1"><a href="/" class="wordmark svelte-1h32yp1">Rotero</a> <button class="hamburger svelte-1h32yp1" aria-label="Toggle menu"><span${attr_class("bar svelte-1h32yp1", void 0, { "open": mobileOpen })}></span> <span${attr_class("bar svelte-1h32yp1", void 0, { "open": mobileOpen })}></span> <span${attr_class("bar svelte-1h32yp1", void 0, { "open": mobileOpen })}></span></button> <div${attr_class("nav-links svelte-1h32yp1", void 0, { "mobile-open": mobileOpen })}><a href="#features" class="svelte-1h32yp1">Features</a> <a href="#why" class="svelte-1h32yp1">Why Rotero</a> <a href="#download" class="svelte-1h32yp1">Download</a></div> <div class="nav-actions svelte-1h32yp1"><button class="theme-toggle svelte-1h32yp1" aria-label="Toggle theme">`);
    if (store_get($$store_subs ??= {}, "$theme", theme) === "dark") {
      $$renderer2.push("<!--[0-->");
      $$renderer2.push(`<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"></circle><line x1="12" y1="1" x2="12" y2="3"></line><line x1="12" y1="21" x2="12" y2="23"></line><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"></line><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"></line><line x1="1" y1="12" x2="3" y2="12"></line><line x1="21" y1="12" x2="23" y2="12"></line><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"></line><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"></line></svg>`);
    } else {
      $$renderer2.push("<!--[-1-->");
      $$renderer2.push(`<svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"></path></svg>`);
    }
    $$renderer2.push(`<!--]--></button> <a href="https://github.com/pslai/rotero" class="github-link svelte-1h32yp1" target="_blank" rel="noopener" aria-label="GitHub"><svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"></path></svg></a></div></div></nav>`);
    if ($$store_subs) unsubscribe_stores($$store_subs);
  });
}
function Hero($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    let loaded = false;
    $$renderer2.push(`<section${attr_class("hero svelte-1q37ri0", void 0, { "loaded": loaded })}><div class="hero-bg svelte-1q37ri0"><div class="glow glow-1 svelte-1q37ri0"></div> <div class="glow glow-2 svelte-1q37ri0"></div></div> <div class="container hero-content svelte-1q37ri0"><div class="hero-text svelte-1q37ri0"><p class="hero-eyebrow svelte-1q37ri0">Paper reading, reimagined</p> <h1 class="hero-headline svelte-1q37ri0">Research,<br class="svelte-1q37ri0"/><span class="accent svelte-1q37ri0">refined.</span></h1> <p class="hero-sub svelte-1q37ri0">A fast, private, local-first reference manager built with Rust.
        Read, annotate, cite, and explore your papers — without the bloat.</p> <div class="hero-actions svelte-1q37ri0"><a href="#download" class="btn-primary svelte-1q37ri0">Download Rotero <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1q37ri0"><path d="M6 9l6 6 6-6" class="svelte-1q37ri0"></path></svg></a> <a href="https://github.com/pslai/rotero" target="_blank" rel="noopener" class="btn-ghost svelte-1q37ri0">View source <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1q37ri0"><line x1="7" y1="17" x2="17" y2="7" class="svelte-1q37ri0"></line><polyline points="7 7 17 7 17 17" class="svelte-1q37ri0"></polyline></svg></a></div></div> <div class="hero-visual svelte-1q37ri0"><div class="app-mock svelte-1q37ri0"><div class="mock-titlebar svelte-1q37ri0"><span class="dot red svelte-1q37ri0"></span> <span class="dot yellow svelte-1q37ri0"></span> <span class="dot green svelte-1q37ri0"></span></div> <div class="mock-body svelte-1q37ri0"><div class="mock-sidebar svelte-1q37ri0"><div class="mock-sidebar-item active svelte-1q37ri0"></div> <div class="mock-sidebar-item svelte-1q37ri0"></div> <div class="mock-sidebar-item svelte-1q37ri0"></div> <div class="mock-sidebar-divider svelte-1q37ri0"></div> <div class="mock-sidebar-item short svelte-1q37ri0"></div> <div class="mock-sidebar-item svelte-1q37ri0"></div></div> <div class="mock-main svelte-1q37ri0"><div class="mock-toolbar svelte-1q37ri0"><div class="mock-tab active svelte-1q37ri0"></div> <div class="mock-tab svelte-1q37ri0"></div></div> <div class="mock-pdf svelte-1q37ri0"><div class="mock-page svelte-1q37ri0"><div class="mock-line w-80 svelte-1q37ri0"></div> <div class="mock-line w-60 svelte-1q37ri0"></div> <div class="mock-line svelte-1q37ri0"></div> <div class="mock-line svelte-1q37ri0"></div> <div class="mock-line highlight svelte-1q37ri0"></div> <div class="mock-line highlight svelte-1q37ri0"></div> <div class="mock-line w-90 svelte-1q37ri0"></div> <div class="mock-line svelte-1q37ri0"></div> <div class="mock-line w-70 svelte-1q37ri0"></div> <div class="mock-line svelte-1q37ri0"></div> <div class="mock-line w-40 svelte-1q37ri0"></div></div></div></div> <div class="mock-detail svelte-1q37ri0"><div class="mock-detail-title svelte-1q37ri0"></div> <div class="mock-detail-meta svelte-1q37ri0"></div> <div class="mock-detail-meta short svelte-1q37ri0"></div> <div class="mock-detail-divider svelte-1q37ri0"></div> <div class="mock-detail-label svelte-1q37ri0"></div> <div class="mock-detail-tag svelte-1q37ri0"></div> <div class="mock-detail-tag svelte-1q37ri0"></div></div></div></div> <div class="float-annotation svelte-1q37ri0"><svg width="16" height="16" viewBox="0 0 24 24" fill="var(--accent)" opacity="0.8" class="svelte-1q37ri0"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="var(--accent)" stroke-width="2" class="svelte-1q37ri0"></rect><path d="M8 7h8M8 11h5" stroke="var(--accent)" stroke-width="2" stroke-linecap="round" class="svelte-1q37ri0"></path></svg></div> <div class="float-cite svelte-1q37ri0"><span class="svelte-1q37ri0">[1]</span></div></div></div></section>`);
  });
}
function Features($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    const features = [
      {
        title: "PDF Annotations",
        desc: "Highlights, sticky notes, underlines, ink, and area annotations — saved directly in your PDFs.",
        icon: "highlight"
      },
      {
        title: "Smart Import",
        desc: "742 community web translators scrape metadata from Google Scholar, arXiv, PubMed, and 40+ academic sites.",
        icon: "import"
      },
      {
        title: "Full-Text Search",
        desc: "Instant full-text search across your entire library, powered by SQLite FTS.",
        icon: "search"
      },
      {
        title: "Citation Generation",
        desc: "14 CSL styles including APA, IEEE, Chicago, Harvard, and MLA. Export to BibTeX with auto-sync.",
        icon: "cite"
      },
      {
        title: "Browser Extension",
        desc: "Chrome extension for one-click saving from arXiv, DOI.org, PubMed, Semantic Scholar, and more.",
        icon: "extension"
      },
      {
        title: "Cloud Sync",
        desc: "File-based sync via Dropbox, iCloud, or Google Drive. Your data, your storage.",
        icon: "sync"
      }
    ];
    $$renderer2.push(`<section id="features" class="svelte-1dpem8h"><div class="container svelte-1dpem8h"><div class="section-header reveal svelte-1dpem8h"><p class="section-eyebrow svelte-1dpem8h">Capabilities</p> <h2 class="section-title svelte-1dpem8h">Everything you need,<br class="svelte-1dpem8h"/>nothing you don't.</h2></div> <div class="features-grid svelte-1dpem8h"><!--[-->`);
    const each_array = ensure_array_like(features);
    for (let i = 0, $$length = each_array.length; i < $$length; i++) {
      let feature = each_array[i];
      $$renderer2.push(`<div${attr_class(`feature-card reveal reveal-delay-${stringify(i + 1)}`, "svelte-1dpem8h")}><div class="feature-icon svelte-1dpem8h">`);
      if (feature.icon === "highlight") {
        $$renderer2.push("<!--[0-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><path d="M12 20h9" class="svelte-1dpem8h"></path><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" class="svelte-1dpem8h"></path></svg>`);
      } else if (feature.icon === "import") {
        $$renderer2.push("<!--[1-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" class="svelte-1dpem8h"></path><polyline points="7 10 12 15 17 10" class="svelte-1dpem8h"></polyline><line x1="12" y1="15" x2="12" y2="3" class="svelte-1dpem8h"></line></svg>`);
      } else if (feature.icon === "search") {
        $$renderer2.push("<!--[2-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><circle cx="11" cy="11" r="8" class="svelte-1dpem8h"></circle><line x1="21" y1="21" x2="16.65" y2="16.65" class="svelte-1dpem8h"></line></svg>`);
      } else if (feature.icon === "cite") {
        $$renderer2.push("<!--[3-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><path d="M6 17C3.5 17 2 15 2 12s1.5-5 4-5c1.5 0 2.5 1 3 2" class="svelte-1dpem8h"></path><path d="M15 17c-2.5 0-4-2-4-5s1.5-5 4-5c1.5 0 2.5 1 3 2" class="svelte-1dpem8h"></path><path d="M10 19h4" class="svelte-1dpem8h"></path></svg>`);
      } else if (feature.icon === "extension") {
        $$renderer2.push("<!--[4-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z" class="svelte-1dpem8h"></path></svg>`);
      } else if (feature.icon === "sync") {
        $$renderer2.push("<!--[5-->");
        $$renderer2.push(`<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><polyline points="23 4 23 10 17 10" class="svelte-1dpem8h"></polyline><polyline points="1 20 1 14 7 14" class="svelte-1dpem8h"></polyline><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15" class="svelte-1dpem8h"></path></svg>`);
      } else {
        $$renderer2.push("<!--[-1-->");
      }
      $$renderer2.push(`<!--]--></div> <h3 class="feature-title svelte-1dpem8h">${escape_html(feature.title)}</h3> <p class="feature-desc svelte-1dpem8h">${escape_html(feature.desc)}</p></div>`);
    }
    $$renderer2.push(`<!--]--></div> <div class="showcase reveal svelte-1dpem8h"><div class="showcase-text svelte-1dpem8h"><div class="showcase-eyebrow svelte-1dpem8h"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><circle cx="6" cy="6" r="3" class="svelte-1dpem8h"></circle><circle cx="18" cy="6" r="3" class="svelte-1dpem8h"></circle><circle cx="12" cy="18" r="3" class="svelte-1dpem8h"></circle><line x1="8.5" y1="7.5" x2="10" y2="16" class="svelte-1dpem8h"></line><line x1="15.5" y1="7.5" x2="14" y2="16" class="svelte-1dpem8h"></line><line x1="9" y1="6" x2="15" y2="6" class="svelte-1dpem8h"></line></svg> Citation Graph</div> <h3 class="showcase-title svelte-1dpem8h">See how your<br class="svelte-1dpem8h"/>papers connect.</h3> <p class="showcase-desc svelte-1dpem8h">An interactive graph visualization maps relationships across your library.
          Group by tags, collections, authors, or journals — discover hidden connections
          between papers you'd never notice scrolling a list.</p></div> <div class="showcase-visual svelte-1dpem8h"><div class="graph-mock svelte-1dpem8h"><svg viewBox="0 0 400 280" class="graph-svg svelte-1dpem8h"><line x1="200" y1="80" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="200" y1="80" x2="280" y2="160" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="200" y1="80" x2="320" y2="100" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="120" y1="180" x2="200" y2="230" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="280" y1="160" x2="200" y2="230" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="80" y1="100" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="80" y1="100" x2="200" y2="80" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="320" y1="100" x2="360" y2="200" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="280" y1="160" x2="360" y2="200" stroke="var(--accent)" stroke-width="1.5" opacity="0.3" class="svelte-1dpem8h"></line><line x1="40" y1="180" x2="80" y2="100" stroke="var(--accent)" stroke-width="1.5" opacity="0.2" class="svelte-1dpem8h"></line><line x1="40" y1="180" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.2" class="svelte-1dpem8h"></line><circle cx="200" cy="80" r="14" fill="var(--accent)" opacity="0.9" class="node node-1 svelte-1dpem8h"></circle><circle cx="120" cy="180" r="11" fill="var(--accent)" opacity="0.7" class="node node-2 svelte-1dpem8h"></circle><circle cx="280" cy="160" r="12" fill="var(--accent)" opacity="0.75" class="node node-3 svelte-1dpem8h"></circle><circle cx="200" cy="230" r="10" fill="var(--accent)" opacity="0.6" class="node node-4 svelte-1dpem8h"></circle><circle cx="320" cy="100" r="9" fill="var(--accent)" opacity="0.55" class="node node-5 svelte-1dpem8h"></circle><circle cx="80" cy="100" r="10" fill="var(--accent)" opacity="0.6" class="node node-6 svelte-1dpem8h"></circle><circle cx="360" cy="200" r="8" fill="var(--accent)" opacity="0.45" class="node node-7 svelte-1dpem8h"></circle><circle cx="40" cy="180" r="7" fill="var(--accent)" opacity="0.35" class="node node-8 svelte-1dpem8h"></circle><text x="200" y="56" text-anchor="middle" fill="var(--text-secondary)" font-size="9" font-family="var(--font-sans)" class="svelte-1dpem8h">Vaswani et al.</text><text x="120" y="204" text-anchor="middle" fill="var(--text-tertiary)" font-size="8" font-family="var(--font-sans)" class="svelte-1dpem8h">Devlin 2019</text><text x="280" y="184" text-anchor="middle" fill="var(--text-tertiary)" font-size="8" font-family="var(--font-sans)" class="svelte-1dpem8h">Brown 2020</text></svg></div></div></div> <div class="showcase showcase-reverse reveal svelte-1dpem8h"><div class="showcase-text svelte-1dpem8h"><div class="showcase-eyebrow svelte-1dpem8h"><svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="svelte-1dpem8h"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" class="svelte-1dpem8h"></path></svg> AI Research Assistant</div> <h3 class="showcase-title svelte-1dpem8h">Chat with your<br class="svelte-1dpem8h"/>papers.</h3> <p class="showcase-desc svelte-1dpem8h">Ask questions about any paper in your library. The built-in MCP server gives Claude
          full context — your annotations, highlights, notes, and the paper's full text.
          Research conversations grounded in your actual reading.</p></div> <div class="showcase-visual svelte-1dpem8h"><div class="chat-mock svelte-1dpem8h"><div class="chat-header svelte-1dpem8h"><div class="chat-header-dot svelte-1dpem8h"></div> <span class="chat-header-title svelte-1dpem8h">Chat</span></div> <div class="chat-messages svelte-1dpem8h"><div class="chat-msg user svelte-1dpem8h"><div class="chat-bubble user-bubble svelte-1dpem8h">What's the key contribution of this paper?</div></div> <div class="chat-msg assistant svelte-1dpem8h"><div class="chat-bubble assistant-bubble svelte-1dpem8h"><div class="chat-line w-full svelte-1dpem8h"></div> <div class="chat-line w-90 svelte-1dpem8h"></div> <div class="chat-line w-full svelte-1dpem8h"></div> <div class="chat-line w-70 svelte-1dpem8h"></div></div></div> <div class="chat-msg user svelte-1dpem8h"><div class="chat-bubble user-bubble svelte-1dpem8h">How does it compare to my highlighted section on p.12?</div></div> <div class="chat-msg assistant svelte-1dpem8h"><div class="chat-bubble assistant-bubble svelte-1dpem8h"><div class="chat-line w-full svelte-1dpem8h"></div> <div class="chat-line w-80 svelte-1dpem8h"></div> <div class="chat-line w-full svelte-1dpem8h"></div> <div class="chat-line w-60 svelte-1dpem8h"></div> <div class="chat-line w-90 svelte-1dpem8h"></div></div></div></div> <div class="chat-input svelte-1dpem8h"><div class="chat-input-placeholder svelte-1dpem8h">Ask about this paper...</div></div></div></div></div></div></section>`);
  });
}
function WhyRotero($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    $$renderer2.push(`<section id="why"><div class="container"><div class="section-header reveal svelte-cida2x"><p class="section-eyebrow svelte-cida2x">Why Rotero</p> <h2 class="section-title svelte-cida2x">Built different.</h2></div> <div class="pillars svelte-cida2x"><div class="pillar reveal reveal-delay-1 svelte-cida2x"><div class="pillar-accent svelte-cida2x"></div> <div class="pillar-icon svelte-cida2x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-cida2x"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"></polygon></svg></div> <h3 class="pillar-title svelte-cida2x">Blazing fast</h3> <p class="pillar-desc svelte-cida2x">Native Rust binary. No Electron, no JVM, no garbage collector.
          Starts instantly, stays responsive with thousands of papers.</p> <div class="pillar-detail svelte-cida2x"><span class="mono svelte-cida2x">~8 MB</span> binary size</div></div> <div class="pillar reveal reveal-delay-2 svelte-cida2x"><div class="pillar-accent svelte-cida2x"></div> <div class="pillar-icon svelte-cida2x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-cida2x"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"></rect><path d="M7 11V7a5 5 0 0 1 10 0v4"></path></svg></div> <h3 class="pillar-title svelte-cida2x">Truly private</h3> <p class="pillar-desc svelte-cida2x">Your library lives on your machine. SQLite database, local PDF storage.
          No accounts, no telemetry, no cloud dependency.</p> <div class="pillar-detail svelte-cida2x"><span class="mono svelte-cida2x">0</span> data sent to servers</div></div> <div class="pillar reveal reveal-delay-3 svelte-cida2x"><div class="pillar-accent svelte-cida2x"></div> <div class="pillar-icon svelte-cida2x"><svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" class="svelte-cida2x"><path d="M12 1v4M12 19v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M1 12h4M19 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"></path></svg></div> <h3 class="pillar-title svelte-cida2x">Open source</h3> <p class="pillar-desc svelte-cida2x">AGPL-3.0 licensed. Read every line of code. Contribute features.
          No subscription fees, no vendor lock-in.</p> <div class="pillar-detail svelte-cida2x"><span class="mono svelte-cida2x">AGPL-3.0</span> licensed</div></div></div></div></section>`);
  });
}
function Download($$renderer, $$props) {
  $$renderer.component(($$renderer2) => {
    $$renderer2.push(`<section id="download" class="svelte-12wdzqw"><div class="container"><div class="download-card reveal svelte-12wdzqw"><div class="download-glow svelte-12wdzqw"></div> <p class="section-eyebrow svelte-12wdzqw">Get started</p> <h2 class="download-title svelte-12wdzqw">Ready to refine<br/>your research?</h2> <p class="download-sub svelte-12wdzqw">Free and open source. No account required.</p> <div class="download-buttons svelte-12wdzqw"><a href="https://github.com/pslai/rotero/releases" class="dl-btn macos svelte-12wdzqw"><svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"></path></svg> <div><span class="dl-label svelte-12wdzqw">Download for</span> <span class="dl-platform svelte-12wdzqw">macOS</span></div></a> <a href="https://github.com/pslai/rotero/releases" class="dl-btn linux svelte-12wdzqw"><svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489a.424.424 0 00-.11.135c-.26.268-.45.6-.663.839-.199.199-.485.267-.797.4-.313.136-.658.269-.864.68-.09.189-.136.394-.132.602 0 .199.027.4.055.536.058.399.116.728.04.97-.249.68-.28 1.145-.106 1.484.174.334.535.47.94.601.81.2 1.91.135 2.774.6.926.466 1.866.67 2.616.47.526-.116.97-.464 1.208-.946.587-.003 1.23-.269 2.26-.334.699-.058 1.574.267 2.577.2.025.134.063.198.114.333l.003.003c.391.778 1.113 1.368 1.884 1.43.199.023.395-.048.543-.164.734-.484.996-1.23 1.246-1.836.028-.067.058-.131.086-.2.016-.027.028-.058.042-.089.074.015.147.028.221.042h.006c.549.106 1.058-.058 1.412-.378.354-.32.59-.805.763-1.334l.005-.01c.348-1.084.276-2.083-.34-2.678-.09-.085-.178-.162-.266-.228-.053-.098-.108-.197-.164-.297-.34-.614-.655-1.397-.773-2.1a4.96 4.96 0 01-.033-.365c.035-.396.098-.728.163-1.085.076-.353.153-.748.166-1.2.012-.453-.048-.993-.364-1.484-.32-.49-.874-.843-1.594-.977a4.88 4.88 0 00-.382-.052c-.068-.275-.28-.673-.654-.924-.455-.306-1.14-.414-2.1-.277-.04-.133-.076-.27-.113-.396l-.001-.001c-.198-.704-.461-1.434-.73-1.89-.267-.45-.556-.76-.946-.86-.39-.1-.76 0-1.1.2a3.427 3.427 0 00-.397.297z"></path></svg> <div><span class="dl-label svelte-12wdzqw">Download for</span> <span class="dl-platform svelte-12wdzqw">Linux</span></div></a></div> <p class="download-note svelte-12wdzqw">Or build from source: <code class="svelte-12wdzqw">git clone &amp;&amp; cargo build --release</code></p></div></div></section>`);
  });
}
function Footer($$renderer) {
  $$renderer.push(`<footer class="footer svelte-jz8lnl"><div class="container footer-inner svelte-jz8lnl"><div class="footer-brand"><span class="wordmark svelte-jz8lnl">Rotero</span> <p class="footer-tagline svelte-jz8lnl">Research, refined.</p></div> <div class="footer-links svelte-jz8lnl"><a href="https://github.com/pslai/rotero" target="_blank" rel="noopener" class="svelte-jz8lnl">GitHub</a> <a href="https://github.com/pslai/rotero/blob/master/LICENSE" target="_blank" rel="noopener" class="svelte-jz8lnl">AGPL-3.0</a></div> <p class="footer-built svelte-jz8lnl">Built with Rust &amp; <a href="https://dioxuslabs.com" target="_blank" rel="noopener" class="svelte-jz8lnl">Dioxus</a></p></div></footer>`);
}
function _page($$renderer) {
  head("1uha8ag", $$renderer, ($$renderer2) => {
    $$renderer2.push(`<meta property="og:title" content="Rotero — Research, refined."/> <meta property="og:description" content="A fast, private, local-first paper reading app built with Rust."/> <meta property="og:type" content="website"/>`);
  });
  Nav($$renderer);
  $$renderer.push(`<!----> <main>`);
  Hero($$renderer);
  $$renderer.push(`<!----> `);
  Features($$renderer);
  $$renderer.push(`<!----> `);
  WhyRotero($$renderer);
  $$renderer.push(`<!----> `);
  Download($$renderer);
  $$renderer.push(`<!----></main> `);
  Footer($$renderer);
  $$renderer.push(`<!---->`);
}
export {
  _page as default
};
