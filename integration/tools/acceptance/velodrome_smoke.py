#!/usr/bin/env python3
"""Pinned Velodrome models through normal Go, real PG/Kafka and two Routers.

RPC state is controlled on evm:31337; the unaffected Solana lane uses its fixture
binary. This is neither an OP historical-state nor production admission gate.
"""
import hashlib
import json
from pathlib import Path
import stable_smoke
from weighted_evidence import ROOT, OUT, runtime_fingerprint, write_result

TARGET = OUT / 'core-velodrome-process.json'

def run():
    if not __debug__:
        raise ValueError('SMOKE_REQUIRES_ASSERTIONS_ENABLED')
    before = runtime_fingerprint()
    vectors = ROOT / 'integration/test/contracts/velodrome-v2-quotes.json'
    stable_smoke.NAMES = ['velodrome-v2-volatile', 'velodrome-v2-stable']
    stable_smoke.GOLDEN = json.loads(vectors.read_text())
    report = stable_smoke.run(faults=['aero-pause'], prefix='core-velodrome')
    if runtime_fingerprint() != before:
        raise ValueError('CORE_OR_BINARIES_CHANGED_DURING_SMOKE')
    if report.get('passed') is not True or len(report['checks']) != 5:
        raise ValueError('INCOMPLETE_VELODROME_PROCESS_MATRIX')
    report['checks'] = [
        'velodrome-stable-and-volatile-at-one-block-in-two-router-replicas',
        'both-directions-match-pinned-original-Solidity-reference',
        'scales-fees-factory-and-model-version-preserved-through-public-wire',
        'reorg-atomically-updates-complete-scope-and-canonical-epoch',
        'factory-pause-revokes-scope-and-qualified-baseline-restores',
    ]
    report.update(
        schema_version=1,
        profile='velodrome-b3065d8b-controlled-rpc-normal-go-pg-kafka-two-routers',
        local_chain_key='evm:31337',
        source_tree_sha256=before['source_tree_sha256'],
        runtime_fingerprint=before,
        reference_sha256=hashlib.sha256(vectors.read_bytes()).hexdigest(),
        original_math_cases=216,
        original_sequences=8,
        original_sequence_steps=24,
        authority='controlled RPC fields; original extracted Solidity math; normal Go and Router binaries; actual local PG/Kafka',
        unaffected_solana_lane='fixture-parser-rust, not a new live Solana claim',
        original_full_swap_executed=False,
        real_optimism_deployment_qualified=False,
        all_requested_features_complete=False,
        production_ready=False,
    )
    return report

if __name__ == '__main__':
    try:
        result = run()
    except Exception as error:
        write_result(TARGET, {'passed':False, 'production_ready':False, 'error':str(error), 'error_type':type(error).__name__})
        raise
    write_result(TARGET, result)
    print(json.dumps(result, indent=2))
