<script lang="ts">
  import { onMount } from 'svelte';

  let loaded = $state(false);

  onMount(() => {
    requestAnimationFrame(() => { loaded = true; });
  });
</script>

<section class="hero" class:loaded>
  <div class="hero-bg">
    <div class="glow glow-1"></div>
    <div class="glow glow-2"></div>
  </div>

  <div class="container hero-content">
    <div class="hero-text">
      <p class="hero-eyebrow">Paper reading, reimagined</p>
      <h1 class="hero-headline">Research,<br/><span class="accent">refined.</span></h1>
      <p class="hero-sub">
        A fast, private, local-first reference manager built with Rust.
        Read, annotate, cite, and explore your papers — without the bloat.
      </p>
      <div class="hero-actions">
        <a href="#download" class="btn-primary">
          Download Rotero
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 9l6 6 6-6"/></svg>
        </a>
        <a href="https://github.com/PoHsuanLai/rotero" target="_blank" rel="noopener" class="btn-ghost">
          View source
          <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><line x1="7" y1="17" x2="17" y2="7"/><polyline points="7 7 17 7 17 17"/></svg>
        </a>
      </div>
    </div>

    <div class="hero-visual">
      <div class="app-screenshot">
        <img src="/screenshot-library.png" alt="Rotero library view" />
      </div>
    </div>
  </div>
</section>

<style>
  .hero {
    position: relative;
    min-height: 100vh;
    display: flex;
    align-items: center;
    padding-top: 80px;
    overflow: hidden;
  }

  .hero-bg {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }

  .glow {
    position: absolute;
    border-radius: 50%;
    filter: blur(120px);
    opacity: 0;
    transition: opacity 1.5s var(--ease);
  }

  .loaded .glow { opacity: 1; }

  .glow-1 {
    width: 600px;
    height: 600px;
    background: var(--accent);
    opacity: 0.06;
    top: -200px;
    right: -100px;
  }

  .loaded .glow-1 { opacity: 0.06; }

  .glow-2 {
    width: 400px;
    height: 400px;
    background: var(--accent);
    opacity: 0.04;
    bottom: -100px;
    left: -50px;
  }

  .loaded .glow-2 { opacity: 0.04; }

  .hero-content {
    display: grid;
    grid-template-columns: 1fr 1.1fr;
    gap: var(--space-16);
    align-items: center;
    position: relative;
    z-index: 1;
  }

  .hero-text {
    opacity: 0;
    transform: translateY(32px);
    transition: opacity 0.8s var(--ease), transform 0.8s var(--ease);
  }

  .loaded .hero-text {
    opacity: 1;
    transform: translateY(0);
  }

  .hero-eyebrow {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
    color: var(--accent);
    margin-bottom: var(--space-4);
  }

  .hero-headline {
    font-family: var(--font-brand);
    font-size: clamp(3rem, 6vw, var(--text-7xl));
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tighter);
    color: var(--text-primary);
    margin-bottom: var(--space-6);
  }

  .accent {
    color: var(--accent);
  }

  .hero-sub {
    font-size: var(--text-lg);
    line-height: var(--leading-normal);
    color: var(--text-secondary);
    max-width: 480px;
    margin-bottom: var(--space-10);
  }

  .hero-actions {
    display: flex;
    gap: var(--space-4);
    align-items: center;
    flex-wrap: wrap;
  }

  .btn-primary {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    padding: var(--space-3) var(--space-6);
    background: var(--accent);
    color: #fff;
    border-radius: var(--radius-lg);
    transition: background var(--transition-fast), transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .btn-primary:hover {
    background: var(--accent-hover);
    transform: translateY(-1px);
    box-shadow: 0 4px 16px var(--accent-ring);
  }

  .btn-primary:active {
    background: var(--accent-pressed);
    transform: translateY(0);
  }

  .btn-ghost {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 500;
    padding: var(--space-3) var(--space-5);
    color: var(--text-secondary);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    transition: color var(--transition-fast), border-color var(--transition-fast), background var(--transition-fast);
  }

  .btn-ghost:hover {
    color: var(--text-primary);
    border-color: var(--text-tertiary);
    background: var(--bg-muted);
  }

  /* App screenshot */
  .hero-visual {
    position: relative;
    opacity: 0;
    transform: translateY(32px) scale(0.97);
    transition: opacity 1s var(--ease) 0.3s, transform 1s var(--ease) 0.3s;
  }

  .loaded .hero-visual {
    opacity: 1;
    transform: translateY(0) scale(1);
  }

  .app-screenshot {
    border-radius: var(--radius-xl);
    border: 1px solid var(--border);
    box-shadow: var(--shadow-card);
    overflow: hidden;
  }

  .app-screenshot img {
    display: block;
    width: 100%;
    height: auto;
  }

  @media (max-width: 1024px) {
    .hero-content {
      grid-template-columns: 1fr;
      text-align: center;
      gap: var(--space-10);
    }

    .hero-sub {
      margin-left: auto;
      margin-right: auto;
    }

    .hero-actions {
      justify-content: center;
    }
  }

  @media (max-width: 640px) {
    .hero {
      min-height: auto;
      padding-top: 120px;
      padding-bottom: var(--space-12);
    }
  }
</style>
