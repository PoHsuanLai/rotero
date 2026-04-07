<footer class="footer">
  <div class="container footer-inner">
    <div class="footer-brand">
      <span class="wordmark">Rotero</span>
      <p class="footer-tagline">Research, refined.</p>
    </div>

    <div class="footer-links">
      <a href="https://github.com/PoHsuanLai/rotero" target="_blank" rel="noopener">GitHub</a>
      <a href="https://github.com/PoHsuanLai/rotero/blob/master/LICENSE" target="_blank" rel="noopener">AGPL-3.0</a>
    </div>

    <p class="footer-built">
      Built with Rust & <a href="https://dioxuslabs.com" target="_blank" rel="noopener">Dioxus</a>
    </p>
  </div>
</footer>

<style>
  .footer {
    padding: var(--space-12) 0;
    border-top: 1px solid var(--border);
  }

  .footer-inner {
    display: flex;
    align-items: center;
    justify-content: space-between;
    flex-wrap: wrap;
    gap: var(--space-6);
  }

  .wordmark {
    font-family: var(--font-brand);
    font-size: var(--text-xl);
    color: var(--text-primary);
  }

  .footer-tagline {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
    margin-top: 2px;
  }

  .footer-links {
    display: flex;
    gap: var(--space-6);
  }

  .footer-links a {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-secondary);
    transition: color var(--transition-fast);
  }

  .footer-links a:hover {
    color: var(--accent);
  }

  .footer-built {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
  }

  .footer-built a {
    color: var(--text-secondary);
    transition: color var(--transition-fast);
  }

  .footer-built a:hover {
    color: var(--accent);
  }

  @media (max-width: 640px) {
    .footer-inner {
      flex-direction: column;
      text-align: center;
    }
  }
</style>
