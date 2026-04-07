<script lang="ts">
  import Nav from '$lib/components/Nav.svelte';
  import Hero from '$lib/components/Hero.svelte';
  import Features from '$lib/components/Features.svelte';
  import WhyRotero from '$lib/components/WhyRotero.svelte';
  import Download from '$lib/components/Download.svelte';
  import Footer from '$lib/components/Footer.svelte';
</script>

<svelte:head>
  <meta property="og:title" content="Rotero — Research, refined." />
  <meta property="og:description" content="A fast, private, local-first paper reading app built with Rust." />
  <meta property="og:type" content="website" />
</svelte:head>

<Nav />
<main>
  <Hero />
  <Features />
  <WhyRotero />
  <Download />
</main>
<Footer />
