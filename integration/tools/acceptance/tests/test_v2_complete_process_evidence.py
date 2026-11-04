import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import v2_complete_smoke as m
from v2_complete_bank import NativeCompleteBank
class FullPoststate(unittest.TestCase):
 def evidence(self):
  bank=NativeCompleteBank();p=bank.case['expected']['pool_after'];s=p['v2']['swap'];word=lambda n:format(int(n),'064x')
  values={'pair_reserves_after':'0x'+word(p['reserve0'])+word(p['reserve1'])+word(s['block_timestamp_last']),'pair_balance0_after':'0x'+word(s['raw_balance0']),'pair_balance1_after':'0x'+word(s['raw_balance1']),'pair_price0_cumulative_after':'0x'+word(s['price0_cumulative_last']),'pair_price1_cumulative_after':'0x'+word(s['price1_cumulative_last']),'pair_lock_after':'0x'+word(1)}
  spec={'pair_reserves_after':(p['pool_id'],'0x0902f1ac'),'pair_balance0_after':(p['token0'],'0x70a08231'+'0'*24+p['pool_id'][2:]),'pair_balance1_after':(p['token1'],'0x70a08231'+'0'*24+p['pool_id'][2:]),'pair_price0_cumulative_after':(p['pool_id'],'0x5909c0d5'),'pair_price1_cumulative_after':(p['pool_id'],'0x5a3d5493'),'pair_lock_after':(p['pool_id'],'')}
  obs=[dict(phase=key,target=spec[key][0],calldata=spec[key][1],output=value,usage={'reads':1,'read_bytes':84,'instructions':1,'gas_used':21001,'input_bytes':0,'executions':1}) for key,value in values.items()]
  lock=obs[-1];lock.update(type='STORAGE',slot='0x'+word(12),calldata='');lock['usage'].update(instructions=0,gas_used=0,executions=0)
  return bank,{'outcome':{'observations':obs,'receipt_hash':'a'*64}}
 def test_actual_poststate_shape_requires_every_observed_native_value(self):
  bank,result=self.evidence();r=m.check_complete_poststate(bank,result);self.assertEqual(r['case'],bank.case['name']);self.assertEqual(r['source_block_hash'],bank.header['hash'])
  for mutation in ['missing','duplicate','reserve','raw','cumulative','lock','kind','slot','target','reads']:
   bad=copy.deepcopy(result);obs=bad['outcome']['observations']
   if mutation=='missing':obs.pop()
   elif mutation=='duplicate':obs.append(copy.deepcopy(obs[0]))
   elif mutation=='reserve':obs[0]['output']='0x'+'00'*96
   elif mutation=='raw':obs[1]['output']='0x'+'00'*32
   elif mutation=='cumulative':obs[3]['output']='0x'+format(1,'064x')
   elif mutation=='lock':obs[-1]['output']='0x'+'00'*32
   elif mutation=='kind':obs[-1]['type']='CALL'
   elif mutation=='slot':obs[-1]['slot']='0x'+format(13,'064x')
   elif mutation=='target':obs[0]['target']=bank.case['token_in']
   else:obs[-1]['usage']['reads']=0
   with self.subTest(mutation=mutation),self.assertRaises(ValueError):m.check_complete_poststate(bank,bad)
if __name__=='__main__':unittest.main()
