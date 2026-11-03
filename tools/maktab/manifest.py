#!/usr/bin/env python3
"""Write dist/maktab/<slug>-update.json for each app's updater (app/src/update.rs) from the
update archives in dist/maktab (<App>-<version>-<os>-<arch>.zip).
Usage: tools/maktab/manifest.py [base-url] [notes-file]"""
import hashlib, json, pathlib, re, sys

here = pathlib.Path(__file__).resolve().parent
dist = here.parent.parent / "dist" / "maktab"
base = (sys.argv[1] if len(sys.argv) > 1 else "https://woosool.com/maktab").rstrip("/")
notes = pathlib.Path(sys.argv[2]).read_text() if len(sys.argv) > 2 else ""
written = 0
for line in (here / "apps.tsv").read_text(encoding="utf-8").splitlines():
    if not line.strip() or line.startswith("#"):
        continue
    slug, _dir, _pkg, _bin, name = line.split("|")[:5]
    downloads, version = {}, None
    for z in sorted(dist.rglob(f"{name}-*.zip")):
        m = re.match(rf"{name}-(.+)-(macos|windows|linux)-(\w+)\.zip$", z.name)
        if not m:
            continue
        version = m.group(1)
        downloads[f"{m.group(2)}-{m.group(3)}"] = {"url": f"{base}/{z.name}", "sha256": hashlib.sha256(z.read_bytes()).hexdigest()}
    if not downloads:
        continue
    out = {"version": version, "notes": notes, "page": f"https://woosool.com/en/products/{slug}", "downloads": downloads}
    (dist / f"{slug}-update.json").write_text(json.dumps(out, indent=2, ensure_ascii=False) + "\n")
    written += 1
    print(f"{slug}: {version} for {', '.join(downloads)}")
if not written:
    sys.exit("No update archives in dist/maktab")
