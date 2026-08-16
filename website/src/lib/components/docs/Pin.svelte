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

<!--
  The marker and the label are positioned separately rather than laid out as
  one row, so that a gutter figure can keep the marker on its target while the
  label sits in the margin at the same height. In the default (wide) figure both
  land at the same point and read as a single unit.
-->
<span class="marker" aria-hidden="true" style="left: {x}%; top: {y}%;">{n}</span>
<span class="label" class:left={side === 'left'} style="left: {x}%; top: {y}%;">
  {@render children()}
</span>

<style>
  .marker,
  .label {
    position: absolute;
    /* The pins layer is inert; re-enable hit testing so labels stay selectable. */
    pointer-events: auto;
  }

  .marker {
    display: flex;
    align-items: center;
    justify-content: center;
    /* Centred on the point it names. */
    transform: translate(-50%, -50%);
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
    /*
      Sits to the right of the marker, vertically centred on it: the marker is
      22px wide and centred on the point, so clearing it means starting half a
      marker past the point rather than offsetting the label's own width.
    */
    transform: translateY(-50%);
    margin-left: calc(11px + var(--space-2));
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

  /* Mirrored: the label ends where the marker begins. */
  .label.left {
    transform: translate(-100%, -50%);
    margin-left: calc(-11px - var(--space-2));
  }

  /*
    Narrow figures (the extension popup, the Word task pane) are dense enough
    that any label drawn over them covers what it names. Here the marker stays
    on its target and the label moves out past the frame's right edge, keeping
    only its vertical position — so the two still read as a pair.
  */
  :global(.frame.gutter) .label,
  :global(.frame.gutter) .label.left {
    /*
      `!important` because the pin's x is written as an inline style — the
      component sets it without knowing whether it sits in a gutter figure, and
      an inline declaration outranks any selector here.
    */
    left: 100% !important;
    transform: translateY(-50%);
    margin-left: var(--space-4);
    margin-right: 0;
  }

  /* On narrow screens the labels collide with each other and with the image;
     the numbered markers still key to the list below the figure. */
  @media (max-width: 720px) {
    .label {
      display: none;
    }
  }
</style>
