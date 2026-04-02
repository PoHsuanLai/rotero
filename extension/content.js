// Content script: extracts metadata from academic paper pages
// Runs on publisher sites, arXiv, Google Scholar, etc.

(function () {
  function extractMetadata() {
    const meta = {
      url: window.location.href,
      title: null,
      doi: null,
      authors: [],
      pdf_url: null,
    };

    // Try DOI from meta tags
    const doiMeta =
      document.querySelector('meta[name="citation_doi"]') ||
      document.querySelector('meta[name="DC.identifier"]') ||
      document.querySelector('meta[name="dc.identifier"]') ||
      document.querySelector('meta[scheme="doi"]');
    if (doiMeta) {
      meta.doi = doiMeta.content.replace(/^https?:\/\/doi\.org\//, "");
    }

    // Try DOI from URL
    if (!meta.doi) {
      const doiMatch = window.location.href.match(
        /(?:doi\.org\/|doi=)(10\.\d{4,}\/[^\s&?#]+)/i
      );
      if (doiMatch) meta.doi = doiMatch[1];
    }

    // Title from meta tags
    const titleMeta =
      document.querySelector('meta[name="citation_title"]') ||
      document.querySelector('meta[name="DC.title"]') ||
      document.querySelector('meta[name="dc.title"]');
    meta.title = titleMeta ? titleMeta.content : document.title;

    // Authors from meta tags
    const authorMetas = document.querySelectorAll(
      'meta[name="citation_author"], meta[name="DC.creator"], meta[name="dc.creator"]'
    );
    authorMetas.forEach((m) => {
      if (m.content) meta.authors.push(m.content);
    });

    // PDF URL from meta tags
    const pdfMeta = document.querySelector('meta[name="citation_pdf_url"]');
    if (pdfMeta) meta.pdf_url = pdfMeta.content;

    // arXiv-specific: extract arXiv ID as DOI-like identifier
    if (window.location.hostname.includes("arxiv.org") && !meta.doi) {
      const arxivMatch = window.location.pathname.match(
        /\/(?:abs|pdf)\/(\d+\.\d+)/
      );
      if (arxivMatch) {
        meta.doi = "arXiv:" + arxivMatch[1];
        if (!meta.pdf_url) {
          meta.pdf_url = `https://arxiv.org/pdf/${arxivMatch[1]}.pdf`;
        }
      }
    }

    return meta;
  }

  // Store extracted metadata for the popup to access
  window.__rotero_metadata = extractMetadata();
})();
