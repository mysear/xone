"""Audit one stopped Go owner's HTTP-development archive against observed publications."""
from collections import Counter, defaultdict
import json

from current_v2_evidence import (ENDPOINTS, METHODS, POOL, pairs, provider_id, require,
                                 rpc_observation, sealed_payload)

LIMITS = dict(max_bytes=4 << 30, max_record_bytes=128 << 20, max_records=10000)
ACCOUNTS = {POOL['id'], POOL['v2']['expected_factory'], POOL['token0'], POOL['token1']}
PROVIDERS = {provider_id(url): url for url in ENDPOINTS}


def audit_archive(directory, blocks, endpoints=ENDPOINTS):
    """Check archive integrity and coverage. The running owner verifies MPT and consensus."""
    providers = {provider_id(url): PROVIDERS.get(provider_id(url), provider_id(url)) for url in endpoints}
    body_sources = {provider_id(url) for url in endpoints[:2]}
    require(len(body_sources) == 2 and len(endpoints) in (2, 3), 'CAPTURE_PROVIDER_COUNT')
    proof_sources = {provider_id(endpoints[2])} if len(endpoints) == 3 else body_sources
    methods = defaultdict(Counter)
    headers = defaultdict(set)
    proofs = defaultdict(set)
    derived = {}
    errors = []
    total = count = 0
    for path in directory.glob('*.raw'):
        total += path.lstat().st_size
        count += 1
        require(total <= LIMITS['max_bytes'] and count <= LIMITS['max_records'], 'ARCHIVE_BUDGET')
        owner, data, identity = sealed_payload(path, LIMITS['max_record_bytes'] * 2 + (256 << 10))
        source = owner['ProviderID']
        if source == 'derived-state-from-recorded-rpc':
            block = json.loads(data, object_pairs_hook=pairs)['block']
            require(block['chain_key'] == owner['Chain'] and block['hash'] == owner['Hash']
                    and block['number_or_slot'] == owner['Height'], 'STAGED_BLOCK_IDENTITY')
            derived[block['hash']] = block
            continue
        require(source in providers, 'CAPTURE_PROVIDER')
        method, params, body = rpc_observation(owner, data, identity, METHODS | {'eth_getRawTransactionByHash'})
        methods[source][method] += 1
        try:
            response = json.loads(body, object_pairs_hook=pairs)
        except (UnicodeDecodeError, json.JSONDecodeError):
            response = None
        error = {'provider': providers[source], 'method': method, 'params': params, 'file': path.name}
        if not isinstance(response, dict) or response.get('jsonrpc') != '2.0' or response.get('id') != 1:
            errors.append(dict(error, reason='RPC_ENVELOPE_UNAVAILABLE'))
            continue
        if 'error' in response or 'result' not in response:
            rpc_error = response.get('error')
            if source not in PROVIDERS:
                # Operator endpoints may echo credentials in their free-form diagnostics.
                code = rpc_error.get('code') if isinstance(rpc_error, dict) else None
                rpc_error = {'code': code if type(code) is int else None}
            errors.append(dict(error, rpc_error=rpc_error))
            continue
        result = response['result']
        if method == 'eth_getBlockByNumber' and isinstance(result, dict) and params[1] is True:
            headers[(int(result['number'], 16), result['hash'])].add(source)
        if source in proof_sources and method == 'eth_getProof' and isinstance(result, dict) and isinstance(params[-1], dict):
            if (params[-1].get('requireCanonical') is True and result.get('accountProof')
                    and result.get('address', '').lower() == params[0].lower()):
                proofs[params[-1]['blockHash']].add(params[0].lower())
    gaps = []
    for index, block in enumerate(blocks):
        key = block['hash']
        missing = sorted(ACCOUNTS - proofs[key])
        witnesses = sorted(body_sources - headers[(block['number_or_slot'], key)])
        staged_missing = index > 0 and derived.get(key) != block
        if missing or witnesses or staged_missing:
            gaps.append(dict(height=block['number_or_slot'], missing_accounts=missing,
                             missing_body_providers=witnesses, missing_staged_block=staged_missing))
    return dict(records=count, bytes=total, methods=dict(methods), derived_blocks=len(derived),
                observed_blocks=len(blocks), coverage_gaps=gaps, rpc_errors=errors)
