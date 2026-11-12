"""Mutations remain invalid even when the outer source commitment is recomputed."""
import base64
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import meteora_exact_out_reference as reference


class TestMeteoraExactOutReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.raw = json.loads(reference.ORIGINAL.read_bytes())
        cls.projected = json.loads(reference.PROJECTED.read_bytes())

    def test_rehashed_exact_output_dust_and_hard_limit_controls(self):
        for mutation in ['vault-dust', 'input-bound', 'input-control-verdict', 'missing-target', 'wrong-instruction', 'sequence-bank', 'sequence-drop', 'native-credit']:
            with self.subTest(mutation=mutation):
                raw, projected = copy.deepcopy(self.raw), copy.deepcopy(self.projected)
                entry, row = raw['records'][2], projected['cases'][2]['steps'][0]
                step = entry['native']['sequence'][0]
                if mutation == 'vault-dust':
                    # Keep the normalized bytes consistent with a forged raw bank,
                    # but make the physical bin debit smaller than the token credit.
                    array = next(a for a in row['after']['meteora']['bin_arrays'] if a['index'] == 0)
                    offset = 56 + 15 * 144 + 8
                    data = bytearray.fromhex(array['data'])
                    value = int.from_bytes(data[offset:offset+8], 'little')
                    data[offset:offset+8] = (value + 2).to_bytes(8, 'little')
                    array['data'] = data.hex()
                    step['after'][array['address']]['data_base64'] = base64.b64encode(data).decode()
                elif mutation == 'input-bound':
                    entry['input_minus_one']['limit'] = step['payer_input_delta']
                elif mutation == 'input-control-verdict':
                    entry['input_minus_one']['native'].pop('err')
                elif mutation == 'missing-target':
                    raw['records'].pop()
                    projected['cases'].pop()
                elif mutation == 'wrong-instruction':
                    step['native']['logs'] = [s.replace('SwapExactOut2', 'Swap2') for s in step['native']['logs']]
                elif mutation == 'sequence-bank':
                    raw['records'][8]['native']['sequence'][1]['before'][reference.PAIR]['lamports'] += 1
                elif mutation == 'sequence-drop':
                    raw['records'][8]['native']['sequence'].pop()
                    projected['cases'][8]['steps'].pop()
                else:
                    step['recipient_output_delta'] = '999'
                digest = hashlib.sha256(json.dumps(raw, sort_keys=True).encode()).hexdigest()
                projected['source_sha256'] = digest
                with self.assertRaises(ValueError):
                    reference.verify(raw, projected, digest)


if __name__ == '__main__':
    unittest.main()
