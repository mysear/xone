#!/usr/bin/env python3
"""Independent complete source/runtime inventory for the pinned local ramp oracle.

Membership is derived from fixed compiler roots and the full Node package tree,
rather than trusting producer-supplied lists of which dependencies were loaded.
"""
import hashlib
import json
import os
import shutil
import subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
ORIGINAL = ROOT / 'integration/tools/reference/curve-crypto-full'
SOURCE_MANIFEST_SHA256 = '0d83e10c84b7789258002dfb20be7f3992bb2bceda99d7e0e347db62af4fdd5d'
NODE_SUFFIXES = {'.js', '.cjs', '.json', '.wasm', '.node', '.mjs'}
COMPILER_SUFFIXES = {'.py', '.pyc', '.pth', '.so', '.dylib', '.zip', '.json', '.txt'}

def require(ok, message):
    if not ok:
        raise ValueError(message)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def pins():
    path = ORIGINAL / 'sources.json'
    require(sha(path) == SOURCE_MANIFEST_SHA256, 'ramp mandatory pinned source inventory changed')
    values = json.loads(path.read_text())
    require(len(values) == 5 and len({p['local'] for p in values}) == 5, 'ramp pinned sources omitted or duplicated')
    for pin in values:
        require(sha(ORIGINAL / pin['local']) == pin['sha256'], 'ramp original compiled source changed')
    return values

def compiler_roots():
    return [ROOT / 'integration/tools/reference/curve-models/.runtime/vyper-0.3.3', ORIGINAL / '.runtime/vyper-0.3.1']

def expected_paths():
    source_pins = pins()
    node = shutil.which('node')
    require(node is not None, 'ramp Node executable missing')
    paths = {Path(node), ORIGINAL / 'sources.json', ROOT / 'integration/.runtime/stable-reference/package-lock.json', Path(__file__), ROOT / 'integration/tools/ci/curve-crypto-ramp-golden.cjs'}
    paths.update(ORIGINAL / pin['local'] for pin in source_pins)
    module_root = ROOT / 'integration/.runtime/stable-reference/node_modules'
    require(module_root.is_dir(), 'ramp Node package tree missing')
    for directory, folders, names in os.walk(module_root):
        folders[:] = sorted(d for d in folders if d not in {'__pycache__', '.cache'})
        for name in sorted(names):
            path = Path(directory) / name
            if path.suffix in NODE_SUFFIXES and path.is_file():
                paths.add(path)
    for compiler in compiler_roots():
        python = compiler / 'bin/python'
        paths.update([python, compiler / 'bin/vyper', compiler / 'pyvenv.cfg'])
        require('include-system-site-packages = false' in (compiler / 'pyvenv.cfg').read_text().lower(), 'ramp compiler global site packages enabled')
        # -I/-S avoids executing site hooks just to discover the interpreter's
        # own standard library. Actual compiler hooks/cache are all hashed.
        info = json.loads(subprocess.run([str(python), '-I', '-S', '-B', '-c', "import sys,sysconfig,json; print(json.dumps({'paths':sys.path,'stdlib':sysconfig.get_path('stdlib'),'platstdlib':sysconfig.get_path('platstdlib')}))"], text=True, capture_output=True, check=True).stdout)
        roots = {compiler, *[Path(value) for value in info['paths'] if value], Path(info['stdlib']), Path(info['platstdlib'])}
        for scope in sorted(roots):
            if scope.is_file():
                paths.add(scope)
                continue
            if not scope.is_dir():
                continue
            for directory, folders, names in os.walk(scope):
                # A venv disallows global site packages; its own package tree
                # and every executable bytecode cache/startup hook remain in scope.
                folders[:] = sorted(d for d in folders if d != 'site-packages' or (Path(directory) / d).is_relative_to(compiler))
                for name in sorted(names):
                    path = Path(directory) / name
                    if path.is_file() and path.suffix in COMPILER_SUFFIXES:
                        paths.add(path)
    normalized = {path.resolve(strict=True) for path in paths}
    require(all(path.is_file() for path in normalized), 'ramp dependency is not a regular file')
    return sorted(normalized)

def expected_inputs():
    return {str(path): sha(path) for path in expected_paths()}

if __name__ == '__main__':
    print(json.dumps(expected_inputs(), sort_keys=True))
