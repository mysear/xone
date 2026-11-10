#!/usr/bin/env python3
"""Check isolated LM math and original-runtime authority before profile admission."""
from pathlib import Path
import argparse
import copy
import hashlib
import json
import os
import subprocess
import sys
import tempfile
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'acceptance'))
from weighted_evidence import ROOT, OUT, require, sha, source_hash, write_result

REFERENCE = OUT / 'core-pancake-v3-lm-original.json'
DIRECTORY = ROOT / 'integration/tools/reference/pancake-v3-lm'


def validate_authority(reference, current_artifacts):
    metadata_path = DIRECTORY / 'source-metadata.json'
    metadata = json.loads(metadata_path.read_text())
    require(reference['source_commit'] == metadata['commit']
            and reference['source_metadata_sha256'] == sha(metadata_path), 'PANCAKE_REFERENCE_SOURCE_BINDING')
    for entry in metadata['files'] + metadata['dependencies']['openzeppelin']['files']:
        require(sha(DIRECTORY / entry['path']) == entry['sha256'], 'PANCAKE_REFERENCE_SOURCE_CHANGED')
    require(reference['compilation'] == current_artifacts['compilation']
            and reference['compilers'] == current_artifacts['compilers']
            and reference['source_metadata_sha256'] == current_artifacts['source_metadata_sha256'],
            'PANCAKE_RECOMPILED_INPUT_BINDING')
    for role in ['pool', 'lm', 'masterChef', 'factory', 'token0', 'token1', 'caller']:
        require(reference['artifacts'][role] == current_artifacts['artifacts'][role],
                'PANCAKE_RECOMPILED_ARTIFACT_CHANGED:' + role)
        artifact = reference['artifacts'][role]['evm']['deployedBytecode']
        observed = bytearray.fromhex(reference['deployment'][role]['code'][2:])
        compiled = bytes.fromhex(artifact['object'])
        require(len(observed) == len(compiled) and len(observed) > 2, 'PANCAKE_RUNTIME_LENGTH:' + role)
        for refs in artifact['immutableReferences'].values():
            for ref in refs:
                start, size = ref['start'], ref['length']
                require(size == 32 and 0 <= start <= len(observed)-size, 'PANCAKE_IMMUTABLE_RANGE')
                observed[start:start+size] = bytes(size)
        require(bytes(observed) == compiled, 'PANCAKE_RUNTIME_COMPILED_SOURCE:' + role)
        if role in reference['layouts']:
            require(reference['layouts'][role] == reference['artifacts'][role]['storageLayout'], 'PANCAKE_RUNTIME_LAYOUT')
    return True


