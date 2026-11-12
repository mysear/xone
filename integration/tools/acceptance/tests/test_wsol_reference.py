import copy,hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from meteora_wsol_reference import ROOT,verify
class WrappedFundingControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        path=Path(os.environ.get('XONE_WRAP_REFERENCE_INPUT',ROOT/'integration/artifacts/local/phase59-wrap-preparation.json'))
        value=json.loads(path.read_bytes());cls.value=value.get('preparation',value)
    def bad(self,change):
        value=copy.deepcopy(self.value);change(value);value.pop('preparationHash',None);value['preparationHash']=hashlib.sha256(json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
        with tempfile.TemporaryDirectory()as folder:
            path=Path(folder)/'wrap.json';path.write_text(json.dumps(value))
            with self.assertRaises(ValueError):verify(path)
    def test_rehashed_wrap_amount_cannot_override_transfer(self):self.bad(lambda p:p.update(wrapLamports='1'))
    def test_rehashed_fee_cannot_override_real_payer_debit(self):self.bad(lambda p:p.update(totalFundingLamports='1'))
    def test_rehashed_source_cannot_override_canonical_wsol_ata(self):self.bad(lambda p:p.update(sourceTokenAccount=p['destinationTokenAccount']))
    def test_rehashed_sync_instruction_cannot_override_wire(self):self.bad(lambda p:p['instructions'][2].update(dataBase64='AA=='))
if __name__=='__main__':unittest.main()
