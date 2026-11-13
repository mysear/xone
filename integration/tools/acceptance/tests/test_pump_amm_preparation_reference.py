import base64,copy,hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from pump_amm_preparation_reference import ROOT,verify
from r01_public_catalog_solana_bank import native_imports
class PumpSwapUnsignedControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  directory=Path(os.environ.get('XONE_PUMP_REFERENCE_DIR',ROOT/'integration/artifacts/local/phase67-preflight'))
  path=directory/'pump-amm-preparation.json'
  if not path.exists():path=directory/'pump-amm-8-preparation.json'
  d=json.loads(path.read_bytes());cls.value=d.get('preparation',d)
 def bad(self,mutation):
  p=copy.deepcopy(self.value);mutation(p);p.pop('preparationHash',None);p['preparationHash']=hashlib.sha256(json.dumps(p,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  with tempfile.TemporaryDirectory()as directory:
   path=Path(directory)/'wire.json';path.write_text(json.dumps(p))
   with self.assertRaises(ValueError):verify(path,8)
 def test_reported_net_credit_cannot_hide_real_surplus(self):self.bad(lambda p:p.update(quotedOutput='1000'))
 def test_minimum_target_cannot_be_replaced_with_surplus(self):self.bad(lambda p:p.update(minBuyAmount='48185'))
 def test_network_fee_cannot_be_relabelled_as_trading_output(self):self.bad(lambda p:p.update(totalFundingLamports='0'))
 def test_fee_destination_descriptor_cannot_override_actual_wire(self):self.bad(lambda p:p['instructions'][0]['accounts'][10].update(pubkey=p['destinationTokenAccount']))
 def test_recompiled_fee_redirection_is_rejected(self):
  def change(p):
   _,P,K,I,M,T,_,_=native_imports();tx=T.from_bytes(base64.b64decode(p['unsignedTransaction']));d=p['instructions'][0];d['accounts'][10]['pubkey']=p['destinationTokenAccount'];instruction=I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']]);key=K.from_seed(hashlib.sha256(b'xone-pump-amm-user').digest());signed=T.new_signed_with_payer([instruction],key.pubkey(),[key],tx.message.recent_blockhash);p['messageBase64']=base64.b64encode(bytes(signed.message)).decode();p['unsignedTransaction']=base64.b64encode(bytes(T.new_unsigned(signed.message))).decode()
  self.bad(change)
if __name__=='__main__':unittest.main()
