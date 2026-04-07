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
      <div class="app-mock">
        <div class="mock-titlebar">
          <span class="dot red"></span>
          <span class="dot yellow"></span>
          <span class="dot green"></span>
        </div>
        <div class="mock-body">
          <div class="mock-sidebar">
            <div class="mock-sidebar-item active"></div>
            <div class="mock-sidebar-item"></div>
            <div class="mock-sidebar-item"></div>
            <div class="mock-sidebar-divider"></div>
            <div class="mock-sidebar-item short"></div>
            <div class="mock-sidebar-item"></div>
          </div>
          <div class="mock-main">
            <div class="mock-toolbar">
              <div class="mock-tab active"></div>
              <div class="mock-tab"></div>
            </div>
            <div class="mock-pdf">
              <div class="mock-page">
                <div class="mock-line w-80"></div>
                <div class="mock-line w-60"></div>
                <div class="mock-line"></div>
                <div class="mock-line"></div>
                <div class="mock-line highlight"></div>
                <div class="mock-line highlight"></div>
                <div class="mock-line w-90"></div>
                <div class="mock-line"></div>
                <div class="mock-line w-70"></div>
                <div class="mock-line"></div>
                <div class="mock-line w-40"></div>
              </div>
            </div>
          </div>
          <div class="mock-detail">
            <div class="mock-detail-title"></div>
            <div class="mock-detail-meta"></div>
            <div class="mock-detail-meta short"></div>
            <div class="mock-detail-divider"></div>
            <div class="mock-detail-label"></div>
            <div class="mock-detail-tag"></div>
            <div class="mock-detail-tag"></div>
          </div>
        </div>
      </div>

      <!-- Floating decorative elements -->
      <div class="float-annotation">
        <svg width="16" height="16" viewBox="0 0 24 24" fill="var(--accent)" opacity="0.8"><rect x="3" y="3" width="18" height="18" rx="2" fill="none" stroke="var(--accent)" stroke-width="2"/><path d="M8 7h8M8 11h5" stroke="var(--accent)" stroke-width="2" stroke-linecap="round"/></svg>
      </div>
      <div class="float-cite">
        <span>[1]</span>
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

  /* App mockup */
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

  .app-mock {
    border-radius: var(--radius-xl);
    border: 1px solid var(--border);
    background: var(--bg-surface);
    box-shadow: var(--shadow-card);
    overflow: hidden;
  }

  .mock-titlebar {
    display: flex;
    gap: 6px;
    padding: 10px 14px;
    background: var(--bg-surface);
    border-bottom: 1px solid var(--border-subtle);
  }

  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
  }
  .dot.red { background: #ef4444; }
  .dot.yellow { background: #f59e0b; }
  .dot.green { background: #22c55e; }

  .mock-body {
    display: grid;
    grid-template-columns: 140px 1fr 120px;
    height: 320px;
  }

  .mock-sidebar {
    padding: 12px 10px;
    border-right: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 4px;
  }

  .mock-sidebar-item {
    height: 8px;
    border-radius: 3px;
    background: var(--text-muted);
    opacity: 0.3;
    width: 80%;
  }

  .mock-sidebar-item.active {
    background: var(--accent);
    opacity: 0.6;
  }

  .mock-sidebar-item.short {
    width: 50%;
  }

  .mock-sidebar-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 6px 0;
  }

  .mock-main {
    display: flex;
    flex-direction: column;
  }

  .mock-toolbar {
    display: flex;
    gap: 2px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--border-subtle);
  }

  .mock-tab {
    height: 6px;
    width: 60px;
    border-radius: 3px;
    background: var(--text-muted);
    opacity: 0.2;
  }

  .mock-tab.active {
    background: var(--accent);
    opacity: 0.5;
  }

  .mock-pdf {
    flex: 1;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 16px;
    background: var(--bg-muted);
  }

  .mock-page {
    background: var(--bg-primary);
    border-radius: 4px;
    padding: 16px 14px;
    box-shadow: var(--shadow-sm);
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
    max-width: 280px;
  }

  .mock-line {
    height: 4px;
    border-radius: 2px;
    background: var(--text-muted);
    opacity: 0.2;
    width: 100%;
  }

  .mock-line.w-80 { width: 80%; }
  .mock-line.w-90 { width: 90%; }
  .mock-line.w-70 { width: 70%; }
  .mock-line.w-60 { width: 60%; }
  .mock-line.w-40 { width: 40%; }

  .mock-line.highlight {
    background: var(--accent);
    opacity: 0.25;
  }

  .mock-detail {
    padding: 12px 10px;
    border-left: 1px solid var(--border-subtle);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }

  .mock-detail-title {
    height: 10px;
    width: 85%;
    border-radius: 3px;
    background: var(--text-muted);
    opacity: 0.35;
  }

  .mock-detail-meta {
    height: 6px;
    width: 70%;
    border-radius: 3px;
    background: var(--text-muted);
    opacity: 0.2;
  }

  .mock-detail-meta.short { width: 45%; }

  .mock-detail-divider {
    height: 1px;
    background: var(--border-subtle);
    margin: 4px 0;
  }

  .mock-detail-label {
    height: 5px;
    width: 35%;
    border-radius: 3px;
    background: var(--accent);
    opacity: 0.4;
  }

  .mock-detail-tag {
    height: 14px;
    width: 55px;
    border-radius: var(--radius-full);
    background: var(--accent);
    opacity: 0.15;
  }

  /* Floating elements */
  .float-annotation {
    position: absolute;
    top: 20%;
    right: -16px;
    padding: 8px;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    box-shadow: var(--shadow-md);
    animation: float 6s ease-in-out infinite;
    opacity: 0;
    transition: opacity 1s var(--ease) 0.8s;
  }

  .loaded .float-annotation { opacity: 1; }

  .float-cite {
    position: absolute;
    bottom: 25%;
    left: -12px;
    padding: 4px 10px;
    background: var(--accent);
    color: #fff;
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    font-weight: 600;
    border-radius: var(--radius-sm);
    box-shadow: var(--shadow-md);
    animation: float 5s ease-in-out infinite 1s;
    opacity: 0;
    transition: opacity 1s var(--ease) 1s;
  }

  .loaded .float-cite { opacity: 1; }

  @keyframes float {
    0%, 100% { transform: translateY(0px); }
    50% { transform: translateY(-10px); }
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

    .mock-body {
      grid-template-columns: 100px 1fr 90px;
      height: 260px;
    }

    .float-annotation, .float-cite {
      display: none;
    }
  }

  @media (max-width: 640px) {
    .hero {
      min-height: auto;
      padding-top: 120px;
      padding-bottom: var(--space-12);
    }

    .mock-detail {
      display: none;
    }

    .mock-body {
      grid-template-columns: 80px 1fr;
      height: 220px;
    }
  }
</style>
