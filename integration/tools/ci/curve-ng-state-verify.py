#!/usr/bin/env python3
"""Fail-closed offline verification of original NG state execution evidence.

Use --recompile to repeat the pinned Vyper compilation and compare the complete
artifact. No EVM startup or corpus rewrites occur in this verifier.
"""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/curve-ng-state'
MODEL = ROOT / 'integration/tools/reference/curve-models'
NODE = ROOT / 'integration/.runtime/stable-reference/node_modules'
sha = lambda b: hashlib.sha256(b).hexdigest()


def check(condition, message):
    if not condition:
        raise ValueError(message)


def verify():
    closure = json.loads((REF / 'execution-closure.json').read_text())
    check(closure.get('generation_complete') is True, 'No completed generation receipt')
    pin = closure['source_pin']
    check(pin['repo'] == 'curvefi/stableswap-ng' and
          pin['revision'] == '3332cd44656ec64b3c048885e1dd71955254b262' and
          pin['path'] == 'contracts/main/CurveStableSwapNG.vy' and
          pin['sha256'] == 'dfe8f8a4dda288c41c630ea4be9bb0e5d61244c50dfdeaf1981a098b55ee12ab',
          'Original source pin changed')
    check(sha((MODEL / pin['local']).read_bytes()) == pin['sha256'], 'Original source bytes changed')
    generator = ROOT / 'integration/tools/ci/curve-ng-state-golden.cjs'
    check(sha(generator.read_bytes()) == closure['generator_sha256'], 'Generator changed since execution')
    fixture = ROOT / 'integration/test/contracts/curve-ng-state.json'
    data = fixture.read_bytes()
    check(sha(data) == closure['fixture_sha256'], 'Fixture does not match completed generation')
    check(data == (ROOT / 'dex-router/test/contracts/curve-ng-state.json').read_bytes(), 'Fixture copies differ')
    rows = json.loads(data)
    artifact = json.loads((REF / 'compiler-artifact.json').read_text())
    layout = json.loads((REF / 'compiler-layout.json').read_text())
    check(layout == artifact['layout'], 'Compiler layout copies differ')
    check(sha(json.dumps(layout, separators=(',', ':')).encode()) == closure['layout_sha256'], 'Compiler layout changed')
    template = bytes.fromhex(artifact['runtime'][2:])
    check(len(template) == 21491 == closure['runtime_template_bytes'], 'Runtime template length changed')
    check(sha(template) == closure['runtime_template_sha256'], 'Runtime template changed')
    check(sha(bytes.fromhex(artifact['bytecode'][2:])) == closure['creation_bytecode_sha256'], 'Creation bytecode changed')
    check(sha((REF / 'original-helpers.vy').read_bytes()) == closure['helper_source_sha256'], 'Original helper probe changed')
    code_layout = layout['code_layout']
    immutable_len = max(v['offset'] + v['length'] for v in code_layout.values())
    check(immutable_len == 2144, 'Immutable layout extent changed')
    getters = {item.get('name') for item in artifact['abi'] if item['type'] == 'function'}
    check({'N_COINS', 'coins', 'get_balances', 'stored_rates', 'admin_balances',
           'last_price', 'ema_price', 'price_oracle', 'D_oracle', 'ma_last_time',
           'ma_exp_time', 'D_ma_time'} <= getters, 'Required original getters missing')
    check(not {'asset_types', 'rate_oracles', 'stored_balances', 'last_prices_packed', 'last_D_packed'} & getters,
          'Private metadata/getter boundary changed')
    check(len(rows['scenarios']) == len(closure['deployments']) == closure['scenarios'], 'Scenario count mismatch')
    steps = 0
    reverts = 0
    for scenario, deployed in zip(rows['scenarios'], closure['deployments']):
        check(scenario['name'] == deployed['scenario'], 'Deployment order mismatch')
        runtime = bytes.fromhex((REF / ('runtime-' + scenario['name'] + '.hex')).read_text().strip()[2:])
        check(len(runtime) == deployed['runtime_bytes'] == 23635, 'Deployed runtime length changed')
        check(sha(runtime) == deployed['runtime_sha256'] == scenario['pool']['runtime_sha256'], 'Deployed runtime changed')
        offset = len(runtime) - immutable_len
        check(offset == deployed['immutable_offset'] == 21491, 'Immutable suffix offset changed')
        check(runtime[:offset] == template, 'Deployed prefix differs from original compiler output')
        blob = runtime[offset:]
        check('0x' + blob.hex() == deployed['immutable_hex'], 'Immutable blob changed')
        word = lambda at: int.from_bytes(blob[at:at + 32], 'big')
        n = deployed['public_n_coins']
        check(word(code_layout['N_COINS']['offset']) == n, 'N_COINS getter/immutable disagreement')
        check(word(code_layout['N_COINS_128']['offset']) == n, 'N_COINS_128 disagreement')
        check(word(code_layout['pool_contains_rebasing_tokens']['offset']) == 0, 'Rebasing profile')
        for key in ['coins', 'asset_types', 'rate_multipliers', 'rate_oracles', 'call_amount', 'scale_factor']:
            at = code_layout[key]['offset']
            check(word(at) == n, key + ' immutable length mismatch')
            for i in range(8):
                expect = 0
                if i < n:
                    if key == 'coins':
                        expect = int(deployed['public_coins'][i], 16)
                    elif key == 'rate_multipliers':
                        expect = int(deployed['public_rates'][i])
                        check(expect == 10 ** (36 - scenario['pool']['decimals'][i]), 'Decimal rate mismatch')
                check(word(at + 32 * (i + 1)) == expect, key + ' immutable value/unused slot mismatch')
        for row in scenario['steps']:
            steps += 1
            check(row['snapshot']['timestamp'] == row['state']['timestamp'] == row['execution']['timestamp'], 'Execution timestamp mismatch')
            check(row['execution']['transaction_hash'].startswith('0x') and len(row['execution']['transaction_hash']) == 66, 'No transaction evidence')
            if row['expected'].get('reverted'):
                reverts += 1
                check(row['execution']['status'] == 0, 'Revert lacks failed transaction receipt')
            else:
                check(row['execution']['status'] == 1, 'Success lacks transaction receipt')
    check(steps == closure['steps'] and reverts == closure['reverts'], 'Step/revert counts changed')
    # Verify the actual loaded Node module files plus the compiler environment.
    check(sha((NODE.parent / 'package-lock.json').read_bytes()) == closure['node_lock_sha256'], 'Node lockfile changed')
    for module in closure['node_modules']:
        check(sha((NODE / module['path']).read_bytes()) == module['sha256'], 'Loaded Node module changed: ' + module['path'])
    site = MODEL / '.runtime/vyper-0.3.10-py313/lib/python3.13/site-packages'
    for module in closure['compiler_packages']:
        check(sha((site / module['path']).read_bytes()) == module['sha256'], 'Compiler dependency changed: ' + module['path'])
    vyper = MODEL / '.runtime/vyper-0.3.10-py313/bin/vyper'
    check(sha(vyper.read_bytes()) == closure['compiler_executable_sha256'], 'Compiler executable changed')
    if '--recompile' in sys.argv:
        result = subprocess.run([str(vyper), '-f', 'bytecode,bytecode_runtime,abi,layout', str(MODEL / pin['local'])],
                                env={**os.environ, 'PYTHONWARNINGS': 'ignore'}, check=True, capture_output=True, text=True)
        output = result.stdout.strip()
        check(sha(output.encode()) == closure['compiler_output_sha256'], 'Recompiled source/output mismatch')
        creation, deployed, abi, emitted_layout = output.splitlines()
        check({'bytecode': creation, 'runtime': deployed, 'abi': json.loads(abi), 'layout': json.loads(emitted_layout)} == artifact,
              'Recompiled complete artifact differs')
    print(json.dumps({'source_verified': True, 'runtime_template_bytes': len(template),
                      'immutable_offset': 21491, 'immutable_bytes': immutable_len,
                      'deployments_verified': len(closure['deployments']), 'steps': steps, 'reverts': reverts,
                      'fixture_sha256': sha(data), 'recompiled': '--recompile' in sys.argv}))


if __name__ == '__main__':
    try:
        verify()
    except (ValueError, KeyError, FileNotFoundError, subprocess.CalledProcessError) as error:
        raise SystemExit('Curve NG state evidence INVALID: ' + str(error))