def run(binary):
    reference = json.loads(REFERENCE.read_text())
    before = source_hash()
    artifact_path = OUT / 'core-pancake-v3-lm-current-artifacts.json'
    artifact_log = OUT / 'core-pancake-v3-lm-current-artifacts.log'
    with artifact_log.open('w') as stream:
        compiled = subprocess.run(['node', str(ROOT/'integration/tools/ci/pancake-v3-lm-reference.cjs'),
                                   '--artifacts-only', str(artifact_path)], stdout=stream,
                                  stderr=subprocess.STDOUT, timeout=300)
    require(compiled.returncode == 0, 'PANCAKE_CURRENT_ARTIFACT_COMPILATION')
    current_artifacts = json.loads(artifact_path.read_text())
    validate_authority(reference, current_artifacts)
    binary = Path(binary).resolve()
    binary_digest = sha(binary)
    reference_digest = sha(REFERENCE)
    draft_digest = sha(DIRECTORY/'draft/src/main.rs')
    output = OUT / 'core-pancake-v3-lm-draft-final.json'
    log = OUT / 'core-pancake-v3-lm-draft-final.log'
    with log.open('w') as stream:
        result = subprocess.run([str(binary), str(REFERENCE), str(output)], stdout=stream, stderr=subprocess.STDOUT, timeout=180)
    require(result.returncode == 0, 'PANCAKE_DRAFT_DIFFERENTIAL_FAILED')
    projection = json.loads(output.read_text())
    require(projection.get('passed') is True, 'PANCAKE_DRAFT_DIFFERENTIAL_RECEIPT')
    checks = []
    complete = next(i for i,c in enumerate(reference['cases']) if not c['expected']['reverted'] and c['expected']['consumed_in'] == c['amount_in'] and int(c['amount_in']) >= 10**9)
    crossing = next(i for i,c in enumerate(reference['cases']) if c['amount_in'] == '20000000000' and not c['expected']['reverted'])
    def amount_mutation(document, path):
        node = document
        for key in path[:-1]:
            node = node[key]
        node[path[-1]] = str(int(node[path[-1]])+1)
    mutations = [
        ('recipient-output', ['cases', complete, 'expected', 'amount_out']),
        ('reward-growth', ['cases', complete, 'expected', 'after', 'lm', 'rewardGrowthGlobalX128']),
        ('lm-cross-liquidity', ['cases', crossing, 'expected', 'after', 'lm', 'liquidity']),
        ('lp-fee-growth', ['cases', crossing, 'expected', 'after', 'feeGrowthGlobal0X128']),
        ('protocol-fees', ['cases', crossing, 'expected', 'after', 'protocolFees', 'token0']),
        ('oracle-cumulative', ['cases', crossing, 'expected', 'after', 'observations', 1, 'secondsPerLiquidityCumulativeX128']),
        ('carried-bank', ['sequences', 0, 'steps', 1, 'before', 'lm', 'rewardGrowthGlobalX128']),
    ]
    with tempfile.TemporaryDirectory(prefix='xone-pancake-lm-guards-') as temporary:
        temporary = Path(temporary)
        for name, path in mutations:
            document = copy.deepcopy(reference)
            amount_mutation(document, path)
            source, destination = temporary / (name+'.json'), temporary / (name+'-out.json')
            write_result(source, document)
            process = subprocess.run([str(binary), str(source), str(destination)], capture_output=True, text=True, timeout=180)
            require(process.returncode != 0 and not destination.exists(), 'PANCAKE_DRAFT_MUTATION_ACCEPTED:' + name)
            checks.append(dict(name=name, rejected=True, output_sha256=hashlib.sha256((process.stdout+process.stderr).encode()).hexdigest()))
    document = copy.deepcopy(reference)
    document['deployment']['lm']['code'] = document['deployment']['lm']['code'][:10] + 'ff' + document['deployment']['lm']['code'][12:]
    try:
        validate_authority(document, current_artifacts)
    except ValueError:
        checks.append(dict(name='runtime-source-binding', rejected=True))
    else:
        raise ValueError('PANCAKE_MUTATED_RUNTIME_ACCEPTED')
    document = copy.deepcopy(reference)
    document['artifacts']['lm']['evm']['deployedBytecode']['object'] = document['deployment']['lm']['code'][2:]
    try:
        validate_authority(document, current_artifacts)
    except ValueError:
        checks.append(dict(name='artifact-source-binding', rejected=True))
    else:
        raise ValueError('PANCAKE_MUTATED_ARTIFACT_ACCEPTED')
    require(source_hash() == before and sha(binary) == binary_digest
            and sha(REFERENCE) == reference_digest and sha(DIRECTORY/'draft/src/main.rs') == draft_digest, 'PANCAKE_DRAFT_SOURCE_CHANGED')
    report = dict(schema_version=1, passed=True, source_tree_sha256=before,
                  authority='isolated calculation draft; complete unchanged original Pool/LM/MasterChef code checked; no normal source/wire/routing admission',
                  reference=dict(path=str(REFERENCE.relative_to(ROOT)), sha256=sha(REFERENCE)),
                  current_artifacts=dict(path=str(artifact_path.relative_to(ROOT)), sha256=sha(artifact_path)),
                  draft_source=dict(path=str((DIRECTORY/'draft/src/main.rs').relative_to(ROOT)), sha256=sha(DIRECTORY/'draft/src/main.rs')),
                  draft_lock_sha256=sha(DIRECTORY/'draft/Cargo.lock'), binary_sha256=binary_digest,
                  projection=projection, negative_controls=checks, normal_profile_implemented=False,
                  all_requested_features_complete=False, production_ready=False)
    write_result(OUT/'core-pancake-v3-lm-draft-acceptance.json', report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    args = parser.parse_args()
    print(json.dumps(run(args.binary), indent=2))
