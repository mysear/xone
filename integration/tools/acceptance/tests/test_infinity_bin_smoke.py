import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import infinity_bin_smoke as m
class BinSmoke(unittest.TestCase):
 def test_original_callmaps_remain_hash_pinned_for_every_bank(self):
  c=m.Chain()
  for phase in range(5):
   if phase:c.advance(phase)
   anchor={'blockHash':c.current['block']['hash'],'requireCanonical':True}
   for to,raw in c.current['code'].items():self.assertEqual(c.rpc('eth_getCode',[to,anchor]),raw)
   for to,calls in c.current['callmap'].items():
    for data,raw in calls.items():self.assertEqual(c.rpc('eth_call',[{'to':to,'data':data},anchor]),raw)
  with self.assertRaises(ValueError):c.rpc('eth_getCode',[c.doc['manager'],'latest'])
 def test_reference_replays_carried_bins_and_checks_total_and_app_reserve(self):
  c=m.Chain();p=c.items[0]['pool'];s=p['infinity_bin'];seen=[]
  class Native:
   def request(self,r):
    seen.append(copy.deepcopy(r));bins={str(b['id']):{**b,'reserve0':str(int(b['reserve0'])+10)}for b in r['state']['bins']};return {'ok':True,'amount_in':r['amount'],'amount_out':'20','protocol_amount':'1','nextstate':{'active_id':s['active_id'],'bins':bins}}
  edge={'pool_id':p['pool_id'],'protocol':'pancake-infinity-bin','zero_for_one':True};route=lambda:{'path':{'edges':[edge],'amount_out':'20'},'amount_in':'10','amount_out':'20'};q={'routes':[route(),route()],'consumedIn':'20','grossBuyAmount':'40','refund':'0'};request={'sellToken':p['token0'],'buyToken':p['token1'],'sellAmount':'20'};self.assertEqual(m.replay_plan(q,request,c.items,Native()),2);self.assertEqual(int(seen[1]['state']['bins'][0]['reserve0']),int(seen[0]['state']['bins'][0]['reserve0'])+10);q['grossBuyAmount']='41'
  with self.assertRaises(ValueError):m.replay_plan(q,request,c.items,Native())
if __name__=='__main__':unittest.main()
