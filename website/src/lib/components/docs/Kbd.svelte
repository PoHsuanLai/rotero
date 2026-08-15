<script lang="ts">
  import { onMount } from 'svelte';

  interface Props {
    /**
     * The shortcut in macOS notation, e.g. "⌘⇧F". On other platforms ⌘ is
     * rewritten to Ctrl, matching the app: the handler accepts either modifier.
     */
    keys: string;
  }

  let { keys }: Props = $props();
  let display = $state(keys);

  onMount(() => {
    const isApple = /Mac|iPhone|iPad/.test(navigator.platform ?? navigator.userAgent);
    if (!isApple) {
      display = keys.replace(/⌘/g, 'Ctrl+').replace(/⇧/g, 'Shift+').replace(/\+\+/g, '+');
    }
  });
</script>

<kbd>{display}</kbd>

<style>
  kbd {
    display: inline-block;
    padding: 1px 6px;
    border: 1px solid var(--border);
    border-bottom-width: 2px;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    color: var(--text-primary);
    font-family: var(--font-mono);
    font-size: 0.85em;
    line-height: 1.5;
    white-space: nowrap;
  }
</style>
