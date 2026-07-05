const API = 'http://127.0.0.1:21984';

const dot = document.getElementById('dot');
const disconnected = document.getElementById('disconnected');
const main = document.getElementById('main');
const paperTitle = document.getElementById('paperTitle');
const paperAuthors = document.getElementById('paperAuthors');
const paperDoi = document.getElementById('paperDoi');
const collList = document.getElementById('collList');
const tagsSection = document.getElementById('tagsSection');
const tagChips = document.getElementById('tagChips');
const addBtn = document.getElementById('addBtn');
const result = document.getElementById('result');

let pageMetadata = null;
let selectedTagIds = new Set();
let selectedCollectionId = null; // null = Library (no collection)
let selectedCollectionName = 'Library';

// Bootstrap-icon-style folder glyphs as inline SVG (extension has no icon font)
const ICON_FOLDER = '<svg class="coll-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M.54 3.87.5 3a2 2 0 0 1 2-2h3.672a2 2 0 0 1 1.414.586l.828.828A2 2 0 0 0 9.828 3h3.982a2 2 0 0 1 1.992 2.181l-.637 7A2 2 0 0 1 13.174 14H2.826a2 2 0 0 1-1.991-1.819l-.637-7a1.99 1.99 0 0 1 .342-1.31zM2.19 4a1 1 0 0 0-.996 1.09l.637 7a1 1 0 0 0 .995.91h10.348a1 1 0 0 0 .995-.91l.637-7A1 1 0 0 0 14.81 4z"/></svg>';
const ICON_FOLDER_FILL = '<svg class="coll-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M9.828 3h3.982a2 2 0 0 1 1.992 2.181l-.637 7A2 2 0 0 1 13.174 14H2.826a2 2 0 0 1-1.991-1.819l-.637-7a1.99 1.99 0 0 1 .342-1.31L.5 3a2 2 0 0 1 2-2h3.672a2 2 0 0 1 1.414.586l.828.828A2 2 0 0 0 9.828 3m-8.322.12q.322-.119.684-.12h5.396l-.707-.707A1 1 0 0 0 6.172 2H2.5a1 1 0 0 0-1 .981z"/></svg>';
const ICON_FOLDER_OPEN = '<svg class="coll-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M9.828 4a3 3 0 0 1-2.12-.879l-.83-.828A1 1 0 0 0 6.173 2H2.5a1 1 0 0 0-1 .981L1.546 4zm-8.322.12C1.72 3.042 1.95 3 2.19 3h5.396l-.707-.707A1 1 0 0 0 6.172 2H2.5a1 1 0 0 0-1 .981zM0 5c0-.552.446-1 .996-1h14.008A1 1 0 0 1 16 5.19l-.637 7A1.5 1.5 0 0 1 13.87 13.5H2.13A1.5 1.5 0 0 1 .637 12.19z"/></svg>';
const ICON_LIBRARY = '<svg class="coll-icon" width="16" height="16" viewBox="0 0 16 16" fill="currentColor"><path d="M2.5 3.5a.5.5 0 0 1 0-1h11a.5.5 0 0 1 0 1zm2-2a.5.5 0 0 1 0-1h7a.5.5 0 0 1 0 1zM0 13a1.5 1.5 0 0 0 1.5 1.5h13A1.5 1.5 0 0 0 16 13V6a1.5 1.5 0 0 0-1.5-1.5h-13A1.5 1.5 0 0 0 0 6zm1.5.5A.5.5 0 0 1 1 13V6a.5.5 0 0 1 .5-.5h13a.5.5 0 0 1 .5.5v7a.5.5 0 0 1-.5.5z"/></svg>';

// Check connection and load collections + tags
async function init() {
  try {
    const resp = await fetch(`${API}/api/status`, { signal: AbortSignal.timeout(2000) });
    const data = await resp.json();
    if (data.status === 'ok') {
      dot.className = 'dot ok';
      main.style.display = 'block';
      await Promise.all([loadCollections(), loadTags(), loadMetadata()]);
      addBtn.disabled = false;
      return;
    }
  } catch {}

  dot.className = 'dot error';
  disconnected.style.display = 'block';
}

