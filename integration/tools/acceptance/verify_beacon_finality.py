"""Offline finality regression using the captured Fulu deployment and consensus responses."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

ROOT = '0xe8c92887b96654f4756fe7ac8e67b6fb7fa7170779953dd79276ef7d92626837'
EXECUTION = '0x229e4daabd79adb69b56a3e94cddea29e22bcc6daf987c76b9b6a215488efb2b'
ZERO = '0x' + '00' * 32


def replay(binary, evidence, bootstrap, update):
    recorded = json.loads(update.read_bytes())
    checks = []
    with tempfile.TemporaryDirectory(prefix='xone-beacon-finality-') as tmp:
        proof = Path(tmp) / 'update.json'
        for name, path, replacement, reason in [
            ('original', (), None, None),
            ('fork', ('version',), 'deneb', 'BEACON_UPDATE_FORK'),
            ('next-period', ('data', 'signature_slot'), '15695872', 'BEACON_UPDATE_SLOTS'),
            ('out-of-order', ('data', 'signature_slot'), '15687981', 'BEACON_UPDATE_SLOTS'),
            ('before-pin', ('data', 'finalized_header', 'beacon', 'slot'), '15687679', 'BEACON_UPDATE_SLOTS'),
            ('finality-branch', ('data', 'finality_branch', 0), ZERO, 'BEACON_FINALITY_BRANCH'),
            ('finalized-payload', ('data', 'finalized_header', 'execution_branch', 0), ZERO, 'BEACON_PAYLOAD_BRANCH'),
            ('attested-payload', ('data', 'attested_header', 'execution_branch', 0), ZERO, 'BEACON_PAYLOAD_BRANCH'),
            ('attested-state', ('data', 'attested_header', 'beacon', 'state_root'), ZERO, 'BEACON_FINALITY_BRANCH'),
            ('wrong-signed-header', ('data', 'attested_header', 'beacon', 'parent_root'), ZERO, 'BEACON_UPDATE_SIGNATURE'),
            ('no-supermajority', ('data', 'sync_aggregate', 'sync_committee_bits'), '0x' + 'ff'*42 + '1f' + '00'*21, 'BEACON_UPDATE_PARTICIPATION'),
            ('wrong-members', ('data', 'sync_aggregate', 'sync_committee_bits'), '0x'+'ff'*64, 'BEACON_UPDATE_SIGNATURE'),
            ('signature', ('data', 'sync_aggregate', 'sync_committee_signature'), '0x'+'00'*96, 'BEACON_UPDATE_SIGNATURE'),
        ]:
            body = copy.deepcopy(recorded)
            parent = body
            for key in path[:-1]:
                parent = parent[key]
            if path:
                parent[path[-1]] = replacement
            proof.write_text(json.dumps(body))
            proc = subprocess.run([binary, '--verify', evidence, '--beacon-root', ROOT,
                                   '--beacon-bootstrap', bootstrap, '--beacon-finality', proof],
                                  capture_output=True, timeout=20)
            if reason:
                if proc.returncode != 1 or proc.stdout or proc.stderr.decode().strip() != 'PROVENANCE_'+reason:
                    raise ValueError((name, proc.returncode, proc.stderr))
            else:
                report = json.loads(proc.stdout)
                finality = report['beacon_finality']
                if (proc.returncode or not report['passed'] or report['consensus_verified'] or
                        report['beacon_checkpoint']['root'] != ROOT or report['block_number'] != 26448094 or
                        report['transactions'] != 100 or report['block_hash'] != EXECUTION or
                        finality['execution_hash'] != EXECUTION or finality['signers'] != 508 or
                        finality['finalized_slot'] != 15687904 or finality['signature_slot'] != 15687982):
                    raise ValueError((name, report))
            checks.append(name)
    return {'passed': True, 'checks': checks, 'inputs': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                                                         for p in (evidence, bootstrap, update)}}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ['binary', 'evidence', 'bootstrap', 'update']:
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.binary.resolve(), args.evidence, args.bootstrap, args.update)))
