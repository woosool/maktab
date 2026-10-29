#!/usr/bin/env python3
"""Builds engine/src/geo/regions.bin from Natural Earth 1:110m (public domain) GeoJSON:
   python3 tools/make-geo.py ne_110m_admin_0_countries.geojson ne_110m_admin_1_states_provinces.geojson
Format (little endian): b"JGEO", u16 count; per region: u8 kind (0 country, 1 US state),
u8 len + names (';'-separated, first is the display name), u16 rings; per ring: u16 n, then n
points as i16 lon*100, i16 lat*100 (simplified with Douglas-Peucker, tolerance 0.15 degree)."""
import json, struct, sys, os

def dp(pts, tol):
    if len(pts) < 4: return pts
    a, b = pts[0], pts[-1]
    best, bi = -1, 0
    for i in range(1, len(pts) - 1):
        p = pts[i]
        dx, dy = b[0] - a[0], b[1] - a[1]
        L = (dx * dx + dy * dy) ** 0.5
        d = abs(dy * p[0] - dx * p[1] + b[0] * a[1] - b[1] * a[0]) / L if L else ((p[0]-a[0])**2 + (p[1]-a[1])**2) ** 0.5
        if d > best: best, bi = d, i
    if best <= tol: return [a, b]
    return dp(pts[:bi + 1], tol)[:-1] + dp(pts[bi:], tol)

def rings(geom):
    polys = geom['coordinates'] if geom['type'] == 'MultiPolygon' else [geom['coordinates']]
    out = []
    for poly in polys:
        r = dp(poly[0], 0.15)
        if len(r) >= 4: out.append(r)
    return out

def main():
    regions = []
    for f in json.load(open(sys.argv[1]))['features']:
        p = f['properties']
        names = []
        for k in ('NAME', 'NAME_LONG', 'ADMIN', 'FORMAL_EN', 'ABBREV', 'ISO_A3', 'ISO_A2', 'ADM0_A3'):
            v = p.get(k)
            if v and v != '-99' and v not in names: names.append(v)
        regions.append((0, names, rings(f['geometry'])))
    if len(sys.argv) > 2:
        for f in json.load(open(sys.argv[2]))['features']:
            p = f['properties']
            if p.get('adm0_a3') != 'USA': continue
            names = [p['name'], p['postal'], p['iso_3166_2']]
            regions.append((1, names, rings(f['geometry'])))
    out = bytearray(b'JGEO') + struct.pack('<H', len(regions))
    for kind, names, rs in regions:
        n = ';'.join(names).encode('utf-8')[:255]
        out += struct.pack('<BB', kind, len(n)) + n + struct.pack('<H', len(rs))
        for r in rs:
            out += struct.pack('<H', len(r))
            for x, y in r: out += struct.pack('<hh', round(x * 100), round(y * 100))
    dst = os.path.join(os.path.dirname(__file__), '..', 'engine', 'src', 'geo', 'regions.bin')
    open(dst, 'wb').write(out)
    print(len(regions), 'regions,', len(out), 'bytes')

main()
