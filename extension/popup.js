const API = 'http://127.0.0.1:21984';

const dot = document.getElementById('dot');
const disconnected = document.getElementById('disconnected');
const main = document.getElementById('main');
const paperTitle = document.getElementById('paperTitle');
const paperAuthors = document.getElementById('paperAuthors');
const paperDoi = document.getElementById('paperDoi');
const folderSelect = document.getElementById('folderSelect');
const tagsSection = document.getElementById('tagsSection');
const tagChips = document.getElementById('tagChips');
const addBtn = document.getElementById('addBtn');
const result = document.getElementById('result');

let pageMetadata = null;
let selectedTagIds = new Set();

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

// Fetch collections from Rotero
async function loadCollections() {
  try {
    const resp = await fetch(`${API}/api/collections`);
    const data = await resp.json();
    for (const coll of data.collections) {
      const opt = document.createElement('option');
      opt.value = coll.id;
      opt.textContent = coll.name;
      folderSelect.appendChild(opt);
    }
  } catch {}
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
        const id = parseInt(chip.dataset.tagId);
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

  // Fallback: if client-side extraction found no DOI or authors, try server-side scrape
  if (pageMetadata && !pageMetadata.doi && (!pageMetadata.authors || pageMetadata.authors.length === 0)) {
    try {
      const scrapeResp = await fetch(`${API}/api/scrape`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ url: pageMetadata.url }),
      });
      if (scrapeResp.ok) {
        const scrapeData = await scrapeResp.json();
        const m = scrapeData.success ? scrapeData.metadata : null;
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
      }
    } catch {}
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

function extractMetadata() {
  const meta = { url: window.location.href };

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

  return meta;
}

// Update button text based on folder selection
folderSelect.addEventListener('change', () => {
  const selected = folderSelect.options[folderSelect.selectedIndex];
  addBtn.textContent = folderSelect.value
    ? `Add to "${selected.textContent}"`
    : 'Add to Library';
});

// Save paper
addBtn.addEventListener('click', async () => {
  if (!pageMetadata) return;

  addBtn.disabled = true;
  addBtn.textContent = 'Adding...';
  result.style.display = 'none';

  const collectionId = folderSelect.value ? parseInt(folderSelect.value) : null;
  const selectedName = folderSelect.value
    ? folderSelect.options[folderSelect.selectedIndex].textContent
    : 'Library';

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
  const selected = folderSelect.options[folderSelect.selectedIndex];
  addBtn.textContent = folderSelect.value
    ? `Add to "${selected.textContent}"`
    : 'Add to Library';
});

init();
