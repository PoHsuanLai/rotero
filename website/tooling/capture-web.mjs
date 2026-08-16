/**
 * Captures the browser-extension popup and the Word task pane.
 *
 *     node website/tooling/capture-web.mjs [popup|taskpane ...]
 *
 * Both are plain HTML that normally gets its content from the running
 * connector — the popup additionally uses `chrome.*`, and the task pane loads
 * `office.js` from Microsoft's CDN. None of that exists in a headless browser,
 * so this stubs the three connector endpoints, the two `chrome` namespaces, and
 * `Office`, then screenshots the result.
 *
 * The markup, the stylesheet and the fonts are the real ones; only the data is
 * injected, and it is the same fixture data the desktop shots are seeded with,
 * so a reader sees one consistent library across the whole guide.
 *
 * Unlike capture.sh this needs no GUI and no macOS, so it runs anywhere.
 */

// `playwright-core`, not `playwright`: the full package downloads a browser on
// every `npm ci`, which both CI jobs run and neither needs — this script is not
// part of the build. `installedChromium()` below supplies the browser instead.
import { chromium } from 'playwright-core';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import fs from 'node:fs';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../..');
const OUT = path.join(ROOT, 'website/static/docs/shots');

/**
 * Mirrors `crates/rotero-db/examples/seed_fixture.rs` — same collections, same
 * tag names and colours. Kept in sync by hand; it is a handful of strings, and
 * importing it would mean running the seeder just to draw a screenshot.
 */
const COLLECTIONS = [
	{ id: 'c1', name: 'Compilers', parent_id: null },
	{ id: 'c2', name: 'Optimization', parent_id: 'c1' },
	{ id: 'c3', name: 'Measurement', parent_id: null },
	{ id: 'c4', name: 'Data Structures', parent_id: null }
];

const TAGS = [
	{ id: 't1', name: 'to-read', color: '#e8a33d' },
	{ id: 't2', name: 'foundational', color: '#d9534f' },
	{ id: 't3', name: 'methods', color: '#5cb85c' },
	{ id: 't4', name: 'benchmarks', color: '#4a90d9' },
	{ id: 't5', name: 'survey', color: '#9b59b6' },
	{ id: 't6', name: 'reproducible', color: '#4dbdb0' }
];

/**
 * The page the popup is pretending to be open on. A real publisher page rather
 * than an invented one, so the metadata the popup shows is the metadata that
 * page really has.
 */
const PAGE = {
	url: 'https://dl.acm.org/doi/10.1145/1543135.1542528',
	title: 'Trace-based Just-in-Time Type Specialization for Dynamic Languages',
	authors: ['Andreas Gal', 'Brendan Eich', 'Mike Shaver'],
	journal: 'PLDI',
	year: '2009',
	doi: '10.1145/1543135.1542528'
};

/**
 * The display names from `AVAILABLE_STYLES` in
 * `crates/rotero-bib/src/citation.rs`, in the order the app lists them. The
 * task pane only reads `id` and `name`, so the ids are slugs of the names.
 */
const CSL_STYLES = [
	'APA 7th',
	'Chicago Author-Date',
	'Chicago Notes',
	'Harvard Cite Them Right',
	'Vancouver',
	'MLA 9th',
	'Nature',
	'ACM',
	'ACS',
	'AMA',
	'AIP',
	'APS'
].map((name) => ({ id: name.toLowerCase().replace(/\s+/g, '-'), name }));

/** Answers the connector calls both surfaces make. */
async function stubConnector(page, { papers = [] } = {}) {
	await page.route('**/api/**', (route) => {
		const url = route.request().url();
		const json = (body) =>
			route.fulfill({ status: 200, contentType: 'application/json', body: JSON.stringify(body) });

		if (url.includes('/api/status')) return json({ status: 'ok', version: '0.2.2' });
		if (url.includes('/api/collections')) return json({ collections: COLLECTIONS });
		if (url.includes('/api/tags')) return json({ tags: TAGS });
		// The task pane namespaces its own routes under /api/cite/.
		if (url.includes('/api/cite/styles')) return json({ styles: CSL_STYLES });
		if (url.includes('/api/cite/search')) return json({ papers });
		return json({});
	});
}

