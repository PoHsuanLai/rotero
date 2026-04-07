<script lang="ts">
  import { theme, toggleTheme } from '$lib/theme';
  import { onMount } from 'svelte';

  let scrolled = $state(false);
  let mobileOpen = $state(false);

  onMount(() => {
    const onScroll = () => { scrolled = window.scrollY > 40; };
    window.addEventListener('scroll', onScroll, { passive: true });
    return () => window.removeEventListener('scroll', onScroll);
  });

  function navClick() {
    mobileOpen = false;
  }
</script>

<nav class="nav" class:scrolled>
  <div class="nav-inner container">
    <a href="/" class="wordmark" onclick={navClick}>Rotero</a>

    <button class="hamburger" onclick={() => mobileOpen = !mobileOpen} aria-label="Toggle menu">
      <span class="bar" class:open={mobileOpen}></span>
      <span class="bar" class:open={mobileOpen}></span>
      <span class="bar" class:open={mobileOpen}></span>
    </button>

    <div class="nav-links" class:mobile-open={mobileOpen}>
      <a href="#features" onclick={navClick}>Features</a>
      <a href="#why" onclick={navClick}>Why Rotero</a>
      <a href="#download" onclick={navClick}>Download</a>
    </div>

    <div class="nav-actions">
      <button class="theme-toggle" onclick={toggleTheme} aria-label="Toggle theme">
        {#if $theme === 'dark'}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="12" cy="12" r="5"/><line x1="12" y1="1" x2="12" y2="3"/><line x1="12" y1="21" x2="12" y2="23"/><line x1="4.22" y1="4.22" x2="5.64" y2="5.64"/><line x1="18.36" y1="18.36" x2="19.78" y2="19.78"/><line x1="1" y1="12" x2="3" y2="12"/><line x1="21" y1="12" x2="23" y2="12"/><line x1="4.22" y1="19.78" x2="5.64" y2="18.36"/><line x1="18.36" y1="5.64" x2="19.78" y2="4.22"/></svg>
        {:else}
          <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z"/></svg>
        {/if}
      </button>
      <a href="https://github.com/PoHsuanLai/rotero" class="github-link" target="_blank" rel="noopener" aria-label="GitHub">
        <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z"/></svg>
      </a>
    </div>
  </div>
</nav>

<style>
  .nav {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    z-index: 100;
    padding: var(--space-4) 0;
    transition: background-color var(--transition-normal), backdrop-filter var(--transition-normal), padding var(--transition-normal);
  }

  .nav.scrolled {
    background: var(--nav-bg);
    backdrop-filter: blur(16px) saturate(1.8);
    -webkit-backdrop-filter: blur(16px) saturate(1.8);
    padding: var(--space-3) 0;
    border-bottom: 1px solid var(--border-subtle);
  }

  .nav-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: var(--space-8);
  }

  .wordmark {
    font-family: var(--font-brand);
    font-size: var(--text-2xl);
    letter-spacing: var(--tracking-tight);
    color: var(--text-primary);
    transition: color var(--transition-fast);
  }

  .wordmark:hover {
    color: var(--accent);
  }

  .nav-links {
    display: flex;
    gap: var(--space-8);
  }

  .nav-links a {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 500;
    color: var(--text-secondary);
    letter-spacing: var(--tracking-wide);
    text-transform: uppercase;
    transition: color var(--transition-fast);
  }

  .nav-links a:hover {
    color: var(--accent);
  }

  .nav-actions {
    display: flex;
    align-items: center;
    gap: var(--space-3);
  }

  .theme-toggle, .github-link {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 36px;
    height: 36px;
    border-radius: var(--radius-md);
    color: var(--text-secondary);
    transition: color var(--transition-fast), background-color var(--transition-fast);
  }

  .theme-toggle:hover, .github-link:hover {
    color: var(--text-primary);
    background: var(--accent-subtle);
  }

  .hamburger {
    display: none;
    flex-direction: column;
    gap: 5px;
    padding: var(--space-2);
    z-index: 101;
  }

  .bar {
    display: block;
    width: 20px;
    height: 2px;
    background: var(--text-primary);
    border-radius: 1px;
    transition: transform var(--transition-normal), opacity var(--transition-normal);
  }

  .bar.open:nth-child(1) { transform: translateY(7px) rotate(45deg); }
  .bar.open:nth-child(2) { opacity: 0; }
  .bar.open:nth-child(3) { transform: translateY(-7px) rotate(-45deg); }

  @media (max-width: 768px) {
    .hamburger {
      display: flex;
    }

    .nav-links {
      position: fixed;
      inset: 0;
      flex-direction: column;
      align-items: center;
      justify-content: center;
      gap: var(--space-10);
      background: var(--bg-primary);
      opacity: 0;
      pointer-events: none;
      transition: opacity var(--transition-slow);
    }

    .nav-links.mobile-open {
      opacity: 1;
      pointer-events: all;
    }

    .nav-links a {
      font-size: var(--text-xl);
    }

    .nav-actions {
      display: none;
    }
  }
</style>
