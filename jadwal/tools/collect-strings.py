#!/usr/bin/env python3
"""List Jadwal's English UI strings missing from the shared i18n JSON files.

Collects string literals passed to tr/trf and the kit's labelled widgets (menu_item, small,
large, group, theme::dialog, check, section…), literals in arrays that the app maps through
tr(), the engine's function categories, number-format categories and error messages.
    python3 sheet/tools/collect-strings.py [--all]
"""
import json, os, re, sys

ROOT = os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', '..')
APP = os.path.join(ROOT, 'sheet', 'app', 'src')
ENGINE = os.path.join(ROOT, 'sheet', 'engine', 'src')
I18N = os.path.join(ROOT, 'word', 'assets', 'i18n')

LIT = r'"((?:[^"\\]|\\.)*)"'
# Labelled calls: function name → which positional string literals are labels (others are icons/ids).
CALL = re.compile(r'\b(tr|trf|menu_item|item|small|large|large_menu|small_menu|large_split|small_split|color_split|color_menu|group|group_l|dialog|primary|secondary|check|radio_value|radio|section|menu_heading|none_row|label|on_hover_text|heading|button|selectable_label|selectable_value|tip|status|alert|set_status|toast|confirm|ask|hint_text|tab)\s*\(')
ICON_ARG = {'menu_item', 'item', 'small', 'large', 'large_menu', 'small_menu', 'large_split', 'small_split', 'color_split'}

def unescape(s):
    if '\\(' in s: return s
    return s.encode('utf-8').decode('unicode_escape').encode('latin-1').decode('utf-8') if '\\' in s else s

def args_of(src, start):
    """Return the text of a call's argument list starting just after '('."""
    depth, i, out = 1, start, []
    while i < len(src) and depth:
        c = src[i]
        if c == '"':
            j = i + 1
            while src[j] != '"':
                j += 2 if src[j] == '\\' else 1
            out.append(src[i:j + 1]); i = j + 1; continue
        if c in '([{': depth += 1
        elif c in ')]}': depth -= 1
        out.append(c); i += 1
    return ''.join(out)

def looks_ui(s):
    if not re.search(r'[A-Za-z]', re.sub(r'\{\w*\}', '', s)): return False   # placeholders only: "{icon}{name}", "{n}:"
    if re.fullmatch(r'\d[A-Z][A-Za-z]*\d?', s) or s.startswith('@{'): return False  # icon-set ids (3Arrows)
    if not re.search(r'[A-Za-z]', s) or len(s) < 2: return False
    if re.fullmatch(r'[a-z][A-Za-z0-9]*', s): return False      # icon names, ids
    if re.fullmatch(r'[a-z_][a-z0-9_.\-/]*', s): return False   # keys, paths
    if re.fullmatch(r'[A-Z0-9_.]+', s): return False             # function names, env vars
    if re.search(r'#,##|_-|h:mm|dddd|\[\$|^\{\w*\}\(?$|0\.00E', s): return False  # format codes, bare placeholders
    if s.startswith(('&', '-', '$')) or re.fullmatch(r'[A-Z]+\d+(:[A-Z]+\d+)?|[{}\w ]*\{\w*\} \{\w*\}', s): return False  # header codes, CLI flags, refs
    if re.fullmatch(r'[a-z]+/[\w.+-]+', s) or 'U+{' in s: return False  # MIME types, code points
    if s.startswith(('PivotStyle', 'TableStyle')): return False  # built-in style ids
    if s in ('Aptos', 'Aptos Display', 'AaBbCcYyZz') or s.startswith('select '): return False
    if s.startswith(('⌘', '⇧', '⌥', '⌃', '../')) or s in ('Jadwal  جدول', 'Copyright © 2026 Woosool'): return False  # shortcut keys, include_str paths, untranslated brand lines
    if s.startswith(('http', '#', '{:', 'i18n/')) or s.endswith(('.json', '.png', '.ttf')): return False
    return True

