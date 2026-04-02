const API = 'http://127.0.0.1:21984';

const dot = document.getElementById('dot');
const disconnected = document.getElementById('disconnected');
const main = document.getElementById('main');
const paperTitle = document.getElementById('paperTitle');
const paperAuthors = document.getElementById('paperAuthors');
const paperDoi = document.getElementById('paperDoi');
const folderSelect = document.getElementById('folderSelect');
const addBtn = document.getElementById('addBtn');
const result = document.getElementById('result');

let pageMetadata = null;

// Check connection and load collections
async function init() {
  try {
    const resp = await fetch(`${API}/api/status`, { signal: AbortSignal.timeout(2000) });
    const data = await resp.json();
    if (data.status === 'ok') {
      dot.className = 'dot ok';
      main.style.display = 'block';
      await loadCollections();
      await loadMetadata();
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

  if (pageMetadata) {
    paperTitle.textContent = pageMetadata.title || 'Untitled';
    paperAuthors.textContent = pageMetadata.authors?.join(', ') || '';
    paperDoi.textContent = pageMetadata.doi ? `DOI: ${pageMetadata.doi}` : '';
    paperAuthors.style.display = pageMetadata.authors?.length ? 'block' : 'none';
    paperDoi.style.display = pageMetadata.doi ? 'block' : 'none';
  }
}

function extractMetadata() {
  const meta = { url: window.location.href };

  // DOI
  const doiMeta = document.querySelector('meta[name="citation_doi"], meta[name="DC.identifier"], meta[property="citation_doi"]');
  if (doiMeta) meta.doi = doiMeta.content.replace(/^https?:\/\/doi\.org\//, '');
  if (!meta.doi) {
    const m = window.location.href.match(/doi\.org\/(10\.[^/]+\/.+?)(?:\?|#|$)/);
    if (m) meta.doi = m[1];
  }

  // Title
  const titleMeta = document.querySelector('meta[name="citation_title"], meta[property="og:title"], meta[name="DC.title"]');
  meta.title = titleMeta ? titleMeta.content : document.title;

  // Authors
  const authorMetas = document.querySelectorAll('meta[name="citation_author"], meta[name="DC.creator"]');
  if (authorMetas.length) meta.authors = Array.from(authorMetas).map(m => m.content);

  // PDF URL
  const pdfMeta = document.querySelector('meta[name="citation_pdf_url"]');
  if (pdfMeta) meta.pdf_url = pdfMeta.content;

  // arXiv
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
      body: JSON.stringify({ ...pageMetadata, collection_id: collectionId }),
    });
    const data = await resp.json();

    if (data.success) {
      result.className = 'result success';
      result.innerHTML = `Added to <strong>${selectedName}</strong>`;
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
