"""Authenticated frozen Managed V2 bank projection. No mathematical outputs computed.

load(path) authenticates one independent local original copy and its native input
closure. resolve(bank, ref, kind) verifies complete record digests and reference
closure on every call. project_snapshot returns {pool, authority, source_refs};
origin is an explicit snapshot ref whose header anchors a derived after-state.
project_case returns original before, original after, derived after, params,
original native expectation and public config. All returned values are detached.
"""
from pathlib import Path
import copy
import hashlib
import json
import re
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/balancer-managed-v2-aum'
BANK_SHA256 = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66'
BANK_BYTES = 7905624
PROTOCOL = 'balancer-v2-managed-aum'
# Frozen native reference metadata remains aum-1; wallet and separate Pool/Vault pause state use model aum-3.
ADAPTER = 'balancer-v2/20230411-managed-aum-3'
DEPENDENCIES = {
    'CircuitBreakerLib': 'circuit-breaker-lib',
    'ManagedPoolAddRemoveTokenLib': 'managed-pool-add-remove-token-lib',
    'ManagedPoolAmmLib': 'managed-pool-amm-lib',
    'ExternalWeightedMath': 'external-weighted-math',
    'RecoveryModeHelper': 'recovery-mode-helper',
}


# Exact typed reference positions inventoried from the frozen native records.
_REFERENCE_KINDS = {
    'snapshot': {('header',): 'header', ('getters', '*'): 'getter',
                 ('accounts', '*'): 'account', ('state', 'underlying', '*'): 'token_state'},
    'identity': {('code',): 'code', ('creation_transaction',): 'transaction'},
    'account': {('code',): 'code'},
    'transaction': {('pre_header',): 'header', ('execution_header',): 'header'},
    'getter': {}, 'header': {}, 'code': {}, 'token_state': {},
}


def require(value, message):
    if not value:
        raise ValueError(message)


def canonical(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False, allow_nan=False).encode()


def _digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def _pairs(pairs):
    value = {}
    for key, item in pairs:
        require(key not in value, 'duplicate JSON key: ' + key)
        value[key] = item
    return value


class _Bank(dict):
    """Qualification receipt is deliberately unavailable on ordinary parsed dicts."""


def resolve(bank, ref, kind=None):
    """Resolve a typed ref, validating SHA, all descendant refs, and acyclicity."""
    visiting, done = set(), set()

    def visit(link, expected=None):
        require(type(link) is dict and set(link) == {'ref'}, 'exact ref required')
        ident = link['ref']
        require(isinstance(ident, str) and re.fullmatch('[0-9a-f]{64}', ident), 'invalid object id')
        require(ident not in visiting, 'cyclic object graph')
        obj = bank['objects'].get(ident)
        require(type(obj) is dict and set(obj) == {'kind', 'value'}, 'missing/invalid object record')
        require(type(obj['kind']) is str and obj['kind'] in _REFERENCE_KINDS, 'unknown object kind')
        require(_digest(obj) == ident, 'object SHA256 mismatch')
        require(expected is None or obj['kind'] == expected, 'object kind mismatch')
        if ident in done:
            return obj['value']
        visiting.add(ident)

        def walk(v, path=()):
            if isinstance(v, dict):
                if 'ref' in v:
                    types = _REFERENCE_KINDS[obj['kind']]
                    require(path in types, 'unexpected reference position')
                    visit(v, types[path])
                else:
                    for key, x in v.items(): walk(x, path + (key,))
            elif isinstance(v, list):
                for x in v: walk(x, path + ('*',))
        walk(obj['value'])
        visiting.remove(ident)
        done.add(ident)
        return obj['value']
    return copy.deepcopy(visit(ref, kind))


