#!/usr/bin/env python3
"""Run only the Curve NG state target using real already-built dependencies."""
from pathlib import Path
import subprocess
import tempfile

root = Path(__file__).resolve().parents[3]
deps = root / 'dex-router/target/debug/deps'
rustc = Path.home() / '.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin/rustc'
with tempfile.TemporaryDirectory(prefix='curve-ng-state-') as tmp:
    binary = Path(tmp) / 'state-tests'
    cmd = [str(rustc), '--edition=2021', '--test',
           str(root / 'dex-router/crates/protocol-math/tests/curve_ng_state.rs'),
           '-L', 'dependency=' + str(deps), '-o', str(binary)]
    for name in ['num_bigint', 'num_traits', 'xone_domain', 'serde_json', 'sha2']:
        lib = max(deps.glob('lib' + name + '-*.rlib'), key=lambda p: p.stat().st_mtime)
        cmd += ['--extern', name + '=' + str(lib)]
    subprocess.run(cmd, check=True)
    raise SystemExit(subprocess.run([str(binary), '--nocapture']).returncode)