// Fetch collections from Rotero and render as a nested folder tree
async function loadCollections() {
  let collections = [];
  try {
    const resp = await fetch(`${API}/api/collections`);
    const data = await resp.json();
    collections = data.collections || [];
  } catch {
    return;
  }

  // Group by parent for tree traversal
  const childrenOf = new Map();
  for (const c of collections) {
    const key = c.parent_id || null;
    if (!childrenOf.has(key)) childrenOf.set(key, []);
    childrenOf.get(key).push(c);
  }

  collList.innerHTML = '';

  // "Library (no collection)" root row
  collList.appendChild(makeCollRow(null, 'Library (no collection)', 0, false));

  const seen = new Set();
  const renderLevel = (parentId, depth) => {
    const kids = childrenOf.get(parentId) || [];
    for (const c of kids) {
      if (seen.has(c.id)) continue; // guard against cycles
      seen.add(c.id);
      const hasChildren = (childrenOf.get(c.id) || []).length > 0;
      collList.appendChild(makeCollRow(c.id, c.name, depth, hasChildren));
      renderLevel(c.id, depth + 1);
    }
  };
  renderLevel(null, 0);

  // Select the Library row by default
  selectCollection(null, 'Library');
}

function makeCollRow(id, name, depth, hasChildren) {
  const key = id === null ? '' : id;
  const row = document.createElement('div');
  row.className = 'coll-item';
  row.dataset.collId = key;
  row.dataset.hasChildren = hasChildren ? '1' : '';
  row.style.paddingLeft = `${12 + depth * 16}px`;

  const iconSvg = id === null
    ? ICON_LIBRARY
    : (hasChildren ? ICON_FOLDER_FILL : ICON_FOLDER);
  row.innerHTML = iconSvg + `<span class="coll-name">${escapeHtml(name)}</span>`;

  row.addEventListener('click', () => selectCollection(id, name));
  return row;
}

function selectCollection(id, name) {
  selectedCollectionId = id;
  selectedCollectionName = id === null ? 'Library' : name;

  const key = id === null ? '' : id;
  for (const row of collList.querySelectorAll('.coll-item')) {
    const isSel = row.dataset.collId === key;
    row.classList.toggle('selected', isSel);
    // Active collections with children show the "open folder" glyph
    if (row.dataset.hasChildren) {
      const svg = row.querySelector('.coll-icon');
      if (svg) svg.outerHTML = isSel ? ICON_FOLDER_OPEN : ICON_FOLDER_FILL;
    }
  }

  updateAddBtnLabel();
}

function updateAddBtnLabel() {
  addBtn.textContent = selectedCollectionId
    ? `Add to "${selectedCollectionName}"`
    : 'Add to Library';
}

function escapeHtml(s) {
  return String(s).replace(/[&<>"']/g, (c) => ({
    '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;',
  }[c]));
}

// Fetch tags from Rotero
async function loadTags() {
  try {
    const resp = await fetch(`${API}/api/tags`);
    const data = await resp.json();
    if (!data.tags || data.tags.length === 0) return;

    tagsSection.style.display = 'block';
    for (const tag of data.tags) {
      const chip = document.createElement('button');
      chip.className = 'tag-chip';
      chip.dataset.tagId = tag.id;

      const color = tag.color || '#6b7280';

      const dot = document.createElement('span');
      dot.className = 'tag-dot';
      dot.style.background = color;
      chip.appendChild(dot);

      const label = document.createTextNode(tag.name);
      chip.appendChild(label);

      chip.addEventListener('click', () => {
        const id = chip.dataset.tagId;
        if (selectedTagIds.has(id)) {
          selectedTagIds.delete(id);
          chip.classList.remove('selected');
          chip.style.background = '';
          chip.style.borderColor = '';
          chip.style.color = '';
          dot.style.background = color;
        } else {
          selectedTagIds.add(id);
          chip.classList.add('selected');
          chip.style.background = color;
          chip.style.borderColor = color;
          chip.style.color = 'white';
          dot.style.background = 'rgba(255,255,255,0.8)';
        }
      });

      tagChips.appendChild(chip);
    }
  } catch {}
}