# Labels the app passes through variables, so the call scan can't see them.
EXTRA = [# File ▸ Keyboard Shortcuts (filemenu.rs SHORTCUTS, shown through tr()).
         'New workbook', 'Open', 'Save', 'Save As', 'Print', 'Close', 'Options', 'Undo / Redo', 'Copy / Cut / Paste', 'Paste Special', 'Bold / Italic / Underline', 'Select the current region, then all', 'Fill down / right', 'Find / Replace', 'Go To', 'Format Cells', 'Insert hyperlink', "Insert today's date / the time", 'Select column / row', 'Edit the active cell', 'Calculate now', 'Insert Function', 'Show formulas', 'Move', 'Jump to the edge of the data', 'Row start / A1 / last cell',
         'Waterfall', 'Histogram', 'Pareto', 'Box and Whisker', 'Treemap', 'Sunburst', 'Funnel', 'Hierarchy',
         'Statistic', 'Waterfall, Funnel', 'Line', 'changes', 'steps', 'Go to Header', 'Go to Footer',
         'YEARS', 'QUARTERS', 'MONTHS', 'DAYS',
         # Power Query labels the editor maps through tr() (types, aggregations, join kinds,
         # filter conditions, its own dialogs' titles).
         'Any', 'Whole Number', 'Decimal Number', 'Fixed Decimal Number', 'Percentage', 'Date/Time', 'True/False',
         'Sum', 'Average', 'Median', 'Min', 'Max', 'Count Rows', 'Count Distinct Rows',
         'Left Outer (all from first, matching from second)', 'Right Outer (all from second, matching from first)',
         'Full Outer (all rows from both)', 'Inner (only matching rows)', 'Left Anti (rows only in first)', 'Right Anti (rows only in second)',
         'equals', 'does not equal', 'is greater than', 'is greater than or equal to', 'is less than', 'is less than or equal to',
         'begins with', 'does not begin with', 'ends with', 'does not end with', 'contains', 'does not contain',
         'Keep Top Rows', 'Keep Bottom Rows', 'Remove Top Rows', 'Remove Bottom Rows', 'Replace Values', 'Split Column by Delimiter',
         'Split Column by Positions', 'Group By', 'Custom Column', 'Add Index Column', 'Merge', 'Append', 'Pivot Column', 'Merge Columns',
         'Rename', 'Choose Columns', 'Filter Rows', 'Advanced Editor', 'Rename Step', 'Rename Query', 'Edit Step', 'Number of rows']

