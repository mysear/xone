#!/usr/bin/env python3
"""Bounded standalone math tests using compatible, already built Cargo dependencies.
Avoids rebuilding shared domain/proto while concurrent owners change their schema.
No mock domain: links the actual prebuilt xone_domain rlib. Prints artifact hashes.
Ordinary integration remains: cargo test -p xone-protocol-math --test balancer_multi*.
"""
import argparse,hashlib,json,os,subprocess,tempfile
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
p=argparse.ArgumentParser();p.add_argument('tests',nargs='*',default=['balancer_multi','balancer_multi_differential']);p.add_argument('--rustc',default=str(Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc'));args=p.parse_args()
deps=ROOT/'dex-router/target/debug/deps'; extern=[]; hashes={}
for name in ['num_bigint','num_traits','serde_json','sha2','xone_domain','xone_protocol_math']:
    artifact=max(deps.glob('lib'+name+'-*.rlib'),key=lambda p:p.stat().st_mtime)
    extern+=['--extern',name+'='+str(artifact)]
    hashes[str(artifact.relative_to(ROOT))]=hashlib.sha256(artifact.read_bytes()).hexdigest()
with tempfile.TemporaryDirectory(prefix='balancer-multi-tests-') as temp:
    for test in args.tests:
        if not test.startswith(('balancer_multi','balancer_composable')):raise ValueError('Out-of-scope test')
        source=ROOT/'dex-router/crates/protocol-math/tests'/(test+'.rs')
        subprocess.run([args.rustc,'--test','--edition=2021',str(source),'-L','dependency='+str(deps),'-o',str(Path(temp)/test),*extern],check=True)
        subprocess.run([str(Path(temp)/test),'--nocapture'],check=True)
print(json.dumps({'passed':True,'mode':'standalone tests with actual prebuilt domain dependency; no shared-schema rebuild','rustc':subprocess.check_output([args.rustc,'--version'],text=True).strip(),'dependencies':hashes,'tests':args.tests},indent=2))
