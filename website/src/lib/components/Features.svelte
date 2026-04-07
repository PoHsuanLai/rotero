<script lang="ts">
  import { onMount } from 'svelte';

  const features = [
    {
      title: 'PDF Annotations',
      desc: 'Highlights, sticky notes, underlines, ink, and area annotations — saved directly in your PDFs.',
      icon: 'highlight'
    },
    {
      title: 'Smart Import',
      desc: '742 community web translators scrape metadata from Google Scholar, arXiv, PubMed, and 40+ academic sites.',
      icon: 'import'
    },
    {
      title: 'Full-Text Search',
      desc: 'Instant full-text search across your entire library, powered by SQLite FTS.',
      icon: 'search'
    },
    {
      title: 'Citation Generation',
      desc: '14 CSL styles including APA, IEEE, Chicago, Harvard, and MLA. Export to BibTeX with auto-sync.',
      icon: 'cite'
    },
    {
      title: 'Browser Extension',
      desc: 'Chrome extension for one-click saving from arXiv, DOI.org, PubMed, Semantic Scholar, and more.',
      icon: 'extension'
    },
    {
      title: 'Cloud Sync',
      desc: 'File-based sync via Dropbox, iCloud, or Google Drive. Your data, your storage.',
      icon: 'sync'
    }
  ];

  let items: HTMLElement[] = [];

  onMount(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        entries.forEach((e) => {
          if (e.isIntersecting) e.target.classList.add('visible');
        });
      },
      { threshold: 0.1 }
    );
    items.forEach((el) => el && observer.observe(el));
    return () => observer.disconnect();
  });
</script>

