<script lang="ts">
  import { base } from '$app/paths';

  interface Result {
    url: string;
    meta: { title?: string };
    excerpt: string;
  }

  let query = $state('');
  let results = $state<Result[]>([]);
  let open = $state(false);
  let loading = $state(false);
  let pagefind: any = null;

  /**
   * The index only exists in a production build, so it is imported lazily and
   * by a path Vite will not try to resolve — in `npm run dev` there is nothing
   * to load and search stays inert rather than breaking the page.
   *
   * The directory is `pagefind`, not Pagefind's default `_pagefind`: Vite's
   * preview server refuses to serve underscore-prefixed paths, so the default
   * works on GitHub Pages but 404s locally.
   */
  async function ensureIndex() {
    if (pagefind) return pagefind;
    try {
      pagefind = await import(/* @vite-ignore */ `${base}/pagefind/pagefind.js`);
      await pagefind.options({ baseUrl: `${base}/` });
      return pagefind;
    } catch {
      return null;
    }
  }

  async function search() {
    const term = query.trim();
    if (term.length < 2) {
      results = [];
      return;
    }

    loading = true;
    const index = await ensureIndex();
    if (!index) {
      loading = false;
      return;
    }

    const found = await index.search(term);
    results = await Promise.all(found.results.slice(0, 8).map((r: any) => r.data()));
    loading = false;
  }

  let debounce: ReturnType<typeof setTimeout>;
  function onInput() {
    open = true;
    clearTimeout(debounce);
    debounce = setTimeout(search, 150);
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      open = false;
      (event.target as HTMLInputElement).blur();
    }
  }
</script>

<svelte:window onclick={() => (open = false)} />

<div class="search" onclick={(e) => e.stopPropagation()} role="search">
  <input
    type="search"
    bind:value={query}
    oninput={onInput}
    onkeydown={onKeydown}
    onfocus={() => (open = true)}
    placeholder="Search the guide"
    aria-label="Search the documentation"
  />

  {#if open && query.trim().length >= 2}
    <div class="results" role="listbox">
      {#if loading}
        <p class="hint">Searching…</p>
      {:else if results.length === 0}
        <p class="hint">No matches for “{query}”.</p>
      {:else}
        {#each results as result (result.url)}
          <a href={result.url} onclick={() => (open = false)}>
            <span class="result-title">{result.meta.title ?? 'Untitled'}</span>
            <span class="excerpt">{@html result.excerpt}</span>
          </a>
        {/each}
      {/if}
    </div>
  {/if}
</div>

<style>
  .search {
    position: relative;
    margin-bottom: var(--space-6);
  }

  input {
    width: 100%;
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    color: var(--text-primary);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }

  input:focus {
    outline: none;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ring);
  }

  .results {
    position: absolute;
    z-index: 20;
    top: calc(100% + 4px);
    left: 0;
    /* Wider than the sidebar rail so excerpts stay readable. */
    width: max(100%, 320px);
    max-height: 60vh;
    overflow-y: auto;
    padding: var(--space-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-primary);
    box-shadow: var(--shadow-lg);
  }

  .results a {
    display: block;
    padding: var(--space-2) var(--space-3);
    border-radius: var(--radius-sm);
  }

  .results a:hover {
    background: var(--bg-muted);
  }

  .result-title {
    display: block;
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-primary);
  }

  .excerpt {
    display: block;
    margin-top: 2px;
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    line-height: var(--leading-snug);
    color: var(--text-secondary);
  }

  .excerpt :global(mark) {
    background: var(--accent-subtle);
    color: var(--accent);
    font-weight: 600;
  }

  .hint {
    padding: var(--space-2) var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
  }
</style>
