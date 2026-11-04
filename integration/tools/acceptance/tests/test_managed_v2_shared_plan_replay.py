"""Selected shared Managed2 plans: original signed history and Vault bytecode only.

XONE_SHARED_REFERENCE optionally selects an absolute, fully validated raw file.
The default is the new service's repository copy; no temporary path is baked in.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[4]
SCRIPT = ROOT / 'integration/tools/ci/managed-v2-shared-plan-replay.cjs'
BANK = ROOT / 'integration/test/contracts/managed-v2-aum.json'
SHARED = Path(os.environ.get('XONE_SHARED_REFERENCE', str(ROOT / 'integration/test/contracts/managed-v2-shared-vault.json')))
NODE = shutil.which('node25.3.0') or '/usr/local/bin/node'


def selected_input(indices=(0,), source=None, recipient=None):
    raw = SHARED.read_bytes()
    shared = json.loads(raw)
    swaps = [c for c in shared['cases'] if c['operation'] == 'swap']
    cases = [swaps[i] for i in indices]
    source = source or cases[0]['source_bank2pools']
    header = shared['banks'][source]['header']
    routes = []
    for case in cases:
        single, funds, *_ = case['execution']['args']
        amount_in, amount_out = case['expected_native']['amount_in'], case['expected_native']['amount_out']
        pool = next(s['pool'] for s in shared['scenarios'] if s['pool_id'] == single['poolId'])
        tokens = [single['assetIn'], single['assetOut']]
        edge = dict(pool_id=pool, protocol='balancer-v2-managed-aum', from_token=tokens[0], to=tokens[1])
        path = dict(tokens=tokens, edges=[edge], amount_in=amount_in, amount_out=amount_out)
        routes.append(dict(amount_in=amount_in, amount_out=amount_out, path=path))
    sell = str(sum(int(r['amount_in']) for r in routes))
    gross = str(sum(int(r['amount_out']) for r in routes))
    request = dict(chainKey='evm:31337', kind='EXACT_IN', sellToken=routes[0]['path']['tokens'][0],
                   buyToken=routes[0]['path']['tokens'][-1], sellAmount=sell, caller=funds['sender'],
                   recipient=recipient or funds['recipient'], verification='FAST')
    quote = dict(chainKey='evm:31337', sellAmount=sell, consumedIn=sell, refund='0', grossBuyAmount=gross,
                 routes=routes, verification=dict(level='INDICATIVE', success=False, verified=False),
                 snapshotRef=dict(block=dict(chain_key='evm:31337', number_or_slot=int(header['number'], 16),
                     hash=header['hash'], parent_hash=header['parentHash'], timestamp_ms=int(header['timestamp'], 16)*1000)))
    return dict(schema_version=1, original_bank_sha256=hashlib.sha256(BANK.read_bytes()).hexdigest(),
                shared_reference_sha256=hashlib.sha256(raw).hexdigest(), request=request, quote=quote)


class ManagedV2SharedPlanReplayTests(unittest.TestCase):
    def run_replay(self, value):
        self.assertTrue(SCRIPT.is_file(), 'missing shared original Vault plan replay')
        p = subprocess.run([NODE, str(SCRIPT)], input=json.dumps(value).encode(), capture_output=True, timeout=180)
        self.assertTrue(p.stdout, p.stderr.decode())
        return p, json.loads(p.stdout)

    def passed(self, value):
        p, out = self.run_replay(value)
        self.assertEqual(p.returncode, 0, p.stderr.decode() + '\n' + str(out.get('error')))
        self.assertTrue(out['passed'])
        self.assertTrue(out['vm_started'])
        for field in ('verified', 'production_ready', 'all_requested_features_complete'):
            self.assertIs(out[field], False)
        self.assertEqual(out['shared_reference_sha256'], value['shared_reference_sha256'])
        self.assertEqual(out['source']['block'], value['quote']['snapshotRef']['block'])
        self.assertEqual(out['source']['getter_records_checked'], 152)
        self.assertLess(out['source']['unique_getter_calls'], 152)
        self.assertEqual(out['prefix_transaction_count'], 28)
        self.assertEqual(out['actual_amount_in'], value['request']['sellAmount'])
        self.assertEqual(out['actual_amount_out'], value['quote']['grossBuyAmount'])
        self.assertEqual(out['selected_route_count'], len(value['quote']['routes']))
        self.assertEqual(len(out['final_state']['pools']), 2)
        for snapshot in (out['source']['state'], out['final_state']):
            self.assertEqual(sum(len(p['getters']) for p in snapshot['pools']), 152)
            for pool in snapshot['pools']:
                for g in pool['getters']:
                    self.assertTrue(g['result'].startswith('0x'))
        for tx in out['transactions']:
            self.assertTrue(tx['signed_transaction'].startswith('0x'))
            if tx['phase'] == 'selected-plan':
                self.assertEqual(tx['receipt']['status'], '0x1')
            else:
                self.assertIn(tx['receipt']['status'], ('0x0', '0x1'))
            self.assertEqual(len(tx['header_sha256']), 64)
            self.assertEqual(len(tx['receipt_sha256']), 64)
        return out

    def test_script_exists(self):
        self.assertTrue(SCRIPT.is_file(), 'missing shared original Vault plan replay')

    def test_single_pool_a_and_b_at_common_h38_and_reverse_at_h40(self):
        for indices, source in (((0,), 'H38'), ((1,), 'H38'), ((2,), 'H40'), ((3,), 'H41')):
            with self.subTest(indices=indices):
                self.passed(selected_input(indices, source))

    def test_two_ordered_routes_share_vault_and_carry_real_wallet_state(self):
        value = selected_input((0, 1), 'H38')
        out = self.passed(value)
        self.assertEqual(out['actual_amount_out'], '468277430204450327644')
        self.assertEqual(out['routes'][0]['after'], out['routes'][1]['before'])
        self.assertEqual(out['native_transaction_count'], 38)
        self.assertEqual(out['plan_transaction_count'], 2)
        self.assertEqual([r['actual_amount_out'] for r in out['routes']],
                         [r['amount_out'] for r in value['quote']['routes']])

    def test_valid_different_recipient_receives_real_output(self):
        recipient = '0x' + '11' * 20
        out = self.passed(selected_input(recipient=recipient))
        step = out['routes'][0]['steps'][0]
        self.assertEqual(step['recipient'], recipient)
        self.assertEqual(int(step['wallets_after']['recipient_buy']) - int(step['wallets_before']['recipient_buy']),
                         int(out['actual_amount_out']))
        self.assertEqual(step['wallets_before']['sender_buy'], step['wallets_after']['sender_buy'])

    def test_consistent_forged_output_fails_only_after_real_execution(self):
        value = selected_input()
        value['quote']['grossBuyAmount'] = '1'
        value['quote']['routes'][0]['amount_out'] = '1'
        value['quote']['routes'][0]['path']['amount_out'] = '1'
        p, out = self.run_replay(value)
        self.assertNotEqual(p.returncode, 0)
        self.assertFalse(out['passed'])
        self.assertTrue(out['vm_started'])
        self.assertEqual(out['routes'][0]['actual_amount_out'], '370247922332383787644')
        self.assertEqual(out['plan_transaction_count'], 1)
        self.assertIn('actual route output', out['error'])

    def test_reused_physical_pool_multihop_and_multiple_routes_use_actual_deltas(self):
        # Probe an unrecorded amount using the original VM, then replay its measured
        # output. This proves deterministic carry, not independent price authority.
        value = selected_input()
        route = value['quote']['routes'][0]
        a, b = route['path']['tokens']
        forward = route['path']['edges'][0]
        reverse = dict(forward, from_token=b, to=a)
        route['path']['tokens'] = [a, b, a, b]
        route['path']['edges'] = [forward, reverse, copy.deepcopy(forward)]
        route['amount_in'] = route['path']['amount_in'] = value['request']['sellAmount'] = value['quote']['sellAmount'] = value['quote']['consumedIn'] = '50000000000000000000'
        route['amount_out'] = route['path']['amount_out'] = value['quote']['grossBuyAmount'] = '1'
        p, probe = self.run_replay(value)
        self.assertNotEqual(p.returncode, 0)
        self.assertTrue(probe['vm_started'], probe.get('error'))
        measured = probe['routes'][0]['actual_amount_out']
        route['amount_out'] = route['path']['amount_out'] = value['quote']['grossBuyAmount'] = measured
        out = self.passed(value)
        steps = out['routes'][0]['steps']
        self.assertEqual(len(steps), 3)
        for left, right in zip(steps, steps[1:]):
            self.assertEqual(left['actual_amount_out'], right['actual_amount_in'])
            self.assertEqual(left['after'], right['before'])
        # Reuse the physical pool across routes too; the second expected output
        # deliberately starts stale and must fail until its actual value is used.
        value = selected_input((0, 0), 'H38')
        p, probe = self.run_replay(value)
        self.assertNotEqual(p.returncode, 0)
        self.assertTrue(probe['vm_started'], probe.get('error'))
        measured = probe['routes'][1]['actual_amount_out']
        second = value['quote']['routes'][1]
        self.assertNotEqual(measured, second['amount_out'])
        second['amount_out'] = second['path']['amount_out'] = measured
        value['quote']['grossBuyAmount'] = str(sum(int(r['amount_out']) for r in value['quote']['routes']))
        self.passed(value)

    def test_original_empty_h43_and_30_day_clock_are_replayed_exactly(self):
        value = selected_input((0,), 'H43')
        p, probe = self.run_replay(value)
        self.assertTrue(probe['vm_started'], probe.get('error'))
        if p.returncode:
            self.assertIn('actual route output', probe['error'])
            measured = probe['routes'][0]['actual_amount_out']
            value['quote']['routes'][0]['amount_out'] = measured
            value['quote']['routes'][0]['path']['amount_out'] = measured
            value['quote']['grossBuyAmount'] = measured
        out = self.passed(value)
        self.assertEqual(out['source']['state']['header']['number'], '0x2b')
        self.assertEqual(out['source']['state']['header']['transactions'], [])
        self.assertEqual(out['native_transaction_count'], 42)
        self.assertEqual(out['final_state']['header']['number'], '0x2c')
        self.assertEqual(out['final_state']['header']['timestamp'], out['source']['state']['header']['timestamp'])

    def test_forged_plan_and_unbounded_inputs_fail_before_vm(self):
        original = selected_input()
        def edge(v): return v['quote']['routes'][0]['path']['edges'][0]
        edits = {
            'source chain': lambda v: v['quote']['snapshotRef']['block'].update(chain_key='evm:1'),
            'quote chain': lambda v: v['quote'].update(chainKey='evm:1'),
            'source hash': lambda v: v['quote']['snapshotRef']['block'].update(hash='0x'+'ff'*32),
            'source parent': lambda v: v['quote']['snapshotRef']['block'].update(parent_hash='0x'+'ff'*32),
            'source time': lambda v: v['quote']['snapshotRef']['block'].update(timestamp_ms=1),
            'source height': lambda v: v['quote']['snapshotRef']['block'].update(number_or_slot=39),
            'caller': lambda v: v['request'].update(caller='0x'+'11'*20),
            'recipient invalid': lambda v: v['request'].update(recipient='not-an-address'),
            'recipient zero': lambda v: v['request'].update(recipient='0x'+'00'*20),
            'pool': lambda v: edge(v).update(pool_id='0x'+'11'*20),
            'protocol': lambda v: edge(v).update(protocol='uniswap-v2'),
            'from': lambda v: edge(v).update(from_token=v['request']['buyToken']),
            'to': lambda v: edge(v).update(to=v['request']['sellToken']),
            'continuity': lambda v: v['quote']['routes'][0]['path']['tokens'].insert(1, v['request']['buyToken']),
            'bpt': lambda v: v['quote']['routes'][0]['path']['tokens'].__setitem__(0, edge(v)['pool_id']),
            'amount sum': lambda v: v['request'].update(sellAmount='1'),
            'path amount': lambda v: v['quote']['routes'][0]['path'].update(amount_in='1'),
            'route output': lambda v: v['quote']['routes'][0].update(amount_out='1'),
            'refund': lambda v: v['quote'].update(refund='1'),
            'empty': lambda v: v['quote']['routes'].clear(),
            'too many routes': lambda v: v['quote'].update(routes=v['quote']['routes']*9),
            'old SHA': lambda v: v.update(original_bank_sha256='0'*64),
            'raw SHA': lambda v: v.update(shared_reference_sha256='0'*64),
            'verified': lambda v: v['quote']['verification'].update(verified=True),
            'verification success': lambda v: v['quote']['verification'].update(success=True),
            'FAST only': lambda v: v['request'].update(verification='VERIFIED'),
            'one quote per VM': lambda v: v.update(quotes=[dict(request=v['request'], quote=v['quote'])]),
        }
        def bounded_path(v, count):
            p = v['quote']['routes'][0]['path']
            a, b = p['tokens'][:2]
            e = p['edges'][0]
            p['tokens'] = [a if i % 2 == 0 else b for i in range(count+1)]
            p['edges'] = [dict(e, from_token=p['tokens'][i], to=p['tokens'][i+1]) for i in range(count)]
        edits['too many path edges'] = lambda v: bounded_path(v, 5)
        def too_many_total_edges(v):
            bounded_path(v, 3)
            route = v['quote']['routes'][0]
            v['quote']['routes'] = [copy.deepcopy(route) for _ in range(8)]
            v['quote']['routes'][-1]['path']['edges'].append(copy.deepcopy(route['path']['edges'][0]))
            v['request']['sellAmount'] = v['quote']['sellAmount'] = v['quote']['consumedIn'] = str(int(route['amount_in'])*8)
            v['quote']['grossBuyAmount'] = str(int(route['amount_out'])*8)
        edits['too many total edges'] = too_many_total_edges
        for amount in ('', '-1', 'NaN', '01', '0', str(2**256), 1):
            edits['noncanonical '+repr(amount)] = lambda v, amount=amount: v['request'].update(sellAmount=amount)
        for name, edit in edits.items():
            with self.subTest(name=name):
                value = copy.deepcopy(original)
                edit(value)
                p, out = self.run_replay(value)
                self.assertNotEqual(p.returncode, 0)
                self.assertFalse(out['passed'])
                self.assertFalse(out['vm_started'], out.get('error'))

    def test_raw_contents_with_matching_digest_still_need_formal_validation(self):
        # An absolute override changes file acquisition, never validation authority.
        import tempfile
        with tempfile.TemporaryDirectory() as directory:
            raw = json.loads(SHARED.read_bytes())
            raw['banks']['H38']['pools'][0]['getters'][0]['result'] = '0x' + '00'*32
            target = Path(directory) / 'tampered.json'
            target.write_text(json.dumps(raw))
            value = selected_input()
            value['shared_reference_sha256'] = hashlib.sha256(target.read_bytes()).hexdigest()
            env = dict(os.environ, XONE_SHARED_REFERENCE=str(target))
            p = subprocess.run([NODE, str(SCRIPT)], input=json.dumps(value).encode(), capture_output=True, env=env, timeout=180)
            out = json.loads(p.stdout)
            self.assertNotEqual(p.returncode, 0)
            self.assertFalse(out['passed'])
            self.assertFalse(out['vm_started'])


if __name__ == '__main__':
    unittest.main()
