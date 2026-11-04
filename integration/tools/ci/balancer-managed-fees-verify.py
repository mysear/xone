#!/usr/bin/env python3
"""Validate the bounded original ManagedPool fee corpus and its current receipt.

This verifies bytes and recorded finite-case coverage, not deployment provenance
or completeness of joins, exits, AUM collection, BPT routing, or all pool states.
"""
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/balancer-managed-fees'
REV = 'e281ff40583c39bf1d46c322334b2b13936e0e33'


def require(condition, message):
    if not condition:
        raise ValueError(message)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def verify(root=ROOT):
    ref = root / 'integration/tools/reference/balancer-managed-fees'
    receipt = json.loads((ref / 'generation.json').read_bytes())
    require(receipt.get('passed') is True, 'generation receipt is not successful')
    require(receipt.get('revision') == REV, 'receipt revision')
    for path, digest in receipt['source_and_dependency_sha256'].items():
        require(sha((root / path).read_bytes()) == digest, f'stale input: {path}')
    meta_bytes = (ref / 'runtime-metadata.json').read_bytes()
    require(sha(meta_bytes) == receipt['runtime_metadata_sha256'], 'runtime metadata changed')
    meta = json.loads(meta_bytes)
    require(receipt.get('service_admission') is False, 'reference evidence cannot enable service admission')
    require(meta.get('admission_status') == 'reference-only-over-eip170' and meta.get('eip170_deployable') is False, 'oversized runtime must be reference-only')
    normal = json.loads((ref / 'standard-evm-deployment.json').read_bytes())
    require(normal['code_size_limit_enforced'] is True and normal['deployed'] is False and normal['status'] == 0, 'normal EVM rejection evidence')
    require(normal['normalized_runtime_sha256'] == meta['normalized_runtime_sha256'], 'normal EVM runtime binding')
    require(meta['revision'] == REV and meta['solc'].startswith('0.7.6+'), 'compiler/pin')
    require(meta['source_and_dependency_sha256'] == receipt['source_and_dependency_sha256'], 'input closure')
    template = bytearray.fromhex(meta['deployedBytecode'])
    require(len(template) == meta['runtime_bytes'], 'runtime byte length')
    expected = sorted((s['start'], s['length']) for spans in meta['immutableReferences'].values() for s in spans)
    require(expected == sorted((s['start'], s['length']) for s in meta['immutable_spans']), 'immutable span projection')
    require(len(expected) == 15 and all(length == 32 for _, length in expected), 'immutable span count/width')
    end = 0
    for start, length in expected:
        require(start >= end and start + length <= len(template), 'invalid immutable span')
        template[start:start+length] = bytes(length)
        end = start + length
    require(sha(template) == meta['normalized_runtime_sha256'], 'normalized runtime hash')
    require(set(meta['immutableReferences']) == set(meta['immutable_ast']), 'immutable AST coverage')
    names = [x['name'] for x in meta['immutable_ast'].values()]
    for name in ['_owner', '_vault', '_poolId', '_delegatedProtocolFees', '_fixedProtocolSwapFeePercentage']:
        require(names.count(name) == 1, f'immutable name {name}')
    require(names.count('_protocolFeesCollector') == 2, 'both collector immutables')
    corpus_bytes = (root / 'integration/test/contracts/balancer-managed-fees.json').read_bytes()
    require(corpus_bytes == (root / 'dex-router/test/contracts/balancer-managed-fees.json').read_bytes(), 'corpus copies differ')
    require(sha(corpus_bytes) == receipt['corpus_sha256'], 'corpus changed')
    corpus = json.loads(corpus_bytes)
    require(corpus['reference']['revision'] == REV, 'corpus pin')
    require(corpus['reference']['runtime_normalized_sha256'] == meta['normalized_runtime_sha256'], 'corpus runtime')
    require(len(corpus['cases']) == receipt['cases'] and len(corpus['sequences']) == receipt['sequences'], 'corpus counts')
    require(sum(len(s['steps']) for s in corpus['sequences']) == receipt['steps'], 'step count')
    require({len(c['balances']) for c in corpus['cases']} == {2, 3, 8, 20, 38}, 'token counts')
    require({c['profile'] for c in corpus['cases']} >= {'protocol_only', 'management_only', 'both', 'zero', 'full_management', 'same_recipient'}, 'fee profiles')
    successes = reverts = scheduled = rounding = 0
    for c in corpus['cases'] + [c for s in corpus['sequences'] for c in s['steps']]:
        state = c['state']
        require(0 <= int(state['protocol_swap_fee']) <= 5 * 10**17, 'original protocol cap')
        require(0 <= int(state['management_swap_fee']) <= 10**18, 'original management cap')
        e = c['expected']
        require(e['transaction_hash'].startswith('0x') and len(e['transaction_hash']) == 66, 'transaction receipt missing')
        if e.get('reverted'):
            require(e.get('rollback_verified') is True and e['next_state'] == state, 'rollback not verified')
            reverts += 1
            continue
        internal = c['internal']
        require(int(internal['pre_invariant']) > 0 and int(internal['post_invariant']) > 0, 'partial invariants')
        require(int(e['next_state']['total_supply']) == int(state['total_supply']) + int(e['protocol_bpt']) + int(e['management_bpt']), 'supply transition')
        if state['same_recipient']:
            require(e['next_state']['owner_balance'] == e['next_state']['protocol_collector_balance'], 'coalesced balance fields')
        successes += 1
        scheduled += bool(c.get('weight_schedule'))
        rounding += bool(internal['reconstructed_gross_differs'] or internal['pre_downscale_differs'])
    require(successes >= 100 and reverts >= 30 and scheduled >= 20 and rounding >= 10, 'insufficient finite-case coverage')
    require({g['kind'] for g in corpus['guards']} >= {'fee_mint_overflow_rollback', 'vault_uint112_rollback'}, 'rollback guards')
    return {'passed': True, 'cases': receipt['cases'], 'sequences': receipt['sequences'], 'steps': receipt['steps'], 'successes': successes, 'reverts': reverts, 'scheduled': scheduled, 'rounding_sensitive': rounding, 'corpus_sha256': receipt['corpus_sha256'], 'normalized_runtime_sha256': meta['normalized_runtime_sha256']}


if __name__ == '__main__':
    print(json.dumps(verify(), sort_keys=True))
