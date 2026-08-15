<script lang="ts">
  import { onMount } from 'svelte';
  import { page } from '$app/state';

  interface Heading {
    id: string;
    text: string;
    level: number;
  }

  let headings = $state<Heading[]>([]);
  let activeId = $state('');

  // Rebuilt per page: mdsvex renders the article's headings into the DOM, so
  // reading them back is simpler than threading structured data out of it.
  $effect(() => {
    // Re-run on navigation.
    void page.url.pathname;
    collect();
  });

  function collect() {
    const article = document.querySelector('article');
    if (!article) return;
    headings = [...article.querySelectorAll('h2, h3')].map((el) => ({
      id: el.id,
      text: el.textContent ?? '',
      level: Number(el.tagName[1])
    }));
  }

  onMount(() => {
    collect();

    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) activeId = entry.target.id;
        }
      },
      // Bias toward the heading nearest the top of the viewport.
      { rootMargin: '0px 0px -75% 0px' }
    );

    const article = document.querySelector('article');
    article?.querySelectorAll('h2, h3').forEach((el) => observer.observe(el));
    return () => observer.disconnect();
  });
</script>

{#if headings.length > 1}
  <nav class="toc" aria-label="On this page">
    <p class="toc-title">On this page</p>
    <ul>
      {#each headings as heading (heading.id)}
        <li class:nested={heading.level === 3}>
          <a href="#{heading.id}" class:active={activeId === heading.id}>{heading.text}</a>
        </li>
      {/each}
    </ul>
  </nav>
{/if}

<style>
  .toc {
    font-family: var(--font-sans);
  }

  .toc-title {
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
    border-left: 1px solid var(--border);
  }

  li.nested a {
    padding-left: var(--space-5);
  }

  a {
    display: block;
    padding: 3px var(--space-3);
    margin-left: -1px;
    border-left: 1px solid transparent;
    font-size: var(--text-sm);
    line-height: var(--leading-snug);
    color: var(--text-tertiary);
    transition: color var(--transition-fast), border-color var(--transition-fast);
  }

  a:hover {
    color: var(--text-primary);
  }

  a.active {
    color: var(--accent);
    border-left-color: var(--accent);
  }
</style>
