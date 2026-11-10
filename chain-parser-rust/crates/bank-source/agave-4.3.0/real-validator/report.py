#!/usr/bin/env python3
"""Verify one matching build/check/real-Bank evidence set and hash its outputs."""
import argparse
import base64
import hashlib
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pubkey(text):
    number = 0
    for char in text:
        number = number * 58 + ALPHABET.index(char)
    body = number.to_bytes((number.bit_length() + 7) // 8, 'big')
    result = b'\0' * (len(text) - len(text.lstrip('1'))) + body
    assert len(result) == 32, 'invalid key length'
    return result


def fingerprint(row):
    h = hashlib.sha256(b'xone/solana/account-v1\0')
    for name in ['write_slot', 'write_bank_id']:
        h.update(row[name].to_bytes(8, 'big'))
    h.update(pubkey(row['pubkey']))
    h.update(pubkey(row['owner']))
    h.update(row['lamports'].to_bytes(8, 'big'))
    h.update(bytes([int(row['executable']), 0]))
    h.update(row['rent_epoch'].to_bytes(8, 'big'))
    h.update(row['write_version'].to_bytes(8, 'big'))
    data = base64.b64decode(row['data_base64'], validate=True)
    h.update(len(data).to_bytes(8, 'big'))
    h.update(data)
    signature = row['txn_signature_base64']
    h.update(bytes([int(signature is not None)]))
    if signature is not None:
        raw = base64.b64decode(signature, validate=True)
        assert len(raw) == 64
        h.update(raw)
    return h.hexdigest()


def same_account(row, raw):
    for key in ['pubkey', 'owner', 'lamports', 'executable', 'rent_epoch']:
        assert row[key] == raw[key], (key, row[key], raw[key])
    assert base64.b64decode(row['data_base64'], validate=True) == bytes(raw['raw_data'])


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--build', required=True, type=Path)
    p.add_argument('--check', required=True, type=Path)
    p.add_argument('--test', required=True, type=Path)
    p.add_argument('--kernel-sha256', required=True)
    p.add_argument('--output', required=True, type=Path)
    args = p.parse_args()
    stages = {name: json.loads(path.read_text()) for name, path in
              [('build', args.build), ('check', args.check), ('test', args.test)]}
    pin = json.loads((HERE.parent/'source-pin.json').read_text())
    assert digest(HERE.parent/'native-observer.patch') == pin['patch_sha256']
    assert digest(HERE.parents[1]/'src/lib.rs') == args.kernel_sha256
    for name, stage in stages.items():
        assert stage['status'] == 'passed' and stage['exit_code'] == 0, (name, stage['status'])
        assert not stage['timed_out']
        assert stage['kernel_sha256'] == stage['kernel_sha256_after'] == args.kernel_sha256
        assert stage['patch_sha256'] == pin['patch_sha256']
        assert stage['pin_sha256'] == digest(HERE.parent/'source-pin.json')
        assert stage['harness_sha256'] == digest(HERE/'bank_export.rs')
        assert stage['rustc_version'].startswith('rustc 1.97.1 ')
        assert stage['removed_or_changed_upstream_lock_inputs'] == []
    assert len({s['archive_sha256'] for s in stages.values()}) == 1
    assert len({s['lock_sha256'] for s in stages.values()}) == 1
    root = Path(stages['test']['environment']['XONE_REAL_BANK_OUTPUT'])
    result = json.loads((root/'real-bank-result.json').read_text())
    assert result['status'] == 'passed'
    for key in ['bank_ticks_complete', 'env_initializer', 'native_zero_preserved', 'repeated_freeze', 'fork_isolation']:
        assert result[key] is True, key
    assert result['abandoned_unfrozen_banks_dropped'] == 16
    trace = json.loads((root/'native-notifications.json').read_text())
    assert len(trace) == 21, len(trace)
    assert trace[0]['write_version'] == 0 and trace[0]['pubkey'] == result['changed']
    assert trace[0]['bank_id'] == result['parent']['bank_id']
    assert all(a['write_version'] < b['write_version'] for a, b in zip(trace, trace[1:]))
    event_index = {(r['slot'], r['bank_id'], r['pubkey'], r['write_version']): r for r in trace}
    assert len(event_index) == len(trace)
    retired = [r for r in trace if 10 <= r['slot'] < 26]
    assert len(retired) == len({r['bank_id'] for r in retired}) == 16
    manifests = []
    for name, expected_updates in [('child', 2), ('sibling', 1)]:
        bank = result[name]
        assert bank['complete'] and bank['frozen']
        path = root/'manifests'/f"real-bank-{bank['slot']}-{bank['bank_id']}.json"
        m = json.loads(path.read_text())
        assert m['version'] == 2 and m['expected_account_updates'] == expected_updates
        assert len(m['native_writes']) == expected_updates
        for key in ['slot', 'bank_id', 'blockhash']:
            assert m['bank'][key] == bank[key]
            assert m['parent'][key] == result['parent'][key]
        assert len(m['parent_accounts']) == len(m['final_versions']) == 2
        rows = {r['pubkey']: r for r in m['parent_accounts']}
        same_account(rows[result['changed']], result['bank_reads']['parent_changed'])
        assert rows[result['changed']]['write_version'] == 0
        for write in m['native_writes']:
            assert write['write_slot'] == bank['slot'] and write['write_bank_id'] == bank['bank_id']
            raw = event_index[(write['write_slot'], write['write_bank_id'], write['pubkey'], write['write_version'])]
            same_account(write, raw)
            rows[write['pubkey']] = write
        same_account(rows[result['changed']], result['bank_reads'][name+'_changed'])
        same_account(rows[result['inherited']], result['bank_reads']['child_inherited'])
        for version in m['final_versions']:
            row = rows[version['pubkey']]
            assert version['write_version'] == row['write_version']
            assert version['account_sha256'] == fingerprint(row)
        manifests.append({'path': str(path), 'sha256': digest(path), 'bank_id': bank['bank_id'],
                          'slot': bank['slot'], 'native_updates': expected_updates})
    outputs = {str(path.relative_to(root)): digest(path) for path in root.rglob('*') if path.is_file()}
    report = {
        'schema_version': 1, 'status': 'passed', 'real_bank_export_passed': True,
        'kernel_sha256': args.kernel_sha256, 'patch_sha256': pin['patch_sha256'],
        'source_pin_sha256': digest(HERE.parent/'source-pin.json'), 'harness_sha256': digest(HERE/'bank_export.rs'),
        'archive_sha256': stages['test']['archive_sha256'], 'lock_sha256': stages['test']['lock_sha256'],
        'stage_results': {'build': str(args.build), 'check': str(args.check), 'test': str(args.test)},
        'actual_bank_result': result, 'raw_native_callbacks': len(trace), 'zero_native_version_preserved': True,
        'manifests': manifests, 'evidence_files_sha256': outputs,
        'required_private_user_inputs': [], 'production_ready': False,
        'scope': 'Pinned native libraries and one real staked-genesis BankForks/notifier/freeze/drop integration; no mainnet sync or transaction-execution claim.',
    }
    args.output.write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({'status': report['status'], 'kernel_sha256': args.kernel_sha256,
                      'raw_native_callbacks': len(trace), 'manifest_count': len(manifests)}, indent=2))


if __name__ == '__main__':
    main()