# Uses only the shipped primary validator/artifact loader. It executes no bank
# data or remote code. The validator checks exact immutable/link bindings first.
_NATIVE_MANIFEST = r"""
const fs=require('node:fs'),assert=require('node:assert/strict');
const dir=process.argv[1], V=require(dir+'/validate.cjs'), A=require(dir+'/artifacts.cjs');
const bank=JSON.parse(fs.readFileSync(0,'utf8')); V.validate(bank);
const {builds}=A.load(), R=(r,k)=>V.resolveObject(bank.objects,r.ref,k), rows=[];
for(const ref of bank.reference.identities){
 const i=R(ref,'identity'), a=A.locate(builds,i.artifact.build,i.artifact.name);
 assert.equal(a.source,i.artifact.source);
 const d=a.c.evm.deployedBytecode, actual=Buffer.from(R(i.code,'code').hex.slice(2),'hex');
 const normalized=Buffer.from(actual), template=d.object.split(''), spans=[];
 function span(start,length,kind,name){
  assert(Number.isSafeInteger(start)&&Number.isSafeInteger(length)&&start>=0&&length>0&&start+length<=actual.length);
  spans.push({start,length,kind,name});
 }
 if(a.library){
  assert.equal(d.object.slice(0,42),'73'+'0'.repeat(40));
  assert.equal(actual.subarray(1,21).toString('hex'),i.address.slice(2));
  span(1,20,'library-self-address',i.artifact.name);
 }
 for(const [source,libs]of Object.entries(d.linkReferences||{}))
  for(const [name,ss]of Object.entries(libs))for(const s of ss){assert.equal(s.length,20);span(s.start,s.length,'link',source+':'+name);}
 for(const [id,ss]of Object.entries(d.immutableReferences||{}))
  for(const s of ss){assert.equal(s.length,32);span(s.start,s.length,'immutable',id);}
 spans.sort((a,b)=>a.start-b.start);let end=0;
 for(const s of spans){assert(s.start>=end,'overlapping declared spans');end=s.start+s.length;
  normalized.fill(0,s.start,end);template.fill('0',s.start*2,end*2);}
 const encoded=template.join('');assert(/^[0-9a-f]*$/.test(encoded));
 assert.equal(normalized.toString('hex'),encoded,'exact normalized original compiler template');
 rows.push({address:i.address,label:i.label,artifact:i.artifact,code_sha256:V.sha(actual),
 template_sha256:V.sha(normalized),normalization_spans:spans,identity_ref:ref});
}
console.log(JSON.stringify(rows));
"""


def load(path):
    """Authenticate exactly one local copy, without consulting sibling banks."""
    raw = Path(path).read_bytes()
    require(len(raw) == BANK_BYTES and hashlib.sha256(raw).hexdigest() == BANK_SHA256,
            'unqualified original bank SHA256/length')
    b = _Bank(json.loads(raw, object_pairs_hook=_pairs))
    # Check graph independently, including cycles (the native validator also
    # enforces complete object inventory and every interpreted getter).
    for ref in b['reference']['identities']:
        resolve(b, ref, 'identity')
    result = subprocess.run(['node', '-e', _NATIVE_MANIFEST, str(REFERENCE)], input=raw,
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=False)
    require(result.returncode == 0, 'native bank/identity/input validation failed: ' + result.stderr.decode()[-4000:])
    b._identities = json.loads(result.stdout, object_pairs_hook=_pairs)
    b._seal = _digest(b)
    b._manifest_seal = _digest(b._identities)
    return b


def _qualified(bank):
    require(isinstance(bank, _Bank), 'load() qualified bank required')
    require(_digest(bank) == bank._seal, 'qualified bank changed')
    require(_digest(bank._identities) == bank._manifest_seal, 'identity manifest changed')


def _uint(value, bits=256):
    require(type(value) is str and re.fullmatch('0|[1-9][0-9]*', value), 'canonical decimal required')
    n = int(value)
    require(n < 1 << bits, 'unsigned integer width')
    return n


def _address(value):
    require(type(value) is str and re.fullmatch('0x[0-9a-f]{40}', value), 'canonical address required')
    return value


