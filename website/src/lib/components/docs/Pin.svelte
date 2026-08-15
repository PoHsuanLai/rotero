<script lang="ts">
  interface Props {
    /** The number shown in the marker. */
    n: number;
    /** Position as a percentage of the figure, so it survives re-captures. */
    x: number;
    y: number;
    /** Which side the label sits on when it would otherwise run off the edge. */
    side?: 'right' | 'left';
    children: import('svelte').Snippet;
  }

  let { n, x, y, side = 'right', children }: Props = $props();
</script>

<div class="pin" class:left={side === 'left'} style="left: {x}%; top: {y}%;">
  <span class="marker" aria-hidden="true">{n}</span>
  <span class="label">{@render children()}</span>
</div>

<style>
  .pin {
    position: absolute;
    display: flex;
    align-items: center;
    gap: var(--space-2);
    transform: translate(-50%, -50%);
    /* The pins layer is inert; re-enable hit testing so labels stay selectable. */
    pointer-events: auto;
  }

  .pin.left {
    flex-direction: row-reverse;
  }

  .marker {
    display: flex;
    align-items: center;
    justify-content: center;
    flex-shrink: 0;
    width: 22px;
    height: 22px;
    border-radius: var(--radius-full);
    background: var(--accent);
    color: #fff;
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    font-weight: 600;
    font-variant-numeric: tabular-nums;
    box-shadow: 0 0 0 3px var(--bg-primary);
  }

  .label {
    padding: 2px var(--space-2);
    border-radius: var(--radius-sm);
    background: var(--bg-primary);
    border: 1px solid var(--border);
    color: var(--text-primary);
    font-family: var(--font-sans);
    font-size: var(--text-xs);
    line-height: var(--leading-snug);
    white-space: nowrap;
    box-shadow: var(--shadow-sm);
  }

  /* On narrow screens the labels collide with each other and with the image;
     the numbered markers still key to the list below the figure. */
  @media (max-width: 720px) {
    .label {
      display: none;
    }
  }
</style>