// Extract metadata from current page
async function loadMetadata() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const results = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: extractMetadata,
    });
    pageMetadata = results[0]?.result;
  } catch {}

  if (!pageMetadata) {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    pageMetadata = { url: tab.url, title: tab.title };
  }

  // If we're on a PDF page and don't have a pdf_url yet, use the current URL
  if (pageMetadata && !pageMetadata.pdf_url) {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const url = tab.url || '';
    if (url.match(/\.pdf(\?.*)?$/i) || url.match(/\/pdf\//) || url.match(/content-type=application\/pdf/i)) {
      pageMetadata.pdf_url = url;
    }
  }

  // Fallback: if client-side extraction found no DOI or authors, ask the
  // connector's translators to scrape the page.
  if (pageMetadata && !pageMetadata.doi && (!pageMetadata.authors || pageMetadata.authors.length === 0)) {
    try {
      const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
      const m = await runScrape(tab?.id, pageMetadata);
      if (m) {
        if (m.title) pageMetadata.title = m.title;
        if (m.authors?.length) pageMetadata.authors = m.authors;
        if (m.doi) pageMetadata.doi = m.doi;
        if (m.pdf_url) pageMetadata.pdf_url = m.pdf_url;
        if (m.url) pageMetadata.url = m.url;
        if (m.journal) pageMetadata.journal = m.journal;
        if (m.year) pageMetadata.year = m.year;
        if (m.volume) pageMetadata.volume = m.volume;
        if (m.issue) pageMetadata.issue = m.issue;
        if (m.pages) pageMetadata.pages = m.pages;
        if (m.publisher) pageMetadata.publisher = m.publisher;
        if (m.abstract_text) pageMetadata.abstract_text = m.abstract_text;
      }
    } catch {}
  }

  // The captured HTML was only needed for the scrape request above; drop it so
  // it doesn't bloat the later /api/save body (which spreads pageMetadata).
  if (pageMetadata) {
    delete pageMetadata.html;
    delete pageMetadata.rawHtml;
  }

  // If title is still empty/generic, try to derive from URL
  if (pageMetadata && (!pageMetadata.title || pageMetadata.title === 'Untitled')) {
    try {
      const url = new URL(pageMetadata.url || pageMetadata.pdf_url || '');
      const path = url.pathname.split('/').filter(Boolean).pop() || '';
      const decoded = decodeURIComponent(path).replace(/\.pdf$/i, '').replace(/[_-]/g, ' ');
      if (decoded && decoded.length > 2) {
        pageMetadata.title = decoded;
      }
    } catch {}
  }

  if (pageMetadata) {
    paperTitle.textContent = pageMetadata.title || 'Untitled';
    paperAuthors.textContent = pageMetadata.authors?.join(', ') || '';
    paperAuthors.style.display = pageMetadata.authors?.length ? 'block' : 'none';

    const paperJournal = document.getElementById('paperJournal');
    const journalParts = [pageMetadata.journal, pageMetadata.year].filter(Boolean);
    paperJournal.textContent = journalParts.join(', ');
    paperJournal.style.display = journalParts.length ? 'block' : 'none';

    paperDoi.textContent = pageMetadata.doi ? `DOI: ${pageMetadata.doi}` : '';
    paperDoi.style.display = pageMetadata.doi ? 'block' : 'none';
  }
}

// Maximum number of browser-proxied follow-up fetches for one scrape, a guard
// against a translator that would otherwise loop indefinitely.
const MAX_SCRAPE_HOPS = 20;

// Run the connector scrape for a page and return its extracted metadata (or null
// on a miss). Sends the captured page HTML so the connector translates the real
// authenticated DOM instead of re-fetching and hitting anti-bot walls, with
// raw_html for inline-<script> translators. When a site translator needs a
// follow-up request the connector can't authenticate (a citation/export endpoint
// behind session cookies), the connector pauses and asks us to perform the fetch
// in the page context — where the tab's cookies apply — and continue.
async function runScrape(tabId, meta) {
  let resp = await fetch(`${API}/api/scrape`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ url: meta.url, html: meta.html, raw_html: meta.rawHtml }),
  });
  if (!resp.ok) return null;
  let data = await resp.json();

  let hops = 0;
  while (!data.done && data.fetch && data.run_id != null) {
    if (++hops > MAX_SCRAPE_HOPS || tabId == null) break;
    const outcome = await performPageFetch(tabId, data.fetch);
    resp = await fetch(`${API}/api/scrape/continue`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ run_id: data.run_id, url: meta.url, response: outcome }),
    });
    if (!resp.ok) return null;
    data = await resp.json();
  }

  return data.done && data.success ? data.metadata : null;
}

// Perform a connector-requested follow-up fetch inside the page's own context so
// the tab's session cookies (including HttpOnly) authenticate it. Returns the
// { ok, status, body } outcome the connector feeds back to its translator.
async function performPageFetch(tabId, instruction) {
  try {
    const [res] = await chrome.scripting.executeScript({
      target: { tabId },
      args: [instruction],
      func: async (f) => {
        try {
          const init = {
            method: f.method || 'GET',
            credentials: 'include',
            redirect: 'follow',
          };
          if (f.headers?.length) init.headers = Object.fromEntries(f.headers);
          if (f.method === 'POST') {
            init.body = f.body || '';
            init.headers = init.headers || {};
            if (f.content_type) init.headers['Content-Type'] = f.content_type;
          }
          const r = await fetch(f.url, init);
          return { ok: r.ok, status: r.status, body: await r.text() };
        } catch (e) {
          return { ok: false, status: 0, body: '', error: String(e) };
        }
      },
    });
    return res?.result || { ok: false, body: '', error: 'page fetch failed' };
  } catch (e) {
    return { ok: false, body: '', error: String(e) };
  }
}