def _snapshot(bank, ref, origin=None):
    snap = resolve(bank, ref, 'snapshot')
    own_header = resolve(bank, snap['header'], 'header')
    origin_snapshot = snap if origin is None else resolve(bank, origin, 'snapshot')
    header = own_header if origin is None else resolve(bank, origin_snapshot['header'], 'header')
    require(origin_snapshot['state']['pool'] == snap['state']['pool'], 'derived origin pool mismatch')
    require(int(header['number'], 16) <= int(own_header['number'], 16)
            and int(header['timestamp'], 16) <= int(own_header['timestamp'], 16), 'derived origin is in the future')
    s = snap['state']
    number, timestamp = int(own_header['number'], 16), int(own_header['timestamp'], 16)
    require(0 <= number < 2**64 and 0 <= timestamp < 2**64, 'header width')
    require(_uint(s['registration']['last_change_block'], 64) <= number, 'future Vault registration')
    require(_uint(s['last_collection'], 32) <= timestamp, 'future source collection timestamp')
    ids = {i['address']: i for i in bank._identities}
    labels = {i['label']: i for i in bank._identities}

    def identity(address):
        _address(address)
        require(address in ids, 'unknown runtime identity')
        return ids[address]
    pool_id = _address(s['pool'])
    pool_identity = identity(pool_id)
    vault = identity(s['vault'])
    factory, provider, collector = (labels[k] for k in ('ManagedPoolFactory', 'ProtocolFeePercentagesProvider', 'ProtocolFeesCollector'))
    require(s['roles']['collector'] == collector['address'] and s['roles']['vault'] == vault['address'], 'role identity mismatch')
    require(s['registration']['specialization'] == 1 and s['pool_id'].startswith(pool_id + '0001') and len(s['pool_id']) == 66, 'MINIMAL specialization required')
    assets, infos = [], []
    for index, token_ref in enumerate(s['underlying']):
        t = resolve(bank, token_ref, 'token_state')
        token = identity(t['address'])
        require(type(t['decimals']) is int and 0 <= t['decimals'] <= 18, 'unsupported token decimals')
        balance = _uint(t['cash']) + _uint(t['managed'])
        actual_balances = t.get('actual_balances')
        require(type(actual_balances) is dict and s['vault'] in actual_balances, 'missing Vault token actual balance')
        actual = actual_balances[s['vault']]
        require(_uint(actual) >= _uint(t['cash']), 'Vault token actual balance below cash')
        require(balance < 2**112, 'Vault token balance width')
        require(_uint(t['last_change_block'], 64) <= number, 'future token source last change')
        require(t['breaker'] == ['0'] * 6, 'circuit breaker unsupported')
        scaling = 10 ** (36 - t['decimals'])
        require(str(scaling) == s['scaling_factors'][index], 'scaling/getter mismatch')
        assets.append({'token': t['address'], 'balance': str(balance), 'scaling': str(scaling), 'code_sha256': token['code_sha256']})
        infos.append({'cash': t['cash'], 'managed': t['managed'], 'vault_actual_balance': actual, 'asset_manager': _address(t['asset_manager']),
                      'last_change_block': int(t['last_change_block']), 'circuit_breaker_enabled': False})
    require(2 <= len(assets) <= 50 and [a['token'] for a in assets] == sorted({a['token'] for a in assets}), 'sorted underlying required')
    require(pool_id not in [a['token'] for a in assets], 'BPT cannot be a quote asset')
    deps = [dict(role=role, address=labels[label]['address'], code_sha256=labels[label]['code_sha256'],
                 template_sha256=labels[label]['template_sha256']) for label, role in DEPENDENCIES.items()]
    fee_id = str(_uint(s['provider_fee_id']))
    require(fee_id in s['provider_percentages'], 'unknown provider fee id')
    m = {'timestamp': int(header['timestamp'], 16), 'block_number': int(header['number'], 16), 'block_hash': header['hash'],
         'factory': factory['address'], 'factory_code_sha256': factory['code_sha256'],
         'pool_runtime_template_sha256': pool_identity['template_sha256'],
         'fee_provider': provider['address'], 'fee_provider_code_sha256': provider['code_sha256'],
         'protocol_collector': collector['address'], 'protocol_collector_code_sha256': collector['code_sha256'],
         'dependencies': deps, 'bpt_index': s['registration']['bpt_index'], 'manager': _address(s['roles']['manager']),
         'manager_bpt_balance': s['bpt_balances'][s['roles']['manager']],
         'protocol_collector_bpt_balance': s['bpt_balances'][collector['address']],
         'management_aum_fee': s['annual_aum_fee'], 'last_collection_timestamp': _uint(s['last_collection'], 32),
         'provider_fee_id': fee_id, 'live_provider_aum_percentage': s['provider_percentages'][fee_id],
         'cached_protocol_aum_percentage': s['cached_aum_share'], 'swaps_enabled': s['swaps_enabled'],
         'paused': s['paused'], 'vault_paused': s['vault_paused'],
         'vault_pause_window_end': _uint(s['vault_pause_window_end'], 64),
         'vault_buffer_period_end': _uint(s['vault_buffer_period_end'], 64), 'recovery_mode': s['recovery'], 'allowlist_enabled': s['must_allowlist_lps'],
         'pause_window_end': _uint(s['pause_window_end'], 64), 'buffer_period_end': _uint(s['buffer_period_end'], 64),
         'gradual_weight_end': _uint(s['weight_schedule'][1], 64), 'gradual_fee_end': _uint(s['fee_schedule'][1], 64),
         'token_info': infos}
    for key in ('total_supply', 'virtual_supply', 'actual_supply', 'pending_aum'):
        _uint(s[key]); m[key] = s[key]
    for key in ('cash', 'managed'):
        _uint(s['vault_bpt'][key]); m['vault_bpt_' + key] = s['vault_bpt'][key]
    bpt_actual = s['vault_bpt'].get('actual_balance')
    require(_uint(bpt_actual) >= _uint(s['vault_bpt']['cash']), 'Vault BPT actual balance below cash')
    m['vault_bpt_actual_balance'] = bpt_actual
    if origin is not None:
        # This is the original after-header clock, with the origin bank frozen.
        # No fee or swap mathematics is synthesized by this projection.
        m['execution_context'] = {'block_number': number, 'timestamp': timestamp}
    multi = {'code_sha256': pool_identity['code_sha256'], 'vault': s['vault'], 'vault_code_sha256': vault['code_sha256'],
             'pool_id': s['pool_id'], 'assets': assets, 'weights': s['weights'], 'amplification': '0', 'admin_fee': 0, 'managed_v2': m}
    pool = {'chain_key': 'evm:31337', 'pool_id': pool_id, 'protocol': PROTOCOL, 'token0': assets[0]['token'],
            'token1': assets[1]['token'], 'reserve0': assets[0]['balance'], 'reserve1': assets[1]['balance'],
            'fee_numerator': _uint(s['swap_fee'], 64), 'fee_denominator': 10**18,
            'adapter_version': ADAPTER, 'token_behavior': 'standard', 'multi_asset': multi}
    return {'pool': pool, 'authority': 'original-local-node-observation' if origin is None else 'derived-after-state-at-source-header',
            'source_refs': {'snapshot': copy.deepcopy(ref), 'state_header': snap['header'],
                            'origin_snapshot': copy.deepcopy(ref if origin is None else origin),
                            'bank_sha256': BANK_SHA256}}