async function capturePopup(browser) {
	// The popup is a fixed 340px-wide panel; the height follows its content.
	const context = await browser.newContext({ deviceScaleFactor: 2 });
	const page = await context.newPage();
	await stubConnector(page);

	// `chrome.tabs`/`chrome.scripting` only exist inside an extension. The popup
	// calls them to read the open page, so hand back the page it is posing on.
	await page.addInitScript((meta) => {
		window.chrome = {
			tabs: { query: async () => [{ id: 1, url: meta.url, title: meta.title }] },
			scripting: { executeScript: async () => [{ result: meta }] },
			runtime: { getURL: (p) => p }
		};
	}, PAGE);

	await page.setViewportSize({ width: 340, height: 640 });
	await page.goto(`file://${path.join(ROOT, 'extension/popup.html')}`);

	// The popup renders after three awaited fetches; wait for the last thing
	// they produce rather than guessing at a delay.
	await page.waitForSelector('.tag-chip', { timeout: 10_000 });
	await page.waitForFunction(() => {
		const btn = document.getElementById('addBtn');
		return btn && !btn.disabled;
	}, { timeout: 10_000 });

	// `loadCollections` selects the Library row itself once it has rendered.
	// Clicking before that lands would be undone by it, so wait for the default
	// to settle before choosing a different row.
	await page.waitForSelector('.coll-item.selected', { timeout: 10_000 });

	// Pre-select a collection and two tags: the empty state says less about how
	// the popup is used than a filled-in one does.
	await page.evaluate(() => {
		const rows = [...document.querySelectorAll('.coll-item')];
		rows.find((r) => r.textContent.includes('Optimization'))?.click();
		const chips = [...document.querySelectorAll('.tag-chip')];
		chips.find((c) => c.textContent.includes('methods'))?.click();
		chips.find((c) => c.textContent.includes('to-read'))?.click();
	});

	// Chips and rows animate over 0.15s; capturing mid-transition renders the
	// selected ones in a washed-out half-state. Wait for the end state itself
	// rather than for a duration.
	await page.waitForFunction(() => {
		const sel = [...document.querySelectorAll('.coll-item.selected')];
		const chips = [...document.querySelectorAll('.tag-chip.selected')];
		return (
			sel.length === 1 &&
			sel[0].textContent.includes('Optimization') &&
			chips.length === 2 &&
			chips.every((c) => getComputedStyle(c).color === 'rgb(255, 255, 255)')
		);
	}, { timeout: 10_000 });

	const file = path.join(OUT, 'extension-popup.png');
	await page.locator('body').screenshot({ path: file });
	await context.close();
	return file;
}

