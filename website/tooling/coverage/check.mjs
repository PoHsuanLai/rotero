#!/usr/bin/env node
/**
 * Measures how much of the app the user guide actually documents.
 *
 *     node website/tooling/coverage/check.mjs [--json]
 *
 * Four inventories are extracted from source rather than maintained by hand,
 * so they cannot drift from the code:
 *
 *   commands   BINDINGS       src/ui/keybindings/desktop.rs
 *   settings   SettingsField  src/ui/settings/
 *   endpoints  .route(...)    crates/rotero-connector/src/lib.rs
 *   mcp-tools  #[tool(...)]   crates/rotero-mcp/src/server/tools.rs
 *   styles     AVAILABLE_STYLES  crates/rotero-bib/src/citation.rs
 *
 * Two directions are reported, because both are real failures:
 *
 *   undocumented  in the code, absent from the guide
 *   stale         described in the guide, no longer in the code
 *
 * `stale` is the one that catches a rename or a removal — the failure mode
 * that let the README keep claiming IEEE was a supported citation style.
 */

import { readFile, readdir, writeFile } from 'node:fs/promises';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, '../../..');
const DOCS = path.join(ROOT, 'website/src/routes/docs');
const REPORT = path.join(HERE, 'report.md');
const ALLOWLIST = path.join(HERE, 'allowlist.json');

const read = (rel) => readFile(path.join(ROOT, rel), 'utf8');

/** Every `.md` file under the docs routes, concatenated. */
async function guideText() {
	const chunks = [];
	async function walk(dir) {
		let entries;
		try {
			entries = await readdir(dir, { withFileTypes: true });
		} catch {
			return;
		}
		for (const entry of entries) {
			const full = path.join(dir, entry.name);
			if (entry.isDirectory()) await walk(full);
			else if (entry.name.endsWith('.md')) chunks.push(await readFile(full, 'utf8'));
		}
	}
	await walk(DOCS);
	return chunks.join('\n');
}

/** Splits a Rust identifier so `ToggleFavorite` can match "toggle favorite". */
function humanize(identifier) {
	return identifier
		.replace(/([a-z0-9])([A-Z])/g, '$1 $2')
		.replace(/[_-]+/g, ' ')
		.toLowerCase()
		.trim();
}

/**
 * An item counts as documented if the guide contains its human-readable form.
 * Matching is deliberately loose — the goal is to notice what was never
 * written about, not to police phrasing.
 */
function mentions(haystack, item) {
	// Both forms count: a tool named in a table as `get_library_graph`, and a
	// command referred to in prose as "toggle favorite". Checking only the
	// humanized form misses every literal identifier the guide quotes.
	const needles = [item.id, ...(item.aliases ?? [])].filter(Boolean).flatMap((raw) => {
		const literal = String(raw).toLowerCase();
		return [literal, humanize(String(raw))];
	});
	return needles.some((needle) => needle.length > 2 && haystack.includes(needle));
}