async function extractMetadata() {
  const meta = { url: window.location.href };

  // Capture the rendered page HTML so the connector can translate it directly
  // instead of re-fetching server-side (which publisher anti-bot walls block).
  // This runs in the page context, so it's the real, authenticated DOM.
  try {
    meta.html = document.documentElement.outerHTML;
  } catch {}

  // Helper: get content of first matching meta tag
  const getMeta = (...selectors) => {
    for (const sel of selectors) {
      const el = document.querySelector(sel);
      if (el?.content) return el.content.trim();
    }
    return null;
  };

  // Helper: get content of all matching meta tags
  const getAllMeta = (...selectors) => {
    const results = [];
    for (const sel of selectors) {
      document.querySelectorAll(sel).forEach(el => {
        if (el?.content?.trim()) results.push(el.content.trim());
      });
    }
    return results;
  };

  // --- DOI ---
  let doi = getMeta(
    'meta[name="citation_doi"]',
    'meta[property="citation_doi"]',
    'meta[name="DC.identifier"][scheme="doi"]',
    'meta[name="dc.identifier"][scheme="doi"]',
    'meta[name="prism.doi"]',
  );
  // DC.identifier without scheme might contain a DOI
  if (!doi) {
    const dcId = getMeta('meta[name="DC.identifier"]', 'meta[name="dc.identifier"]');
    if (dcId && /^10\.\d{4,}\//.test(dcId)) doi = dcId;
  }
  if (doi) doi = doi.replace(/^https?:\/\/doi\.org\//, '');
  // Extract from URL as fallback
  if (!doi) {
    const m = window.location.href.match(/doi\.org\/(10\.[^/]+\/.+?)(?:\?|#|$)/);
    if (m) doi = m[1];
  }
  if (doi) meta.doi = doi;

  // --- Title ---
  meta.title = getMeta(
    'meta[name="citation_title"]',
    'meta[name="DC.title"]',
    'meta[name="dc.title"]',
    'meta[name="eprints.title"]',
    'meta[property="og:title"]',
  ) || document.title;

  // --- Authors ---
  const authors = getAllMeta(
    'meta[name="citation_author"]',
    'meta[name="DC.creator"]',
    'meta[name="dc.creator"]',
    'meta[name="eprints.creators_name"]',
    'meta[name="author"]',
  );
  if (authors.length) meta.authors = authors;

  // --- PDF URL ---
  const pdfUrl = getMeta('meta[name="citation_pdf_url"]');
  if (pdfUrl) meta.pdf_url = pdfUrl;

  // --- Journal ---
  const journal = getMeta(
    'meta[name="citation_journal_title"]',
    'meta[name="prism.publicationName"]',
    'meta[name="DC.source"]',
    'meta[name="dc.source"]',
  );
  if (journal) meta.journal = journal;

  // --- Year ---
  const dateStr = getMeta(
    'meta[name="citation_publication_date"]',
    'meta[name="citation_date"]',
    'meta[name="DC.date"]',
    'meta[name="dc.date"]',
    'meta[property="article:published_time"]',
  );
  if (dateStr) {
    const yearMatch = dateStr.match(/(\d{4})/);
    if (yearMatch) meta.year = parseInt(yearMatch[1]);
  }

  // --- Volume / Issue / Pages ---
  const volume = getMeta('meta[name="citation_volume"]');
  if (volume) meta.volume = volume;
  const issue = getMeta('meta[name="citation_issue"]');
  if (issue) meta.issue = issue;
  const firstPage = getMeta('meta[name="citation_firstpage"]');
  const lastPage = getMeta('meta[name="citation_lastpage"]');
  if (firstPage) {
    meta.pages = lastPage ? `${firstPage}-${lastPage}` : firstPage;
  }

  // --- Publisher ---
  const publisher = getMeta(
    'meta[name="citation_publisher"]',
    'meta[name="DC.publisher"]',
    'meta[name="dc.publisher"]',
  );
  if (publisher) meta.publisher = publisher;

  // --- Abstract ---
  const abstract_text = getMeta(
    'meta[name="citation_abstract"]',
    'meta[name="DC.description"]',
    'meta[name="dc.description"]',
    'meta[name="description"]',
    'meta[property="og:description"]',
  );
  if (abstract_text) meta.abstract_text = abstract_text;

  // --- JSON-LD structured data ---
  try {
    const ldScripts = document.querySelectorAll('script[type="application/ld+json"]');
    for (const script of ldScripts) {
      const data = JSON.parse(script.textContent);
      const items = Array.isArray(data) ? data : [data];
      for (const item of items) {
        if (!item['@type']) continue;
        const type = Array.isArray(item['@type']) ? item['@type'] : [item['@type']];
        if (!type.some(t => ['ScholarlyArticle', 'Article', 'TechArticle', 'MedicalScholarlyArticle'].includes(t))) continue;

        if (!meta.title && item.name) meta.title = item.name;
        if (!meta.title && item.headline) meta.title = item.headline;
        if (!meta.doi && item.doi) meta.doi = item.doi;
        if (!meta.authors?.length && item.author) {
          const authArr = Array.isArray(item.author) ? item.author : [item.author];
          const names = authArr.map(a => a.name || [a.givenName, a.familyName].filter(Boolean).join(' ')).filter(Boolean);
          if (names.length) meta.authors = names;
        }
        if (!meta.journal && item.isPartOf?.name) meta.journal = item.isPartOf.name;
        if (!meta.year && item.datePublished) {
          const m = item.datePublished.match(/(\d{4})/);
          if (m) meta.year = parseInt(m[1]);
        }
        if (!meta.abstract_text && item.description) meta.abstract_text = item.description;
        if (!meta.publisher) {
          const pub = item.publisher;
          if (typeof pub === 'string') meta.publisher = pub;
          else if (pub?.name) meta.publisher = pub.name;
        }
        if (!meta.pages && item.pagination) meta.pages = item.pagination;
        if (!meta.volume && item.volumeNumber) meta.volume = String(item.volumeNumber);
        if (!meta.issue && item.issueNumber) meta.issue = String(item.issueNumber);

        break; // Use the first scholarly article found
      }
    }
  } catch {}

  // --- arXiv ---
  const arxiv = window.location.href.match(/arxiv\.org\/(?:abs|pdf)\/(\d+\.\d+)/);
  if (arxiv) {
    meta.arxiv_id = arxiv[1];
    if (!meta.pdf_url) meta.pdf_url = `https://arxiv.org/pdf/${arxiv[1]}.pdf`;
  }

  // Raw server HTML for the connector's translators — only when client-side meta
  // extraction fell short (no DOI or no authors), which is exactly when the
  // connector scrape runs. Same-origin credentialed fetch → carries the tab's
  // session cookies (incl. HttpOnly), so it works on gated sites; and unlike the
  // rendered outerHTML it preserves inline <script> data (IEEE
  // global.document.metadata, __NEXT_DATA__, JSON-LD) an SPA strips on hydration.
  // Gated so well-behaved pages don't pay for an extra fetch. Best-effort.
  if (!meta.doi || !meta.authors || meta.authors.length === 0) {
    try {
      const resp = await fetch(window.location.href, {
        credentials: 'include',
        redirect: 'follow',
      });
      if (resp.ok) {
        const ct = resp.headers.get('content-type') || '';
        if (ct.includes('html') || ct === '') {
          meta.rawHtml = await resp.text();
        }
      }
    } catch {}
  }

  return meta;
}

// Save paper
addBtn.addEventListener('click', async () => {
  if (!pageMetadata) return;

  addBtn.disabled = true;
  addBtn.textContent = 'Adding...';
  result.style.display = 'none';

  const collectionId = selectedCollectionId;
  const selectedName = selectedCollectionName;

  try {
    const resp = await fetch(`${API}/api/save`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({
        ...pageMetadata,
        collection_id: collectionId,
        tag_ids: Array.from(selectedTagIds),
      }),
    });
    const data = await resp.json();

    if (data.success) {
      result.className = 'result success';
      result.innerHTML = `Added to <strong>${selectedName}</strong>`;
      addBtn.textContent = 'Added';
      addBtn.disabled = true;
      return;
    } else {
      result.className = 'result fail';
      result.textContent = data.message || 'Failed to save';
    }
  } catch (e) {
    result.className = 'result fail';
    result.textContent = 'Connection lost. Is Rotero running?';
  }

  addBtn.disabled = false;
  updateAddBtnLabel();
});

init();
