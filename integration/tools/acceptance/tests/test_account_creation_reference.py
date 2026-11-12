import copy,hashlib,json,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from meteora_account_creation_reference import verify,ROOT

class AccountCreationControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path=Path(__import__('os').environ.get('XONE_ATA_REFERENCE_INPUT',ROOT/'integration/artifacts/local/phase58-ata-preparation.json'))
        v=json.loads(path.read_bytes());cls.value=v.get('preparation',v)
    def check_bad(self,change):
        v=copy.deepcopy(self.value);change(v);v.pop('preparationHash',None);v['preparationHash']=hashlib.sha256(json.dumps(v,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
        with tempfile.TemporaryDirectory()as folder:
            p=Path(folder)/'ata.json';p.write_text(json.dumps(v))
            with self.assertRaises(ValueError):verify(p)
    def test_rehashed_destination_cannot_override_canonical_ata(self):self.check_bad(lambda v:v.update(destinationTokenAccount=v['sourceTokenAccount']))
    def test_rehashed_instruction_cannot_override_actual_wire(self):self.check_bad(lambda v:v['instructions'][0].update(programId=v['instructions'][1]['programId']))
    def test_rehashed_rent_cannot_override_actual_payer_debit(self):self.check_bad(lambda v:v.update(totalFundingLamports='1'))
if __name__=='__main__':unittest.main()
