"""Signed original Recovery transitions, ordinary swaps and zero AUM collection."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import test_managed_v2_paused_vault_reference as paused

ROOT = paused.ROOT
SCRIPT = ROOT / 'integration/tools/ci/managed-v2-aum-recovery-golden.cjs'
HELPER = paused.SCRIPT
OUTPUTS = [ROOT / p / 'test/contracts/managed-v2-aum-recovery.json'
           for p in ('integration', 'chain-parser-go', 'dex-router')]
NAMES = ['vault-paused-swap', 'vault-paused-collection', 'swap-after-vault-buffer',
         'enable-recovery', 'recovery-token-swap', 'recovery-zero-collection', 'disable-recovery']
managed, getter, fingerprint = paused.managed, paused.getter, paused.fingerprint


class RecoveryReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix='xone-recovery-test-')
        cls.output = Path(cls.tmp.name) / 'reference.json'
        cls.frozen = paused.BANKS + paused.OUTPUTS + list(
            (ROOT / 'integration/tools/reference/balancer-managed-v2-aum').rglob('*'))
        cls.frozen_before = fingerprint(cls.frozen)
        result = cls.run_cli('--output', str(cls.output))
        if result.returncode:
            raise AssertionError(result.stdout + result.stderr)
        cls.value = json.loads(cls.output.read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    @staticmethod
    def run_cli(*args):
        return subprocess.run(['node', str(SCRIPT), *args], cwd=ROOT, capture_output=True,
                              text=True, timeout=180, env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})

    def test_inventory_original_authority_and_script_dependencies(self):
        v = self.value; ref = v['reference']
        self.assertEqual(v['public_adapter_version'], paused.ADAPTER)
        self.assertEqual(v['schema_version'], 1)
        self.assertEqual(v['original_bank_sha256'], paused.SHA)
        self.assertEqual(v['authority'], 'independent-local-original-vault-pause-recovery-reference')
        self.assertEqual([c['name'] for c in v['cases']], NAMES)
        self.assertEqual([c['operation'] for c in v['cases'][3:]], ['control', 'swap', 'collect', 'control'])
        self.assertEqual([c['expected_native']['status'] for c in v['cases']], [0, 1, 1, 1, 1, 1, 1])
        self.assertEqual(ref['source_script_sha256'], hashlib.sha256(HELPER.read_bytes()).hexdigest())
        self.assertEqual(ref['wrapper_sha256'], hashlib.sha256(SCRIPT.read_bytes()).hexdigest())
        self.assertEqual(ref['script_dependencies'], [
            {'path': str(p.relative_to(ROOT)), 'sha256': hashlib.sha256(p.read_bytes()).hexdigest()}
            for p in (HELPER, SCRIPT)])
        bank = json.loads(paused.BANKS[0].read_bytes())
        for key in ('node_closure', 'chain_options'):
            self.assertEqual(ref[key], bank['reference'][key])
        self.assertEqual(ref['raw_prefix_transactions'], 28)
        self.assertEqual(len(ref['original_prefix_hashes']), 28)
        self.assertTrue(ref['original_prefix_receipts_and_headers_matched'])
        self.assertEqual(v['cases'][:3], json.loads(paused.OUTPUTS[0].read_bytes())['cases'])
        self.assertNotIn('wrapper_sha256', json.loads(paused.OUTPUTS[0].read_bytes())['reference'])
        self.assertEqual(fingerprint(self.frozen), self.frozen_before)

    def test_complete_fixed_header_original_getters_and_registered_bpt(self):
        # Reuse the original 39-field/ABI assertions for every before and after snapshot.
        check = paused.PausedVaultReferenceTests()
        check.value = self.value
        check.test_every_numeric_state_has_fixed_block_original_getter_evidence()
        for c in self.value['cases']:
            for side in ('source_pool', 'observed_pool_after'):
                p = c[side]; m = managed(p)
                self.assertEqual(m['bpt_index'], 0)
                self.assertGreater(int(m['vault_bpt_cash']), 0)  # Original preminted BPT cash.
                self.assertEqual(m['vault_bpt_managed'], '0')
                for info in m['token_info']:
                    self.assertEqual(set(info), {'cash', 'managed', 'vault_actual_balance',
                                               'asset_manager', 'last_change_block', 'circuit_breaker_enabled'})
                self.assertEqual(m['management_aum_fee'], '0' if m['recovery_mode'] else '10000000000000000')

    def test_authorizer_action_ids_and_real_signed_branch_receipts(self):
        v = self.value; headers = v['headers']; by_hash = {h['hash']: h for h in headers}
        self.assertEqual(int(headers[0]['number'], 16), 28)
        for a, b in zip(headers, headers[1:]):
            self.assertEqual(int(b['number'], 16), int(a['number'], 16)+1)
            self.assertEqual(b['parentHash'], a['hash'])
        txs = v['reference']['setup_transactions'] + [c['transaction'] for c in v['cases']]
        self.assertEqual(len(v['receipts']), 11)
        self.assertEqual(len(txs), 11)
        self.assertEqual({t['receipt']['transactionHash'] for t in txs},
                         {r['transactionHash'] for r in v['receipts']})
        for t in txs:
            receipt = t['receipt']; h = by_hash[receipt['blockHash']]
            self.assertIn(receipt, v['receipts'])
            self.assertEqual(h, t['execution_header'])
            self.assertEqual(h['number'], receipt['blockNumber'])
            self.assertIn(receipt['transactionHash'], h['transactions'])
            self.assertEqual(t['transaction']['hash'], receipt['transactionHash'])
            self.assertEqual(h['parentHash'], t['pre_header']['hash'])
            self.assertTrue(t['signed_transaction'].startswith('0x'))
            self.assertEqual(int(t['transaction']['type'], 16), 0)
            self.assertEqual(int(t['transaction']['gasPrice'], 16), 2000000000)
            self.assertEqual(int(t['transaction']['gas'], 16), 10000000)
        grants = v['reference']['setup_transactions'][2:]
        self.assertEqual([t['method'] for t in grants], ['grantRole', 'grantRole'])
        pool = v['cases'][3]['source_pool']['pool_id']
        rows = [g for g in v['reference']['setup_getters'] if g['to'] == pool and g['method'] == 'getActionId']
        self.assertEqual(len(rows), 2)
        for row, grant, control in zip(rows, grants, (v['cases'][3], v['cases'][6])):
            self.assertIn(row['block_tag'], [h['number'] for h in headers])
            self.assertEqual(grant['args'][0], row['result'])
            self.assertEqual(grant['args'][1], control['transaction']['transaction']['from'])
            self.assertEqual(row['args'], [control['transaction']['transaction']['input'][:10]])

    def test_four_recovery_transitions_wallet_deltas_and_zero_mint(self):
        enable, swap, collect, disable = self.value['cases'][3:]
        self.assertGreater(int(managed(enable['source_pool'])['pending_aum']), 0)
        self.assertEqual([managed(c['source_pool'])['recovery_mode'] for c in (enable, swap, collect, disable)],
                         [False, True, True, True])
        self.assertEqual([managed(c['observed_pool_after'])['recovery_mode'] for c in (enable, swap, collect, disable)],
                         [True, True, True, False])
        for c in (enable, swap, collect, disable):
            b, a = managed(c['source_pool']), managed(c['observed_pool_after'])
            self.assertEqual(a['pending_aum'], '0')
            self.assertFalse(a['vault_paused'])
            for key in ('total_supply', 'virtual_supply', 'manager_bpt_balance', 'protocol_collector_bpt_balance',
                        'vault_bpt_cash', 'vault_bpt_managed', 'vault_bpt_actual_balance'):
                self.assertEqual(a[key], b[key], (c['name'], key))
            for wallet in (b['manager'], b['protocol_collector'], c['transaction']['transaction']['from']):
                self.assertEqual(getter(c['getters_before'], c['source_pool']['pool_id'], 'balanceOf', [wallet]),
                                 getter(c['getters_after'], c['source_pool']['pool_id'], 'balanceOf', [wallet]))
            if c['operation'] != 'swap':
                self.assertEqual(c['source_pool']['multi_asset']['assets'], c['observed_pool_after']['multi_asset']['assets'])
                self.assertEqual(a['token_info'], b['token_info'])
                self.assertEqual(c['expected_native']['quantity'], '0')
            if c['operation'] == 'control':
                self.assertFalse(c['expected_native']['model_execution'])
        self.assertEqual(managed(enable['source_pool'])['last_collection_timestamp'],
                         managed(enable['observed_pool_after'])['last_collection_timestamp'])
        for c in (collect, disable):
            b, a = managed(c['source_pool']), managed(c['observed_pool_after'])
            self.assertGreater(a['last_collection_timestamp'], b['last_collection_timestamp'])
            self.assertEqual(a['last_collection_timestamp'], int(c['execution_header']['timestamp'], 16))
        self.assertEqual(collect['transaction']['method'], 'collectAumManagementFees')
        sender = swap['transaction']['transaction']['from']; p = swap['source_pool']; vault = p['multi_asset']['vault']
        for idx, sign, quantity in ((0, -1, swap['expected_native']['actual_amount_in']),
                                    (1, 1, swap['expected_native']['quantity'])):
            token = p['multi_asset']['assets'][idx]['token']
            delta = getter(swap['getters_after'], token, 'balanceOf', [sender])[0] - getter(swap['getters_before'], token, 'balanceOf', [sender])[0]
            vault_delta = getter(swap['getters_after'], token, 'balanceOf', [vault])[0] - getter(swap['getters_before'], token, 'balanceOf', [vault])[0]
            self.assertEqual(delta, sign*int(quantity)); self.assertEqual(vault_delta, -delta)
            self.assertGreater(int(quantity), 0)
        self.assertEqual(swap['params']['amount'], str(10**20))
        self.assertEqual(swap['expected_native']['actual_amount_in'], str(10**20))

    def test_source_execution_clock_and_historical_binding(self):
        v = self.value
        for c in v['cases']:
            tx = c['transaction']; before = managed(c['source_pool'])
            self.assertEqual(c['before_header'], tx['pre_header'])
            self.assertEqual(c['execution_header'], tx['execution_header'])
            self.assertEqual(int(tx['receipt']['status'], 16), c['expected_native']['status'])
            expected = copy.deepcopy(c['observed_pool_after']); em = managed(expected)
            for key in ('timestamp', 'block_number', 'block_hash'):
                em[key] = before[key]
            em['execution_context'] = {'block_number': int(c['execution_header']['number'], 16),
                                       'timestamp': int(c['execution_header']['timestamp'], 16)}
            self.assertEqual(c['expected_pool_after'], expected)
            self.assertEqual(c['params']['execution_timestamp'], em['execution_context']['timestamp'])
            self.assertEqual(c['params']['execution_block'], em['execution_context']['block_number'])
        third = v['cases'][2]
        self.assertEqual(int(third['source_header']['number'], 16), 33)
        self.assertEqual(int(third['execution_source_header']['number'], 16), 34)
        self.assertEqual(int(third['execution_header']['number'], 16), 35)
        self.assertEqual(third['source_pool'], v['cases'][1]['observed_pool_after'])
        recovery_clock = int(third['execution_header']['timestamp'], 16)
        self.assertLess(recovery_clock, 1780272000)  # 2026-06-01 UTC: original May clock.
        for c in v['cases'][3:]:
            self.assertGreaterEqual(int(c['execution_header']['timestamp'], 16), recovery_clock)
            self.assertLessEqual(int(c['execution_header']['timestamp'], 16), recovery_clock+2)
            self.assertEqual(c['source_header'], c['before_header'])
        self.assertEqual(v['cases'][4]['source_pool'], v['cases'][3]['observed_pool_after'])

    def test_default_copies_match_independent_temporary_generation(self):
        for output in OUTPUTS:
            self.assertEqual(output.read_bytes(), self.output.read_bytes())

    def test_check_is_readonly_and_rejects_tampering(self):
        protected = self.frozen + OUTPUTS + [SCRIPT, HELPER, self.output]
        before = fingerprint(protected)
        result = self.run_cli('--check', '--input', str(self.output))
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertTrue(json.loads(result.stdout)['passed'])
        self.assertEqual(fingerprint(protected), before)
        edits = [lambda v: managed(v['cases'][4]['source_pool']).update(management_aum_fee='10000000000000000'),
                 lambda v: v['cases'][4]['expected_native'].update(quantity='0'),
                 lambda v: v['cases'][5]['expected_native'].update(quantity='1'),
                 lambda v: v['cases'][6]['source_header'].update(hash='0x'+'00'*32),
                 lambda v: v['reference'].update(wrapper_sha256='0'*64),
                 lambda v: v['reference'].update(raw_prefix_transactions=27)]
        broken = Path(self.tmp.name) / 'broken.json'
        for edit in edits:
            value = copy.deepcopy(self.value); edit(value); broken.write_text(json.dumps(value))
            stamp = fingerprint([broken])
            result = self.run_cli('--check', '--input', str(broken))
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertFalse(json.loads(result.stdout)['passed'])
            self.assertEqual(fingerprint([broken]), stamp)
        self.assertEqual(fingerprint(protected), before)

    def test_output_scope_refuses_frozen_inputs_and_symlinks(self):
        protected = self.frozen + OUTPUTS + [HELPER, SCRIPT]
        before = fingerprint(protected)
        link = Path(self.tmp.name) / 'redirect.json'; link.symlink_to(paused.BANKS[0])
        for target in (paused.BANKS[0], paused.OUTPUTS[0], HELPER,
                       ROOT / 'integration/tools/reference/balancer-managed-v2-aum/forbidden.json', link):
            result = self.run_cli('--output', str(target))
            self.assertNotEqual(result.returncode, 0, result.stdout)
            self.assertIn('output refused', json.loads(result.stdout)['error'])
        self.assertEqual(fingerprint(protected), before)


if __name__ == '__main__':
    unittest.main()
