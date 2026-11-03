#!/usr/bin/env python3
"""Write dist/update.json for the update check (app/src/update.rs) from the update archives in
dist/ (Jadwal-<version>-<os>-<arch>.zip). Usage: tools/manifest.py [base-url] [notes-file]"""
import hashlib, json, pathlib, re, sys

dist = pathlib.Path(__file__).resolve().parent.parent / "dist"
base = (sys.argv[1] if len(sys.argv) > 1 else "https://woosool.com/jadwal").rstrip("/")
notes = pathlib.Path(sys.argv[2]).read_text() if len(sys.argv) > 2 else ""
downloads, version = {}, None
for z in sorted(dist.rglob("Jadwal-*.zip")):
    m = re.match(r"Jadwal-(.+)-(macos|windows|linux)-(\w+)\.zip$", z.name)
    if not m:
        continue
    version = m.group(1)
    downloads[f"{m.group(2)}-{m.group(3)}"] = {"url": f"{base}/{z.name}", "sha256": hashlib.sha256(z.read_bytes()).hexdigest()}
if not downloads:
    sys.exit("No update archives in dist/")
out = {"version": version, "notes": notes, "page": "https://woosool.com/en/products/jadwal", "downloads": downloads}
(dist / "update.json").write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")
print(json.dumps(out, indent=2, ensure_ascii=False))