def project_snapshot(bank, ref, origin=None):
    _qualified(bank)
    return copy.deepcopy(_snapshot(bank, ref, origin))


def _config(pool):
    v = pool['multi_asset']; m = v['managed_v2']
    names = ('factory', 'factory_code_sha256', 'pool_runtime_template_sha256', 'fee_provider',
             'fee_provider_code_sha256', 'protocol_collector', 'protocol_collector_code_sha256',
             'manager', 'provider_fee_id', 'management_aum_fee')
    managed = {'expected_' + key: m[key] for key in names}
    managed['dependencies'] = m['dependencies']
    multi = {'managed_v2': managed, 'expected_code_sha256': v['code_sha256'], 'expected_vault': v['vault'],
             'expected_vault_code_sha256': v['vault_code_sha256'], 'pool_id': v['pool_id'],
             'tokens': [{'token': a['token'], 'expected_code_sha256': a['code_sha256']} for a in v['assets']],
             'token_policy': 'standard-no-bpt'}
    return {'id': pool['pool_id'], 'protocol': PROTOCOL, 'token0': pool['token0'], 'token1': pool['token1'],
            'fee_numerator': pool['fee_numerator'], 'fee_denominator': pool['fee_denominator'], 'multi_asset': multi}


def project_case(bank, case):
    _qualified(bank)
    require(case in bank['cases'], 'case not in qualified original bank')
    execution = case['execution']
    before = _snapshot(bank, execution['before'])
    after = _snapshot(bank, execution['after'])
    derived = _snapshot(bank, execution['after'], origin=execution['before'])
    tx = resolve(bank, execution['transaction'], 'transaction')
    header = resolve(bank, tx['execution_header'], 'header')
    require(derived['pool']['multi_asset']['managed_v2']['execution_context'] ==
            {'block_number': int(header['number'], 16), 'timestamp': int(header['timestamp'], 16)},
            'derived after clock differs from original execution header')
    e = case['expectation']
    params = {'execution_block': int(header['number'], 16), 'execution_timestamp': int(header['timestamp'], 16)}
    if case['operation'] == 'swap':
        params.update(index_in=e['index_in'], index_out=e['index_out'], amount=e['raw_in'])
    elif case['operation'] in ('withdraw', 'deposit'):
        params.update(token_index=e['token_index'], amount=e['amount'])
    else:
        require(case['operation'] == 'collect', 'unknown operation')
    native = {'status': int(tx['receipt']['status'], 16), 'expectation': copy.deepcopy(e)}
    if native['status'] == 0:
        native['rejection'] = 'original-transaction-reverted'
    elif case['operation'] == 'swap':
        native['quantity'] = e['raw_out']
    # Mint deltas are subtraction of original observed ERC20 wallet values only.
    bm = before['pool']['multi_asset']['managed_v2']; am = after['pool']['multi_asset']['managed_v2']
    native['observed_bpt_deltas'] = {key: str(int(am[key]) - int(bm[key])) for key in
                                   ('total_supply', 'manager_bpt_balance', 'protocol_collector_bpt_balance')}
    if case['operation'] == 'collect':
        native['quantity'] = native['observed_bpt_deltas']['total_supply']
    elif case['operation'] in ('withdraw', 'deposit'):
        native['quantity'] = e['amount']
    return copy.deepcopy({'name': case['name'], 'scenario': case['scenario'], 'operation': case['operation'],
        'source_pool': before['pool'], 'observed_pool_after': after['pool'], 'expected_pool_after': derived['pool'],
        'params': params, 'expected_native': native, 'config': _config(before['pool']),
        'authority': {'source_pool': before['authority'], 'observed_pool_after': after['authority'],
                      'expected_pool_after': derived['authority'], 'expected_native': 'original-transaction-and-getter-observations'},
        'source_refs': {'bank_sha256': BANK_SHA256, 'before': execution['before'], 'after': execution['after'],
                        'transaction': execution['transaction'], 'scenario_source_h': case['source_h']}})


def service_eligible(pool, *, now=None, max_future_seconds=30):
    """Clock eligibility only; full process admission remains the service's job."""
    require(type(max_future_seconds) is int and max_future_seconds >= 0, 'invalid future threshold')
    now = time.time() if now is None else now
    m = pool['multi_asset']['managed_v2']
    return (m['timestamp'] <= now + max_future_seconds and m['last_collection_timestamp'] <= m['timestamp']
            and all(t['last_change_block'] <= m['block_number'] for t in m['token_info']))


def project_bank(bank):
    """Thin derivative preserving the entire original producer/compiler closure."""
    _qualified(bank)
    return {'schema_version': 1, 'profile': PROTOCOL, 'adapter_version': ADAPTER,
            'authority': 'derived-public-pool-projection-of-frozen-original-bank',
            'original_bank': {'filename': 'managed-v2-aum.json', 'bytes': BANK_BYTES, 'sha256': BANK_SHA256,
                              'reference': copy.deepcopy(bank['reference'])},
            'identity_manifest': copy.deepcopy(bank._identities),
            'cases': [project_case(bank, c) for c in bank['cases']]}
