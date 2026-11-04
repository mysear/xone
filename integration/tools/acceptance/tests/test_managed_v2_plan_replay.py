"""Independent original Vault transaction replay for the selected single hop."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[4]
SCRIPT = ROOT / 'integration/tools/ci/managed-v2-aum-plan-replay.cjs'
BANK = ROOT / 'integration/test/contracts/managed-v2-aum.json'


def selected_input():
    bank = json.loads(BANK.read_text())
    resolve = lambda ref: bank['objects'][ref['ref']]['value']
    rows = []
    for case in bank['cases'][:2]:
        state = resolve(case['execution']['before'])
        header = resolve(state['header'])
        expectation = case['expectation']
        tx = resolve(case['execution']['transaction'])
        assets = [resolve(ref)['address'] for ref in state['state']['underlying']]
        tokens = [assets[expectation['index_in']], assets[expectation['index_out']]]
        request = dict(chainKey='evm:31337', kind='EXACT_IN', sellToken=tokens[0], buyToken=tokens[1],
                       sellAmount=expectation['raw_in'], caller=tx['transaction']['from'],
                       recipient=state['state']['roles']['trader'], verification='FAST')
        path = dict(tokens=tokens, amount_in=expectation['raw_in'], amount_out=expectation['raw_out'],
                    edges=[dict(pool_id=state['state']['pool'], protocol='balancer-v2-managed-aum',
                                from_token=tokens[0], to=tokens[1])])
        quote = dict(snapshotRef=dict(block=dict(chain_key='evm:31337', number_or_slot=int(header['number'], 16),
                     hash=header['hash'], parent_hash=header['parentHash'], timestamp_ms=int(header['timestamp'], 16)*1000)),
                     consumedIn=expectation['raw_in'], grossBuyAmount=expectation['raw_out'], refund='0',
                     routes=[dict(path=path, amount_in=expectation['raw_in'], amount_out=expectation['raw_out'])])
        rows.append(dict(case=case['name'], request=request, quote=quote))
    return dict(schema_version=1, original_bank_sha256=hashlib.sha256(BANK.read_bytes()).hexdigest(), quotes=rows)


class ManagedV2PlanReplayTests(unittest.TestCase):
    def run_replay(self, value):
        self.assertTrue(SCRIPT.is_file(), 'missing independent original Vault plan replay')
        process = subprocess.run(['node', str(SCRIPT)], input=json.dumps(value).encode(), capture_output=True, timeout=120)
        output = json.loads(process.stdout)
        return process, output

    def test_selected_native_routes_reexecute_with_real_input_and_recipient_balances(self):
        process, value = self.run_replay(selected_input())
        self.assertEqual(process.returncode, 0, process.stderr.decode() + '\n' + json.dumps(value))
        self.assertTrue(value['passed'])
        self.assertFalse(value['verified'])
        self.assertEqual(value['selected_route_count'], 2)
        self.assertEqual(value['native_transaction_count'], 28)
        for row in value['routes']:
            self.assertEqual(row['actual_amount_in'], row['quoted_amount_in'])
            self.assertEqual(row['actual_amount_out'], row['quoted_amount_out'])
            self.assertGreater(row['before_getters_checked'], 70)
            self.assertGreater(row['after_getters_checked'], 70)
            self.assertTrue(row['complete_original_poststate_matches'])

    def test_forged_native_plan_parameters_fail_before_vm_start(self):
        original = selected_input()
        edits = [lambda v: v['quotes'][0]['request'].update(caller='0x'+'11'*20),
                 lambda v: v['quotes'][0]['request'].update(recipient='0x'+'11'*20),
                 lambda v: v['quotes'][0]['quote']['routes'][0].update(amount_in='1'),
                 lambda v: v['quotes'][0]['quote']['routes'][0]['path']['edges'][0].update(pool_id='0x'+'11'*20),
                 lambda v: v['quotes'][0]['quote']['snapshotRef']['block'].update(hash='0x'+'ff'*32),
                 lambda v: v.update(original_bank_sha256='0'*64),
                 lambda v: v['quotes'].clear()]
        for edit in edits:
            value = copy.deepcopy(original)
            edit(value)
            process, result = self.run_replay(value)
            self.assertNotEqual(process.returncode, 0)
            self.assertFalse(result['passed'])
            self.assertFalse(result['vm_started'])


if __name__ == '__main__':
    unittest.main()
