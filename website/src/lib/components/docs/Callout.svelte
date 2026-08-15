<script lang="ts">
  interface Props {
    type?: 'note' | 'tip' | 'warning';
    title?: string;
    children: import('svelte').Snippet;
  }

  let { type = 'note', title, children }: Props = $props();

  const defaultTitles = { note: 'Note', tip: 'Tip', warning: 'Careful' };
</script>

<aside class="callout {type}">
  <p class="title">{title ?? defaultTitles[type]}</p>
  <div class="body">{@render children()}</div>
</aside>

<style>
  .callout {
    margin: var(--space-6) 0;
    padding: var(--space-4) var(--space-5);
    border-radius: var(--radius-md);
    border-left: 3px solid var(--tone);
    background: var(--bg-muted);
  }

  .note {
    --tone: var(--text-tertiary);
  }
  .tip {
    --tone: var(--accent);
  }
  .warning {
    --tone: #d97706;
  }

  .title {
    margin-bottom: var(--space-2);
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    font-weight: 600;
    letter-spacing: var(--tracking-wide);
    text-transform: uppercase;
    color: var(--tone);
  }

  .body :global(p) {
    margin: 0;
  }

  .body :global(p + p) {
    margin-top: var(--space-3);
  }
</style>
