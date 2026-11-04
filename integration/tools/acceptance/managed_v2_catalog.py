#!/usr/bin/env python3
"""Authenticated projection of one explicitly selected local catalog; no pricing math.

Only the original native ABI/template validator is invoked. --check never deploys,
regenerates or writes a fixture. Source and observed-after remain observations;
derived-after retains source H and an explicit execution context.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
from typing import Any, TypedDict

ROOT = Path(os.environ.get('XONE_SOURCE_ROOT', Path(__file__).resolve().parents[3])).resolve()
CHECKER = Path(os.environ.get('XONE_CATALOG_CHECKER', Path(__file__).resolve().parents[1] / 'ci/managed-v2-aum-catalog-check.cjs'))
ADAPTER = 'balancer-v2/20230411-managed-aum-3'
CATALOG_SHA256 = 'b843e859a5e92efcc1880eed437e475042785a7967bf2277b6f9770cf93b2813'
CATALOG_BYTES = 13785866
BANK_SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66'

MANAGED_FIELDS = ('timestamp', 'block_number', 'block_hash', 'factory', 'factory_code_sha256', 'pool_runtime_template_sha256', 'fee_provider', 'fee_provider_code_sha256', 'protocol_collector', 'protocol_collector_code_sha256', 'dependencies', 'bpt_index', 'manager', 'manager_bpt_balance', 'protocol_collector_bpt_balance', 'management_aum_fee', 'last_collection_timestamp', 'provider_fee_id', 'live_provider_aum_percentage', 'cached_protocol_aum_percentage', 'swaps_enabled', 'paused', 'vault_paused', 'vault_pause_window_end', 'vault_buffer_period_end', 'recovery_mode', 'allowlist_enabled', 'pause_window_end', 'buffer_period_end', 'gradual_weight_end', 'gradual_fee_end', 'token_info', 'total_supply', 'virtual_supply', 'actual_supply', 'pending_aum', 'vault_bpt_cash', 'vault_bpt_managed', 'vault_bpt_actual_balance')

class PoolProjection(TypedDict):
    scenario: str
    source_pool: dict[str, Any]
    config: dict[str, Any]
    keyrefs: dict[str, Any]

class CatalogProjection(TypedDict):
    schema_version: int
    profile: str
    adapter_version: str
    authority: str
    original_raw: dict[str, Any]
    banks: dict[str, Any]
    cases: list[dict[str, Any]]

def require(condition, message):
    if not condition:
        raise ValueError(message)

def _pairs(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key: ' + key)
        result[key] = value
    return result

def _bytes(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()

def _sha(data):
    return hashlib.sha256(data).hexdigest()

class Catalog:
    def __init__(self, raw, size, digest):
        self.raw, self.size, self.digest = raw, size, digest
        self.seal = _sha(_bytes(raw))

def validate_shape(raw):
    require(raw['schema_version'] == 1 and raw['public_adapter_version'] == ADAPTER, 'catalog schema/adapter')
    counts = raw['counts']
    require(all(counts[k] == v for k, v in {'pools': 4, 'underlying_tokens': 60, 'swap_cases': 4,
            'collection_cases': 4, 'banks': 10, 'raw_getters': 9800, 'prefix_transactions': 28}.items()), 'catalog scope')
    require(raw['original_bank_sha256'] == BANK_SHA and raw['original_bank_bytes'] == 7905624, 'original raw identity')
    require(len(raw['banks']) == 10 and len(raw['cases']) == 8, 'exact ten banks/eight cases')
    require(raw['chain_options']['chain']['allowUnlimitedContractSize'] is False and
            raw['chain_options']['chain']['allowUnlimitedInitCodeSize'] is False, 'normal EIP170 required')
    names = [s['name'] for s in raw['scenarios']]
    require(names == ['two', 'three', 'five', 'fifty'], 'scenario order')
    require([s['underlying_count'] for s in raw['scenarios']] == [2, 3, 5, 50], 'all four pools')
    getter_count = 0
    for key, bank in raw['banks'].items():
        h = bank['header']
        require(key == bank['id'] and key == 'H' + str(int(h['number'], 16)), 'bank ID/header')
        require(h == bank['header_anchor_before'] == bank['header_anchor_after'], 'common fixed source H')
        require([p['scenario'] for p in bank['pools']] == names, 'four pool registration scope')
        require(bank['getter_count'] == sum(p['getter_count'] for p in bank['pools']), 'bank getter scope')
        for item, n in zip(bank['pools'], [2, 3, 5, 50]):
            require(item['header'] == item['header_anchor_before'] == item['header_anchor_after'] == h, 'no spliced source H')
            pool = item['pool']; multi = pool['multi_asset']; m = multi['managed_v2']
            require(set(m) == set(MANAGED_FIELDS) and 'execution_context' not in m, '39 observed public fields')
            require((m['block_number'], m['timestamp'], m['block_hash']) ==
                    (int(h['number'], 16), int(h['timestamp'], 16), h['hash']), 'full pool source H')
            require(len(multi['assets']) == n and len(m['token_info']) == n, 'complete token state')
            require(len(item['getters']) == item['getter_count'], 'getter references')
            require(all(g['block_hash'] == h['hash'] and g['block_tag'] == h['number'] for g in item['getters']), 'getter fixed H')
        getter_count += bank['getter_count']
    require(getter_count == 9800, 'all original getters')
    require(len({c['name'] for c in raw['cases']}) == 8, 'unique cases')
    require(sorted((c['scenario'], c['operation']) for c in raw['cases']) ==
            sorted((s, op) for s in names for op in ['swap', 'collect']), '4 swap + 4 collect')
    for case in raw['cases']:
        before = raw['banks'][case['source_bank4pools']]
        after = raw['banks'][case['expected_after_bank4pools']]
        require(case['execution']['pre_header'] == before['header'] and
                case['execution']['execution_header'] == after['header'], 'execution/source banks')
        require(case['expected_native']['status'] == 1 and case['execution']['receipt']['status'] == '0x1', 'actual successful execution')

def load(path: str | Path) -> Catalog:
    """Authenticate this independent copy only, without reading sibling catalogs."""
    path = Path(path).resolve(strict=True)
    data = path.read_bytes()
    require(len(data) == CATALOG_BYTES and _sha(data) == CATALOG_SHA256, 'unqualified catalog SHA256/length')
    raw = json.loads(data, object_pairs_hook=_pairs)
    validate_shape(raw)
    env = dict(os.environ, XONE_SOURCE_ROOT=str(ROOT), PYTHONDONTWRITEBYTECODE='1')
    run = subprocess.run([env.get('XONE_NODE', '/usr/local/bin/node'), str(CHECKER), str(path)],
                         env=env, capture_output=True, text=True, check=False)
    require(run.returncode == 0, 'original native ABI/input/template authentication failed: ' + run.stderr[-4000:])
    require(path.read_bytes() == data, 'local copy changed during authentication')
    return Catalog(raw, len(data), _sha(data))

def config(pool: dict[str, Any]) -> dict[str, Any]:
    """Trust values only after native raw authentication; no per-pool constants."""
    v = pool['multi_asset']; m = v['managed_v2']
    names = ('factory', 'factory_code_sha256', 'pool_runtime_template_sha256', 'fee_provider',
             'fee_provider_code_sha256', 'protocol_collector', 'protocol_collector_code_sha256',
             'manager', 'provider_fee_id', 'management_aum_fee')
    managed = {'expected_' + key: copy.deepcopy(m[key]) for key in names}
    managed['dependencies'] = copy.deepcopy(m['dependencies'])
    return {'id': pool['pool_id'], 'protocol': pool['protocol'], 'token0': pool['token0'], 'token1': pool['token1'],
            'fee_numerator': pool['fee_numerator'], 'fee_denominator': pool['fee_denominator'],
            'multi_asset': {'expected_code_sha256': v['code_sha256'], 'expected_vault': v['vault'],
                'expected_vault_code_sha256': v['vault_code_sha256'], 'pool_id': v['pool_id'],
                'tokens': [{'token': a['token'], 'expected_code_sha256': a['code_sha256']} for a in v['assets']],
                'token_policy': 'standard-no-bpt', 'managed_v2': managed}}

def _keyrefs(bank_id, index, pool):
    pointer = '/banks/' + bank_id + '/pools/' + str(index)
    return {'pool': pointer + '/pool', 'getters': pointer + '/getters',
            'managed_v2': {key: pointer + '/pool/multi_asset/managed_v2/' + key for key in pool['multi_asset']['managed_v2']}}

def project(catalog: Catalog) -> CatalogProjection:
    require(type(catalog) is Catalog and catalog.seal == _sha(_bytes(catalog.raw)), 'unqualified or mutated catalog')
    raw = catalog.raw
    validate_shape(raw)
    banks = {}
    for ident, bank in raw['banks'].items():
        banks[ident] = {'id': ident, 'header': copy.deepcopy(bank['header']), 'pools': [
            {'scenario': row['scenario'], 'source_pool': copy.deepcopy(row['pool']), 'config': config(row['pool']),
             'keyrefs': _keyrefs(ident, i, row['pool'])} for i, row in enumerate(bank['pools'])]}
    cases = []
    for index, case in enumerate(raw['cases']):
        source_id = case['source_bank4pools']; after_id = case['expected_after_bank4pools']
        pos = next(i for i, row in enumerate(raw['banks'][source_id]['pools']) if row['scenario'] == case['scenario'])
        before = copy.deepcopy(raw['banks'][source_id]['pools'][pos]['pool'])
        after = copy.deepcopy(raw['banks'][after_id]['pools'][pos]['pool'])
        expected = copy.deepcopy(after)
        bm = before['multi_asset']['managed_v2']; em = expected['multi_asset']['managed_v2']
        for field in ('block_number', 'timestamp', 'block_hash'):
            em[field] = bm[field]
        h = case['execution']['execution_header']
        em['execution_context'] = {'block_number': int(h['number'], 16), 'timestamp': int(h['timestamp'], 16)}
        params = {'execution_block': int(h['number'], 16), 'execution_timestamp': int(h['timestamp'], 16)}
        if case['operation'] == 'swap':
            swap = case['execution']['args'][0]; assets = [a['token'] for a in before['multi_asset']['assets']]
            params.update(amount=swap['amount'], index_in=assets.index(swap['assetIn']), index_out=assets.index(swap['assetOut']))
        cases.append({'name': case['name'], 'scenario': case['scenario'], 'operation': case['operation'],
            'source_bank4pools': source_id, 'expected_after_bank4pools': after_id,
            'source_pool': before, 'observed_pool_after': after, 'expected_pool_after': expected,
            'config': config(before), 'params': params, 'expected_native': copy.deepcopy(case['expected_native']),
            'keyrefs': _keyrefs(source_id, pos, before),
            'source_refs': {'source': _keyrefs(source_id, pos, before)['pool'],
                            'after': _keyrefs(after_id, pos, after)['pool'], 'execution': '/cases/' + str(index) + '/execution'},
            'authority': {'expected_pool_after': 'complete observed selected pool at retained source H plus execution context',
                'siblings_and_globals': 'full original common-H bank observations; no timestamp or wallet synthesis'}})
    metadata = {k: copy.deepcopy(raw[k]) for k in ('schema_version', 'profile', 'authority', 'public_adapter_version',
        'generator', 'original_reference', 'original_bank', 'source_preservation', 'chain_options', 'fixture_date', 'actual_iso',
        'clock_authority', 'reference_outputs', 'counts', 'limitations')}
    metadata['projection_tools_inputs'] = [
        {'path': 'integration/tools/acceptance/managed_v2_catalog.py', 'role': 'public-projection',
         'sha256': _sha(Path(__file__).read_bytes())},
        {'path': 'integration/tools/ci/managed-v2-aum-catalog-check.cjs', 'role': 'native-raw-authenticator',
         'sha256': _sha(CHECKER.read_bytes())}]
    return {'schema_version': 1, 'profile': raw['profile'], 'adapter_version': ADAPTER,
            'authority': 'authenticated local original-bytecode catalog public projection; no math outputs',
            'original_raw': {'filename': 'managed-v2-aum-catalog.json', 'bytes': catalog.size, 'sha256': catalog.digest, 'reference': metadata},
            'banks': banks, 'cases': cases}

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, required=True, help='one explicit independent local raw copy')
    choice = parser.add_mutually_exclusive_group(required=True)
    choice.add_argument('--output', type=Path)
    choice.add_argument('--check', type=Path, metavar='PROJECTION', help='read-only exact thin projection check')
    args = parser.parse_args()
    value = project(load(args.input))
    text = json.dumps(value, indent=2, ensure_ascii=False) + '\n'
    if args.check:
        require(args.check.read_text() == text, 'thin projection differs from authenticated local raw')
    else:
        target = args.output.resolve()
        require(target != args.input.resolve(), 'raw overwrite refused')
        require(not args.output.is_symlink(), 'output symlink refused')
        # Uninstalled candidates can only write tmp, and never the read-only ROOT.
        if Path(__file__).resolve() != ROOT / 'integration/tools/acceptance/managed_v2_catalog.py':
            require(target.is_relative_to(Path('/tmp').resolve()) and not target.is_relative_to(ROOT), 'candidate output must stay in tmp')
        args.output.write_text(text)
    print(json.dumps({'passed': True, 'mode': 'check' if args.check else 'project', 'banks': len(value['banks']),
                      'cases': len(value['cases']), 'bytes': len(text.encode()), 'raw_sha256': value['original_raw']['sha256']}))

if __name__ == '__main__':
    main()
