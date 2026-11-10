#!/usr/bin/env python3
"""Project authenticated scheduled original-VM observations; no pricing oracle.

Every getter is authenticated against the original ABI by the native checker.
Only supply bookkeeping (addition/subtraction), numeric encoding and source-H
retention are projected here. No fee, interpolation, power or swap math runs.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

import managed_v2_projection as base

ROOT = Path(__file__).resolve().parents[3]
PROTOCOL = 'balancer-v2-managed-scheduled'
ADAPTER = 'balancer-v2/20230411-managed-scheduled-1'
CHECKER = ROOT / 'integration/tools/ci/managed-v2-scheduled-golden.cjs'
require = base.require


class _Bank(dict):
    """Qualification and detached compiler-template identities from load only."""


def load(path, original_input=None):
    path = Path(path).resolve()
    raw = path.read_bytes()
    bank = _Bank(json.loads(raw, object_pairs_hook=base._pairs))
    result = subprocess.run(['node', str(CHECKER), '--validate', '--input', str(path)],
                            capture_output=True, text=True, check=False)
    require(result.returncode == 0, 'original scheduled validation failed: ' + result.stderr[-4000:])
    require(path.read_bytes() == raw, 'source changed during qualification')
    original = base.load(Path(original_input) if original_input else path.with_name('managed-v2-aum.json'))
    bank._templates = {i['label']: copy.deepcopy(i) for i in original._identities}
    pools = [i for i in original._identities if i['artifact']['name'] == 'ManagedPool']
    require(len({i['template_sha256'] for i in pools}) == 1, 'original pool compiler template ambiguity')
    bank._templates['ManagedPool'] = copy.deepcopy(pools[0])
    original_ids = {i['address']: i['code_sha256'] for i in original._identities}
    bank._runtime_ids = {ns: {**original_ids, **{i['address']: i['runtime_sha256'] for i in br.get('new_identities', [])}} for ns, br in bank['branches'].items()}
    bank._runtime_seal = base._digest(bank._runtime_ids)
    bank._seal = base._digest(bank)
    bank._template_seal = base._digest(bank._templates)
    bank._raw_identity = {'filename': path.name, 'bytes': len(raw),
                          'sha256': hashlib.sha256(raw).hexdigest(),
                          'original_aum_sha256': base.BANK_SHA256}
    bank._identity_seal = base._digest(bank._raw_identity)
    return bank


def _qualified(bank):
    require(isinstance(bank, _Bank), 'qualified scheduled bank required')
    require(base._digest(bank) == bank._seal, 'qualified scheduled bank changed after seal')
    require(base._digest(bank._templates) == bank._template_seal, 'compiler templates changed after seal')
    require(base._digest(bank._runtime_ids) == bank._runtime_seal, 'runtime identities changed after seal')
    require(base._digest(bank._raw_identity) == bank._identity_seal, 'raw identity changed after seal')


def _pool(bank, namespace, state):
    branch = bank['branches'][namespace]
    config = branch.get('configuration', branch)
    pid = config['pool']
    vault = config['vault']
    tokens = config['tokens']
    header = state['header']
    require(state['header_anchor_after'] == header, 'source H reanchor')
    getters = {}
    for g in state['getters']:
        require((g['block_number'], g['block_hash']) == (header['number'], header['hash']), 'getter source H')
        key = (g['to'], g['method'], tuple(map(str, g['args'])))
        require(key not in getters or getters[key] == g['decoded'], 'conflicting observed getter')
        getters[key] = g['decoded']

    def get(address, method, args=(), single=True):
        key = (address, method, tuple(map(str, args)))
        require(key in getters, f'{namespace}/{state["id"]} missing original getter {key}')
        v = getters[key]
        require(not single or len(v) == 1, 'getter arity')
        return copy.deepcopy(v[0] if single else v)

    accounts = {a['address']: a for a in state['accounts']}
    def code(address):
        require(address in accounts, 'missing original runtime account')
        observed = accounts[address]['code_sha256']
        require(bank._runtime_ids[namespace].get(address) == observed, 'observed runtime detached from authenticated original identity')
        return observed
    templates = bank._templates
    factory = config.get('factory', templates['ManagedPoolFactory']['address'])
    provider = get(factory, 'getProtocolFeePercentagesProvider')
    collector = get(pid, 'getProtocolFeesCollector')
    require(get(factory, 'getWeightedMath') == templates['ExternalWeightedMath']['address'], 'factory weighted math binding')
    require(get(factory, 'getRecoveryModeHelper') == templates['RecoveryModeHelper']['address'], 'factory recovery binding')
    manager = get(pid, 'getOwner')
    pool_id = get(pid, 'getPoolId')
    registration = get(vault, 'getPoolTokens', [pool_id], False)
    require(get(pid, 'getVault') == vault and get(vault, 'getPool', [pool_id], False) == [pid, 1], 'pool registration')
    require(registration[0] == [pid, *tokens], 'sorted registered tokens/BPT')
    require(get(factory, 'isPoolFromFactory', [pid]) is True, 'factory membership')
    deps = []
    for label, role in base.DEPENDENCIES.items():
        identity = templates[label]
        deps.append({'role': role, 'address': identity['address'], 'code_sha256': code(identity['address']),
                     'template_sha256': identity['template_sha256']})
    paused, pause_end, buffer_end = get(pid, 'getPausedState', single=False)
    vp, vpe, vbe = get(vault, 'getPausedState', single=False)
    annual, last = get(pid, 'getManagementAumFeeParams', single=False)
    total = get(pid, 'totalSupply')
    actual = get(pid, 'getActualSupply')
    bpt = get(vault, 'getPoolTokenInfo', [pool_id, pid], False)
    virtual = int(total) - int(bpt[0]) - int(bpt[1])
    fee_id = get(pid, 'getProviderFeeId', [3])
    weight_schedule = get(pid, 'getGradualWeightUpdateParams', single=False)
    fee_schedule = get(pid, 'getGradualSwapFeeUpdateParams', single=False)
    scales = get(pid, 'getScalingFactors')
    weights = get(pid, 'getNormalizedWeights')
    assets, infos = [], []
    for index, token in enumerate(tokens):
        cash, managed, changed, asset_manager = get(vault, 'getPoolTokenInfo', [pool_id, token], False)
        require(get(pid, 'getCircuitBreakerState', [token], False) == ['0'] * 6, 'breaker outside profile')
        assets.append({'token': token, 'balance': str(int(cash) + int(managed)),
                       'scaling': scales[index], 'code_sha256': code(token)})
        infos.append({'cash': cash, 'managed': managed, 'vault_actual_balance': get(token, 'balanceOf', [vault]),
                      'asset_manager': asset_manager, 'last_change_block': int(changed),
                      'circuit_breaker_enabled': False})
    packed = state['packed']
    require([t['token'] for t in packed['tokens']] == tokens, 'raw token order')
    schedule = {'pool_state_word': str(int(packed['pool_word'], 16)), 'tokens': [
        {'token': t['token'], 'state_word': str(int(t.get('raw_word', t.get('word')), 16))}
        for t in packed['tokens']]}
    m = {'timestamp': int(header['timestamp'], 16), 'block_number': int(header['number'], 16),
         'block_hash': header['hash'], 'factory': factory, 'factory_code_sha256': code(factory),
         'pool_runtime_template_sha256': templates['ManagedPool']['template_sha256'],
         'fee_provider': provider, 'fee_provider_code_sha256': code(provider),
         'protocol_collector': collector, 'protocol_collector_code_sha256': code(collector),
         'dependencies': deps, 'total_supply': total, 'virtual_supply': str(virtual),
         'actual_supply': actual, 'pending_aum': str(int(actual) - virtual),
         'vault_bpt_cash': bpt[0], 'vault_bpt_managed': bpt[1], 'bpt_index': 0,
         'vault_bpt_actual_balance': get(pid, 'balanceOf', [vault]),
         'manager': manager, 'manager_bpt_balance': get(pid, 'balanceOf', [manager]),
         'protocol_collector_bpt_balance': get(pid, 'balanceOf', [collector]),
         'management_aum_fee': annual, 'last_collection_timestamp': int(last), 'provider_fee_id': fee_id,
         'live_provider_aum_percentage': get(provider, 'getFeeTypePercentage', [fee_id]),
         'cached_protocol_aum_percentage': get(pid, 'getProtocolFeePercentageCache', [3]),
         'swaps_enabled': get(pid, 'getSwapEnabled'), 'paused': paused, 'vault_paused': vp,
         'vault_pause_window_end': int(vpe), 'vault_buffer_period_end': int(vbe),
         'recovery_mode': get(pid, 'inRecoveryMode'), 'allowlist_enabled': get(pid, 'getMustAllowlistLPs'),
         'pause_window_end': int(pause_end), 'buffer_period_end': int(buffer_end),
         'gradual_weight_end': int(weight_schedule[1]), 'gradual_fee_end': int(fee_schedule[1]),
         'token_info': infos, 'schedule': schedule}
    multi = {'code_sha256': code(pid), 'vault': vault, 'vault_code_sha256': code(vault),
             'pool_id': pool_id, 'assets': assets, 'weights': weights,
             'amplification': '0', 'admin_fee': 0, 'managed_v2': m}
    return {'chain_key': 'evm:31337', 'pool_id': pid, 'protocol': PROTOCOL, 'token0': tokens[0],
            'token1': tokens[1], 'reserve0': assets[0]['balance'], 'reserve1': assets[1]['balance'],
            'fee_numerator': int(get(pid, 'getSwapFeePercentage')), 'fee_denominator': 10**18,
            'adapter_version': ADAPTER, 'token_behavior': 'standard', 'multi_asset': multi}


def _derived(before, after):
    p = copy.deepcopy(after)
    m = p['multi_asset']['managed_v2']
    m['execution_context'] = {key: m[key] for key in ('block_number', 'timestamp')}
    for key in ('block_number', 'timestamp', 'block_hash'):
        m[key] = before['multi_asset']['managed_v2'][key]
    return p


def project(bank):
    _qualified(bank)
    states = {namespace + '/' + s['id']: _pool(bank, namespace, s)
              for namespace, branch in bank['branches'].items() for s in branch['states']}
    cases = []
    for indexed in bank['cases']:
        ns = indexed['namespace']
        c = next(c for c in bank['branches'][ns]['cases'] if c['id'] == indexed['atomic_case'])
        before, after = indexed['before'], indexed['after']
        assets = [a['token'] for a in states[before]['multi_asset']['assets']]
        cases.append({'id': indexed['id'], 'before': before, 'after': after,
                      'token_in': c['token_in'], 'token_out': c['token_out'],
                      'amount_in': c['amount_in'], 'index_in': assets.index(c['token_in']),
                      'index_out': assets.index(c['token_out']), 'expected_raw_out': indexed['expected_raw_out'],
                      'expected_pool_after': _derived(states[before], states[after]),
                      'transaction': indexed['transaction']})
    collections = []
    for ns, branch in bank['branches'].items():
        if 'aum_collection' in branch:
            c = branch['aum_collection']
            before, after = ns + '/' + c['before'], ns + '/' + c['after']
            collections.append({'id': ns + '/collection', 'before': before, 'after': after,
                                'transaction': ns + '/' + c['transaction'],
                                'expected_pool_after': _derived(states[before], states[after])})
    configs = {}
    for ident, pool in states.items():
        config = base._config(pool)
        config['protocol'] = PROTOCOL
        configs[ident] = config
    return {'schema_version': 1, 'adapter_version': ADAPTER,
            'authority': 'authenticated original ABI/getter/raw state projection; no pricing math',
            'original_raw': copy.deepcopy(bank._raw_identity),
            'projection_inputs': [{'path': str(p.relative_to(ROOT)), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
                                  for p in (Path(__file__).resolve(), Path(base.__file__).resolve())],
            'states': states, 'configs': configs,
            'cases': cases, 'checkpoints': copy.deepcopy(bank['checkpoints']), 'collections': collections}


def output_path(path, source, original_input=None):
    path = Path(path).absolute()
    require(not path.is_symlink(), 'projection output symlink refused')
    if path.exists():
        require(path.is_file() and path.stat().st_nlink == 1, 'projection output hardlink/nonfile refused')
    canonical = path.resolve()
    protected = {Path(source).resolve(), Path(original_input).resolve() if original_input else Path(source).with_name('managed-v2-aum.json').resolve()}
    require(canonical not in protected, 'source/original input overwrite refused')
    projects = ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')
    registered = {ROOT / p / 'test/contracts/managed-v2-scheduled-pools.json' for p in projects}
    protected_names = {p.name for project in projects for p in (ROOT / project / 'test/contracts').iterdir()
                       if p.name != 'managed-v2-scheduled-pools.json'}
    temporary = (canonical.is_relative_to(Path(tempfile.gettempdir()).resolve())
                 or canonical.is_relative_to(Path('/tmp').resolve()))
    require(canonical in registered or temporary and not canonical.is_relative_to(ROOT)
            and canonical.suffix == '.json' and canonical.name not in protected_names, 'projection output boundary')
    return canonical


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', required=True, type=Path)
    parser.add_argument('--original-input', type=Path, help='explicit frozen AUM/compiler input, default: sibling managed-v2-aum.json')
    group = parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--output', type=Path)
    group.add_argument('--check', type=Path)
    args = parser.parse_args()
    destination = output_path(args.output, args.input, args.original_input) if args.output else None
    value = project(load(args.input, args.original_input))
    data = (json.dumps(value, indent=2, ensure_ascii=False) + '\n').encode()
    if args.check:
        require(args.check.read_bytes() == data, 'scheduled projection bytes differ')
    else:
        output_path(args.output, args.input, args.original_input)
        fd, temporary = tempfile.mkstemp(prefix=destination.name + '.', suffix='.tmp', dir=destination.parent)
        try:
            with os.fdopen(fd, 'wb') as stream:
                stream.write(data)
            Path(temporary).replace(destination)
        finally:
            Path(temporary).unlink(missing_ok=True)
    print(json.dumps({'passed': True, 'states': len(value['states']), 'swaps': len(value['cases']),
                      'checkpoints': len(value['checkpoints']), 'collections': len(value['collections']),
                      'bytes': len(data), 'sha256': hashlib.sha256(data).hexdigest(),
                      'original_raw_sha256': value['original_raw']['sha256']}))


if __name__ == '__main__':
    main()
