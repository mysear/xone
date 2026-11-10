#!/usr/bin/env python3
"""Verify exact public Agave v4.3.0 inputs and check/apply the native source hook."""
import argparse, hashlib, json, pathlib, subprocess
p=argparse.ArgumentParser();p.add_argument('--agave-root',required=True,type=pathlib.Path);p.add_argument('--apply',action='store_true');args=p.parse_args()
here=pathlib.Path(__file__).resolve().parent;root=args.agave_root.resolve();source=here.parent.resolve()
pins=json.loads((here/'source-pin.json').read_text())
if hashlib.sha256((here/'native-observer.patch').read_bytes()).hexdigest()!=pins['patch_sha256']:
    raise SystemExit('Refusing changed native observer patch')
for record in pins['files']:
    path=root/record['path']
    if hashlib.sha256(path.read_bytes()).hexdigest()!=record['upstream_sha256']:
        raise SystemExit(f"Refusing non-pinned Agave input: {record['path']}")
patch=(here/'native-observer.patch').read_text().replace('__XONE_NATIVE_SOURCE__',str(source).replace('\\','\\\\').replace('"','\\"'))
command=['patch','--batch','-p1']
if not args.apply:command.append('--dry-run')
subprocess.run(command,cwd=root,input=patch,text=True,check=True)
print('Agave v4.3.0 native observer: '+('applied' if args.apply else 'exact inputs and patch check passed'))
