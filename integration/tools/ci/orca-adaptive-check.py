#!/usr/bin/env python3
"""Build/verify the isolated native oracle and run the assigned kernel tests.

Only --startup-guard-test temporarily appends whitespace to this sidecar's NEW
Cargo.lock, restores its exact bytes, and proves the compiled oracle fails closed.
No upstream files, existing harnesses, profiles or Cargo manifests are modified.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT/'integration/tools/reference/orca-adaptive'
TOOLCHAIN = Path.home()/'.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
NATIVE_TARGET = Path('/tmp/xone-orca-adaptive-native')
BINARY = NATIVE_TARGET/'debug/orca-adaptive-reference'

def require(condition, message):
    if not condition:
        raise ValueError(message)

def run(command, **kwargs):
    subprocess.run([str(x) for x in command], cwd=ROOT, check=True, **kwargs)

def startup_guard():
    lock = REF/'Cargo.lock'
    original = lock.read_bytes()
    timestamps = lock.stat()
    try:
        lock.write_bytes(original+b'\n# adaptive reference closure mismatch probe\n')
        result = subprocess.run([str(BINARY)], input='{"action":"identity"}\n',text=True,
            capture_output=True,cwd=ROOT,timeout=30)
        require((result.returncode != 0 and not result.stdout), 'oracle accepted a modified build closure')
        require(('reference closure changed after compilation' in result.stderr), 'Reference validation failed')
    finally:
        lock.write_bytes(original)
        os.utime(lock, ns=(timestamps.st_atime_ns,timestamps.st_mtime_ns))
    require((hashlib.sha256(lock.read_bytes()).digest() == hashlib.sha256(original).digest()), 'Reference validation failed')
    print(json.dumps({'startup_closure_mismatch':'rejected','cargo_lock_restored':True}),flush=True)

def main():
    parser=argparse.ArgumentParser()
    parser.add_argument('--standalone',action='store_true',help='use test-only error boundary adapter during concurrent domain edits')
    parser.add_argument('--startup-guard-test',action='store_true')
    args=parser.parse_args()
    env=dict(os.environ);env['PATH']=str(TOOLCHAIN)+os.pathsep+env['PATH']
    env['PYTHONDONTWRITEBYTECODE']='1'
    run(['cargo','build','--locked','--offline','--manifest-path',REF/'Cargo.toml','--target-dir',NATIVE_TARGET],env=env)
    run([sys.executable,ROOT/'integration/tools/ci/orca-adaptive-golden.py','--check','--binary',BINARY],env=env)
    if args.startup_guard_test: startup_guard()
    manifest=REF/'standalone/Cargo.toml' if args.standalone else ROOT/'dex-router/Cargo.toml'
    command=['cargo','test','--locked','--offline','--manifest-path',manifest]
    if not args.standalone:command+=['-p','xone-protocol-math']
    command+=['--test','orca_adaptive','--target-dir',Path('/tmp/xone-orca-adaptive-standalone' if args.standalone else '/tmp/xone-orca-adaptive-router')]
    run(command,env=env)
if __name__=='__main__':main()
