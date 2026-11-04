import sys,unittest,base64,copy
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import orca_adaptive_smoke as m
class AdaptiveSmoke(unittest.TestCase):
 def test_original_banks_are_replayed_with_one_finalized_slot(self):
  c=m.Chain()
  for phase in range(5):
   if phase:c.advance(phase)
   ids=set().union(*[c.required(i)for i in c.items]);v=c.rpc('getMultipleAccounts',[sorted(ids),{'commitment':'finalized','encoding':'base64'}]);self.assertEqual(v['context']['slot'],c.head);self.assertEqual(len(ids),len(v['value']));clock=base64.b64decode(c.accounts[m.CLOCK]['data'][0]);self.assertEqual(int.from_bytes(clock[:8],'little'),c.head);self.assertEqual(int.from_bytes(clock[32:40],'little'),c.timestamp);self.assertEqual({i['pool']['orca_whirlpool']['bank_slot']for i in c.items},{c.head});self.assertEqual(c.block(c.head)['blockTime'],c.timestamp)
 def test_unfinalized_or_wrong_readset_is_rejected(self):
  c=m.Chain();ids=sorted(set().union(*[c.required(i)for i in c.items]))
  for args in [[ids,{'commitment':'confirmed','encoding':'base64'}],[ids[:-1],{'commitment':'finalized','encoding':'base64'}],[ids,{'commitment':'finalized','encoding':'base64','minContextSlot':c.head}]]:
   with self.assertRaises(ValueError):c.rpc('getMultipleAccounts',args)
 def test_faults_modify_actual_source_dependencies(self):
  c=m.Chain();ids=sorted(set().union(*[c.required(i)for i in c.items]));before=c.rpc('getMultipleAccounts',[ids,{'commitment':'finalized','encoding':'base64'}])['value']
  for fault in m.FAULTS:
   c.advance(0,fault);after=c.rpc('getMultipleAccounts',[ids,{'commitment':'finalized','encoding':'base64'}])['value'];changed=[key for key,a,b in zip(ids,before,after)if a!=b];self.assertTrue(changed,fault);self.assertTrue(any(key!=m.CLOCK for key in changed)or fault=='clock-slot',fault)
 def test_reference_replays_selected_competing_pool_routes_with_carried_state(self):
  c=m.Chain();items=c.items;pid=items[0]['pool']['pool_id'];p=items[0]['pool'];requests=[]
  class Native:
   def request(self,req):
    requests.append(copy.deepcopy(req))
    return {'status':'fulfilled','amount_in':req['params']['amount'],'amount_out':str(int(req['params']['amount'])*2),'amount_remaining':'0','post_accounts':{'next':len(requests)},'next_state':{}}
  req={'sellToken':p['token0'],'buyToken':p['token1'],'sellAmount':'30'};edge={'pool_id':pid,'protocol':m.clean_config(items[0]['original_config'])['protocol'],'zero_for_one':True}
  routes=[{'amount_in':str(n),'amount_out':str(n*2),'path':{'edges':[edge],'amount_out':str(n*2)}}for n in [10,20]];quote={'routes':routes,'consumedIn':'30','grossBuyAmount':'60','refund':'0'}
  self.assertEqual(m.replay_plan(quote,req,items,Native()),2);self.assertEqual(requests[1]['bank']['accounts'],{'next':1});quote['grossBuyAmount']='61'
  with self.assertRaises(ValueError):m.replay_plan(quote,req,items,Native())
if __name__=='__main__':unittest.main()
