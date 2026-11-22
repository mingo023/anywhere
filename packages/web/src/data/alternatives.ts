import conductor from "./alternatives/conductor";
import orca from "./alternatives/orca";
import paseo from "./alternatives/paseo";
import t3Code from "./alternatives/t3-code";
import vibeKanban from "./alternatives/vibe-kanban";
import claudeSquad from "./alternatives/claude-squad";
import crystal from "./alternatives/crystal";
import monocode from "./alternatives/monocode";
import paradev from "./alternatives/paradev";
import zeron from "./alternatives/zeron";
import synara from "./alternatives/synara";

/** One competitor page. Fields named `html` or holding links are HTML; `faq` and plain names are text. */
export type Alternative = {
	slug: string;
	name: string;
	url: string;
	title: string;
	description: string;
	h1: string;
	sources: string;
	lede: string;
	intro: string;
	screen: string;
	screenAlt: string;
	caption: string;
	summary: string;
	sections: { id: string; title: string; html: string }[];
	table: [label: string, anywhere: string, other: string][];
	betterPickTitle?: string;
	betterPick: string[];
	whyAnywhere: string[];
	faq: { q: string; a: string }[];
	cta: string;
};

export type Faq = Alternative["faq"];

/** Hub order; each page's "Other comparisons" follows it too. */
export const alternatives: Alternative[] = [conductor, orca, paseo, t3Code, vibeKanban, claudeSquad, crystal, monocode, paradev, zeron, synara];

const site = "https://getanywhere.dev";
const hubUrl = `${site}/alternatives/`;
const dateModified = "2026-11-22";

export const altUrl = (slug: string) => `${site}/alternatives/${slug}`;

const app = {
	"@type": "SoftwareApplication",
	"@id": `${site}/#app`,
	name: "Anywhere",
	alternateName: "Anywhere coding agent workspace",
	description: "Free, open-source coding agent workspace for running Claude Code, Codex, Cursor and more coding agents side by side.",
	url: site,
	applicationCategory: "DeveloperApplication",
	operatingSystem: "macOS, Linux",
	offers: { "@type": "Offer", price: "0", priceCurrency: "USD" },
	isAccessibleForFree: true,
	license: "https://opensource.org/licenses/MIT",
	downloadUrl: "https://github.com/mingo023/anywhere/releases/latest/download/Anywhere.dmg",
	sameAs: ["https://github.com/mingo023/anywhere", "https://x.com/immint023"],
};

const website = { "@type": "WebSite", "@id": `${site}/#website`, name: "Anywhere, coding agent workspace", url: site };

const crumbs = (items: { name: string; item: string }[]) =>
	items.map((c, i) => ({ "@type": "ListItem", position: i + 1, ...c }));

const faqPage = (id: string, faq: Faq) => ({
	"@type": "FAQPage",
	"@id": id,
	mainEntity: faq.map(({ q, a }) => ({ "@type": "Question", name: q, acceptedAnswer: { "@type": "Answer", text: a } })),
});

export function alternativeGraph(alt: Alternative) {
	const url = altUrl(alt.slug);
	return {
		"@context": "https://schema.org",
		"@graph": [
			app,
			{
				"@type": "WebPage",
				"@id": url,
				url,
				name: `${alt.name} alternative: Anywhere vs ${alt.name}`,
				description: alt.description,
				inLanguage: "en",
				dateModified,
				isPartOf: website,
				about: { "@id": app["@id"] },
				mentions: { "@type": "SoftwareApplication", name: alt.name, url: alt.url },
				breadcrumb: { "@id": `${url}#breadcrumb` },
				mainEntity: { "@id": `${url}#faq` },
			},
			{
				"@type": "BreadcrumbList",
				"@id": `${url}#breadcrumb`,
				itemListElement: crumbs([
					{ name: "Anywhere", item: `${site}/` },
					{ name: "Alternatives", item: hubUrl },
					{ name: alt.name, item: url },
				]),
			},
			faqPage(`${url}#faq`, alt.faq),
		],
	};
}

export function hubGraph(title: string, description: string, faq: Faq) {
	return {
		"@context": "https://schema.org",
		"@graph": [
			app,
			{
				"@type": "CollectionPage",
				"@id": hubUrl,
				url: hubUrl,
				name: title,
				description,
				inLanguage: "en",
				dateModified,
				isPartOf: website,
				about: { "@id": app["@id"] },
				breadcrumb: { "@id": `${hubUrl}#breadcrumb` },
				mainEntity: {
					"@type": "ItemList",
					itemListElement: alternatives.map((alt, i) => ({ "@type": "ListItem", position: i + 1, name: `Anywhere vs ${alt.name}`, url: altUrl(alt.slug) })),
				},
			},
			{
				"@type": "BreadcrumbList",
				"@id": `${hubUrl}#breadcrumb`,
				itemListElement: crumbs([
					{ name: "Anywhere", item: `${site}/` },
					{ name: "Alternatives", item: hubUrl },
				]),
			},
			faqPage(`${hubUrl}#faq`, faq),
		],
	};
}
