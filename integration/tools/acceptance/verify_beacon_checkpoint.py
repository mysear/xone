"""Replay the historical mainnet checkpoint and check the CLI's rejection paths."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = '0xd90307852f840a1dba1a8e9bb015ae5c2c6deda28a1fca280449550e53651949'
ZERO = '0x' + '00' * 32
EXECUTION = '0xbcbf29e3d91c7c707827ae95fbaced24fe8df6b704b0db277d16f45f40e50ffd'


def replay(binary, evidence, bootstrap):
    captured = bootstrap.read_bytes()
    original = json.loads(captured)
    checks = []
    with tempfile.TemporaryDirectory(prefix='xone-beacon-replay-') as tmp:
        proof = Path(tmp) / 'proof.json'
        for name, path, replacement, reason in [
            ('original', (), None, None),
            ('other-root', (), None, 'BEACON_CHECKPOINT_MISMATCH'),
            ('other-evidence', (), None, 'BEACON_EXECUTION_HASH'),
            ('wrong-fork', ('version',), 'deneb', 'BEACON_FORK_OR_HEADER'),
            ('beacon-body', ('data', 'header', 'beacon', 'body_root'), ZERO, 'BEACON_CHECKPOINT_MISMATCH'),
            ('payload', ('data', 'header', 'execution', 'state_root'), ZERO, 'BEACON_PAYLOAD_BRANCH'),
            ('missing-amount', ('data', 'header', 'execution', 'gas_used'), None, 'BEACON_PAYLOAD_FIELDS'),
            ('overlong-extra', ('data', 'header', 'execution', 'extra_data'), '0x' + '22'*33, 'BEACON_EXTRA_DATA'),
            ('timestamp', ('data', 'header', 'execution', 'timestamp'), '1', 'BEACON_SLOT_TIME'),
            ('payload-branch', ('data', 'header', 'execution_branch', 1), ZERO, 'BEACON_PAYLOAD_BRANCH'),
            ('committee-branch', ('data', 'current_sync_committee_branch', 0), ZERO, 'BEACON_COMMITTEE_BRANCH'),
            ('committee-key', ('data', 'current_sync_committee', 'pubkeys', 0), '0x'+'00'*48, 'BEACON_COMMITTEE_BRANCH'),
        ]:
            body = copy.deepcopy(original)
            parent = body
            for key in path[:-1]:
                parent = parent[key]
            if path:
                parent[path[-1]] = replacement
            proof.write_text(json.dumps(body))
            bundle = evidence
            if name == 'other-evidence':
                bundle = Path(tmp) / 'evidence.json'
                altered = json.loads(evidence.read_bytes())
                altered['expected_block_hash'] = ZERO
                bundle.write_text(json.dumps(altered))
            pin = EXECUTION if name == 'other-root' else ROOT
            result = subprocess.run([binary, '--verify', bundle, '--beacon-root', pin,
                                     '--beacon-bootstrap', proof], capture_output=True, timeout=20)
            if reason:
                if result.returncode != 1 or result.stdout or result.stderr.decode().strip() != 'PROVENANCE_' + reason:
                    raise ValueError((name, result.returncode, result.stderr))
            else:
                report = json.loads(result.stdout)
                binding = report['beacon_checkpoint']
                if (result.returncode or not report['passed'] or report['consensus_verified'] or
                        binding['execution_hash'] != EXECUTION or report['block_hash'] != EXECUTION or
                        binding['root'] != ROOT or binding['slot'] != 15595392):
                    raise ValueError((name, report))
            checks.append(name)
    return {'bootstrap_sha256': hashlib.sha256(captured).hexdigest(), 'checks': checks, 'passed': True}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ['binary', 'evidence', 'bootstrap']:
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.binary.resolve(), args.evidence, args.bootstrap)))
