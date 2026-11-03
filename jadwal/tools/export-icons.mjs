// Jadwal's spreadsheet command icons (ribbon: Home, Insert, Formulas, Data, Page Layout, View,
// Review), drawn in the kit's Fluent-like style: gray outlines, paper fills and soft colored
// accents with the same gradients as the slides set. Shapes follow the JS spreadsheet's
// commands (spreadsheet/src/ui/command-icons.js) but are redrawn here, not copied from lucide.
//
//   node sheet/tools/export-icons.mjs   →   kit/src/sheet_icon_data.rs

import { readFileSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import path from 'node:path';

const here = path.dirname(fileURLToPath(import.meta.url));
const kit = path.join(here, '../../kit/src');
const slide = readFileSync(path.join(kit, 'slide_icon_data.rs'), 'utf8');
const defs = slide.match(/pub const DEFS: &str = r##"(.*?)"##;/s)[1];

const C = { ink: '#636366', dark: '#2c2c2e', blue: '#007aff', green: '#1fa34a', red: '#e8352b', amber: '#ea8600', purple: '#a845d6', yellow: '#f2b600' };
const HUE = { b: 'blue', g: 'green', r: 'red', n: 'amber', p: 'purple', y: 'yellow' };
// Class → presentation attributes; sw is in device pixels and is scaled per size.
function style(cls) {
  const p = { sw: 0.9, fill: 'none', stroke: C.ink };
  for (const c of cls.split(/\s+/).filter(Boolean)) {
    if (c === 'o') Object.assign(p, { fill: 'none', stroke: C.ink });
    else if (c === 'd') Object.assign(p, { fill: 'none', stroke: C.dark });
    else if (c === 'of') Object.assign(p, { fill: 'url(#ico-g-paper)', stroke: C.ink, sw: 0.75 });
    else if (c === 's') Object.assign(p, { fill: C.ink, stroke: 'none' });
    else if (c === 'ds') Object.assign(p, { fill: C.dark, stroke: 'none' });
    else if (c === 'wh') Object.assign(p, { fill: 'none', stroke: '#fff' });
    else if (c === 'thick') p.sw = 1.3;
    else if (c === 'thin') p.sw = 0.75;
    else if (c === 'ln') p.opacity = 0.4;
    else if (c === 'dash') p.dash = 1.5;
    else if (/^[bgrnpy][fsw]?$/.test(c)) {
      const hue = HUE[c[0]], col = c[0] === 'y' ? C.amber : C[hue];
      if (c.length === 1) Object.assign(p, { fill: 'none', stroke: c[0] === 'y' ? C.yellow : col });
      else if (c[1] === 'f') Object.assign(p, { fill: `url(#ico-g-${hue}-soft)`, stroke: col, sw: 0.75 });
      else if (c[1] === 's') Object.assign(p, { fill: `url(#ico-g-${hue}-solid)`, stroke: 'none' });
      else Object.assign(p, { fill: `url(#ico-g-${hue}-soft)`, stroke: 'none' });
    } else throw new Error(`unknown class ${c}`);
  }
  return p;
}
const el = (tag, geo, cls) => ({ tag, geo, cls });
const R = (x, y, w, h, cls, rx = 1) => el('rect', `x="${x}" y="${y}" width="${w}" height="${h}" rx="${rx}"`, cls);
const P = (d, cls) => el('path', `d="${d}"`, cls);
const L = (x1, y1, x2, y2, cls = 'o') => P(`M${x1} ${y1}L${x2} ${y2}`, cls);
const O = (cx, cy, r, cls) => el('circle', `cx="${cx}" cy="${cy}" r="${r}"`, cls);
const E = (cx, cy, rx, ry, cls) => el('ellipse', `cx="${cx}" cy="${cy}" rx="${rx}" ry="${ry}"`, cls);

// Building blocks.
function grid(x, y, w, h, cols, rows, cls = 'of') {
  const out = [R(x, y, w, h, cls, 1)];
  let d = '';
  for (let i = 1; i < cols; i++) { const gx = +(x + (w * i) / cols).toFixed(2); d += `M${gx} ${y}V${y + h}`; }
  for (let j = 1; j < rows; j++) { const gy = +(y + (h * j) / rows).toFixed(2); d += `M${x} ${gy}H${x + w}`; }
  if (d) out.push(P(d, 'o ln'));
  return out;
}
const sheet = () => grid(2.5, 3.5, 15, 13, 3, 4);
const badgePlus = (cx = 15, cy = 15) => [O(cx, cy, 3.5, 'gs'), P(`M${cx - 1.8} ${cy}h3.6M${cx} ${cy - 1.8}v3.6`, 'wh thick')];
const badgeX = (cx = 15, cy = 15) => [O(cx, cy, 3.5, 'rs'), P(`M${cx - 1.3} ${cy - 1.3}l2.6 2.6M${cx + 1.3} ${cy - 1.3}l-2.6 2.6`, 'wh thick')];
const badgeCheck = (cx = 15, cy = 15) => [O(cx, cy, 3.5, 'gs'), P(`M${cx - 1.7} ${cy}l1.2 1.2 2.3-2.4`, 'wh thick')];
const sparkle = (x, y, s = 1) => P(`M${x} ${y - 3 * s}l${0.8 * s} ${2.2 * s} ${2.2 * s} ${0.8 * s}-${2.2 * s} ${0.8 * s}-${0.8 * s} ${2.2 * s}-${0.8 * s}-${2.2 * s}-${2.2 * s}-${0.8 * s} ${2.2 * s}-${0.8 * s}z`, 'ys');
const funnel = (cls = 'bf') => P('M3 4h12l-4.6 5.6V15l-2.8 1.5V9.6z', cls);
const book = () => [P('M4 3.5h10.5a1 1 0 0 1 1 1v12a1 1 0 0 1-1 1H4z', 'of'), L(6.5, 3.5, 6.5, 17.5)];
const lock = (x, y) => [P(`M${x + 1} ${y}v-1.5a2 2 0 0 1 4 0V${y}`, 'o thick'), R(x, y, 6, 4.5, 'ns', 0.8)];
const magnifier = (cx, cy, r, cls = 'bf') => [O(cx, cy, r, cls), L(cx + r * 0.7, cy + r * 0.7, cx + r * 0.7 + 4, cy + r * 0.7 + 4, 'o thick')];
const page = () => P('M4.5 2.5h8l3 3v12h-11z', 'of');
const fx = (x, y, k = 1, cls = 'd thick') => P(`M${x + 4 * k} ${y}c-1.4 0-2 .8-2.2 2.2L${x + 0.6 * k} ${y + 10 * k}M${x} ${y + 3.5 * k}h${3.6 * k}M${x + 4.6 * k} ${y + 4 * k}l${3.4 * k} ${5 * k}M${x + 8 * k} ${y + 4 * k}l-${3.4 * k} ${5 * k}`, cls);
const dollar = (cx, cy, s = 1, cls = 'd') => P(`M${cx + 2.3 * s} ${cy - 2.2 * s}c-.4-.8-1.2-1.3-2.3-1.3-1.3 0-2.2.7-2.2 1.7s.9 1.4 2.2 1.7 2.3.8 2.3 1.8-1 1.8-2.3 1.8c-1.1 0-2-.5-2.5-1.4M${cx} ${cy - 4.5 * s}v${9 * s}`, cls);
const clock = (cx, cy, r) => [O(cx, cy, r, 'nf'), P(`M${cx} ${cy - r * 0.6}V${cy}l${r * 0.45} ${r * 0.35}`, 'd')];
const letterA = (x, y, s = 1, cls = 'd thick') => P(`M${x} ${y + 9 * s}l${3.5 * s}-${9 * s} ${3.5 * s} ${9 * s}M${x + 1.3 * s} ${y + 5.8 * s}h${4.4 * s}`, cls);
const calc = () => [R(4.5, 2.5, 11, 15, 'of', 1.5), R(6.5, 4.5, 7, 3, 'gf', 0.5), ...[0, 1, 2].flatMap((i) => [0, 1, 2].map((j) => R(6.5 + i * 2.5, 9.5 + j * 2.4, 1.6, 1.4, j === 2 && i === 2 ? 'bs' : 's', 0.3)))];
const gear = (cx, cy, r) => [P(Array.from({ length: 8 }, (_, i) => { const a = (i * Math.PI) / 4; return `M${(cx + Math.cos(a) * r * 0.75).toFixed(2)} ${(cy + Math.sin(a) * r * 0.75).toFixed(2)}L${(cx + Math.cos(a) * r).toFixed(2)} ${(cy + Math.sin(a) * r).toFixed(2)}`; }).join(''), 'd thick'), O(cx, cy, r * 0.6, 'nf'), O(cx, cy, r * 0.22, 'o')];
const tag = (cls = 'bf') => P('M3 6.5l3.5-3.5H16v7H6.5z', cls);
const pencil = (cls = 'nf') => P('M10.5 17.5l.5-2.6 5.6-5.6 2.1 2.1-5.6 5.6z', cls);

const I = {
  autoSum: [P('M14.5 4.5H5.5l5 5.5-5 5.5h9', 'd thick'), L(14.5, 4.5, 14.5, 6, 'd thick'), L(14.5, 15.5, 14.5, 14, 'd thick')],
  fillDown: [...grid(5.5, 2.5, 9, 15, 1, 4), R(5.5, 2.5, 9, 3.75, 'bf', 1), P('M10 7.5v7M7.5 12l2.5 2.6 2.5-2.6', 'b thick')],
  clear: [P('M3.5 12.5l7-7 5 5-4.5 4.5H7.5z', 'rf'), L(7, 9, 12, 14, 'r'), L(8, 17, 16.5, 17)],
  sortFilter: [P('M5 3.5v12M2.8 13.3L5 15.6l2.2-2.3', 'b thick'), P('M9 4h8.5l-3.3 4.2V13l-1.9 1.2V8.2z', 'bf'), L(9, 16.5, 17.5, 16.5, 'o')],
  findSelect: [...magnifier(8.5, 8.5, 4.5)],
  conditionalFormatting: [...sheet(), R(2.5, 3.5, 5, 3.25, 'rw', 0), R(7.5, 6.75, 5, 3.25, 'yw', 0), R(12.5, 10, 5, 3.25, 'gw', 0), R(2.5, 13.25, 5, 3.25, 'gw', 0), ...grid(2.5, 3.5, 15, 13, 3, 4, 'o')],
  formatAsTable: [...grid(2.5, 3.5, 15, 13, 3, 4), R(2.5, 3.5, 15, 3.25, 'bs', 1), R(2.5, 10, 15, 3.25, 'bw', 0), ...grid(2.5, 3.5, 15, 13, 3, 4, 'o')],
  cellStyles: [R(2.5, 4, 7, 5, 'bf'), R(10.5, 4, 7, 5, 'gf'), R(2.5, 11, 7, 5, 'nf'), R(10.5, 11, 7, 5, 'of'), L(4.5, 6.5, 7.5, 6.5, 'b thick'), L(12.5, 6.5, 15.5, 6.5, 'g thick'), L(4.5, 13.5, 7.5, 13.5, 'n thick'), L(12.5, 13.5, 15.5, 13.5)],
  insertCells: [...grid(2.5, 3, 12, 10, 2, 2), R(8.5, 8, 6, 5, 'gw', 0), ...badgePlus()],
  deleteCells: [...grid(2.5, 3, 12, 10, 2, 2), R(8.5, 8, 6, 5, 'rw', 0), ...badgeX()],
  formatCells: [...grid(2.5, 3, 12, 10, 2, 2), R(2.5, 3, 6, 5, 'bw', 0), ...grid(2.5, 3, 12, 10, 2, 2, 'o'), pencil()],
  accountingFormat: [R(2, 5, 16, 10, 'gf', 1.5), O(10, 10, 3.6, 'of'), dollar(10, 10, 0.62), O(4.8, 10, 0.6, 'g'), O(15.2, 10, 0.6, 'g')],
  percentStyle: [O(6, 6, 2.2, 'bf'), O(14, 14, 2.2, 'bf'), L(15, 4, 5, 16, 'd thick')],
  commaStyle: [E(5, 9, 2, 3.5, 'd thick'), E(15, 9, 2, 3.5, 'd thick'), P('M10.4 12.3v1.4c0 1-.6 1.8-1.4 2.3', 'b thick'), O(10.4, 12.3, 0.9, 'bs')],
  increaseDecimal: [O(3.5, 7.5, 0.8, 'ds'), E(7.5, 5.5, 1.6, 2.6, 'd'), O(8.5, 15, 0.8, 'ds'), E(12, 13, 1.6, 2.6, 'd'), E(16.2, 13, 1.6, 2.6, 'd'), P('M17 5.5h-6M13 3.5l-2 2 2 2', 'g thick')],
  decreaseDecimal: [O(3.5, 7.5, 0.8, 'ds'), E(7.5, 5.5, 1.6, 2.6, 'd'), E(11.7, 5.5, 1.6, 2.6, 'd'), O(8.5, 15, 0.8, 'ds'), E(12, 13, 1.6, 2.6, 'd'), P('M11 15.5h6M15 13.5l2 2-2 2', 'r thick')],
  mergeCenter: [R(2.5, 5, 15, 10, 'of'), P('M2.5 5h15M10 5v2.5M10 12.5V15', 'o ln'), P('M3.8 10h4.5M6.5 8l2 2-2 2M16.2 10h-4.5M13.5 8l-2 2 2 2', 'b thick')],
  wrapText: [L(3, 5, 17, 5, 'd'), P('M3 10h11a2.5 2.5 0 0 1 0 5h-3', 'b thick'), P('M12.5 13l-2 2 2 2', 'b thick'), L(3, 15, 8, 15, 'd')],
  orientation: [P('M4 15.5l7.5-9', 'd thick'), letterA(3, 3, 0.7, 'd'), P('M15.5 4v12M13.5 14l2 2 2-2', 'b thick')],
  alignTop: [L(3, 3.5, 17, 3.5, 'd thick'), R(5, 5.5, 4, 10, 'bf', 0.8), R(11, 5.5, 4, 6, 'bf', 0.8)],
  alignMiddle: [L(3, 10, 17, 10, 'd thick'), R(5, 4, 4, 12, 'bf', 0.8), R(11, 6.5, 4, 7, 'bf', 0.8), L(3, 10, 17, 10, 'd thick')],
  alignBottom: [L(3, 16.5, 17, 16.5, 'd thick'), R(5, 4.5, 4, 10, 'bf', 0.8), R(11, 8.5, 4, 6, 'bf', 0.8)],
  freezePanes: [...sheet(), R(2.5, 3.5, 15, 3.25, 'bw', 0), R(2.5, 3.5, 5, 13, 'bw', 0), ...grid(2.5, 3.5, 15, 13, 3, 4, 'o'), P('M2.5 6.75h15M7.5 3.5v13', 'b thick')],
  filter: [funnel()],
  clearFilter: [funnel(), ...badgeX(15.5, 15)],
  reapply: [funnel(), P('M17.5 14.5a3 3 0 1 1-1-2.3', 'g thick'), P('M16.9 10.6l-.3 1.8 1.8.3', 'g thick')],
  textToColumns: [P('M2.5 4.5h8M2.5 7h8M2.5 9.5h6', 'd'), P('M6 11.5v2.5M4.5 12.8L6 14.3l1.5-1.5', 'b thick'), R(2.5, 15, 7, 3, 'bf', 0.5), R(10.5, 15, 7, 3, 'gf', 0.5), R(12, 3.5, 5.5, 7, 'of', 0.6), P('M14.75 3.5v7', 'o ln')],
  removeDuplicates: [...grid(2.5, 3, 11, 9, 2, 3), R(2.5, 6, 11, 3, 'rw', 0), ...grid(2.5, 3, 11, 9, 2, 3, 'o'), ...badgeX(14.5, 14.5)],
  dataValidation: [...grid(2.5, 3, 12, 10, 2, 2), R(8.5, 8, 6, 5, 'gw', 0), ...badgeCheck()],
  pivotTable: [...sheet(), R(2.5, 3.5, 15, 3.25, 'bw', 0), R(2.5, 3.5, 5, 13, 'bw', 0), ...grid(2.5, 3.5, 15, 13, 3, 4, 'o'), P('M10 13.5a3 3 0 0 0 3-3', 'b thick'), P('M11.5 12l-1.6 1.5 1.6 1.5M11.5 12M14.5 12l-1.5-1.6-1.5 1.6', 'b thick')],
  recommendedPivot: [...grid(2.5, 5, 13, 12, 3, 3), R(2.5, 5, 13, 4, 'bw', 0), R(2.5, 5, 4.33, 12, 'bw', 0), ...grid(2.5, 5, 13, 12, 3, 3, 'o'), sparkle(15.5, 4.5, 1.1)],
  table: [...grid(2.5, 3.5, 15, 13, 3, 4), R(2.5, 3.5, 15, 3.25, 'bf', 1), ...grid(2.5, 3.5, 15, 13, 3, 4, 'o')],
  flashFill: [R(2.5, 3.5, 9, 13, 'of', 1), P('M4.5 7h5M4.5 10h5M4.5 13h5', 'o'), P('M15.5 2.5l-3.5 7h3l-2 8 5.5-9.5h-3.2l2.2-5.5z', 'ys')],
  map: [O(9, 10.5, 6.5, 'bf'), P('M2.5 10.5h13M9 4c-2.4 2-2.4 11 0 13M9 4c2.4 2 2.4 11 0 13', 'o'), P('M15 2.5a2.6 2.6 0 0 1 2.6 2.6c0 2-2.6 4.6-2.6 4.6s-2.6-2.6-2.6-4.6A2.6 2.6 0 0 1 15 2.5z', 'rs'), O(15, 5.1, 0.9, 'wh')],
  slicer: [R(3.5, 2.5, 13, 15, 'of', 1.5), L(5.5, 5, 11, 5, 'd thick'), R(5.5, 7, 9, 2.4, 'bs', 0.6), R(5.5, 10.3, 9, 2.4, 'bf', 0.6), R(5.5, 13.6, 9, 2.4, 'of', 0.6)],
  timeline: [R(2, 5, 16, 10, 'of', 1.5), P('M4 8h12', 'o ln'), P('M5 6.5v1.5M8 6.5v1.5M11 6.5v1.5M14 6.5v1.5', 'o'), R(6.5, 10, 6, 3, 'bs', 0.6), R(4, 10, 12, 3, 'o', 0.6)],
  chartColumn: [L(2.5, 17, 17.5, 17), R(4, 9, 3, 8, 'bs', 0.4), R(8.5, 4, 3, 13, 'gs', 0.4), R(13, 11, 3, 6, 'ns', 0.4)],
  chartLine: [P('M2.5 3v14h15', 'o'), P('M4.5 13l3.5-4 3 2.5 5-6', 'b thick'), O(8, 9, 1, 'bs'), O(11, 11.5, 1, 'bs')],
  chartPie: [O(9.5, 10.5, 6.5, 'bf'), P('M9.5 10.5V4a6.5 6.5 0 0 1 6.5 6.5z', 'ns'), P('M9.5 10.5L4 14a6.5 6.5 0 0 1-.4-6.2z', 'gs')],
  chartBar: [L(3, 2.5, 3, 17.5), R(3, 4, 8, 3, 'bs', 0.4), R(3, 8.5, 13, 3, 'gs', 0.4), R(3, 13, 6, 3, 'ns', 0.4)],
  chartArea: [P('M2.5 3v14h15', 'o'), P('M3 16.5V12l4-4 4 3 6-6v11.5z', 'bf'), P('M3 16.5V14l4-2 4 1.5 6-3v6z', 'gs')],
  chartScatter: [P('M2.5 3v14h15', 'o'), O(6, 13.5, 1.2, 'bs'), O(9, 10, 1.2, 'bs'), O(12, 11.5, 1.2, 'gs'), O(14, 6.5, 1.2, 'bs'), O(15.5, 9, 1.2, 'gs'), O(7, 7, 1.2, 'ns')],
  recommendedCharts: [L(2.5, 17, 15.5, 17), R(4, 10, 3, 7, 'bs', 0.4), R(8.5, 6, 3, 11, 'gs', 0.4), R(13, 12, 2.5, 5, 'ns', 0.4), sparkle(15.5, 4.5, 1.1)],
  sparkline: [...grid(2, 5, 16, 10, 2, 1), P('M3.5 12l2-3 2 2 2-4 2 3 2-2 2.5 1.5', 'b thick')],
  function: [fx(5, 5)],
  financial: [...book(), O(11, 10.5, 3.6, 'gf'), dollar(11, 10.5, 0.62)],
  logical: [...book(), O(11, 10.5, 3.6, 'bf'), P('M9.6 9.4a1.5 1.5 0 1 1 2.1 1.4c-.5.3-.7.6-.7 1.1', 'd'), O(11, 13.2, 0.5, 'ds')],
  text: [...book(), letterA(8, 6, 0.85, 'p thick')],
  dateTime: [...book(), ...clock(11, 10.5, 3.6)],
  lookup: [...book(), O(10.5, 9.5, 2.6, 'bf'), L(12.4, 11.4, 14.5, 13.5, 'd thick')],
  mathTrig: [...book(), P('M8.5 7.5h6M10 7.5v6M13 7.5v5c0 .6.4 1 1 1', 'r thick')],
  moreFunctions: [...book(), O(8.8, 10.5, 0.9, 'ds'), O(11, 10.5, 0.9, 'ds'), O(13.2, 10.5, 0.9, 'ds')],
  nameManager: [tag(), O(6.5, 6.5, 0.8, 'o'), R(4, 12, 13, 5, 'of', 1), P('M6 14.5h7', 'd')],
  defineName: [tag(), O(6.5, 6.5, 0.8, 'o'), pencil()],
  calculateNow: [...calc()],
  calcOptions: [R(3, 2.5, 10, 13.5, 'of', 1.5), R(5, 4.5, 6, 2.6, 'gf', 0.5), P('M5.5 10h1M8.5 10h1M5.5 13h1M8.5 13h1', 'd thick'), ...gear(14.5, 14.5, 3.5)],
  showFormulas: [...grid(2.5, 3.5, 15, 13, 2, 2), fx(6, 5.5, 0.85, 'b thick')],
  traceErrors: [...grid(2.5, 3, 9, 7, 1, 1), R(11, 11, 6.5, 6, 'of', 1), P('M7 10v3.5h3.5M8.8 12l1.7 1.5-1.7 1.5', 'b'), P('M14.25 12l2.5 4.4h-5z', 'rs'), L(14.25, 13.4, 14.25, 14.7, 'wh'), O(14.25, 15.6, 0.15, 'wh')],
  tracePrecedents: [R(2.5, 3, 6, 5, 'of', 0.8), R(11.5, 12, 6, 5, 'bf', 0.8), O(5.5, 5.5, 1, 'bs'), P('M6.3 6.3L13.6 13.2', 'b thick'), P('M11.4 13.6l2.6.2-.2-2.6', 'b thick')],
  traceDependents: [R(2.5, 3, 6, 5, 'bf', 0.8), R(11.5, 12, 6, 5, 'of', 0.8), O(5.5, 5.5, 1, 'bs'), P('M6.3 6.3L13.6 13.2', 'b thick'), P('M11.4 13.6l2.6.2-.2-2.6', 'b thick'), P('M13 9.5l2.5 0M14.25 8.25v2.5', 'g thick')],
  removeArrows: [O(4.5, 5, 1, 'bs'), P('M5.3 5.8L12 12.3', 'b thick'), P('M9.8 12.6l2.6.2-.2-2.6', 'b thick'), ...badgeX(14.5, 14.5)],
  evaluateFormula: [R(2.5, 4, 15, 8, 'of', 1), fx(4, 5, 0.6, 'd'), L(10, 10, 15.5, 10, 'b thick'), P('M7 15h6M11 13.5l2 1.5-2 1.5', 'g thick')],
  watchWindow: [R(2.5, 3, 15, 13, 'of', 1.2), R(2.5, 3, 15, 3, 'bw', 0), P('M5 11c1.5-2.4 3-3.4 5-3.4s3.5 1 5 3.4c-1.5 2.4-3 3.4-5 3.4s-3.5-1-5-3.4z', 'bf'), O(10, 11, 1.4, 'ds')],
  useInFormula: [tag(), O(6.5, 6.5, 0.8, 'o'), fx(10, 10, 0.6, 'b thick')],
  createFromSelection: [...grid(2.5, 3, 12, 11, 3, 3), R(2.5, 3, 12, 3.6, 'bw', 0), ...badgePlus()],
  calcSheet: [...grid(2.5, 3, 11, 11, 2, 2), R(11, 9.5, 6.5, 8, 'of', 1), R(12.3, 10.8, 3.9, 1.8, 'gf', 0.3), P('M12.6 14.5h.8M14.8 14.5h.8M12.6 16.3h.8M14.8 16.3h.8', 'd')],
  errorChecking: [...grid(2.5, 3, 12, 11, 2, 2), P('M3 3.5h3.5L3 7z', 'gs'), ...badgeCheck()],
  recentlyUsed: [...book(), ...clock(11, 10.5, 3.6), P('M8.5 15.5h5', 'b thick')],
  gridlines: [R(2.5, 3.5, 15, 13, 'of', 1), P('M7.5 3.5v13M12.5 3.5v13M2.5 7.75h15M2.5 12.25h15', 'b')],
  headings: [...grid(2.5, 3.5, 15, 13, 3, 3), R(2.5, 3.5, 15, 3, 'bw', 0), R(2.5, 3.5, 4, 13, 'bw', 0), P('M2.5 6.5h15M6.5 3.5v13', 'b')],
  pageBreakPreview: [R(3.5, 2.5, 13, 15, 'of', 1), L(3.5, 10, 16.5, 10, 'b thick dash'), P('M6 5h8M6 7h5M6 13h8M6 15h5', 'o ln')],
  printArea: [page(), R(6.5, 6.5, 7, 8, 'b dash', 0.4), R(6.5, 6.5, 7, 8, 'bw', 0.4)],
  printTitles: [page(), R(4.5, 5.5, 11, 2.5, 'bs', 0), P('M6.5 10.5h7M6.5 12.5h7M6.5 14.5h4', 'o ln')],
  breaks: [P('M4 2.5h12v6H4z', 'of'), P('M4 17.5h12v-6H4z', 'of'), L(2, 10, 18, 10, 'b thick dash')],
  zoomSelection: [R(3, 3, 8, 8, 'b dash', 0.4), ...magnifier(9.5, 9.5, 4, 'bw'), O(9.5, 9.5, 4, 'o')],
  newSheet: [...grid(2.5, 3, 12, 11, 3, 3), ...badgePlus()],
  protectSheet: [...grid(2.5, 2.5, 12, 11, 3, 3), ...lock(11.5, 13)],
  protectWorkbook: [...book(), ...lock(11.5, 13)],
  note: [P('M3.5 3.5h13v9l-4 4h-9z', 'yf'), P('M16.5 12.5h-4v4', 'y'), P('M6 7h7M6 9.5h5', 'o')],
  link: [P('M8.5 11.5a3 3 0 0 0 4.2 0l2.6-2.6a3 3 0 0 0-4.2-4.2l-1 1', 'b thick'), P('M11.5 8.5a3 3 0 0 0-4.2 0l-2.6 2.6a3 3 0 0 0 4.2 4.2l1-1', 'b thick')],
  symbol: [P('M4.5 16h3.5v-1.6a5.5 5.5 0 1 1 4 0V16h3.5', 'd thick')],
  picture: [R(2.5, 4, 15, 12, 'bf', 1.5), O(13.5, 7.5, 1.4, 'ys'), P('M2.8 15l4.7-5.5 3.5 4 2-2 4.2 3.5', 'gf')],
  shapes: [O(7, 7, 4.2, 'bf'), R(9, 9, 8, 8, 'nf', 0.8)],
};

function convert(body, size) {
  const k = 20 / size;
  return body.map(({ tag, geo, cls }) => {
    const p = style(cls);
    let a = `fill="${p.fill}" stroke="${p.stroke}" stroke-width="${+(p.sw * k).toFixed(3)}" stroke-linecap="${p.dash ? 'butt' : 'round'}" stroke-linejoin="round"`;
    if (p.dash) a += ` stroke-dasharray="${+(p.dash * k).toFixed(3)} ${+(p.dash * k).toFixed(3)}"`;
    if (p.opacity !== undefined) a += ` opacity="${p.opacity}"`;
    return `<${tag} ${geo} ${a}/>`;
  }).join('');
}

const rust = (s) => `r##"${s}"##`;
const names = Object.keys(I).sort();
let out = `// Generated by sheet/tools/export-icons.mjs. Do not edit.\n\n`;
out += `/// Gradients shared by every icon (the same as the slides set).\npub const DEFS: &str = ${rust(defs)};\n\n`;
out += `/// (name, body for 16px, body for 32px) in a 20-unit viewBox, sorted by name.\npub static ICONS: &[(&str, &str, &str)] = &[\n`;
for (const n of names) out += `    ("${n}", ${rust(convert(I[n], 16))}, ${rust(convert(I[n], 32))}),\n`;
out += '];\n';
out += `
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sorted_and_every_icon_renders() {
        assert!(ICONS.windows(2).all(|w| w[0].0 < w[1].0), "ICONS must be sorted for binary search");
        for (name, small, large) in ICONS {
            for body in [small, large] {
                let svg = format!(r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" width="32" height="32"><defs>{DEFS}</defs>{body}</svg>"#);
                let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).unwrap_or_else(|e| panic!("{name}: {e}"));
                let mut pixmap = resvg::tiny_skia::Pixmap::new(32, 32).unwrap();
                resvg::render(&tree, resvg::tiny_skia::Transform::identity(), &mut pixmap.as_mut());
                assert!(pixmap.data().chunks(4).any(|p| p[3] > 0), "{name} renders nothing");
            }
        }
    }
}
`;
writeFileSync(path.join(kit, 'sheet_icon_data.rs'), out);
console.log(`${names.length} icons → kit/src/sheet_icon_data.rs (${(out.length / 1024).toFixed(0)} KB)`);
