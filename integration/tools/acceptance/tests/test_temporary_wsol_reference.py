import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from meteora_temporary_wsol_reference import ROOT, verify
from r01_public_catalog_solana_bank import native_imports


class TemporarySettlementControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder = Path(os.environ.get('XONE_TEMP_REFERENCE_DIR', ROOT / 'integration/artifacts/local/phase60-temp-preflight'))
        cls.values = []
        for kind in ['unwrap', 'refund']:
            value = json.loads((folder / (kind + '-preparation.json')).read_bytes())
            cls.values.append(value.get('preparation', value))

    def bad(self, change):
        for original in self.values:
            with self.subTest(refund=original['refundWrappedSource']):
                value = copy.deepcopy(original)
                change(value)
                value.pop('preparationHash', None)
                value['preparationHash'] = hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
                with tempfile.TemporaryDirectory() as folder:
                    path = Path(folder) / 'temporary.json'
                    path.write_text(json.dumps(value))
                    with self.assertRaises(ValueError):
                        verify(path)

    def test_rehashed_native_debit_cannot_override_real_settlement(self):
        self.bad(lambda p: p.update(expectedPayerLamportDelta='1'))

    def test_rehashed_refund_cannot_override_original_amount(self):
        self.bad(lambda p: p.update(expectedNativeRefundLamports='1'))

    def test_rehashed_close_descriptor_cannot_override_wire(self):
        self.bad(lambda p: p['instructions'][-1]['accounts'][0].update(pubkey=p['instructions'][-1]['accounts'][1]['pubkey']))

    def test_recompiled_close_cannot_target_existing_user_account(self):
        def change(p):
            _, P, K, I, M, T, _, _ = native_imports()
            old = T.from_bytes(base64.b64decode(p['unsignedTransaction']))
            target = p['destinationTokenAccount'] if p['refundWrappedSource'] else p['sourceTokenAccount']
            p['instructions'][-1]['accounts'][0]['pubkey'] = target
            instructions = [I(P.from_string(d['programId']), base64.b64decode(d['dataBase64']),
                              [M(P.from_string(a['pubkey']), a['isSigner'], a['isWritable']) for a in d['accounts']]) for d in p['instructions']]
            signer = K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest())
            tx = T.new_signed_with_payer(instructions, signer.pubkey(), [signer], old.message.recent_blockhash)
            p['messageBase64'] = base64.b64encode(bytes(tx.message)).decode()
            p['unsignedTransaction'] = base64.b64encode(bytes(T.new_unsigned(tx.message))).decode()
        self.bad(change)

    def test_rehashed_seed_cannot_override_derivation(self):
        self.bad(lambda p: p.update(temporarySeed='xone-other'))

    def test_rehashed_native_settlement_cannot_hide_wsol(self):
        self.bad(lambda p: p.update(settlementAsset='native:wrong'))


if __name__ == '__main__':
    unittest.main()