def collect():
    found = set(EXTRA)
    for f in sorted(os.listdir(APP)):
        if not f.endswith('.rs') or f == 'tests.rs': continue
        src = open(os.path.join(APP, f), encoding='utf-8').read()
        src = re.sub(r'#\[cfg\(test\)\].*', '', src, flags=re.S)
        for m in CALL.finditer(src):
            name, a = m.group(1), args_of(src, m.end())
            lits = [unescape(x) for x in re.findall(LIT, a)]
            if name in ICON_ARG and lits and re.fullmatch(r'[a-z][A-Za-z0-9]*', lits[0]): lits = lits[1:]
            found.update(l for l in lits if looks_ui(l))
        # Arrays / tuples of labels iterated into tr(l): `["None", "Add"]` or `(X, "Label")`.
        for m in re.finditer(r'\[([^\[\]]*"[^\[\]]*)\]\s*(?:\.iter\(\)|\.into_iter\(\))?', src):
            body = m.group(1)
            tail = src[m.end():m.end() + 400]
            if 'tr(' in tail or 'trf(' in tail or 'menu_item' in tail or 'radio_value' in tail or 'section' in tail:
                found.update(l for l in map(unescape, re.findall(LIT, body)) if looks_ui(l))
    # Category tables (app function categories, engine number-format categories) and engine
    # error messages the app shows through tr().
    for d, f in [(APP, f) for f in sorted(os.listdir(APP))] + [(ENGINE, f) for f in sorted(os.listdir(ENGINE))]:
        if not f.endswith('.rs') or f == 'tests.rs': continue
        src = open(os.path.join(d, f), encoding='utf-8').read()
        src = re.sub(r'#\[cfg\(test\)\].*', '', src, flags=re.S)
        # Category tables: `("General", &[formats…])` — the first element is the label.
        for m in re.finditer(r'CATEGOR\w*\s*:[^=]*=\s*&?\[(.*?)\n\];', src, re.S):
            found.update(unescape(l) for l in re.findall(r'\(\s*' + LIT + r'\s*,', m.group(1)))
        for m in re.finditer(r'\bErr\(\s*(?:format!\()?' + LIT, src):
            s = unescape(m.group(1))
            if looks_ui(s) and ' ' in s: found.add(s)
    # Sample scripts' names and descriptions, and the app's message constants.
    sp = os.path.join(ENGINE, 'scripts.rs')
    if os.path.exists(sp):
        for m in re.finditer(r'\bs\(\s*' + LIT + r'\s*,\s*' + LIT, open(sp, encoding='utf-8').read()):
            found.update(map(unescape, m.groups()))
    for f in sorted(os.listdir(APP)):
        if f.endswith('.rs') and f != 'tests.rs':
            for m in re.finditer(r'\bconst [A-Z_]+: &str = ' + LIT + ';', open(os.path.join(APP, f), encoding='utf-8').read()):
                v = unescape(m.group(1))
                if looks_ui(v) and ' ' in v and v[0].isupper(): found.add(v)
    # Indirection: labels kept in const arrays, (id, "Label"[, "Description"]) tables (Tell Me
    # commands, Protect Sheet actions, templates, shapes), (n, "Label", …) status items, and
    # describe()-style match arms that return messages.
    for d in (APP, ENGINE):
        for f in sorted(os.listdir(d)):
            if not f.endswith('.rs') or f in ('tests.rs', 'fnhelp.rs'): continue
            src = re.sub(r'#\[cfg\(test\)\].*', '', open(os.path.join(d, f), encoding='utf-8').read(), flags=re.S)
            for m in re.finditer(r'\bconst [A-Z_]+: (?:\[&str; \d+\]|&\[&str\]) = &?\[(.*?)\];', src, re.S):
                found.update(l for l in map(unescape, re.findall(LIT, m.group(1))) if looks_ui(l) and not l.startswith('Noto '))
            for m in re.finditer(r'\(\s*"[a-z][A-Za-z0-9]*"\s*,\s*' + LIT + r'(?:\s*,\s*' + LIT + r')?', src):
                for l in m.groups():
                    if l and looks_ui(unescape(l)) and l[0].isupper(): found.add(unescape(l))
            for m in re.finditer(r'\(\s*\d+\s*,\s*' + LIT + r'\s*,', src):
                l = unescape(m.group(1))
                if looks_ui(l) and l[0].isupper(): found.add(l)
            for fn in re.finditer(r'fn (?:describe|message|label|name|title)\w*\([^)]*\)\s*->\s*&\'static str\s*\{(.*?)\n\}', src, re.S):
                for l in map(unescape, re.findall(r'=>\s*' + LIT, fn.group(1))):
                    if looks_ui(l) and l[0].isupper(): found.add(l)
    # Ribbon labels broken where Excel breaks them ("Insert\nFunction") are translated as one line.
    found = {re.sub(r'\n', ' ', k) if (k.count('\n') == 1 and len(k) < 32) else k for k in found} if isinstance(found, set) else found
    return found

if __name__ == '__main__':
    have = json.load(open(os.path.join(I18N, 'fr.json'), encoding='utf-8'))
    have_ar = json.load(open(os.path.join(I18N, 'ar.json'), encoding='utf-8'))
    strings = collect()
    out = sorted(strings) if '--all' in sys.argv else sorted(s for s in strings if s not in have or s not in have_ar)
    print(json.dumps(out, ensure_ascii=False, indent=1))
    print(f'{len(strings)} strings, {len(out)} listed', file=sys.stderr)
