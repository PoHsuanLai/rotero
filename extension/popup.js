const API_BASE = 'http://localhost:21984';

const statusEl = document.getElementById('status');
const metaEl = document.getElementById('meta');
const saveBtn = document.getElementById('saveBtn');
const resultEl = document.getElementById('result');

let pageMetadata = null;

// Check if Rotero is running
async function checkStatus() {
  try {
    const resp = await fetch(`${API_BASE}/api/status`);
    const data = await resp.json();
    if (data.status === 'ok') {
      statusEl.textContent = `Connected to ${data.name} v${data.version}`;
      statusEl.className = 'status ok';
      return true;
    }
  } catch (e) {
    statusEl.textContent = 'Rotero is not running. Please start the app.';
    statusEl.className = 'status error';
  }
  return false;
}

// Extract metadata from the current page via content script
async function getPageMetadata() {
  try {
    const [tab] = await chrome.tabs.query({ active: true, currentWindow: true });
    const results = await chrome.scripting.executeScript({
      target: { tabId: tab.id },
      func: extractMetadata,
    });
    return results[0]?.result || null;
  } catch (e) {
    console.error('Failed to extract metadata:', e);
    return null;
  }
}

// This function runs in the page context
function extractMetadata() {
  const meta = {};

  // Try DOI from meta tags
  const doiMeta = document.querySelector('meta[name="citation_doi"], meta[name="DC.identifier"], meta[property="citation_doi"]');
  if (doiMeta) {
    meta.doi = doiMeta.content.replace(/^https?:\/\/doi\.org\//, '');
  }

  // Try DOI from URL
  if (!meta.doi) {
    const doiMatch = window.location.href.match(/doi\.org\/(10\.[^/]+\/.+?)(?:\?|#|$)/);
    if (doiMatch) meta.doi = doiMatch[1];
  }

  // arXiv ID
  const arxivMatch = window.location.href.match(/arxiv\.org\/(?:abs|pdf)\/(\d+\.\d+)/);
  if (arxivMatch) {
    meta.arxiv_id = arxivMatch[1];
    if (!meta.doi) meta.doi = null; // arXiv papers may not have DOIs
  }

  // Title
  const titleMeta = document.querySelector('meta[name="citation_title"], meta[property="og:title"], meta[name="DC.title"]');
  meta.title = titleMeta ? titleMeta.content : document.title;

  // Authors
  const authorMetas = document.querySelectorAll('meta[name="citation_author"], meta[name="DC.creator"]');
  if (authorMetas.length > 0) {
    meta.authors = Array.from(authorMetas).map(m => m.content);
  }

  // PDF URL
  const pdfMeta = document.querySelector('meta[name="citation_pdf_url"]');
  if (pdfMeta) {
    meta.pdf_url = pdfMeta.content;
  }

  meta.url = window.location.href;

  return meta;
}

// Display metadata in popup
function showMetadata(meta) {
  if (!meta) return;
  let html = '';
  if (meta.title) html += `<div><strong>Title:</strong> ${meta.title}</div>`;
  if (meta.authors?.length) html += `<div><strong>Authors:</strong> ${meta.authors.join(', ')}</div>`;
  if (meta.doi) html += `<div><strong>DOI:</strong> ${meta.doi}</div>`;
  if (meta.pdf_url) html += `<div><strong>PDF:</strong> Available</div>`;
  if (html) {
    metaEl.innerHTML = html;
    metaEl.style.display = 'block';
  }
}

// Save paper to Rotero
async function savePaper() {
  if (!pageMetadata) return;

  saveBtn.disabled = true;
  saveBtn.textContent = 'Saving...';

  try {
    const resp = await fetch(`${API_BASE}/api/save`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify(pageMetadata),
    });
    const data = await resp.json();

    if (data.success) {
      resultEl.textContent = 'Paper saved to Rotero!';
      resultEl.className = 'result success';
    } else {
      resultEl.textContent = `Error: ${data.message}`;
      resultEl.className = 'result fail';
    }
  } catch (e) {
    resultEl.textContent = 'Failed to save. Is Rotero running?';
    resultEl.className = 'result fail';
  }

  resultEl.style.display = 'block';
  saveBtn.disabled = false;
  saveBtn.textContent = 'Save to Rotero';
}

// Initialize
(async () => {
  const connected = await checkStatus();
  if (connected) {
    pageMetadata = await getPageMetadata();
    showMetadata(pageMetadata);
    saveBtn.disabled = false;
  }
})();

saveBtn.addEventListener('click', savePaper);
