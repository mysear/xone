#!/usr/bin/env python3
"""Commit the finite local artifact bytes; no network access."""
import hashlib
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parent
rows = []
for p in sorted(ROOT.rglob('*')):
    if not p.is_file() or p.name in ('manifest.json', 'validation.log') or '__pycache__' in p.parts:
        continue
    raw = p.read_bytes()
    rows.append({'path': p.relative_to(ROOT).as_posix(), 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()})
(ROOT / 'manifest.json').write_text(json.dumps({'schema_version': 1, 'scope': 'finite local public vectors and fixed primary source snapshots', 'files': rows}, indent=2) + '\n')
print('manifest files', len(rows))
