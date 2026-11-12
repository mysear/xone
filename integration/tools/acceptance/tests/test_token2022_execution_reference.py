import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from meteora_token2022_execution_reference import ROOT, verify, extensions
from r01_public_catalog_solana_bank import native_imports


class Token2022PreparationControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        folder=Path(os.environ.get('XONE_TOKEN2022_REFERENCE_DIR',ROOT/'integration/artifacts/local/phase61-preflight'))
        name='token2022-preparation.json' if (folder/'token2022-preparation.json').exists() else 'token2022-1-preparation.json'
        value=json.loads((folder/name).read_bytes());cls.value=value.get('preparation',value)

    def bad(self,change):
        p=copy.deepcopy(self.value);change(p);p.pop('preparationHash',None)
        p['preparationHash']=hashlib.sha256(json.dumps(p,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'token.json';path.write_text(json.dumps(p))
            with self.assertRaises(ValueError):verify(path)

    def test_rehashed_classic_size_cannot_replace_extension_account(self):self.bad(lambda p:p.update(destinationAccountSize=165))
    def test_rehashed_rent_cannot_replace_actual_rent(self):self.bad(lambda p:p.update(rentLamports='2039280'))
    def test_rehashed_fee_cannot_replace_withheld_delta(self):self.bad(lambda p:p.update(expectedDestinationTransferFee='0'))
    def test_rehashed_target_cannot_replace_canonical_token_program(self):self.bad(lambda p:p.update(destinationTokenProgram='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'))
    def test_recompiled_classic_ata_instruction_cannot_replace_token2022(self):
        def change(p):
            _,P,K,I,M,T,_,_=native_imports();old=T.from_bytes(base64.b64decode(p['unsignedTransaction']))
            p['instructions'][0]['accounts'][5]['pubkey']='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
            instructions=[I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']])for d in p['instructions']]
            signer=K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest());tx=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],old.message.recent_blockhash)
            p['messageBase64']=base64.b64encode(bytes(tx.message)).decode();p['unsignedTransaction']=base64.b64encode(bytes(T.new_unsigned(tx.message))).decode()
        self.bad(change)

    def test_tlv_reader_rejects_unknown_duplicate_truncated_and_wrong_type(self):
        base=bytes(165)+b'\x02';fee=b'\x02\x00\x08\x00'+bytes(8);immutable=b'\x07\x00\x00\x00'
        self.assertEqual(extensions(base+fee+immutable),{2:bytes(8),7:b''})
        for raw in [base+fee+b'\x08\x00\x00\x00',base+fee+fee,base+fee[:-1],bytes(165)+b'\x01'+fee,base+immutable+immutable+immutable]:
            with self.subTest(length=len(raw)),self.assertRaises(ValueError):extensions(raw)


if __name__=='__main__':unittest.main()
