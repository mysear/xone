import base64,copy,hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from raydium_preparation_reference import ROOT,verify
from r01_public_catalog_solana_bank import native_imports
class RaydiumUnsignedControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  folder=Path(os.environ.get('XONE_RAYDIUM_REFERENCE_DIR',ROOT/'integration/artifacts/local/phase63-preflight'))
  path=folder/'raydium-preparation.json'
  if not path.exists():path=folder/'raydium-0-preparation.json'
  raw=json.loads(path.read_bytes());cls.value=raw.get('preparation',raw)
 def bad(self,change):
  p=copy.deepcopy(self.value);change(p);p.pop('preparationHash',None);p['preparationHash']=hashlib.sha256(json.dumps(p,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  with tempfile.TemporaryDirectory()as folder:
   path=Path(folder)/'raydium.json';path.write_text(json.dumps(p))
   with self.assertRaises(ValueError):verify(path)
 def test_rehashed_fee_cannot_override_real_native_debit(self):self.bad(lambda p:p.update(feeLamports='1'))
 def test_rehashed_budget_cannot_override_swap_limits(self):self.bad(lambda p:p.update(requiredInput='1'))
 def test_rehashed_recipient_cannot_override_account_roles(self):self.bad(lambda p:p.update(destinationTokenAccount=p['sourceTokenAccount']))
 def test_recompiled_recipient_mutation_is_rejected(self):
  def change(p):
   _,P,K,I,M,T,_,_=native_imports();old=T.from_bytes(base64.b64decode(p['unsignedTransaction']));d=p['instructions'][0];d['accounts'][5]['pubkey']=p['sourceTokenAccount'];ix=I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']]);key=K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest());tx=T.new_signed_with_payer([ix],key.pubkey(),[key],old.message.recent_blockhash);p['messageBase64']=base64.b64encode(bytes(tx.message)).decode();p['unsignedTransaction']=base64.b64encode(bytes(T.new_unsigned(tx.message))).decode()
  self.bad(change)
if __name__=='__main__':unittest.main()
