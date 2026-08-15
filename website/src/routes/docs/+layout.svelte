<script lang="ts">
  import Nav from '$lib/components/Nav.svelte';
  import Footer from '$lib/components/Footer.svelte';
  import DocsSidebar from '$lib/components/docs/DocsSidebar.svelte';
  import Toc from '$lib/components/docs/Toc.svelte';

  let { children } = $props();
  let menuOpen = $state(false);
</script>

<Nav />

<div class="docs container">
  <button class="menu-toggle" onclick={() => (menuOpen = !menuOpen)} aria-expanded={menuOpen}>
    {menuOpen ? 'Hide' : 'Browse'} documentation
  </button>

  <aside class="rail left" class:open={menuOpen}>
    <DocsSidebar onNavigate={() => (menuOpen = false)} />
  </aside>

  <main>
    {@render children()}
  </main>

  <aside class="rail right">
    <Toc />
  </aside>
</div>

<Footer />

<style>
  .docs {
    display: grid;
    grid-template-columns: 220px minmax(0, 1fr) 200px;
    gap: var(--space-10);
    align-items: start;
    /* Clears the fixed nav. */
    padding-top: var(--space-24);
    padding-bottom: var(--space-20);
  }

  .rail {
    position: sticky;
    top: var(--space-20);
    max-height: calc(100vh - var(--space-24));
    overflow-y: auto;
    /* Room for the scrollbar so it doesn't overlap the links. */
    padding-right: var(--space-2);
  }

  main {
    min-width: 0;
    /* A comfortable measure for long-form reading. */
    max-width: 72ch;
  }

  .menu-toggle {
    display: none;
  }

  @media (max-width: 1100px) {
    .docs {
      grid-template-columns: 220px minmax(0, 1fr);
    }

    .rail.right {
      display: none;
    }
  }

  @media (max-width: 860px) {
    .docs {
      grid-template-columns: minmax(0, 1fr);
      gap: var(--space-6);
    }

    .menu-toggle {
      display: block;
      width: 100%;
      padding: var(--space-3);
      border: 1px solid var(--border);
      border-radius: var(--radius-md);
      background: var(--bg-surface);
      color: var(--text-primary);
      font-family: var(--font-sans);
      font-size: var(--text-sm);
      text-align: left;
    }

    .rail.left {
      position: static;
      max-height: none;
      overflow: visible;
      display: none;
      padding: var(--space-4) 0;
    }

    .rail.left.open {
      display: block;
    }
  }
</style>
