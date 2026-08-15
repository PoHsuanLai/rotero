import adapter from '@sveltejs/adapter-static';
import { mdsvex } from 'mdsvex';
import { fileURLToPath } from 'node:url';

// mdsvex resolves layout paths relative to the importing file, so this has to
// be absolute to work from any depth under src/routes/docs.
const docLayout = fileURLToPath(
	new URL('./src/lib/components/docs/DocLayout.svelte', import.meta.url)
);

/** @type {import('@sveltejs/kit').Config} */
const config = {
	extensions: ['.svelte', '.md'],
	preprocess: [
		mdsvex({
			extensions: ['.md'],
			layout: { docs: docLayout }
		})
	],
	kit: {
		adapter: adapter({
			pages: 'build',
			assets: 'build',
			fallback: undefined,
			precompress: false,
			strict: true
		}),
		paths: {
			base: process.env.BASE_PATH || ''
		}
	}
};

export default config;
