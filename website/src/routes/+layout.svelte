<script lang="ts">
  import '../app.css';
  import { theme } from '$lib/theme';
  import { onMount } from 'svelte';

  let { children } = $props();

  // Sync theme on mount
  onMount(() => {
    const unsub = theme.subscribe((t) => {
      document.documentElement.setAttribute('data-theme', t);
    });
    return unsub;
  });
</script>

<div class="grain">
  {@render children()}
</div>
