<script lang="ts">
  import { onMount } from 'svelte';

  let items: HTMLElement[] = [];

  onMount(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((e) => {
          if (e.isIntersecting) e.target.classList.add('visible');
        });
      },
      { threshold: 0.15 }
    );
    items.forEach((el) => el && observer.observe(el));
    return () => observer.disconnect();
  });
</script>

<section id="why">
  <div class="container">
    <div class="section-header reveal" bind:this={items[0]}>
      <p class="section-eyebrow">Why Rotero</p>
      <h2 class="section-title">Built different.</h2>
    </div>

    <div class="pillars">
      <div class="pillar reveal reveal-delay-1" bind:this={items[1]}>
        <div class="pillar-accent"></div>
        <div class="pillar-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polygon points="13 2 3 14 12 14 11 22 21 10 12 10 13 2"/></svg>
        </div>
        <h3 class="pillar-title">Blazing fast</h3>
        <p class="pillar-desc">
          Native Rust binary. No Electron, no JVM, no garbage collector.
          Starts instantly, stays responsive with thousands of papers.
        </p>
        <div class="pillar-detail">
          <span class="mono">~8 MB</span> binary size
        </div>
      </div>

      <div class="pillar reveal reveal-delay-2" bind:this={items[2]}>
        <div class="pillar-accent"></div>
        <div class="pillar-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="11" width="18" height="11" rx="2" ry="2"/><path d="M7 11V7a5 5 0 0 1 10 0v4"/></svg>
        </div>
        <h3 class="pillar-title">Truly private</h3>
        <p class="pillar-desc">
          Your library lives on your machine. SQLite database, local PDF storage.
          No accounts, no telemetry, no cloud dependency.
        </p>
        <div class="pillar-detail">
          <span class="mono">0</span> data sent to servers
        </div>
      </div>

      <div class="pillar reveal reveal-delay-3" bind:this={items[3]}>
        <div class="pillar-accent"></div>
        <div class="pillar-icon">
          <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 1v4M12 19v4M4.93 4.93l2.83 2.83M16.24 16.24l2.83 2.83M1 12h4M19 12h4M4.93 19.07l2.83-2.83M16.24 7.76l2.83-2.83"/></svg>
        </div>
        <h3 class="pillar-title">Open source</h3>
        <p class="pillar-desc">
          AGPL-3.0 licensed. Read every line of code. Contribute features.
          No subscription fees, no vendor lock-in.
        </p>
        <div class="pillar-detail">
          <span class="mono">AGPL-3.0</span> licensed
        </div>
      </div>
    </div>
  </div>
</section>

<style>
  .section-header {
    text-align: center;
    margin-bottom: var(--space-16);
  }

  .section-eyebrow {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
    color: var(--accent);
    margin-bottom: var(--space-3);
  }

  .section-title {
    font-family: var(--font-brand);
    font-size: clamp(var(--text-3xl), 4vw, var(--text-5xl));
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tighter);
    color: var(--text-primary);
  }

  .pillars {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-8);
  }

  .pillar {
    position: relative;
    padding: var(--space-8) var(--space-8) var(--space-8) var(--space-8);
    border-radius: var(--radius-xl);
    background: var(--bg-surface);
    border: 1px solid var(--border);
    overflow: hidden;
  }

  .pillar-accent {
    position: absolute;
    top: 0;
    left: 0;
    right: 0;
    height: 3px;
    background: linear-gradient(90deg, var(--accent), transparent);
  }

  .pillar-icon {
    width: 44px;
    height: 44px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--accent);
    margin-bottom: var(--space-5);
  }

  .pillar-icon svg {
    width: 24px;
    height: 24px;
  }

  .pillar-title {
    font-family: var(--font-sans);
    font-size: var(--text-xl);
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: var(--space-3);
    letter-spacing: var(--tracking-tight);
  }

  .pillar-desc {
    font-size: var(--text-base);
    line-height: var(--leading-normal);
    color: var(--text-secondary);
    margin-bottom: var(--space-6);
  }

  .pillar-detail {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
    padding-top: var(--space-4);
    border-top: 1px solid var(--border);
  }

  .mono {
    font-family: var(--font-mono);
    font-weight: 600;
    color: var(--accent);
  }

  @media (max-width: 1024px) {
    .pillars {
      grid-template-columns: 1fr;
      max-width: 520px;
      margin: 0 auto;
    }
  }
</style>
