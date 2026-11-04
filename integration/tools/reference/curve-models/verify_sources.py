#!/usr/bin/env python3
"""Verify vendored original source digests, optionally fetch exact missing pins."""
import argparse, hashlib, json, urllib.request
from pathlib import Path
root=Path(__file__).resolve().parent
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--fetch-missing',action='store_true')
args=parser.parse_args()
rows=json.loads((root/'sources.json').read_text())
for row in rows:
    p=root/row['local']
    if not p.exists() and args.fetch_missing:
        url=f"https://raw.githubusercontent.com/{row['repo']}/{row['revision']}/{row['path']}"
        data=urllib.request.urlopen(url,timeout=40).read()
        if hashlib.sha256(data).hexdigest()!=row['sha256']:raise SystemExit('Downloaded digest mismatch: '+row['local'])
        p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
    if hashlib.sha256(p.read_bytes()).hexdigest()!=row['sha256']:raise SystemExit('Source digest mismatch: '+row['local'])
print(json.dumps({'official_sources_verified':len(rows),'revisions':sorted(set(r['revision'] for r in rows))}))
