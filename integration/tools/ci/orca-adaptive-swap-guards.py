#!/usr/bin/env python3
"""Prove new swap replay guards work in normal and Python -OO modes."""
import argparse
import ast
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
ROOT=Path(__file__).resolve().parents[3]
GOLDEN=ROOT/'integration/tools/ci/orca-adaptive-swap-golden.py'
REFERENCE=ROOT/'integration/tools/reference/orca-adaptive-swap'
BINARY=Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-swap-reference')

def require(condition,message):
    if not condition:raise RuntimeError(str(message))

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--startup-guard-test',action='store_true');args=parser.parse_args()
    for path in [GOLDEN,Path(__file__)]:
        require(not any(isinstance(n,ast.Assert) for n in ast.walk(ast.parse(path.read_text()))),f'optimization-removable guard: {path}')
    original=json.loads((ROOT/'dex-router/test/contracts/orca-adaptive-swap.json').read_text())
    results=[]
    with tempfile.TemporaryDirectory(prefix='orca-adaptive-swap-guards-') as tmp:
        path=Path(tmp)/'corpus.json'
        for mode in [[],['-OO']]:
            base=[sys.executable,*mode,str(GOLDEN),'--check']
            ok=subprocess.run(base,cwd=ROOT,text=True,capture_output=True,timeout=60)
            require(ok.returncode==0,ok.stderr)
            for mutation in ['observation','dependency-closure','source-closure','delete-failures','delete-sequences','short-sequence','duplicate-case','rename-case']:
                data=copy.deepcopy(original)
                if mutation=='observation':data['cases'][0]['expected']['amount_out']+=1
                elif mutation=='dependency-closure':data['cargo_dependencies'][0]['version']='invalid-closure'
                elif mutation=='source-closure':data['identity']['source_inventory_sha256']='0'*64
                elif mutation=='delete-failures':data['failures']=[]
                elif mutation=='delete-sequences':data['sequences']=[]
                elif mutation=='short-sequence':data['sequences'][0]['trades'].pop()
                elif mutation=='duplicate-case':data['cases'][1]=copy.deepcopy(data['cases'][0])
                else:data['cases'][0]['name']='renamed-case'
                path.write_text(json.dumps(data))
                failed=subprocess.run([*base,'--corpus',str(path),'--router-corpus',str(path)],cwd=ROOT,
                    text=True,capture_output=True,timeout=60)
                require(failed.returncode!=0 and 'RuntimeError:' in failed.stderr,f'corrupt {mutation} accepted in {mode}')
                results.append({'mode':'OO' if mode else 'normal','mutation':mutation,'rejected':True})
    if args.startup_guard_test:
        lock=REFERENCE/'Cargo.lock';content=lock.read_bytes();timestamps=lock.stat()
        try:
            lock.write_bytes(content+b'\n# standalone native closure mismatch probe\n')
            observed=subprocess.run([str(BINARY)],input='{"action":"identity"}\n',cwd=ROOT,
                text=True,capture_output=True,timeout=30)
            require(observed.returncode!=0 and not observed.stdout,'native startup accepted a changed lock')
            require('reference closure changed after compilation' in observed.stderr,'unexpected startup failure')
        finally:
            lock.write_bytes(content);os.utime(lock,ns=(timestamps.st_atime_ns,timestamps.st_mtime_ns))
        require(lock.read_bytes()==content,'lock was not restored')
        results.append({'native_startup_closure':'rejected','lock_restored':True})
    print(json.dumps({'positive_modes':2,'negative_checks':results},sort_keys=True))
if __name__=='__main__':main()
