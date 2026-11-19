"""Check the Fulu two-period handover into the recorded finality update."""
import argparse
import copy
import json
from pathlib import Path
import subprocess
import tempfile

PIN = '0x48176a8eb6476fee86b5540b0764721cb93572a1d83234db95dbedcc845907c7'
ZERO = '0x'+'00'*32


def replay(binary, evidence, bootstrap, finality, updates):
    recorded = json.loads(updates.read_bytes())
    checks = []
    with tempfile.TemporaryDirectory(prefix='xone-transition-replay-') as tmp:
        proof = Path(tmp) / 'updates.json'
        for name, reason in [('original', None), ('empty', 'BEACON_UPDATES_EMPTY'),
                              ('skipped', 'BEACON_TRANSITION_SLOTS'), ('repeated', 'BEACON_TRANSITION_SLOTS'),
                              ('short', 'BEACON_UPDATE_SLOTS'), ('next-root', 'BEACON_TRANSITION_PROOF'),
                              ('next-branch', 'BEACON_TRANSITION_PROOF'), ('finality-branch', 'BEACON_TRANSITION_PROOF'),
                              ('signature', 'BEACON_UPDATE_SIGNATURE'), ('threshold', 'BEACON_UPDATE_PARTICIPATION')]:
            rows = copy.deepcopy(recorded)
            if name == 'empty':
                rows = []
            elif name == 'skipped':
                rows = rows[1:]
            elif name == 'repeated':
                rows[1] = rows[0]
            elif name == 'short':
                rows = rows[:1]
            elif name == 'next-root':
                rows[1]['data']['next_sync_committee']['pubkeys'][0] = '0x'+'00'*48
            elif name in ('finality-branch', 'next-branch'):
                field = 'finality_branch' if name == 'finality-branch' else 'next_sync_committee_branch'
                rows[1]['data'][field][0] = ZERO
            elif name == 'signature':
                rows[1]['data']['sync_aggregate']['sync_committee_signature'] = '0x'+'00'*96
            elif name == 'threshold':
                rows[1]['data']['sync_aggregate']['sync_committee_bits'] = '0x'+'00'*64
            proof.write_text(json.dumps(rows))
            result = subprocess.run([binary, '--verify', evidence, '--beacon-root', PIN,
                                     '--beacon-bootstrap', bootstrap, '--beacon-finality', finality,
                                     '--beacon-updates', proof], capture_output=True, timeout=20)
            if reason:
                if result.returncode != 1 or result.stdout or result.stderr.decode().strip() != 'PROVENANCE_'+reason:
                    raise ValueError((name, result.returncode, result.stderr))
            else:
                report = json.loads(result.stdout)
                transitions = report['beacon_committee_transitions']
                if (result.returncode or not report['passed'] or report['consensus_verified'] or
                        [step['period'] for step in transitions] != [1913, 1914] or
                        report['beacon_finality']['signers'] != 508 or report['block_number'] != 26448094):
                    raise ValueError((name, report))
            checks.append(name)
    return {'passed': True, 'checks': checks}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ['binary', 'evidence', 'bootstrap', 'finality', 'updates']:
        parser.add_argument(arg, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.binary.resolve(), args.evidence, args.bootstrap, args.finality, args.updates)))
