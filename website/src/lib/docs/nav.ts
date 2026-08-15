/**
 * The guide's table of contents.
 *
 * This is the single source of truth for the sidebar, the prev/next footer
 * links, and the set of pages the coverage checker reads. Adding a page means
 * adding it here and creating the matching Markdown file under
 * `src/content/docs/`.
 */

export interface DocPage {
	/** URL slug, relative to /docs. */
	slug: string;
	title: string;
}

export interface DocSection {
	title: string;
	pages: DocPage[];
}

export const nav: DocSection[] = [
	{
		title: 'Getting started',
		pages: [
			{ slug: 'install', title: 'Install' },
			{ slug: 'first-steps', title: 'First steps' },
			{ slug: 'tour', title: 'A tour of the app' }
		]
	},
	{
		title: 'Your library',
		pages: [
			{ slug: 'importing', title: 'Adding papers' },
			{ slug: 'collections-tags', title: 'Collections and tags' },
			{ slug: 'search', title: 'Search' },
			{ slug: 'duplicates', title: 'Duplicates' }
		]
	},
	{
		title: 'Reading',
		pages: [
			{ slug: 'reader', title: 'The PDF reader' },
			{ slug: 'annotations', title: 'Annotations and notes' },
			{ slug: 'citations-in-pdfs', title: 'Following citations' }
		]
	},
	{
		title: 'Citing',
		pages: [
			{ slug: 'citation-styles', title: 'Citation styles' },
			{ slug: 'bibtex', title: 'BibTeX and other formats' },
			{ slug: 'word-addin', title: 'Word add-in' }
		]
	},
	{
		title: 'Connected tools',
		pages: [
			{ slug: 'browser-extension', title: 'Browser extension' },
			{ slug: 'ai-assistant', title: 'AI assistant' },
			{ slug: 'mcp', title: 'MCP server' }
		]
	},
	{
		title: 'Going further',
		pages: [
			{ slug: 'graph', title: 'Citation graph' },
			{ slug: 'sync', title: 'Sync' },
			{ slug: 'settings', title: 'Settings' },
			{ slug: 'shortcuts', title: 'Keyboard shortcuts' }
		]
	},
	{
		title: 'Reference',
		pages: [
			{ slug: 'connector-api', title: 'Connector API' },
			{ slug: 'data-locations', title: 'Where your data lives' }
		]
	}
];

/** Every page in reading order, for prev/next links. */
export const flatPages: DocPage[] = nav.flatMap((section) => section.pages);

export function neighbours(slug: string): { prev?: DocPage; next?: DocPage } {
	const index = flatPages.findIndex((page) => page.slug === slug);
	if (index === -1) return {};
	return {
		prev: index > 0 ? flatPages[index - 1] : undefined,
		next: index < flatPages.length - 1 ? flatPages[index + 1] : undefined
	};
}
