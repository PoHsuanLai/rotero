import adapter from '@sveltejs/adapter-static';
import { mdsvex } from 'mdsvex';
import rehypeSlug from 'rehype-slug';
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
			layout: { docs: docLayout },
			// Gives every heading a stable id, which the on-page contents rail
			// links to and in-page anchors depend on.
			rehypePlugins: [rehypeSlug]
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
		},
		prerender: {
			// Screenshots are captured on macOS by website/tooling/capture.sh and
			// committed, so they are legitimately absent in a fresh checkout or on
			// a CI runner. Warn rather than fail — but stay loud, so a typo in a
			// filename is still visible in the build log. Every other broken link
			// remains a hard error.
			handleHttpError: ({ path, message }) => {
				// Matched anywhere in the path, not anchored: under the deployed
				// BASE_PATH these arrive as `/rotero/docs/shots/...`, so anchoring
				// at the start would let a real deploy fail while local builds pass.
				if (path.includes('/docs/shots/')) {
					console.warn(`  missing screenshot: ${path} (run \`just docs-screenshots\`)`);
					return;
				}
				throw new Error(message);
			}
		}
	}
};

export default config;
