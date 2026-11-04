#!/usr/bin/env python3
"""Authenticate the current original-EVM receipt, sources and identical corpus."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/infinity-bin-multi'

def require(condition, message):
    if not condition:
        raise ValueError(message)

def sha(data):
    return hashlib.sha256(data).hexdigest()

def verify():
    receipt = json.loads((REF / 'execution-receipt.json').read_text())
    require((receipt['passed'] is True), 'original EVM execution did not pass')
    for name, digest in receipt['inputs'].items():
        require((sha((ROOT / name).read_bytes()) == digest), f'stale oracle input: {name}')
    metadata_bytes = (REF / 'source-metadata.json').read_bytes()
    require((sha(metadata_bytes) == receipt['source_metadata_sha256']), 'Reference validation failed')
    metadata = json.loads(metadata_bytes)
    a = (ROOT / 'integration/test/contracts/infinity-bin-multi.json').read_bytes()
    b = (ROOT / 'dex-router/test/contracts/infinity-bin-multi.json').read_bytes()
    require((a == b and sha(a) == receipt['fixture_sha256']), 'fixture mismatch or stale receipt')
    fixture = json.loads(a)
    require((fixture['reference'] == metadata), 'Reference validation failed')
    require((fixture['schema'] == 'infinity-bin-multi/1'), 'Reference validation failed')
    require((len(fixture['cases']) == receipt['cases'] >= 300), 'Reference validation failed')
    runtime = ROOT / 'integration/.runtime/singleton-reference'
    require((sha((runtime / 'package-lock.json').read_bytes()) == receipt['dependency_lock_sha256']), 'Reference validation failed')
    for source in metadata['files']:
        require((sha((runtime / 'sources' / source['local']).read_bytes()) == source['sha256']), source['local'])
        require((source['revision'] == '3c5ef1be1f71442d8bc60cc6fd11f9c677b714df'), 'Reference validation failed')
    require((len(metadata['files']) >= 15), 'Reference validation failed')
    require((metadata['runtime_sha256'] == receipt['runtime_sha256']), 'Reference validation failed')
    require((metadata['runtime_bytes'] == receipt['runtime_bytes'] <= 24576), 'Reference validation failed')
    require((metadata['unlimited_contract_size'] is False), 'Reference validation failed')
    for name, dep in metadata['dependencies'].items():
        package = runtime / 'node_modules' / name / 'package.json'
        require((sha(package.read_bytes()) == dep['package_sha256']), 'Reference validation failed')
        require((json.loads(package.read_text())['version'] == dep['version']), 'Reference validation failed')
        entry = subprocess.check_output(['node', '-p', 'require.resolve(' + json.dumps(str(package.parent)) + ')'], text=True).strip()
        require((sha(Path(entry).read_bytes()) == dep['entry_sha256']), 'Reference validation failed')
    require((sha((runtime / 'node_modules/solc/soljson.js').read_bytes()) == metadata['dependencies']['solc']['soljson_sha256']), 'Reference validation failed')
    counts = {'cases': len(fixture['cases']), 'native_success': 0, 'native_reverts': 0, 'support_budget_rejections': 0}
    names = set()
    for c in fixture['cases']:
        require((c['name'] not in names), c['name'])
        names.add(c['name'])
        e = c['expected']
        require((all(e['post']['membership'])), c['name'])
        if e.get('native_reverted'):
            counts['native_reverts'] += 1
            require((e['revert_data'] != '0x'), 'Reference validation failed')
            require((e['post']['bins'] == c['state']['bins']), 'Reference validation failed')
            require((e['post']['active_id'] == c['state']['active_id']), 'Reference validation failed')
        else:
            counts['native_success'] += 1
            require((e['consumed_in'] == c['input'] and e['remaining_in'] == '0'), 'Reference validation failed')
            require((sum(int(f['amount_in']) for f in e['bin_fills']) == int(c['input'])), 'Reference validation failed')
            require((sum(int(f['amount_out']) for f in e['bin_fills']) == int(e['amount_out'])), 'Reference validation failed')
            require((sum(int(f['protocol_amount']) for f in e['bin_fills']) == int(e['protocol_amount'])), 'Reference validation failed')
            if e.get('support_rejection'):
                require((e['support_rejection'] == 'INFINITY_BIN_STEP_BUDGET'), 'Reference validation failed')
                require((e['steps'] > c['state']['max_steps']), 'Reference validation failed')
                counts['support_budget_rejections'] += 1
    for key, count in counts.items():
        require((count == receipt[key]), key)
    return counts

if __name__ == '__main__':
    try:
        result = verify()
        if '--with-tests' in sys.argv:
            for name in ('test-receipt.json', 'receipt-test-receipt.json'):
                test = json.loads((REF / name).read_text())
                require((test['passed'] is True), name)
                for file, digest in test['inputs'].items():
                    require((sha((ROOT / file).read_bytes()) == digest), f'stale test input: {file}')
                if name == 'test-receipt.json':
                    require((sha((REF / 'test-dependencies.lock').read_bytes()) == test['resolved_lock_sha256']), 'Reference validation failed')
            result['current_test_receipts'] = True
        print(json.dumps(result, sort_keys=True))
    except BaseException as error:
        print(f'infinity-bin-multi evidence check failed: {error}', file=sys.stderr)
        raise
