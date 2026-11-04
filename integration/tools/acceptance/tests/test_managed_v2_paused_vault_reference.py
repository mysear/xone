"""Independent signed original Vault pause transactions and public state evidence."""
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
SCRIPT = ROOT / 'integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs'
OUTPUTS = [ROOT / p / 'test/contracts/managed-v2-aum-paused-vault.json'
           for p in ('integration', 'chain-parser-go', 'dex-router')]
BANKS = [ROOT / p / 'test/contracts/managed-v2-aum.json'
         for p in ('integration', 'chain-parser-go', 'dex-router')]
SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66'
ADAPTER = 'balancer-v2/20230411-managed-aum-3'
FIELDS = set('timestamp block_number block_hash factory factory_code_sha256 pool_runtime_template_sha256 '
             'fee_provider fee_provider_code_sha256 protocol_collector protocol_collector_code_sha256 '
             'dependencies bpt_index manager manager_bpt_balance protocol_collector_bpt_balance '
             'management_aum_fee last_collection_timestamp provider_fee_id live_provider_aum_percentage '
             'cached_protocol_aum_percentage swaps_enabled paused vault_paused vault_pause_window_end '
             'vault_buffer_period_end recovery_mode allowlist_enabled pause_window_end buffer_period_end '
             'gradual_weight_end gradual_fee_end token_info total_supply virtual_supply actual_supply '
             'pending_aum vault_bpt_cash vault_bpt_managed vault_bpt_actual_balance'.split())


def fingerprint(paths):
    return {str(p): (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_size, p.stat().st_mtime_ns)
            for p in paths if p.is_file()}


def managed(pool):
    return pool['multi_asset']['managed_v2']


def words(record):
    return [int(record['result'][i:i+64], 16) for i in range(2, len(record['result']), 64)]


def getter(rows, to, method, args=None):
    matches = [g for g in rows if g['to'] == to and g['method'] == method and g['args'] == (args or [])]
    if len(matches) != 1:
        raise AssertionError((to, method, args, len(matches)))
    return words(matches[0])


class PausedVaultReferenceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory(prefix='xone-paused-vault-test-')
        cls.output = Path(cls.tmp.name) / 'reference.json'
        cls.frozen_paths = BANKS + list((ROOT / 'integration/tools/reference/balancer-managed-v2-aum').rglob('*'))
        cls.frozen_before = fingerprint(cls.frozen_paths)
        cls.generated = cls.run_cli('--output', str(cls.output))
        if cls.generated.returncode:
            raise AssertionError(cls.generated.stdout + '\n' + cls.generated.stderr)
        cls.value = json.loads(cls.output.read_bytes())

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    @staticmethod
    def run_cli(*args):
        return subprocess.run(['node', str(SCRIPT), *args], cwd=ROOT, capture_output=True,
                              text=True, timeout=180, env={**os.environ, 'PYTHONDONTWRITEBYTECODE': '1'})

    def test_native_inventory_and_frozen_authority(self):
        v = self.value
        imported = subprocess.run(['node', '-e',
            "const p=require(process.argv[1]); if(typeof p.generate!=='function') process.exit(1)", str(SCRIPT)],
            cwd=ROOT, capture_output=True, text=True, timeout=30)
        self.assertEqual(imported.returncode, 0, imported.stdout + imported.stderr)
        self.assertEqual(imported.stdout, '')
        self.assertEqual(v['schema_version'], 1)
        self.assertEqual(v['public_adapter_version'], ADAPTER)
        self.assertEqual(v['authority'], 'independent-local-original-vault-pause-reference')
        self.assertEqual(v['original_bank_sha256'], SHA)
        b = json.loads(BANKS[0].read_bytes())
        self.assertEqual(len(BANKS[0].read_bytes()), 7905624)
        self.assertEqual(v['reference']['node_closure'], b['reference']['node_closure'])
        self.assertEqual(v['reference']['chain_options'], b['reference']['chain_options'])
        self.assertEqual(v['reference']['raw_prefix_transactions'], 28)
        self.assertEqual(v['reference']['source_script_sha256'], hashlib.sha256(SCRIPT.read_bytes()).hexdigest())
        self.assertEqual([c['name'] for c in v['cases']],
                         ['vault-paused-swap', 'vault-paused-collection', 'swap-after-vault-buffer'])
        self.assertEqual([c['expected_native']['status'] for c in v['cases']], [0, 1, 1])
        self.assertEqual(fingerprint(self.frozen_paths), self.frozen_before)

    def test_every_numeric_state_has_fixed_block_original_getter_evidence(self):
        for c in self.value['cases']:
            for side, h in [('before', c['source_header']), ('after', c['execution_header'])]:
                pool = c['source_pool'] if side == 'before' else c['observed_pool_after']
                m = managed(pool); multi = pool['multi_asset']; rows = c['getters_' + side]
                self.assertEqual(set(m), FIELDS)
                self.assertEqual(pool['adapter_version'], ADAPTER)
                self.assertEqual((m['block_number'], m['timestamp'], m['block_hash']),
                                 (int(h['number'], 16), int(h['timestamp'], 16), h['hash']))
                self.assertGreaterEqual(len(rows), 76)
                self.assertTrue(all(set(g) >= {'to', 'data', 'result', 'method', 'args'} for g in rows))
                self.assertTrue(all(g['block_tag'] == h['number'] for g in rows))
                call = lambda to, method, args=None: getter(rows, to, method, args)
                for to, prefix in [(pool['pool_id'], ''), (multi['vault'], 'vault_')]:
                    p = call(to, 'getPausedState')
                    self.assertEqual([m[prefix + k] for k in ('paused', 'pause_window_end', 'buffer_period_end')],
                                     [bool(p[0]), p[1], p[2]])
                address = pool['pool_id']
                fees = call(address, 'getManagementAumFeeParams')
                self.assertEqual([m['management_aum_fee'], m['last_collection_timestamp']], [str(fees[0]), fees[1]])
                self.assertEqual(m['provider_fee_id'], str(call(address, 'getProviderFeeId', [3])[0]))
                self.assertEqual(m['live_provider_aum_percentage'], str(call(m['fee_provider'], 'getFeeTypePercentage', [int(m['provider_fee_id'])])[0]))
                self.assertEqual(m['cached_protocol_aum_percentage'], str(call(address, 'getProtocolFeePercentageCache', [3])[0]))
                self.assertEqual(pool['fee_numerator'], call(address, 'getSwapFeePercentage')[0])
                for key, method in [('swaps_enabled', 'getSwapEnabled'), ('recovery_mode', 'inRecoveryMode'), ('allowlist_enabled', 'getMustAllowlistLPs')]:
                    self.assertEqual(m[key], bool(call(address, method)[0]))
                for key, method in [('gradual_weight_end', 'getGradualWeightUpdateParams'), ('gradual_fee_end', 'getGradualSwapFeeUpdateParams')]:
                    self.assertEqual(m[key], call(address, method)[1])
                for method, observed in [('getNormalizedWeights', multi['weights']), ('getScalingFactors', [a['scaling'] for a in multi['assets']])]:
                    encoded = call(address, method)
                    offset = encoded[0]//32
                    self.assertEqual(observed, [str(x) for x in encoded[offset+1:offset+1+encoded[offset]]])
                encoded = call(multi['vault'], 'getPoolTokens', [multi['pool_id']])
                offset = encoded[1]//32
                registered = encoded[offset+1:offset+1+encoded[offset]]
                self.assertEqual(m['bpt_index'], 0)
                self.assertEqual(registered[1:], [int(a['balance']) for a in multi['assets']])
                total = call(pool['pool_id'], 'totalSupply')[0]
                actual = call(pool['pool_id'], 'getActualSupply')[0]
                bpt = call(multi['vault'], 'getPoolTokenInfo', [multi['pool_id'], pool['pool_id']])
                self.assertEqual([m[k] for k in ('total_supply', 'virtual_supply', 'actual_supply', 'pending_aum')],
                                 list(map(str, [total, total-bpt[0]-bpt[1], actual, actual-(total-bpt[0]-bpt[1])])))
                self.assertEqual(registered[0], bpt[0]+bpt[1])
                self.assertEqual([m['vault_bpt_cash'], m['vault_bpt_managed']], [str(bpt[0]), str(bpt[1])])
                self.assertEqual(m['vault_bpt_actual_balance'], str(call(pool['pool_id'], 'balanceOf', [multi['vault']])[0]))
                for asset, info in zip(multi['assets'], m['token_info']):
                    ti = call(multi['vault'], 'getPoolTokenInfo', [multi['pool_id'], asset['token']])
                    self.assertEqual([info['cash'], info['managed'], info['last_change_block']], [str(ti[0]), str(ti[1]), ti[2]])
                    self.assertEqual(asset['balance'], str(ti[0]+ti[1]))
                    self.assertEqual(info['vault_actual_balance'], str(call(asset['token'], 'balanceOf', [multi['vault']])[0]))
                for owner_key, balance_key in [('manager', 'manager_bpt_balance'), ('protocol_collector', 'protocol_collector_bpt_balance')]:
                    self.assertEqual(m[balance_key], str(call(pool['pool_id'], 'balanceOf', [m[owner_key]])[0]))

    def test_signed_receipts_balances_flags_and_execution_clock(self):
        for c in self.value['cases']:
            b, a = managed(c['source_pool']), managed(c['observed_pool_after'])
            tx = c['transaction']; header = c['execution_header']; receipt = tx['receipt']
            self.assertEqual(int(receipt['status'], 16), c['expected_native']['status'])
            self.assertEqual(receipt['blockHash'], header['hash'])
            self.assertEqual(receipt['blockNumber'], header['number'])
            self.assertEqual(tx['transaction']['hash'], receipt['transactionHash'])
            self.assertTrue(tx['signed_transaction'].startswith('0x'))
            self.assertEqual(int(tx['transaction']['type'], 16), 0)
            self.assertEqual(int(tx['transaction']['gasPrice'], 16), 2000000000)
            self.assertEqual(int(tx['transaction']['gas'], 16), 10000000)
            self.assertEqual(c['before_header'], tx['pre_header'])
            self.assertEqual(header['parentHash'], c['before_header']['hash'])
            self.assertFalse(b['paused']); self.assertFalse(a['paused'])
            self.assertTrue(b['vault_paused'])
            expected = copy.deepcopy(c['observed_pool_after']); em = managed(expected)
            for key in ('timestamp', 'block_number', 'block_hash'):
                em[key] = b[key]
            em['execution_context'] = dict(block_number=int(header['number'], 16), timestamp=int(header['timestamp'], 16))
            self.assertEqual(c['expected_pool_after'], expected)
            self.assertEqual(c['params']['execution_block'], int(header['number'], 16))
            self.assertEqual(c['params']['execution_timestamp'], int(header['timestamp'], 16))
            if c['operation'] == 'collect':
                self.assertNotIn('amount', c['params'])
                minted = int(a['total_supply']) - int(b['total_supply'])
                self.assertEqual(c['expected_native']['quantity'], str(minted))
                self.assertEqual(minted, 16604400166043669547)
                self.assertEqual(minted, sum(int(a[k])-int(b[k]) for k in ('manager_bpt_balance', 'protocol_collector_bpt_balance')))
                self.assertTrue(a['vault_paused'])
            else:
                self.assertEqual(c['params']['amount'], '100000000000000000000')
                sender = tx['transaction']['from']; multi = c['source_pool']['multi_asset']
                deltas = []
                for asset in multi['assets']:
                    vals = [getter(c['getters_'+s], asset['token'], 'balanceOf', [sender])[0] for s in ('before', 'after')]
                    deltas.append(vals[1]-vals[0])
                self.assertEqual(c['expected_native']['quantity'], str(deltas[1]))
                self.assertEqual(c['expected_native']['actual_amount_in'], str(-deltas[0]))
                self.assertEqual(-deltas[0], 10**20 if c['expected_native']['status'] else 0)
                self.assertEqual(a['vault_paused'], not bool(c['expected_native']['status']))
        collection, expiry = self.value['cases'][1:]
        self.assertEqual(expiry['source_pool'], collection['observed_pool_after'])
        em = managed(expiry['expected_pool_after'])
        self.assertLess(em['timestamp'], em['vault_buffer_period_end'])
        self.assertEqual(em['execution_context']['timestamp'], em['vault_buffer_period_end']+1)
        self.assertLess(em['execution_context']['timestamp'], em['buffer_period_end'])
        self.assertGreater(int(expiry['before_header']['number'], 16), em['block_number'])

    def test_branch_parent_chain_and_real_execution_source(self):
        v = self.value
        headers = v['headers']
        self.assertEqual(int(headers[0]['number'], 16), 28)
        for before, after in zip(headers, headers[1:]):
            self.assertEqual(int(after['number'], 16), int(before['number'], 16)+1)
            self.assertEqual(after['parentHash'], before['hash'])
        by_hash = {h['hash']: h for h in headers}
        self.assertEqual(len(v['receipts']), 5)
        for receipt in v['receipts']:
            h = by_hash[receipt['blockHash']]
            self.assertEqual(h['number'], receipt['blockNumber'])
            self.assertIn(receipt['transactionHash'], h['transactions'])
        c = v['cases'][2]
        m = managed(c['execution_source_pool'])
        self.assertEqual(set(m), FIELDS)
        self.assertFalse(m['vault_paused'])
        self.assertEqual(c['source_header'], v['cases'][1]['execution_header'])
        self.assertEqual(c['execution_source_header'], c['before_header'])
        self.assertNotEqual(c['before_header'], c['source_header'])
        self.assertNotEqual(c['before_header']['hash'], c['source_header']['hash'])
        self.assertEqual(m['block_hash'], c['before_header']['hash'])
        self.assertEqual(m['timestamp'], c['params']['execution_timestamp'])
        self.assertTrue(all(g['block_tag'] == c['before_header']['number'] for g in c['execution_source_getters']))
        self.assertEqual(getter(c['execution_source_getters'], c['execution_source_pool']['multi_asset']['vault'], 'getPausedState')[0], 0)
        self.assertNotIn('execution_context', m)

    def test_check_exact_regeneration_is_readonly_and_rejects_corrupt_copy(self):
        before = fingerprint(self.frozen_paths + OUTPUTS + [self.output, SCRIPT])
        checked = self.run_cli('--check', '--input', str(self.output))
        self.assertEqual(checked.returncode, 0, checked.stdout + checked.stderr)
        self.assertTrue(json.loads(checked.stdout)['passed'])
        self.assertEqual(fingerprint(self.frozen_paths + OUTPUTS + [self.output, SCRIPT]), before)
        broken = Path(self.tmp.name) / 'broken.json'
        for edit in (lambda v: v['cases'][1]['expected_native'].update(quantity='1'),
                     lambda v: v['cases'][2]['source_pool']['multi_asset']['managed_v2'].update(vault_paused=False),
                     lambda v: v['reference'].update(raw_prefix_transactions=27)):
            value = copy.deepcopy(self.value); edit(value)
            broken.write_text(json.dumps(value))
            broken_before = fingerprint([broken])
            rejected = self.run_cli('--check', '--input', str(broken))
            self.assertNotEqual(rejected.returncode, 0, rejected.stdout)
            self.assertFalse(json.loads(rejected.stdout)['passed'])
            self.assertEqual(fingerprint([broken]), broken_before)
        self.assertEqual(fingerprint(self.frozen_paths + OUTPUTS + [self.output, SCRIPT]), before)

    def test_default_copies_match_independent_temporary_generation(self):
        for output in OUTPUTS:
            self.assertEqual(output.read_bytes(), self.output.read_bytes())


if __name__ == '__main__':
    unittest.main()