<section id="features">
  <div class="container">
    <div class="section-header reveal" bind:this={items[0]}>
      <p class="section-eyebrow">Capabilities</p>
      <h2 class="section-title">Everything you need,<br/>nothing you don't.</h2>
    </div>

    <div class="features-grid">
      {#each features as feature, i}
        <div
          class="feature-card reveal reveal-delay-{i + 1}"
          bind:this={items[i + 1]}
        >
          <div class="feature-icon">
            {#if feature.icon === 'highlight'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M12 20h9"/><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z"/></svg>
            {:else if feature.icon === 'import'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/><polyline points="7 10 12 15 17 10"/><line x1="12" y1="15" x2="12" y2="3"/></svg>
            {:else if feature.icon === 'search'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/></svg>
            {:else if feature.icon === 'cite'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M6 17C3.5 17 2 15 2 12s1.5-5 4-5c1.5 0 2.5 1 3 2"/><path d="M15 17c-2.5 0-4-2-4-5s1.5-5 4-5c1.5 0 2.5 1 3 2"/><path d="M10 19h4"/></svg>
            {:else if feature.icon === 'extension'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><path d="M14.7 6.3a1 1 0 0 0 0 1.4l1.6 1.6a1 1 0 0 0 1.4 0l3.77-3.77a6 6 0 0 1-7.94 7.94l-6.91 6.91a2.12 2.12 0 0 1-3-3l6.91-6.91a6 6 0 0 1 7.94-7.94l-3.76 3.76z"/></svg>
            {:else if feature.icon === 'sync'}
              <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><polyline points="23 4 23 10 17 10"/><polyline points="1 20 1 14 7 14"/><path d="M3.51 9a9 9 0 0 1 14.85-3.36L23 10M1 14l4.64 4.36A9 9 0 0 0 20.49 15"/></svg>
            {/if}
          </div>
          <h3 class="feature-title">{feature.title}</h3>
          <p class="feature-desc">{feature.desc}</p>
        </div>
      {/each}
    </div>

    <!-- Citation Graph showcase -->
    <div class="showcase reveal" bind:this={items[7]}>
      <div class="showcase-text">
        <div class="showcase-eyebrow">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="6" cy="6" r="3"/><circle cx="18" cy="6" r="3"/><circle cx="12" cy="18" r="3"/><line x1="8.5" y1="7.5" x2="10" y2="16"/><line x1="15.5" y1="7.5" x2="14" y2="16"/><line x1="9" y1="6" x2="15" y2="6"/></svg>
          Citation Graph
        </div>
        <h3 class="showcase-title">See how your<br/>papers connect.</h3>
        <p class="showcase-desc">
          An interactive graph visualization maps relationships across your library.
          Group by tags, collections, authors, or journals — discover hidden connections
          between papers you'd never notice scrolling a list.
        </p>
      </div>
      <div class="showcase-visual">
        <div class="graph-mock">
          <svg viewBox="0 0 400 280" class="graph-svg">
            <!-- Edges -->
            <line x1="200" y1="80" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="200" y1="80" x2="280" y2="160" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="200" y1="80" x2="320" y2="100" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="120" y1="180" x2="200" y2="230" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="280" y1="160" x2="200" y2="230" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="80" y1="100" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="80" y1="100" x2="200" y2="80" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="320" y1="100" x2="360" y2="200" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="280" y1="160" x2="360" y2="200" stroke="var(--accent)" stroke-width="1.5" opacity="0.3"/>
            <line x1="40" y1="180" x2="80" y2="100" stroke="var(--accent)" stroke-width="1.5" opacity="0.2"/>
            <line x1="40" y1="180" x2="120" y2="180" stroke="var(--accent)" stroke-width="1.5" opacity="0.2"/>

            <!-- Nodes -->
            <circle cx="200" cy="80" r="14" fill="var(--accent)" opacity="0.9" class="node node-1"/>
            <circle cx="120" cy="180" r="11" fill="var(--accent)" opacity="0.7" class="node node-2"/>
            <circle cx="280" cy="160" r="12" fill="var(--accent)" opacity="0.75" class="node node-3"/>
            <circle cx="200" cy="230" r="10" fill="var(--accent)" opacity="0.6" class="node node-4"/>
            <circle cx="320" cy="100" r="9" fill="var(--accent)" opacity="0.55" class="node node-5"/>
            <circle cx="80" cy="100" r="10" fill="var(--accent)" opacity="0.6" class="node node-6"/>
            <circle cx="360" cy="200" r="8" fill="var(--accent)" opacity="0.45" class="node node-7"/>
            <circle cx="40" cy="180" r="7" fill="var(--accent)" opacity="0.35" class="node node-8"/>

            <!-- Labels on larger nodes -->
            <text x="200" y="56" text-anchor="middle" fill="var(--text-secondary)" font-size="9" font-family="var(--font-sans)">Vaswani et al.</text>
            <text x="120" y="204" text-anchor="middle" fill="var(--text-tertiary)" font-size="8" font-family="var(--font-sans)">Devlin 2019</text>
            <text x="280" y="184" text-anchor="middle" fill="var(--text-tertiary)" font-size="8" font-family="var(--font-sans)">Brown 2020</text>
          </svg>
        </div>
      </div>
    </div>

    <!-- AI Chat showcase -->
    <div class="showcase showcase-reverse reveal" bind:this={items[8]}>
      <div class="showcase-text">
        <div class="showcase-eyebrow">
          <svg viewBox="0 0 24 24" width="16" height="16" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z"/></svg>
          AI Research Assistant
        </div>
        <h3 class="showcase-title">Chat with your<br/>papers.</h3>
        <p class="showcase-desc">
          Ask questions about any paper in your library. The built-in MCP server gives Claude
          full context — your annotations, highlights, notes, and the paper's full text.
          Research conversations grounded in your actual reading.
        </p>
      </div>
      <div class="showcase-visual">
        <div class="chat-mock">
          <div class="chat-header">
            <div class="chat-header-dot"></div>
            <span class="chat-header-title">Chat</span>
          </div>
          <div class="chat-messages">
            <div class="chat-msg user">
              <div class="chat-bubble user-bubble">What's the key contribution of this paper?</div>
            </div>
            <div class="chat-msg assistant">
              <div class="chat-bubble assistant-bubble">
                <div class="chat-line w-full"></div>
                <div class="chat-line w-90"></div>
                <div class="chat-line w-full"></div>
                <div class="chat-line w-70"></div>
              </div>
            </div>
            <div class="chat-msg user">
              <div class="chat-bubble user-bubble">How does it compare to my highlighted section on p.12?</div>
            </div>
            <div class="chat-msg assistant">
              <div class="chat-bubble assistant-bubble">
                <div class="chat-line w-full"></div>
                <div class="chat-line w-80"></div>
                <div class="chat-line w-full"></div>
                <div class="chat-line w-60"></div>
                <div class="chat-line w-90"></div>
              </div>
            </div>
          </div>
          <div class="chat-input">
            <div class="chat-input-placeholder">Ask about this paper...</div>
          </div>
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

  .features-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: var(--space-6);
  }

  .feature-card {
    padding: var(--space-8);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--bg-surface);
    transition: transform var(--transition-normal), box-shadow var(--transition-normal), border-color var(--transition-normal);
  }

  .feature-card:hover {
    transform: translateY(-4px);
    box-shadow: var(--shadow-card);
    border-color: var(--accent-ring);
  }

  .feature-icon {
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: var(--radius-lg);
    background: var(--accent-subtle);
    color: var(--accent);
    margin-bottom: var(--space-5);
  }

  .feature-icon svg {
    width: 20px;
    height: 20px;
  }

  .feature-title {
    font-family: var(--font-sans);
    font-size: var(--text-lg);
    font-weight: 600;
    color: var(--text-primary);
    margin-bottom: var(--space-2);
    letter-spacing: var(--tracking-tight);
  }

  .feature-desc {
    font-size: var(--text-base);
    line-height: var(--leading-normal);
    color: var(--text-secondary);
  }

  /* ====================================
     Showcase sections (Graph + Chat)
     ==================================== */

  .showcase {
    display: grid;
    grid-template-columns: 1fr 1.2fr;
    gap: var(--space-16);
    align-items: center;
    margin-top: var(--space-24);
  }

  .showcase-reverse {
    grid-template-columns: 1.2fr 1fr;
  }

  .showcase-reverse .showcase-text {
    order: 2;
  }

  .showcase-reverse .showcase-visual {
    order: 1;
  }

  .showcase-eyebrow {
    display: inline-flex;
    align-items: center;
    gap: var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: var(--tracking-wider);
    color: var(--accent);
    margin-bottom: var(--space-4);
  }

  .showcase-title {
    font-family: var(--font-brand);
    font-size: clamp(var(--text-2xl), 3vw, var(--text-4xl));
    line-height: var(--leading-tight);
    letter-spacing: var(--tracking-tighter);
    color: var(--text-primary);
    margin-bottom: var(--space-5);
  }

  .showcase-desc {
    font-size: var(--text-base);
    line-height: var(--leading-normal);
    color: var(--text-secondary);
    max-width: 440px;
  }

  /* Graph mock */
  .graph-mock {
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--bg-surface);
    box-shadow: var(--shadow-card);
    padding: var(--space-6);
    overflow: hidden;
  }

  .graph-svg {
    width: 100%;
    height: auto;
  }

  .node {
    transition: opacity var(--transition-normal);
  }

  .graph-mock:hover .node-1 { opacity: 1; }
  .graph-mock:hover .node-2 { opacity: 0.85; }
  .graph-mock:hover .node-3 { opacity: 0.9; }

  @keyframes pulse-node {
    0%, 100% { r: 14; }
    50% { r: 16; }
  }

  .node-1 {
    animation: pulse-node 4s ease-in-out infinite;
  }

  /* Chat mock */
  .chat-mock {
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    background: var(--bg-surface);
    box-shadow: var(--shadow-card);
    overflow: hidden;
    max-width: 380px;
  }

  .showcase-reverse .chat-mock {
    margin-left: auto;
  }

  .chat-header {
    display: flex;
    align-items: center;
    gap: var(--space-2);
    padding: var(--space-3) var(--space-4);
    border-bottom: 1px solid var(--border-subtle);
  }

  .chat-header-dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--accent);
  }

  .chat-header-title {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    font-weight: 600;
    color: var(--text-primary);
  }

  .chat-messages {
    padding: var(--space-4);
    display: flex;
    flex-direction: column;
    gap: var(--space-4);
    min-height: 260px;
  }

  .chat-msg {
    display: flex;
  }

  .chat-msg.user {
    justify-content: flex-end;
  }

  .chat-msg.assistant {
    justify-content: flex-start;
  }

  .chat-bubble {
    max-width: 85%;
    padding: var(--space-3) var(--space-4);
    border-radius: var(--radius-lg);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
  }

  .user-bubble {
    background: var(--accent);
    color: #fff;
    border-bottom-right-radius: var(--radius-sm);
  }

  .assistant-bubble {
    background: var(--bg-elevated);
    color: var(--text-primary);
    border-bottom-left-radius: var(--radius-sm);
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: var(--space-4);
  }

  .chat-line {
    height: 4px;
    border-radius: 2px;
    background: var(--text-muted);
    opacity: 0.3;
  }

  .chat-line.w-full { width: 100%; }
  .chat-line.w-90 { width: 90%; }
  .chat-line.w-80 { width: 80%; }
  .chat-line.w-70 { width: 70%; }
  .chat-line.w-60 { width: 60%; }

  .chat-input {
    padding: var(--space-3) var(--space-4);
    border-top: 1px solid var(--border-subtle);
  }

  .chat-input-placeholder {
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    color: var(--text-tertiary);
    padding: var(--space-2) var(--space-3);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-primary);
  }

  /* Responsive */
  @media (max-width: 1024px) {
    .features-grid {
      grid-template-columns: repeat(2, 1fr);
    }

    .showcase,
    .showcase-reverse {
      grid-template-columns: 1fr;
      gap: var(--space-10);
      text-align: center;
    }

    .showcase-reverse .showcase-text {
      order: 1;
    }

    .showcase-reverse .showcase-visual {
      order: 2;
    }

    .showcase-desc {
      margin: 0 auto;
    }

    .chat-mock {
      margin: 0 auto;
    }

    .showcase-reverse .chat-mock {
      margin: 0 auto;
    }
  }

  @media (max-width: 640px) {
    .features-grid {
      grid-template-columns: 1fr;
    }

    .feature-card {
      padding: var(--space-6);
    }

    .showcase,
    .showcase-reverse {
      margin-top: var(--space-16);
    }
  }
</style>