async function captureTaskpane(browser) {
	const context = await browser.newContext({ deviceScaleFactor: 2 });
	const page = await context.newPage();

	// office.js is fetched from Microsoft's CDN and only resolves inside Word.
	// Block it and stand in for the one call the task pane makes on load.
	await page.route('**/office.js', (route) =>
		route.fulfill({ status: 200, contentType: 'application/javascript', body: '' })
	);
	await stubConnector(page, {
		papers: [
			{
				id: 'p1',
				title: 'Trace-based Just-in-Time Type Specialization for Dynamic Languages',
				authors: ['Andreas Gal', 'Brendan Eich', 'Mike Shaver'],
				year: 2009,
				journal: 'PLDI'
			},
			{
				id: 'p2',
				title: 'Guard Elimination for Speculative Compilation',
				authors: ['Marta Feld', 'Junichi Oyama'],
				year: 2017,
				journal: 'TOPLAS'
			}
		]
	});

	await page.addInitScript(() => {
		const noop = () => {};
		window.Office = {
			onReady: (cb) => cb && cb({ host: 'Word', platform: 'PC' }),
			initialize: noop,
			HostType: { Word: 'Word' },
			context: { document: { settings: { get: () => null, set: noop, saveAsync: noop } } }
		};
		window.Word = { run: async () => {} };
	});

	// The task pane is docked in a narrow column beside the document.
	await page.setViewportSize({ width: 360, height: 620 });
	await page.goto(`file://${path.join(ROOT, 'word-addin/taskpane.html')}`);
	await page.waitForLoadState('domcontentloaded');

	// Wait for the style dropdown to fill in; an empty <select> next to a
	// disabled button is the "connector is down" state, not the working one.
	await page.waitForFunction(
		() => document.getElementById('styleSelect')?.options.length > 0,
		{ timeout: 10_000 }
	);

	// Type a query and pick a paper, so the shot shows what citing actually
	// looks like rather than the empty state. The input is debounced, so drive
	// it through a real keypress and wait for the rendered rows.
	await page.fill('#searchInput', 'trace-based');
	await page.waitForSelector('.result-item', { timeout: 10_000 });
	await page.click('.result-item');

	// The Insert button enables only once a paper is checked; waiting on it
	// confirms the selection registered instead of assuming the click landed.
	await page.waitForFunction(
		() => document.getElementById('insertCiteBtn')?.disabled === false,
		{ timeout: 10_000 }
	);

	const file = path.join(OUT, 'word-taskpane.png');
	await page.locator('body').screenshot({ path: file });
	await context.close();
	return file;
}

const SHOTS = { popup: capturePopup, taskpane: captureTaskpane };

const requested = process.argv.slice(2);
const names = requested.length ? requested : Object.keys(SHOTS);

const unknown = names.filter((n) => !SHOTS[n]);
if (unknown.length) {
	console.error(`unknown shot(s): ${unknown.join(', ')}`);
	console.error(`available: ${Object.keys(SHOTS).join(', ')}`);
	process.exit(1);
}

fs.mkdirSync(OUT, { recursive: true });

/**
 * `playwright-core` ships no browser, so one has to be found on the machine:
 * `PLAYWRIGHT_CHROMIUM` if set, otherwise the newest build in Playwright's
 * cache. Any recent Chromium renders these two pages identically, so an
 * existing install is preferable to pinning a download.
 */
function installedChromium() {
	if (process.env.PLAYWRIGHT_CHROMIUM) return process.env.PLAYWRIGHT_CHROMIUM;

	const cache = path.join(
		process.env.HOME ?? '',
		process.platform === 'darwin' ? 'Library/Caches/ms-playwright' : '.cache/ms-playwright'
	);
	if (!fs.existsSync(cache)) return undefined;

	const builds = fs
		.readdirSync(cache)
		.filter((d) => d.startsWith('chromium-'))
		.sort((a, b) => Number(b.split('-')[1]) - Number(a.split('-')[1]));

	for (const build of builds) {
		for (const rel of [
			'chrome-mac-arm64/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
			'chrome-mac/Google Chrome for Testing.app/Contents/MacOS/Google Chrome for Testing',
			'chrome-linux/chrome'
		]) {
			const exe = path.join(cache, build, rel);
			if (fs.existsSync(exe)) return exe;
		}
	}
	return undefined;
}

const executablePath = installedChromium();
if (!executablePath) {
	console.error(
		'No Chromium found. Install one with `npx playwright install chromium`,\n' +
			'or point PLAYWRIGHT_CHROMIUM at an existing Chrome or Chromium binary.'
	);
	process.exit(1);
}

const browser = await chromium.launch({ executablePath });
try {
	for (const name of names) {
		const file = await SHOTS[name](browser);
		console.log(`==> ${name}\n    ${file}`);
	}
} finally {
	await browser.close();
}
