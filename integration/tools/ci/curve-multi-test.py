#!/usr/bin/env python3
"""Bounded standalone compiler fallback while shared domain/proto edits are in flight.
Links the real previously built xone_domain rlib, not a replacement error module.
Prefer Cargo targets when the shared schema build is coherent.
"""
from pathlib import Path
import subprocess,sys
root=Path(__file__).resolve().parents[3]
deps=root/'dex-router/target/debug/deps'
out=root/'integration/tools/reference/curve-models/.runtime'
out.mkdir(exist_ok=True)
family=sys.argv[1] if len(sys.argv)>1 else 'multi'
rustc=Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc'
test=root/f'dex-router/crates/protocol-math/tests/curve_{family}_standalone.rs'
cmd=[str(rustc),'--edition=2021','--test',str(test),'-L','dependency='+str(deps),'-o',str(out/f'{family}-tests')]
# NG's standalone target also exercises the published NG API after service integration.
names = ['num_bigint','num_traits','xone_domain','serde_json']
if family == 'ng':
    names.append('xone_protocol_math')
for name in names:
    p=max(deps.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime)
    cmd+=['--extern',name+'='+str(p)]
subprocess.run(cmd,check=True)
raise SystemExit(subprocess.run([str(out/f'{family}-tests'),'--nocapture']).returncode)
