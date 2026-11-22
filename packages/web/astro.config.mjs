import { defineConfig } from "astro/config";

export default defineConfig({
	site: "https://getanywhere.dev",
	// Emit docs/hosts.html, not docs/hosts/index.html: Cloudflare serves it at /docs/hosts, the canonical URL.
	build: { format: "preserve" },
	compressHTML: false,
});
