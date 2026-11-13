import base64,copy,hashlib,json,os,sys,tempfile,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from pump_amm_funding_reference import ROOT,verify,PROGRAM
from r01_public_catalog_solana_bank import native_imports
class PumpFundingControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  folder=Path(os.environ.get('XONE_PUMP_FUNDING_DIR',ROOT/'integration/artifacts/local/phase70-preflight'))
  cls.values={i:json.loads((folder/f'pump-amm-{i}-preparation.json').read_bytes())['preparation']for i in [9,10,11,14,19]}
 def bad(self,index,mutation):
  p=copy.deepcopy(self.values[index]);mutation(p);p.pop('preparationHash',None);p['preparationHash']=hashlib.sha256(json.dumps(p,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()
  with tempfile.TemporaryDirectory()as folder:
   path=Path(folder)/'wire.json';path.write_text(json.dumps(p))
   with self.assertRaises(ValueError):verify(path,index)
 def recompile(self,p):
  _,P,K,I,M,T,_,_=native_imports();tx=T.from_bytes(base64.b64decode(p['unsignedTransaction']));ix=[I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']])for d in p['instructions']];key=K.from_seed(hashlib.sha256(b'xone-pump-amm-user').digest());signed=T.new_signed_with_payer(ix,key.pubkey(),[key],tx.message.recent_blockhash);p['messageBase64']=base64.b64encode(bytes(signed.message)).decode();p['unsignedTransaction']=base64.b64encode(bytes(T.new_unsigned(signed.message))).decode()
 def test_rent_cannot_be_counted_as_trade(self):self.bad(9,lambda p:p.update(quotedInput=str(int(p['quotedInput'])+int(p['rentLamports']))))
 def test_prefunded_wsol_cannot_be_counted_as_new_swap_output(self):self.bad(19,lambda p:p.update(quotedOutput=str(int(p['quotedOutput'])+3)))
 def test_topup_cannot_be_hidden(self):self.bad(9,lambda p:p.update(wrapLamports='0'))
 def test_refund_rent_cannot_be_removed(self):self.bad(10,lambda p:p.update(rentRefundLamports='0'))
 def test_native_received_cannot_include_network_fee(self):self.bad(11,lambda p:p.update(expectedPayerLamportDelta=p['quotedOutput']))
 def test_existing_account_cannot_be_closed(self):
  def change(p):
   close=copy.deepcopy(self.values[10]['instructions'][-1]);close['accounts'][0]['pubkey']=p['sourceTokenAccount'];p['instructions'].append(close);self.recompile(p)
  self.bad(9,change)
 def test_recompiled_sol_transfer_redirect_is_rejected(self):
  def change(p):
   transfer=next(d for d in p['instructions']if d['programId']=='11111111111111111111111111111111');transfer['accounts'][1]['pubkey']=p['destinationTokenAccount'];self.recompile(p)
  self.bad(9,change)
 def test_recompiled_wrong_ata_owner_is_rejected(self):
  def change(p):p['instructions'][0]['accounts'][2]['pubkey']=p['destinationTokenAccount'];self.recompile(p)
  self.bad(9,change)
 def test_temp_refund_cannot_be_relabelled_as_persistent(self):self.bad(10,lambda p:p.update(refundWrappedSource=False))
 def test_prefunded_reserve_cannot_inflate_funding(self):self.bad(14,lambda p:p.update(rentLamports='2039280'))
if __name__=='__main__':unittest.main()
