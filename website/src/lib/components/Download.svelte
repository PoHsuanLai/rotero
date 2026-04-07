<script lang="ts">
  import { onMount } from 'svelte';

  let el: HTMLElement;

  onMount(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((e) => {
          if (e.isIntersecting) e.target.classList.add('visible');
        });
      },
      { threshold: 0.15 }
    );
    if (el) observer.observe(el);
    return () => observer.disconnect();
  });
</script>

<section id="download">
  <div class="container">
    <div class="download-card reveal" bind:this={el}>
      <div class="download-glow"></div>
      <p class="section-eyebrow">Get started</p>
      <h2 class="download-title">Ready to refine<br/>your research?</h2>
      <p class="download-sub">Free and open source. No account required.</p>

      <div class="download-buttons">
        <a href="https://github.com/PoHsuanLai/rotero/releases/latest" class="dl-btn macos">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83M13 3.5c.73-.83 1.94-1.46 2.94-1.5.13 1.17-.34 2.35-1.04 3.19-.69.85-1.83 1.51-2.95 1.42-.15-1.15.41-2.35 1.05-3.11z"/></svg>
          <div>
            <span class="dl-label">Download for</span>
            <span class="dl-platform">macOS</span>
          </div>
        </a>

        <a href="https://github.com/PoHsuanLai/rotero/releases/latest" class="dl-btn linux">
          <svg width="20" height="20" viewBox="0 0 24 24" fill="currentColor"><path d="M12.504 0c-.155 0-.315.008-.48.021-4.226.333-3.105 4.807-3.17 6.298-.076 1.092-.3 1.953-1.05 3.02-.885 1.051-2.127 2.75-2.716 4.521-.278.832-.41 1.684-.287 2.489a.424.424 0 00-.11.135c-.26.268-.45.6-.663.839-.199.199-.485.267-.797.4-.313.136-.658.269-.864.68-.09.189-.136.394-.132.602 0 .199.027.4.055.536.058.399.116.728.04.97-.249.68-.28 1.145-.106 1.484.174.334.535.47.94.601.81.2 1.91.135 2.774.6.926.466 1.866.67 2.616.47.526-.116.97-.464 1.208-.946.587-.003 1.23-.269 2.26-.334.699-.058 1.574.267 2.577.2.025.134.063.198.114.333l.003.003c.391.778 1.113 1.368 1.884 1.43.199.023.395-.048.543-.164.734-.484.996-1.23 1.246-1.836.028-.067.058-.131.086-.2.016-.027.028-.058.042-.089.074.015.147.028.221.042h.006c.549.106 1.058-.058 1.412-.378.354-.32.59-.805.763-1.334l.005-.01c.348-1.084.276-2.083-.34-2.678-.09-.085-.178-.162-.266-.228-.053-.098-.108-.197-.164-.297-.34-.614-.655-1.397-.773-2.1a4.96 4.96 0 01-.033-.365c.035-.396.098-.728.163-1.085.076-.353.153-.748.166-1.2.012-.453-.048-.993-.364-1.484-.32-.49-.874-.843-1.594-.977a4.88 4.88 0 00-.382-.052c-.068-.275-.28-.673-.654-.924-.455-.306-1.14-.414-2.1-.277-.04-.133-.076-.27-.113-.396l-.001-.001c-.198-.704-.461-1.434-.73-1.89-.267-.45-.556-.76-.946-.86-.39-.1-.76 0-1.1.2a3.427 3.427 0 00-.397.297z"/></svg>
          <div>
            <span class="dl-label">Download for</span>
            <span class="dl-platform">Linux</span>
          </div>
        </a>
      </div>

      <p class="download-note">
        Or build from source:
        <code>git clone && cargo build --release</code>
      </p>
    </div>
  </div>
</section>

<style>
  section {
    padding: var(--space-16) 0 var(--space-24);
  }

  .download-card {
    position: relative;
    text-align: center;
    padding: var(--space-20) var(--space-8);
    border-radius: var(--radius-xl);
    border: 1px solid var(--border);
    background: var(--bg-surface);
    overflow: hidden;
  }

  .download-glow {
    position: absolute;
    top: -200px;
    left: 50%;
    transform: translateX(-50%);
    width: 600px;
    height: 400px;
    background: var(--accent);
    opacity: 0.05;
    border-radius: 50%;
    filter: blur(100px);
    pointer-events: none;
  }

  .section-eyebrow {
    position: relative;
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
    color: var(--accent);
    margin-bottom: var(--space-3);
  }

  .download-title {
    position: relative;
    font-family: var(--font-brand);
    font-size: clamp(var(--text-3xl), 4vw, var(--text-5xl));
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tighter);
    color: var(--text-primary);
    margin-bottom: var(--space-4);
  }

  .download-sub {
    position: relative;
    font-size: var(--text-lg);
    color: var(--text-secondary);
    margin-bottom: var(--space-10);
  }

  .download-buttons {
    position: relative;
    display: flex;
    gap: var(--space-4);
    justify-content: center;
    flex-wrap: wrap;
    margin-bottom: var(--space-8);
  }

  .dl-btn {
    display: flex;
    align-items: center;
    gap: var(--space-3);
    padding: var(--space-4) var(--space-8);
    border-radius: var(--radius-lg);
    background: var(--accent);
    color: #fff;
    transition: background var(--transition-fast), transform var(--transition-fast), box-shadow var(--transition-fast);
  }

  .dl-btn:hover {
    background: var(--accent-hover);
    transform: translateY(-2px);
    box-shadow: 0 8px 24px var(--accent-ring);
  }

  .dl-btn:active {
    background: var(--accent-pressed);
    transform: translateY(0);
  }

  .dl-label {
    display: block;
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    opacity: 0.8;
    text-align: left;
  }

  .dl-platform {
    display: block;
    font-family: var(--font-sans);
    font-size: var(--text-lg);
    font-weight: 700;
    text-align: left;
  }

  .download-note {
    position: relative;
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
  }

  code {
    font-family: var(--font-mono);
    font-size: var(--text-xs);
    padding: 2px 8px;
    background: var(--bg-muted);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    color: var(--text-secondary);
  }

  @media (max-width: 640px) {
    .download-card {
      padding: var(--space-12) var(--space-5);
    }

    .download-buttons {
      flex-direction: column;
      align-items: center;
    }

    .dl-btn {
      width: 100%;
      max-width: 280px;
      justify-content: center;
    }
  }
</style>
