import base64,copy,hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from pump_curve_preparation_reference import ROOT,verify
from r01_public_catalog_solana_bank import native_imports
class NativeAtaWireControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  folder=Path(os.environ.get('XONE_PUMP_CURVE_ATA_REFERENCE_DIR',ROOT/'integration/artifacts/local/phase69-preflight'))
  path=folder/'pump-curve-ata-preparation.json'
  if not path.exists():path=folder/'pump-curve-6-preparation.json'
  doc=json.loads(path.read_bytes());cls.value=doc.get('preparation',doc)
 def bad(self,mutation):
  p=copy.deepcopy(self.value);mutation(p);p.pop('preparationHash',None);p['preparationHash']=hashlib.sha256(json.dumps(p,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  with tempfile.TemporaryDirectory()as folder:
   path=Path(folder)/'wire.json';path.write_text(json.dumps(p))
   with self.assertRaises(ValueError):verify(path,6)
 def test_rent_cannot_be_reported_as_native_swap_input(self):self.bad(lambda p:p.update(quotedInput='2044283'))
 def test_rent_cannot_be_removed_from_funding_and_user_delta(self):self.bad(lambda p:p.update(rentLamports='0',totalFundingLamports='5004',expectedPayerLamportDelta='-5003'))
 def test_recompiled_creation_for_other_owner_is_rejected(self):
  def mutation(p):
   _,P,K,I,M,T,_,_=native_imports();old=T.from_bytes(base64.b64decode(p['unsignedTransaction']));p['instructions'][0]['accounts'][2]['pubkey']=p['instructions'][-1]['accounts'][1]['pubkey'];instructions=[I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']])for d in p['instructions']];key=K.from_seed(hashlib.sha256(b'xone-pump-trading-user').digest());tx=T.new_signed_with_payer(instructions,key.pubkey(),[key],old.message.recent_blockhash);p['messageBase64']=base64.b64encode(bytes(tx.message)).decode();p['unsignedTransaction']=base64.b64encode(bytes(T.new_unsigned(tx.message))).decode()
  self.bad(mutation)
if __name__=='__main__':unittest.main()
