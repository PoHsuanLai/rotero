<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { neighbours } from '$lib/docs/nav';

  interface Props {
    title?: string;
    description?: string;
    children: import('svelte').Snippet;
  }

  let { title, description, children }: Props = $props();

  const slug = $derived(page.url.pathname.split('/docs/')[1]?.replace(/\/$/, '') ?? '');
  const around = $derived(neighbours(slug));
</script>

<svelte:head>
  <title>{title ? `${title} — Rotero` : 'Rotero documentation'}</title>
  {#if description}
    <meta name="description" content={description} />
  {/if}
</svelte:head>

<article data-pagefind-body>
  {#if title}
    <h1>{title}</h1>
  {/if}
  {#if description}
    <p class="lede">{description}</p>
  {/if}

  {@render children()}
</article>

{#if around.prev || around.next}
  <nav class="pager" aria-label="Previous and next page">
    {#if around.prev}
      <a class="prev" href="{base}/docs/{around.prev.slug}">
        <span class="direction">Previous</span>
        <span class="page-title">{around.prev.title}</span>
      </a>
    {:else}
      <span></span>
    {/if}
    {#if around.next}
      <a class="next" href="{base}/docs/{around.next.slug}">
        <span class="direction">Next</span>
        <span class="page-title">{around.next.title}</span>
      </a>
    {/if}
  </nav>
{/if}

<style>
  article {
    font-size: var(--text-base);
    line-height: var(--leading-normal);
  }

  article :global(h1) {
    font-family: var(--font-brand);
    font-size: var(--text-4xl);
    font-weight: 400;
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tight);
    margin-bottom: var(--space-3);
  }

  .lede {
    margin-bottom: var(--space-8);
    font-size: var(--text-lg);
    color: var(--text-secondary);
  }

  article :global(h2) {
    margin-top: var(--space-12);
    margin-bottom: var(--space-4);
    padding-top: var(--space-2);
    font-family: var(--font-brand);
    font-size: var(--text-2xl);
    font-weight: 400;
    letter-spacing: var(--tracking-tight);
    /* Anchored headings must clear the fixed nav when jumped to. */
    scroll-margin-top: var(--space-20);
  }

  article :global(h3) {
    margin-top: var(--space-8);
    margin-bottom: var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--text-lg);
    font-weight: 600;
    scroll-margin-top: var(--space-20);
  }

  article :global(p) {
    margin-bottom: var(--space-4);
  }

  article :global(ul),
  article :global(ol) {
    margin-bottom: var(--space-4);
    padding-left: var(--space-6);
  }

  article :global(li) {
    margin-bottom: var(--space-2);
  }

  article :global(a) {
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
  }

  article :global(a:hover) {
    color: var(--accent-hover);
  }

  article :global(code) {
    padding: 1px 5px;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    font-family: var(--font-mono);
    font-size: 0.875em;
  }

  article :global(pre) {
    margin-bottom: var(--space-6);
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-muted);
    overflow-x: auto;
  }

  article :global(pre code) {
    padding: 0;
    background: none;
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  article :global(blockquote) {
    margin: var(--space-6) 0;
    padding-left: var(--space-4);
    border-left: 3px solid var(--border);
    color: var(--text-secondary);
  }

  /* Tables carry the reference material, so they need to stay readable on
     narrow screens without forcing the page itself to scroll sideways. */
  article :global(table) {
    display: block;
    width: 100%;
    margin-bottom: var(--space-6);
    border-collapse: collapse;
    overflow-x: auto;
    font-family: var(--font-sans);
    font-size: var(--text-sm);
  }

  article :global(th),
  article :global(td) {
    padding: var(--space-2) var(--space-3);
    border-bottom: 1px solid var(--border);
    text-align: left;
    vertical-align: top;
  }

  article :global(th) {
    font-weight: 600;
    color: var(--text-secondary);
    white-space: nowrap;
  }

  article :global(hr) {
    margin: var(--space-12) 0;
    border: 0;
    border-top: 1px solid var(--border);
  }

  .pager {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: var(--space-4);
    margin-top: var(--space-16);
    padding-top: var(--space-8);
    border-top: 1px solid var(--border);
  }

  .pager a {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: var(--space-4);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    font-family: var(--font-sans);
    transition: border-color var(--transition-fast), background-color var(--transition-fast);
  }

  .pager a:hover {
    border-color: var(--accent);
    background: var(--accent-subtle);
  }

  .pager .next {
    text-align: right;
  }

  .direction {
    font-size: var(--text-xs);
    letter-spacing: var(--tracking-wide);
    text-transform: uppercase;
    color: var(--text-tertiary);
  }

  .page-title {
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-primary);
  }

  @media (max-width: 640px) {
    .pager {
      grid-template-columns: 1fr;
    }

    .pager .next {
      text-align: left;
    }
  }
</style>
