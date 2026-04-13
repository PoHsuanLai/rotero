/* global Office, Word */

const API_BASE = "http://127.0.0.1:21984";

// State
let styles = [];
let selectedPaperIds = new Set();
let searchResults = [];
let searchTimeout = null;

// ── Initialization ──────────────────────────────────────────────────────────

Office.onReady(function (info) {
  if (info.host === Office.HostType.Word) {
    init();
  }
});

async function init() {
  setupNavigation();
  setupSearch();
  setupButtons();
  await loadStyles();
  await checkConnection();

  // Open to the right view based on URL hash
  var hash = window.location.hash.replace("#", "");
  if (hash === "bibliography") switchView("bibliography");
  else if (hash === "refresh") switchView("refresh");
}

// ── Navigation ──────────────────────────────────────────────────────────────

function setupNavigation() {
  document.querySelectorAll(".nav button").forEach(function (btn) {
    btn.addEventListener("click", function () {
      switchView(btn.dataset.view);
    });
  });
}

function switchView(viewName) {
  document.querySelectorAll(".nav button").forEach(function (b) {
    b.classList.toggle("active", b.dataset.view === viewName);
  });
  document.querySelectorAll(".view").forEach(function (v) {
    v.classList.toggle("active", v.id === "view" + capitalize(viewName));
  });
  if (viewName === "bibliography") scanCitedPapers();
}

function capitalize(s) {
  return s.charAt(0).toUpperCase() + s.slice(1);
}

// ── Connection check ────────────────────────────────────────────────────────

async function checkConnection() {
  try {
    var resp = await fetch(API_BASE + "/api/status");
    if (!resp.ok) throw new Error("not ok");
    document.getElementById("connectionBanner").classList.remove("show");
    return true;
  } catch (e) {
    document.getElementById("connectionBanner").classList.add("show");
    return false;
  }
}

// ── Styles ──────────────────────────────────────────────────────────────────

async function loadStyles() {
  try {
    var resp = await fetch(API_BASE + "/api/cite/styles");
    var data = await resp.json();
    styles = data.styles;
  } catch (e) {
    styles = [{ id: "apa-7th", name: "APA 7th" }];
  }

  var saved = localStorage.getItem("rotero-cite-style") || styles[0].id;
  populateStyleSelect("styleSelect", saved);
  populateStyleSelect("bibStyleSelect", saved);
  populateStyleSelect("refreshStyleSelect", saved);
}

function populateStyleSelect(selectId, selectedId) {
  var select = document.getElementById(selectId);
  select.innerHTML = "";
  styles.forEach(function (s) {
    var opt = document.createElement("option");
    opt.value = s.id;
    opt.textContent = s.name;
    if (s.id === selectedId) opt.selected = true;
    select.appendChild(opt);
  });
  select.addEventListener("change", function () {
    localStorage.setItem("rotero-cite-style", select.value);
  });
}

function getSelectedStyle() {
  return document.getElementById("styleSelect").value;
}

// ── Search ──────────────────────────────────────────────────────────────────

function setupSearch() {
  var input = document.getElementById("searchInput");
  input.addEventListener("input", function () {
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(function () {
      doSearch(input.value.trim());
    }, 300);
  });
}

async function doSearch(query) {
  var container = document.getElementById("searchResults");
  if (!query) {
    container.innerHTML = '<div class="empty-state">Type to search your Rotero library</div>';
    return;
  }

  container.innerHTML = '<div class="loading">Searching...</div>';

  try {
    var resp = await fetch(API_BASE + "/api/cite/search?q=" + encodeURIComponent(query));
    var data = await resp.json();
    searchResults = data.papers;
    renderSearchResults();
  } catch (e) {
    container.innerHTML = '<div class="status error">Failed to search. Is Rotero running?</div>';
  }
}

