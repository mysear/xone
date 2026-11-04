import base64
import copy
import importlib.util
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import orca_adaptive_rewards_smoke as m

class AdaptiveRewardBanks(unittest.TestCase):
    def test_complete_original_fixed_bank_contains_initialized_reward_sources(self):
        chain=m.Chain()
        for phase in range(5):
            chain.advance(phase)
            keys=sorted(set().union(*[chain.required(i)for i in chain.items]))
            response=chain.rpc('getMultipleAccounts',[keys,{'commitment':'finalized','encoding':'base64'}])
            self.assertEqual(response['context']['slot'],chain.head)
            observed=dict(zip(keys,response['value']))
            for item in chain.items:
                p=item['pool'];s=p['orca_whirlpool'];raw=base64.b64decode(observed[p['pool_id']]['data'][0])
                self.assertEqual(p['protocol'],'orca-whirlpools-adaptive-rewards')
                self.assertEqual(len(raw),653)
                for index,reward in enumerate(s['reward_infos']):
                    at=269+index*128
                    self.assertEqual(m.b58(raw[at:at+32]),reward['mint'])
                    self.assertEqual(m.b58(raw[at+32:at+64]),reward['vault'])
                    self.assertEqual(int.from_bytes(raw[at+96:at+112],'little'),int(reward['emissions_per_second_x64']))
                    self.assertEqual(int.from_bytes(raw[at+112:at+128],'little'),int(reward['growth_global_x64']))
                self.assertGreater(sum(r['mint']!='1'*32 for r in s['reward_infos']),0)
    def test_new_reward_faults_change_actual_parser_bytes(self):
        chain=m.Chain();keys=sorted(set().union(*[chain.required(i)for i in chain.items]));params=[keys,{'commitment':'finalized','encoding':'base64'}]
        original=dict(zip(keys,chain.rpc('getMultipleAccounts',params)['value']))
        pool=chain.items[0]['pool']['pool_id']
        for fault in ['reward-control','reward-vault','uninitialized-reward-growth']:
            chain.advance(0,fault)
            bad=dict(zip(keys,chain.rpc('getMultipleAccounts',params)['value']))
            self.assertNotEqual(original[pool]['data'],bad[pool]['data'],fault)
    def test_recovery_keeps_latest_clock_and_small_native_fulfilled_request(self):
        chain=m.Chain();chain.advance(4);last=chain.timestamp;chain.advance_recovery()
        self.assertEqual(chain.timestamp,last)
        for item in chain.items:
            self.assertEqual(item['trade']['params']['amount'],'200')
            self.assertEqual(item['trade']['params']['sqrt_price_limit'],'0')
            self.assertEqual(item['original_bank']['timestamp'],last)
    def test_rewards_reference_replays_selected_split_allocations_with_carried_accounts(self):
        spec=importlib.util.spec_from_file_location('reward_native',m.ROOT/'integration/tools/ci/orca-adaptive-rewards-service-golden.py');ref=importlib.util.module_from_spec(spec);spec.loader.exec_module(ref)
        native=ref.Native(ref.BINARY)
        try:
            chain=m.Chain();item=chain.items[0];pid=item['pool']['pool_id'];bank=copy.deepcopy(item['original_bank']);config=item['original_config'];routes=[]
            for amount in ['400','600']:
                result=native.request({'action':'trade','config':config,'bank':bank,'params':dict(amount=amount,a_to_b=True,sqrt_price_limit='0',timestamp=bank['timestamp'],max_steps=8192,amount_specified_is_input=True)})
                self.assertEqual(result['status'],'fulfilled')
                routes.append(dict(amount_in=amount,amount_out=result['amount_out'],path=dict(amount_out=result['amount_out'],edges=[dict(pool_id=pid,protocol='orca-whirlpools-adaptive-rewards',zero_for_one=True)])))
                bank['accounts']=result['post_accounts']
            quote=dict(routes=routes,consumedIn='1000',refund='0',grossBuyAmount=str(sum(int(r['amount_out'])for r in routes)))
            request=dict(sellAmount='1000',sellToken=config['token0'],buyToken=config['token1'])
            self.assertEqual(m.replay_plan(quote,request,chain.items,native),2)
            bad=copy.deepcopy(quote);bad['routes'][1]['amount_out']='1'
            with self.assertRaises(ValueError):m.replay_plan(bad,request,chain.items,native)
        finally:native.close()

if __name__=='__main__':unittest.main()
