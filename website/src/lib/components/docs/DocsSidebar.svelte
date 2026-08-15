<script lang="ts">
  import { base } from '$app/paths';
  import { page } from '$app/state';
  import { nav } from '$lib/docs/nav';

  interface Props {
    onNavigate?: () => void;
  }

  let { onNavigate }: Props = $props();

  const current = $derived(page.url.pathname.replace(/\/$/, ''));

  function href(slug: string) {
    return `${base}/docs/${slug}`;
  }
</script>

<nav class="sidebar" aria-label="Documentation">
  {#each nav as section (section.title)}
    <div class="section">
      <p class="section-title">{section.title}</p>
      <ul>
        {#each section.pages as doc (doc.slug)}
          <li>
            <a
              href={href(doc.slug)}
              class:active={current === href(doc.slug)}
              aria-current={current === href(doc.slug) ? 'page' : undefined}
              onclick={onNavigate}
            >
              {doc.title}
            </a>
          </li>
        {/each}
      </ul>
    </div>
  {/each}
</nav>

<style>
  .sidebar {
    font-family: var(--font-sans);
  }

  .section + .section {
    margin-top: var(--space-6);
  }

  .section-title {
    margin-bottom: var(--space-2);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: var(--tracking-wide);
    text-transform: uppercase;
    color: var(--text-tertiary);
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }

  a {
    display: block;
    padding: 5px var(--space-3);
    margin-left: calc(var(--space-3) * -1);
    border-radius: var(--radius-sm);
    font-size: var(--text-sm);
    line-height: var(--leading-snug);
    color: var(--text-secondary);
    transition: color var(--transition-fast), background-color var(--transition-fast);
  }

  a:hover {
    color: var(--text-primary);
    background: var(--bg-muted);
  }

  a.active {
    color: var(--accent);
    background: var(--accent-subtle);
    font-weight: 500;
  }
</style>