function renderSearchResults() {
  var container = document.getElementById("searchResults");
  if (searchResults.length === 0) {
    container.innerHTML = '<div class="empty-state">No papers found</div>';
    return;
  }

  container.innerHTML = "";
  searchResults.forEach(function (paper) {
    var item = document.createElement("div");
    item.className = "result-item" + (selectedPaperIds.has(paper.id) ? " selected" : "");

    var cb = document.createElement("input");
    cb.type = "checkbox";
    cb.checked = selectedPaperIds.has(paper.id);

    var info = document.createElement("div");
    info.className = "result-info";

    var title = document.createElement("div");
    title.className = "result-title";
    title.textContent = paper.title;

    var meta = document.createElement("div");
    meta.className = "result-meta";
    var parts = [];
    if (paper.authors && paper.authors.length > 0) {
      parts.push(paper.authors.length > 2
        ? paper.authors[0] + " et al."
        : paper.authors.join(", "));
    }
    if (paper.year) parts.push(paper.year);
    if (paper.journal) parts.push(paper.journal);
    meta.textContent = parts.join(" \u2022 ");

    info.appendChild(title);
    info.appendChild(meta);
    item.appendChild(cb);
    item.appendChild(info);

    item.addEventListener("click", function (e) {
      if (e.target === cb) return;
      cb.checked = !cb.checked;
      togglePaper(paper.id, cb.checked);
      item.classList.toggle("selected", cb.checked);
    });
    cb.addEventListener("change", function () {
      togglePaper(paper.id, cb.checked);
      item.classList.toggle("selected", cb.checked);
    });

    container.appendChild(item);
  });

  updateInsertButton();
}

function togglePaper(id, selected) {
  if (selected) selectedPaperIds.add(id);
  else selectedPaperIds.delete(id);
  updateInsertButton();
}

function updateInsertButton() {
  document.getElementById("insertCiteBtn").disabled = selectedPaperIds.size === 0;
}

// ── Buttons ─────────────────────────────────────────────────────────────────

function setupButtons() {
  document.getElementById("insertCiteBtn").addEventListener("click", insertCitation);
  document.getElementById("insertBibBtn").addEventListener("click", insertBibliography);
  document.getElementById("refreshBtn").addEventListener("click", refreshAll);
}

// ── Insert Citation ─────────────────────────────────────────────────────────

async function insertCitation() {
  var btn = document.getElementById("insertCiteBtn");
  var status = document.getElementById("citeStatus");
  var ids = Array.from(selectedPaperIds);
  var style = getSelectedStyle();

  btn.disabled = true;
  status.textContent = "Formatting...";
  status.className = "status";

  try {
    var resp = await fetch(API_BASE + "/api/cite/format", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ paper_ids: ids, style: style }),
    });
    var data = await resp.json();
    if (!data.success) throw new Error(data.error || "Format failed");

    var text = data.combined || data.citations.map(function (c) { return c.text; }).join("; ");

    await Word.run(async function (context) {
      var selection = context.document.getSelection();
      var range = selection.insertText(text, Word.InsertLocation.replace);
      var cc = range.insertContentControl();
      cc.tag = JSON.stringify({
        type: "citation",
        paperIds: ids,
        style: style,
      });
      cc.title = "Rotero Citation";
      cc.appearance = Word.ContentControlAppearance.boundingBox;
      await context.sync();
    });

    status.textContent = "Citation inserted";
    status.className = "status success";
    selectedPaperIds.clear();
    renderSearchResults();
  } catch (e) {
    status.textContent = e.message;
    status.className = "status error";
  }

  btn.disabled = selectedPaperIds.size === 0;
}

// ── Edit Citation (detect on selection) ─────────────────────────────────────

async function detectSelectedCitation() {
  try {
    return await Word.run(async function (context) {
      var selection = context.document.getSelection();
      var cc = selection.parentContentControlOrNullObject;
      cc.load("tag,title,id");
      await context.sync();

      if (cc.isNullObject) return null;
      if (cc.title !== "Rotero Citation") return null;

      var meta = JSON.parse(cc.tag);
      return { id: cc.id, meta: meta };
    });
  } catch (e) {
    return null;
  }
}

