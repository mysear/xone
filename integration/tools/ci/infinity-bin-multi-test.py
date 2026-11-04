#!/usr/bin/env python3
"""Isolated kernel tests using the exact production Amount/Error source slice.

The workspace domain/proto may be undergoing unrelated edits. This runner does
not claim workspace compilation or manager integration. No source is rewritten.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/infinity-bin-multi'

def require(condition, message):
    if not condition:
        raise ValueError(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def main():
    receipt = REF / 'test-receipt.json'
    receipt.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    try:
        # Authenticate the current native corpus before compiling the sidecar.
        subprocess.run([sys.executable, str(ROOT / 'integration/tools/ci/infinity-bin-multi-check.py')], check=True)
        tools = {name: subprocess.check_output(
            [str(Path.home() / '.cargo/bin/rustup'), 'which', '--toolchain', '1.90.0', name], text=True).strip()
            for name in ('cargo', 'rustc', 'rustdoc')}
        source = (ROOT / 'dex-router/crates/domain/src/lib.rs').read_text()
        # Exact production scalar implementation; all imports used by this slice.
        scalar = source[source.index('pub type Result<T>'):source.index('pub fn hash<T:')]
        inputs = [Path(__file__).relative_to(ROOT), Path('integration/tools/ci/infinity-bin-multi-check.py'),
                  Path('integration/tools/ci/infinity-bin-multi-golden.cjs'),
                  Path('integration/tools/reference/infinity-bin-multi/InfinityBinMultiReference.sol'),
                  Path('integration/tools/reference/infinity-bin-multi/source-metadata.json'),
                  Path('integration/tools/reference/infinity-bin-multi/execution-receipt.json'),
                  Path('dex-router/crates/domain/src/lib.rs'),
                  Path('dex-router/crates/protocol-math/src/infinity_bin.rs'),
                  Path('dex-router/crates/protocol-math/src/infinity_bin_multi.rs'),
                  Path('dex-router/crates/protocol-math/tests/infinity_bin_multi.rs')]
        fixture = ROOT / 'dex-router/test/contracts/infinity-bin-multi.json'
        if fixture.exists():
            inputs.extend([fixture.relative_to(ROOT), Path('integration/test/contracts/infinity-bin-multi.json')])
            require((fixture.read_bytes() == (ROOT / inputs[-1]).read_bytes()), 'Reference validation failed')
        hashes = {str(p): sha((ROOT / p).read_bytes()) for p in inputs}
        with tempfile.TemporaryDirectory(prefix='infinity-bin-multi-') as temp:
            temp = Path(temp)
            (temp / 'domain/src').mkdir(parents=True)
            (temp / 'src').mkdir()
            (temp / 'domain/Cargo.toml').write_text('''[package]
name="xone-domain"
version="0.0.0"
edition="2021"
[dependencies]
num-bigint="0.4"
num-traits="0.2"
serde={version="1",features=["derive"]}
thiserror="2"
''')
            (temp / 'domain/src/lib.rs').write_text('use num_bigint::BigUint;\nuse num_traits::Zero;\nuse serde::{Serialize,Serializer,Deserialize,Deserializer};\nuse std::{fmt,str::FromStr};\n' + scalar)
            (temp / 'Cargo.toml').write_text('''[package]
name="xone-protocol-math"
version="0.0.0"
edition="2021"
[dependencies]
xone-domain={path="domain"}
num-bigint="0.4"
num-traits="0.2"
serde={version="1",features=["derive"]}
serde_json="1"
[[test]]
name="infinity_bin_multi"
path=''' + json.dumps(str(ROOT / 'dex-router/crates/protocol-math/tests/infinity_bin_multi.rs')) + '\n')
            (temp / 'src/lib.rs').write_text('#[path=' + json.dumps(str(ROOT / 'dex-router/crates/protocol-math/src/infinity_bin.rs')) + ']\npub mod infinity_bin;\n')
            lock = REF / 'test-dependencies.lock'
            if lock.exists():
                (temp / 'Cargo.lock').write_bytes(lock.read_bytes())
            env = dict(os.environ, RUSTC=tools['rustc'], RUSTDOC=tools['rustdoc'],
                       CARGO_TARGET_DIR=str(ROOT / 'integration/.runtime/infinity-bin-multi-target'))
            command = [tools['cargo'], 'test', '--offline', '--manifest-path', str(temp / 'Cargo.toml'), '--test', 'infinity_bin_multi', '--', '--nocapture']
            if lock.exists():
                command.insert(command.index('--offline'), '--locked')
            run = subprocess.run(command, env=env)
            require((run.returncode == 0), f'kernel tests failed: {run.returncode}')
            require((hashes == {str(p): sha((ROOT / p).read_bytes()) for p in inputs}), 'inputs changed during tests')
            lock_bytes = (temp / 'Cargo.lock').read_bytes()
            if lock.exists():
                require((lock.read_bytes() == lock_bytes), 'Reference validation failed')
            else:
                lock.write_bytes(lock_bytes)
            receipt.write_text(json.dumps({'passed': True, 'scope': 'isolated production scalar slice and kernel; no service wiring',
                'inputs': hashes, 'scalar_sha256': sha(scalar.encode()),
                'tools': {k: subprocess.check_output([v, '--version'], text=True).strip() for k, v in tools.items()},
                'resolved_lock_sha256': sha(lock_bytes)}, indent=2) + '\n')
    except BaseException as error:
        receipt.write_text(json.dumps({'passed': False, 'error': str(error)}) + '\n')
        raise

if __name__ == '__main__':
    main()