async function inventories() {
	const [keybindings, connector, mcpTools, citation] = await Promise.all([
		read('src/ui/keybindings/desktop.rs'),
		read('crates/rotero-connector/src/lib.rs'),
		read('crates/rotero-mcp/src/server/tools.rs'),
		read('crates/rotero-bib/src/citation.rs')
	]);

	// Only the BINDINGS table, so helper functions naming commands don't count.
	const bindingsTable = keybindings.slice(
		keybindings.indexOf('pub const BINDINGS'),
		keybindings.indexOf('/// The default (built-in) key')
	);
	// Identifiers the guide reasonably spells out rather than abbreviating.
	// Without these the checker asks prose to match internal naming, which
	// makes the docs worse to satisfy the tool.
	const commandAliases = {
		PrevTab: ['previous tab'],
		NextTab: ['next tab'],
		FocusLibrarySearch: ['focus library search', 'search your library'],
		OpenPdf: ['open pdf'],
		ImportBibtex: ['import bibtex'],
		ExportBibtex: ['export bibtex']
	};

	const commands = [
		...new Set([...bindingsTable.matchAll(/command:\s*Command::(\w+)/g)].map((m) => m[1]))
	].map((id) => ({ id, aliases: commandAliases[id] }));

	const settingsDir = path.join(ROOT, 'src/ui/settings');
	const settingsFiles = await readdir(settingsDir);
	const settings = [];
	for (const file of settingsFiles) {
		if (!file.endsWith('.rs')) continue;
		const body = await readFile(path.join(settingsDir, file), 'utf8');
		for (const match of body.matchAll(/SettingsField\s*\{\s*label:\s*"([^"]+)"/g)) {
			settings.push({ id: match[1] });
		}
	}

	const endpoints = [...connector.matchAll(/\.route\(\s*"([^"]+)"/g)].map((m) => ({
		id: m[1],
		// The path itself is what a reader searches for, not a prose rendering.
		aliases: [m[1]]
	}));

	const mcp = [...mcpTools.matchAll(/#\[tool\([\s\S]{0,400}?\)\]\s*(?:pub\s+)?async\s+fn\s+(\w+)/g)].map(
		(m) => ({ id: m[1] })
	);

	const stylesBlock = citation.slice(
		citation.indexOf('AVAILABLE_STYLES'),
		citation.indexOf('/// Converts Papers')
	);
	const styles = [...stylesBlock.matchAll(/\(\s*"([^"]+)"/g)].map((m) => ({ id: m[1] }));

	return [
		{ key: 'commands', label: 'Commands and shortcuts', items: commands },
		{ key: 'settings', label: 'Settings fields', items: dedupe(settings) },
		{ key: 'endpoints', label: 'Connector endpoints', items: dedupe(endpoints) },
		{ key: 'mcp-tools', label: 'MCP tools', items: dedupe(mcp) },
		{ key: 'styles', label: 'Citation styles', items: dedupe(styles) }
	];
}

function dedupe(items) {
	const seen = new Set();
	return items.filter((item) => !seen.has(item.id) && seen.add(item.id));
}

async function loadAllowlist() {
	try {
		return JSON.parse(await readFile(ALLOWLIST, 'utf8'));
	} catch {
		return {};
	}
}

/**
 * Finds things the guide claims exist but the code no longer has. Only checked
 * for inventories with a closed, quotable vocabulary — prose mentions of a
 * command name are too loose to invert reliably.
 */
function findStale(guide, styles) {
	const known = new Set(styles.map((s) => s.id.toLowerCase()));

	// Only styles a reader could mistake for supported ones — a bare regex over
	// prose matches fragments like "Chicago entries" and reports them as stale.
	// This list is the well-known styles Rotero does NOT ship; anything here
	// that shows up in the guide is either an error or needs explicit wording
	// saying it is unavailable.
	const notShipped = [
		'IEEE',
		'Harvard Reference format 1',
		'Turabian',
		'AAA',
		'AMS',
		'ASA',
		'Bluebook',
		'OSCOLA'
	];

	return notShipped.filter((name) => {
		if (known.has(name.toLowerCase())) return false;
		const escaped = name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
		const pattern = new RegExp(`\\b${escaped}\\b`, 'gi');

		// A page may name an unsupported style precisely to say it is missing,
		// so each mention is judged against its own sentence — a wider window
		// picks up unrelated prose and excuses a genuine false claim. Any
		// mention not disclaimed in its own sentence is one the code cannot
		// back.
		const disclaimed = new RegExp(
			`(?:\\b(?:not|no longer|unavailable|does not|doesn't|isn't|without)\\b[^.]{0,80}\\b${escaped}\\b` +
				`|\\b${escaped}\\b[^.]{0,80}\\b(?:is not|are not|isn't|is unavailable|not among|not supported|not included)\\b)`,
			'i'
		);

		for (const match of guide.matchAll(pattern)) {
			// The sentence around this mention, bounded by full stops.
			const before = guide.lastIndexOf('.', match.index);
			const after = guide.indexOf('.', match.index + name.length);
			const sentence = guide.slice(
				before === -1 ? 0 : before + 1,
				after === -1 ? guide.length : after + 1
			);
			if (!disclaimed.test(sentence)) return true;
		}
		return false;
	});
}

async function main() {
	const asJson = process.argv.includes('--json');
	const guideRaw = await guideText();
	const guide = guideRaw.toLowerCase();
	const allowlist = await loadAllowlist();
	const groups = await inventories();

	let totalItems = 0;
	let totalCovered = 0;
	const rows = [];

	for (const group of groups) {
		// Each skip entry carries a reason, so a deliberate omission is
		// distinguishable from one nobody has got to yet. Keys starting with
		// "_" are notes to the reader, not ids.
		const skipped = new Set(
			Object.keys(allowlist[group.key] ?? {}).filter((key) => !key.startsWith('_'))
		);
		const counted = group.items.filter((item) => !skipped.has(item.id));
		const missing = counted.filter((item) => !mentions(guide, item));
		const covered = counted.length - missing.length;

		totalItems += counted.length;
		totalCovered += covered;

		rows.push({
			key: group.key,
			label: group.label,
			total: counted.length,
			covered,
			skipped: skipped.size,
			missing: missing.map((m) => m.id)
		});
	}

	const styles = groups.find((g) => g.key === 'styles').items;
	const stale = findStale(guideRaw, styles);

	const pct = totalItems === 0 ? 0 : Math.round((totalCovered / totalItems) * 100);

	if (asJson) {
		console.log(JSON.stringify({ percent: pct, rows, stale }, null, 2));
		return;
	}

	const lines = [];
	lines.push('# User guide coverage', '');
	lines.push(`**${pct}%** of the app's documented surface is covered (${totalCovered}/${totalItems}).`, '');
	lines.push('| Area | Covered | Total | Skipped |', '| --- | ---: | ---: | ---: |');
	for (const row of rows) {
		lines.push(`| ${row.label} | ${row.covered} | ${row.total} | ${row.skipped} |`);
	}

	for (const row of rows) {
		if (row.missing.length === 0) continue;
		lines.push('', `## Undocumented — ${row.label}`, '');
		for (const id of row.missing) lines.push(`- \`${id}\``);
	}

	if (stale.length) {
		lines.push('', '## Stale', '');
		lines.push('Described in the guide but not present in the code:', '');
		for (const name of stale) lines.push(`- \`${name}\``);
	}

	const report = lines.join('\n') + '\n';
	await writeFile(REPORT, report);
	console.log(report);
	console.log(`Written to ${path.relative(ROOT, REPORT)}`);

	if (stale.length) {
		console.error(`\n${stale.length} stale reference(s) — the guide describes something the code does not have.`);
	}
}

main().catch((error) => {
	console.error(error);
	process.exit(1);
});
