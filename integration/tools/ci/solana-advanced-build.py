#!/usr/bin/env python3
"""Build only the two independent pinned native reward oracles (no workspace build)."""
import argparse
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / 'integration/tools/reference/solana-advanced'


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--online', action='store_true', help='Allow Cargo registry access; sources remain pinned')
    args = parser.parse_args()
    tools = {tool: subprocess.check_output(['rustup', 'which', '--toolchain', '1.90.0', tool], text=True).strip()
             for tool in ['cargo', 'rustc', 'rustdoc']}
    for family in ['orca', 'raydium-clmm']:
        manifest = BASE / family / 'Cargo.toml'
        target = ROOT / 'integration/tools/reference/orca-whirlpools/target' if family == 'orca' else Path('/tmp/xone-solana-advanced-ray')
        env = dict(os.environ, RUSTC=tools['rustc'], RUSTDOC=tools['rustdoc'], CARGO_TARGET_DIR=str(target))
        command = [tools['cargo'], 'build', '--manifest-path', str(manifest), '--locked']
        if not args.online:
            command.append('--offline')
        if family == 'orca':
            command.append('--release')
        else:
            command.extend(['--config', str(BASE / family / 'cargo-source.toml')])
        log = BASE / family / 'build.log'
        with log.open('w') as output:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=output, stderr=subprocess.STDOUT)
        if result.returncode:
            raise SystemExit(f'{family} native oracle build failed; see {log}')
        print(f'{family}: built pinned native oracle')


if __name__ == '__main__':
    main()
