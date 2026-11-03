// Builds Saboura's template and icon libraries from the JS whiteboard's own builders, so the
// boards and pictograms match it exactly.
// Usage: node tools/gen-libraries.mjs <whiteboard sources with lucide installed> <out dir>
import { writeFileSync } from "node:fs";
const [root, out] = process.argv.slice(2);
const t = await import(root + "/src/templates.js");
const kit = await import(root + "/src/template-library/kit.js");
const templates = t.TEMPLATE_CATALOG.map((d) => ({
  id: d.id, category: d.categoryId, name: d.name, description: d.description, keywords: d.keywords,
  boards: { en: t.fromTemplate(d.id, "en"), fr: t.fromTemplate(d.id, "fr") },
}));
writeFileSync(out + "/templates.json", JSON.stringify({ categories: kit.CATEGORIES, templates }));
const ic = await import(root + "/src/icon-library.js");
// A lucide node as an SVG drawn in currentColor on a 24-unit grid.
const svg = (node) => node.map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${v}"`).join(" ")}/>`).join("");
const icons = ic.ICONS.map((i) => ({ id: i.id, category: i.category, name: i.name, keywords: i.keywords, svg: svg(i.node) }));
const stickers = ic.STICKERS.map((s) => ({ id: s.id, name: s.name, keywords: s.keywords, background: s.background, glyph: s.glyph, label: s.label || null, svg: s.node ? svg(s.node) : null }));
writeFileSync(out + "/icons.json", JSON.stringify({ categories: ic.ICON_CATEGORIES, icons, stickers }));
console.log(templates.length, "templates,", icons.length, "icons,", stickers.length, "stickers");
