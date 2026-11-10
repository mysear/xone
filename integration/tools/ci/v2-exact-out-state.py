#!/usr/bin/env python3
"""Project observed exact-output banks; deduplicate original code bytes only."""
import hashlib
import json
import sys
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from exact_out_reference import validate, validate_grid
source = ROOT / 'integration/artifacts/local/core-v2-exact-out-original-20261109.json'
validate(source)
original = json.loads(source.read_bytes())
codes = {}


def bank(value):
    reads = []
    for item in value['reads']:
        entry = dict(item)
        if item['method'] == 'eth_getCode':
            digest = item['result_code_sha256']
            code = original['code_ledger'][digest]
            if hashlib.sha256(bytes.fromhex(code[2:])).hexdigest() != digest:
                raise ValueError('EXACT_OUT_CODE_LEDGER_DIGEST')
            codes[digest] = code
        reads.append(entry)
    return dict(header=value['header'], reads=reads, canonical_pools=value['canonical_pools'])


cases = []
for c in original['cases']:
    cases.append(dict(id=c['id'], mode=c['mode'], path=c['path'], amount_out=c['desired_output'],
                      max_input=c['max_input'], pools=c['source']['pools'],
                      source=bank(c['source']), expected=c['expected'] | {'after': None},
                      expected_pools=c['expected']['after']['pools'], after_source=bank(c['expected']['after'])))
    del cases[-1]['expected']['after']
value = dict(schema_version=1, authority='lossless original getter and code-byte-ledger projection; output/input values remain original Router observations',
             original_reference=dict(path=str(source.relative_to(ROOT)), sha256=hashlib.sha256(source.read_bytes()).hexdigest()),
             generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
             identities=original['identities'], context=original['context'], code_ledger=codes, cases=cases, invalid=original['invalid'],
             all_requested_features_complete=False, production_ready=False)
grid_path = ROOT / 'integration/artifacts/local/core-v2-exact-out-grid-original-20261109.json'
if grid_path.exists():
    validate_grid(grid_path)
    grid = json.loads(grid_path.read_bytes())
    if grid.get('passed') is not True or grid['generator_sha256'] != original['generator_sha256']:
        raise ValueError('EXACT_OUT_GRID_SOURCE_CHANGED')
    value['grid'] = dict(reference=dict(path=str(grid_path.relative_to(ROOT)),sha256=hashlib.sha256(grid_path.read_bytes()).hexdigest()),
                        pools=grid['bank']['pools'], context=grid['context'],paths=grid['paths'],
                        target_output=grid['target_output'],max_input=grid['max_input'],parts=grid['grid_parts'],
                        max_splits=grid['max_splits'],best_required_input=grid['best_required_input'],best_plan_id=grid['best_plan_id'],
                        plans=[dict(id=p['id'],proposal=p['proposal'],feasible=p['feasible'],required_input=p['required_input'],
                                    recipient_delta=p['recipient_delta'],expected_pools=p['after']['pools'])for p in grid['plans']])
raw = json.dumps(value, indent=2) + '\n'
for service in ['integration', 'dex-router']:
    (ROOT / service / 'test/contracts/v2-exact-out.json').write_text(raw)
print(json.dumps(dict(passed=True, original_cases=len(cases), unique_code_bodies=len(codes), bytes=len(raw.encode()))))
