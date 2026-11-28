#!/usr/bin/env python3
"""Project the original MetaUSD calls and runtime dependencies into service fixtures."""

import json
import sys
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))

from curve_meta_fixture import NAMES, code_bytes, dependency_code, implementation_code, original_scenario, profile


def write_fixture(services, filename, value):
    text = json.dumps(value, indent=2) + '\n'
    for service in services:
        (ROOT / service / 'test/contracts' / filename).write_text(text, encoding='ascii')


def main():
    cases = []
    runtimes = {}
    for name in NAMES:
        original = original_scenario(name)
        cases.extend(profile(name, index) for index in range(len(original['steps'])))
        codes = {'implementation': implementation_code(), 'clone': code_bytes(name)}
        for dependency, field in (('token0', 'token0'), ('token1', 'token1'), ('base', 'base_pool')):
            codes[dependency] = dependency_code(name, original[field])
        runtimes[name] = {role: '0x' + code.hex() for role, code in codes.items()}
    write_fixture(('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router'),
                  'curve-meta-service.json', {'schema_version': 1, 'cases': cases})
    write_fixture(('integration', 'chain-parser-go'), 'curve-meta-runtime.json', runtimes)
    print(f'{len(cases)} original MetaUSD calls and {len(runtimes)} runtime sets')


if __name__ == '__main__':
    main()