async function updateCitation(ccId, newPaperIds, newStyle) {
  try {
    var resp = await fetch(API_BASE + "/api/cite/format", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ paper_ids: newPaperIds, style: newStyle }),
    });
    var data = await resp.json();
    if (!data.success) throw new Error(data.error || "Format failed");

    var text = data.combined || data.citations.map(function (c) { return c.text; }).join("; ");

    await Word.run(async function (context) {
      var ccs = context.document.contentControls;
      ccs.load("items/id,items/tag,items/title");
      await context.sync();

      for (var i = 0; i < ccs.items.length; i++) {
        if (ccs.items[i].id === ccId) {
          ccs.items[i].insertText(text, Word.InsertLocation.replace);
          ccs.items[i].tag = JSON.stringify({
            type: "citation",
            paperIds: newPaperIds,
            style: newStyle,
          });
          break;
        }
      }
      await context.sync();
    });
    return true;
  } catch (e) {
    return false;
  }
}

// ── Scan cited papers ───────────────────────────────────────────────────────

async function scanCitedPapers() {
  var container = document.getElementById("bibEntries");
  container.innerHTML = '<div class="loading">Scanning document...</div>';

  try {
    var allIds = await getAllCitedPaperIds();
    if (allIds.length === 0) {
      container.innerHTML = '<div class="empty-state">No citations found in this document</div>';
      return;
    }

    var style = document.getElementById("bibStyleSelect").value;
    var resp = await fetch(API_BASE + "/api/cite/bibliography", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ paper_ids: allIds, style: style }),
    });
    var data = await resp.json();
    if (!data.success) throw new Error(data.error);

    container.innerHTML = "";
    data.entries.forEach(function (entry) {
      var div = document.createElement("div");
      div.className = "bib-entry";
      div.textContent = entry.text;
      container.appendChild(div);
    });
  } catch (e) {
    container.innerHTML = '<div class="status error">' + e.message + "</div>";
  }
}

async function getAllCitedPaperIds() {
  return Word.run(async function (context) {
    var ccs = context.document.contentControls;
    ccs.load("items/tag,items/title");
    await context.sync();

    var idSet = {};
    for (var i = 0; i < ccs.items.length; i++) {
      if (ccs.items[i].title !== "Rotero Citation") continue;
      try {
        var meta = JSON.parse(ccs.items[i].tag);
        if (meta.paperIds) {
          meta.paperIds.forEach(function (id) { idSet[id] = true; });
        }
      } catch (e) { /* skip malformed */ }
    }
    return Object.keys(idSet);
  });
}

// ── Insert Bibliography ─────────────────────────────────────────────────────

async function insertBibliography() {
  var btn = document.getElementById("insertBibBtn");
  var status = document.getElementById("bibStatus");

  btn.disabled = true;
  status.textContent = "Generating bibliography...";
  status.className = "status";

  try {
    var allIds = await getAllCitedPaperIds();
    if (allIds.length === 0) {
      status.textContent = "No citations found in this document.";
      status.className = "status error";
      btn.disabled = false;
      return;
    }

    var style = document.getElementById("bibStyleSelect").value;
    var resp = await fetch(API_BASE + "/api/cite/bibliography", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ paper_ids: allIds, style: style }),
    });
    var data = await resp.json();
    if (!data.success) throw new Error(data.error || "Failed");

    var bibText = data.entries.map(function (e) { return e.text; }).join("\n");

    await Word.run(async function (context) {
      var selection = context.document.getSelection();

      // Check if there's already a bibliography content control — update it
      var ccs = context.document.contentControls;
      ccs.load("items/title,items/tag");
      await context.sync();

      var existingBib = null;
      for (var i = 0; i < ccs.items.length; i++) {
        if (ccs.items[i].title === "Rotero Bibliography") {
          existingBib = ccs.items[i];
          break;
        }
      }

      if (existingBib) {
        existingBib.insertText(bibText, Word.InsertLocation.replace);
        existingBib.tag = JSON.stringify({
          type: "bibliography",
          paperIds: allIds,
          style: style,
        });
      } else {
        var range = selection.insertText(bibText, Word.InsertLocation.after);
        var cc = range.insertContentControl();
        cc.tag = JSON.stringify({
          type: "bibliography",
          paperIds: allIds,
          style: style,
        });
        cc.title = "Rotero Bibliography";
        cc.appearance = Word.ContentControlAppearance.boundingBox;
      }

      await context.sync();
    });

    status.textContent = "Bibliography inserted";
    status.className = "status success";
  } catch (e) {
    status.textContent = e.message;
    status.className = "status error";
  }

  btn.disabled = false;
}

