// Export the JS presentation editor's command icons (presentation/src/chrome/icons.js plus
// the Insert and Slide Show sets; original MIT drawings) as self-contained SVGs for the Rust
// apps: CSS classes from icons.css and the gradient defs from index.html become presentation
// attributes (light theme), and the 1px non-scaling strokes are converted for 16px and 32px.
//
//   node slides/tools/export-icons.mjs   →   kit/src/slide_icon_data.rs

import { readFileSync, writeFileSync, mkdtempSync, mkdirSync } from 'node:fs';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { tmpdir } from 'node:os';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const js = path.join(here, '../../presentation');
// The icon modules import their stylesheet; load copies without it.
const tmp = mkdtempSync(path.join(tmpdir(), 'slide-icons-'));
for (const dir of ['chrome', 'insert', 'show']) mkdirSync(path.join(tmp, dir), { recursive: true });
const copy = (rel) => writeFileSync(path.join(tmp, rel), readFileSync(path.join(js, 'src', rel), 'utf8').replace(/^import\s+["'][^"']+\.css["'];?\s*$/m, ''));
['chrome/icons.js', 'chrome/glyphs.js', 'insert/insert-icons.js', 'show/icons.js'].forEach(copy);
writeFileSync(path.join(tmp, 'package.json'), '{"type":"module"}');
const { COMMAND_ICONS } = await import(pathToFileURL(path.join(tmp, 'chrome/icons.js')));
for (const m of ['insert/insert-icons.js', 'show/icons.js']) { try { await import(pathToFileURL(path.join(tmp, m))); } catch (e) { console.warn(`skipped ${m}: ${e.message}`); } }

const css = readFileSync(path.join(js, 'src/chrome/icons.css'), 'utf8');
const rootBlock = css.slice(css.indexOf(':root {'), css.indexOf('}', css.indexOf(':root {')));
const vars = Object.fromEntries([...rootBlock.matchAll(/--([\w-]+):\s*([^;]+);/g)].map((m) => [m[1], m[2].trim()]));
vars['ico-knock'] = '#ffffff';
const v = (name) => vars[name] ?? '#000';
const ink = v('ico-ink');

const html = readFileSync(path.join(js, 'index.html'), 'utf8');
const defs = html.slice(html.indexOf('<linearGradient id="ico-g-paper"'), html.lastIndexOf('</linearGradient>') + '</linearGradient>'.length)
  .replace(/style="\s*stop-color:\s*var\(--([\w-]+)\)\s*"/g, (_, n) => `stop-color="${v(n)}"`)
  .replace(/\s*\n\s*/g, '');

const g = (id) => `url(#ico-g-${id})`;
const RULES = [
  ['thick', { sw: 1.3 }], ['grip', { sw: 1.9 }], ['dash', { dash: 1.5, cap: 'butt' }], ['ln', { opacity: 0.35 }],
  ['o', { fill: 'none', stroke: ink }], ['of', { fill: g('paper'), stroke: v('ico-line') }], ['s', { fill: ink, stroke: 'none' }],
  ['w', { fill: 'none', stroke: v('ico-paper-top') }], ['tx', { fill: ink, stroke: 'none' }], ['txb', { fill: g('blue-solid'), stroke: 'none' }],
  ['txb2', { fill: v('ico-blue'), stroke: 'none' }], ['txk', { fill: g('slide-solid'), stroke: 'none' }], ['txw', { fill: '#fff', stroke: 'none' }],
  ['txo', { fill: 'none', stroke: ink }], ['knock', { fill: v('ico-knock'), stroke: 'none' }],
  ['wh', { fill: 'none', stroke: '#fff' }], ['glint', { fill: '#fff', stroke: 'none', opacity: 0.85 }], ['glassy', { 'fill-opacity': 0.78 }],
  ...['of', 'bf', 'gf', 'rf', 'yf', 'nf', 'pf', 'kf'].map((c) => [c, { sw: 0.75 }]),
  ...[['b', 'blue'], ['g', 'green'], ['r', 'red'], ['n', 'amber'], ['p', 'purple'], ['k', 'slide']].flatMap(([c, hue]) => [
    [c, { fill: 'none', stroke: v(`ico-${hue}`) }], [`${c}f`, { fill: g(`${hue}-soft`), stroke: v(`ico-${hue}`) }],
    [`${c}s`, { fill: g(`${hue}-solid`), stroke: 'none' }], [`${c}w`, { fill: g(`${hue}-soft`), stroke: 'none' }],
  ]),
  ['y', { fill: 'none', stroke: v('ico-yellow') }], ['yf', { fill: g('yellow-soft'), stroke: v('ico-amber') }], ['ys', { fill: g('yellow-solid'), stroke: 'none' }], ['yw', { fill: g('yellow-soft'), stroke: 'none' }],
];

