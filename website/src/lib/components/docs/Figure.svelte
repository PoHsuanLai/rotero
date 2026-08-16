<script lang="ts">
  import { base } from '$app/paths';

  interface Props {
    /** Path under static/docs/shots, e.g. "library-overview.png". */
    src: string;
    /** Describes the image for screen readers. Required — these carry meaning. */
    alt: string;
    caption?: string;
    /** Renders the frame without the window chrome, for cropped detail shots. */
    plain?: boolean;
    /**
     * Intrinsic width in CSS pixels, for shots that are narrower than the text
     * column — the extension popup and the Word task pane are panels a few
     * hundred pixels wide, and stretching them to the full measure only makes
     * them blurry. Omit for full-window screenshots.
     *
     * A width also moves the pin labels into the margin the narrower frame
     * leaves free: a dense panel has no empty space to write on, so a label
     * over the image would cover whatever it names.
     */
    width?: number;
    children?: import('svelte').Snippet;
  }

  let { src, alt, caption, plain = false, width, children }: Props = $props();
</script>

<figure class="figure">
  <div
    class="frame"
    class:plain
    class:gutter={width !== undefined}
    style={width ? `max-width: ${width}px` : undefined}
  >
    <img src="{base}/docs/shots/{src}" {alt} loading="lazy" />
    {#if children}
      <!-- Pins position themselves over this container. -->
      <div class="pins">{@render children()}</div>
    {/if}
  </div>
  {#if caption}
    <figcaption>{caption}</figcaption>
  {/if}
</figure>

<style>
  .figure {
    margin: var(--space-8) 0;
  }

  .frame {
    position: relative;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
    background: var(--bg-surface);
    box-shadow: var(--shadow-card);
  }

  .frame.plain {
    box-shadow: none;
  }

  /*
    Gutter labels sit outside the frame, so it cannot clip its overflow. The
    image keeps the rounded corners the frame would otherwise have imposed.
  */
  .frame.gutter {
    overflow: visible;
  }

  .frame.gutter img {
    border-radius: var(--radius-lg);
  }

  .frame img {
    display: block;
    width: 100%;
    height: auto;
  }

  .pins {
    position: absolute;
    inset: 0;
    pointer-events: none;
  }

  figcaption {
    margin-top: var(--space-3);
    font-family: var(--font-sans);
    font-size: var(--text-sm);
    line-height: var(--leading-normal);
    color: var(--text-secondary);
  }
</style>