// ── Refresh All ─────────────────────────────────────────────────────────────

async function refreshAll() {
  var btn = document.getElementById("refreshBtn");
  var status = document.getElementById("refreshStatus");
  var newStyle = document.getElementById("refreshStyleSelect").value;

  btn.disabled = true;
  status.textContent = "Refreshing...";
  status.className = "status";

  try {
    await Word.run(async function (context) {
      var ccs = context.document.contentControls;
      ccs.load("items/tag,items/title");
      await context.sync();

      var citationCcs = [];
      var bibCc = null;

      for (var i = 0; i < ccs.items.length; i++) {
        var cc = ccs.items[i];
        if (cc.title === "Rotero Citation") citationCcs.push(cc);
        else if (cc.title === "Rotero Bibliography") bibCc = cc;
      }

      // Refresh each citation
      for (var j = 0; j < citationCcs.length; j++) {
        var cc = citationCcs[j];
        try {
          var meta = JSON.parse(cc.tag);
          var resp = await fetch(API_BASE + "/api/cite/format", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ paper_ids: meta.paperIds, style: newStyle }),
          });
          var data = await resp.json();
          if (data.success) {
            var text = data.combined || data.citations.map(function (c) { return c.text; }).join("; ");
            cc.insertText(text, Word.InsertLocation.replace);
            meta.style = newStyle;
            cc.tag = JSON.stringify(meta);
          }
        } catch (e) { /* skip individual failures */ }
      }

      // Refresh bibliography
      if (bibCc) {
        try {
          var bibMeta = JSON.parse(bibCc.tag);
          // Collect ALL cited paper IDs (not just what the bib had)
          var allIds = {};
          citationCcs.forEach(function (c) {
            try {
              var m = JSON.parse(c.tag);
              if (m.paperIds) m.paperIds.forEach(function (id) { allIds[id] = true; });
            } catch (e) {}
          });
          var idList = Object.keys(allIds);

          var bibResp = await fetch(API_BASE + "/api/cite/bibliography", {
            method: "POST",
            headers: { "Content-Type": "application/json" },
            body: JSON.stringify({ paper_ids: idList, style: newStyle }),
          });
          var bibData = await bibResp.json();
          if (bibData.success) {
            var bibText = bibData.entries.map(function (e) { return e.text; }).join("\n");
            bibCc.insertText(bibText, Word.InsertLocation.replace);
            bibMeta.style = newStyle;
            bibMeta.paperIds = idList;
            bibCc.tag = JSON.stringify(bibMeta);
          }
        } catch (e) { /* skip */ }
      }

      await context.sync();
    });

    // Sync style selection across all dropdowns
    localStorage.setItem("rotero-cite-style", newStyle);
    document.getElementById("styleSelect").value = newStyle;
    document.getElementById("bibStyleSelect").value = newStyle;

    status.textContent = "All citations refreshed";
    status.className = "status success";
  } catch (e) {
    status.textContent = e.message;
    status.className = "status error";
  }

  btn.disabled = false;
}