function convert(body, size) {
  const k = 20 / size;
  const stack = [];
  return body.replace(/<(\/?)([\w:]+)([^>]*?)(\/?)>/g, (tag, close, name, attrs, selfClose) => {
    if (close) { if (name === 'g') stack.pop(); return tag; }
    const classes = (attrs.match(/class="([^"]*)"/)?.[1] ?? '').split(/\s+/).filter(Boolean);
    const inText = stack.flat().some((c) => ['tx', 'txb', 'txk', 'txw'].includes(c));
    const p = { sw: 0.9, cap: 'round' };
    for (const [c, props] of RULES) if (classes.includes(c)) Object.assign(p, props);
    if (inText && name === 'path' && !classes.length) { p.stroke = 'none'; }
    const out = [];
    if (p.fill && !(inText && !classes.length)) out.push(`fill="${p.fill}"`);
    if (p.stroke) out.push(`stroke="${p.stroke}"`);
    const sw = inText && !classes.length ? null : +(p.sw * k).toFixed(3);
    if (sw !== null) out.push(`stroke-width="${sw}"`);
    out.push(`stroke-linecap="${p.cap}"`, 'stroke-linejoin="round"');
    if (p.dash) out.push(`stroke-dasharray="${+(p.dash * k).toFixed(3)} ${+(p.dash * k).toFixed(3)}"`);
    if (p.opacity !== undefined) out.push(`opacity="${p.opacity}"`);
    if (p['fill-opacity'] !== undefined) out.push(`fill-opacity="${p['fill-opacity']}"`);
    if (name === 'g' && !selfClose) stack.push(classes);
    if (name === 'g') {
      // Text groups give their fill to the glyph paths inside.
      const fill = out.find((a) => a.startsWith('fill='));
      return `<${name}${attrs.replace(/\s*class="[^"]*"/, '')}${fill ? ' ' + fill : ''}${selfClose}>`;
    }
    return `<${name}${attrs.replace(/\s*class="[^"]*"/, '')} ${out.join(' ')}${selfClose}>`;
  });
}

const rust = (s) => `r##"${s}"##`;
const names = Object.keys(COMMAND_ICONS).filter((n) => typeof COMMAND_ICONS[n] === 'string').sort();
let out = `// Generated by slides/tools/export-icons.mjs from presentation/src/chrome/icons.js (MIT). Do not edit.\n\n`;
out += `/// Gradients shared by every icon.\npub const DEFS: &str = ${rust(defs)};\n\n`;
out += `/// (name, body for 16px, body for 32px) in a 20-unit viewBox, sorted by name.\npub static ICONS: &[(&str, &str, &str)] = &[\n`;
for (const name of names) out += `    ("${name}", ${rust(convert(COMMAND_ICONS[name], 16))}, ${rust(convert(COMMAND_ICONS[name], 32))}),\n`;
out += '];\n';
writeFileSync(path.join(here, '../../kit/src/slide_icon_data.rs'), out);
console.log(`${names.length} icons → kit/src/slide_icon_data.rs (${(out.length / 1024).toFixed(0)} KB)`);
