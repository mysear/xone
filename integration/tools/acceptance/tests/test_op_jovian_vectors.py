import copy
import json
import os
from pathlib import Path
import re
import sys
import unittest
import zipfile
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from op_jovian_vectors import audit, cases
from op_chain_cost_vectors import SOURCE
from op_precompile_vectors import audit as crypto_audit, cases as crypto_cases


class OriginalJovian(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory = Path(os.environ['XONE_OP_JOVIAN_REFERENCE'])
        cls.rows = json.loads((cls.directory / 'execution/vectors.json').read_bytes())['cases']
        cls.crypto = json.loads((cls.directory / 'precompiles/vectors.json').read_bytes())['cases']
        cls.inputs, _ = crypto_cases(include_jovian=True)

    def test_original_registry_time_interval_is_historical_and_chain_specific(self):
        with zipfile.ZipFile(SOURCE / 'superchain/superchain-configs.zip') as z:
            self.assertEqual(z.read('COMMIT').decode().strip(), 'ac4e48516794322e20abfe4f4c175e379b9a5d19')
            text = z.read('configs/mainnet/op.toml').decode()
            self.assertEqual(int(re.search(r'jovian_time = (\d+)', text)[1]), 1764691201)
            self.assertEqual(int(re.search(r'karst_time = (\d+)', text)[1]), 1783526401)
            self.assertTrue(all(1764691201 < r['timestamp'] < 1783526401 and r['chain'] == 10 for r in cases()))
        self.assertIn('karstKillSwitch', (SOURCE / 'consensus/beacon/consensus.go').read_text())

    def test_full_matrix_operator_units_and_caps_match_original(self):
        audit(self.rows)
        crypto_audit(self.inputs, self.crypto)
        self.assertEqual(len(self.crypto), 7056)
        self.assertEqual(sum('transition' in r for r in self.crypto), 160)
        for address, size in [(8, 81984), (12, 288960), (14, 278784), (15, 156672)]:
            row = next(r for r in self.inputs if r['id'] == f'jovian-cap-{address}-cap')
            self.assertEqual(len(row['data']) // 2, size)

    def test_source_coverage_units_funding_and_full_fee_substitutions_are_rejected(self):
        for change in ['drop', 'order', 'chain', 'operator', 'maximum', 'balance']:
            rows = list(self.rows)
            if change == 'drop': rows.pop()
            elif change == 'order': rows[0], rows[1] = rows[1], rows[0]
            else:
                rows[0] = copy.deepcopy(rows[0]); r = rows[0]
                if change == 'chain': r['input']['chain'] = 8453
                elif change == 'operator': r['actual']['operator_fee'] = str(int(r['actual']['operator_fee']) // 100_000_000)
                elif change == 'maximum': r['maximum_funding'] = str(int(r['maximum_funding']) - 1)
                else:
                    owner = next(iter(r['actual']['after']))
                    r['actual']['after'][owner] = str(int(r['actual']['after'][owner]) + 1)
            with self.assertRaises(ValueError, msg=change): audit(rows)

    def test_crypto_fork_cap_gas_and_nonce_substitutions_are_rejected(self):
        for change in ['cap', 'present', 'gas', 'nonce']:
            inputs, rows = list(self.inputs), list(self.crypto)
            index = next(i for i, r in enumerate(rows) if r['input']['id'] == 'jovian-dispatch-isthmus-11')
            rows[index] = copy.deepcopy(rows[index])
            if change == 'cap':
                inputs[index] = dict(inputs[index], gas=inputs[index]['gas'] + 1)
                rows[index]['input'] = inputs[index]
            elif change == 'present': rows[index]['present'] = False
            elif change == 'gas': rows[index]['remaining_gas'] += 1
            else: rows[index]['transition']['nonce_after'] = 0
            with self.assertRaises(ValueError, msg=change): crypto_audit(inputs, rows)
