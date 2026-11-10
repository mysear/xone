"""Common-H replay transport and source-wide withdrawal, without process proof."""
import importlib
import copy
import json
from pathlib import Path
import sys
import unittest

ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))


class SharedReplayTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module=importlib.import_module('managed_v2_scheduled_shared_smoke')
        cls.raw=json.loads((ROOT/'integration/test/contracts/managed-v2-scheduled-shared.json').read_bytes())
        cls.thin=json.loads((ROOT/'integration/test/contracts/managed-v2-scheduled-shared-pools.json').read_bytes())

    def test_every_common_bank_has_exact_canonical_ancestry_and_all_original_reads(self):
        for mode in ('mixed','active'):
            chain=self.module.ReplayChain(self.raw,self.thin,mode)
            for bank in (b for b in self.raw['banks'] if b['namespace']==mode):
                chain.select(bank['id'])
                self.assertEqual(chain.rpc('eth_getBlockByNumber',['latest',False]),bank['header'])
                self.assertEqual(set(chain.canonical),set(range(chain.head+1)))
                for n in range(1,chain.head+1):
                    self.assertEqual(chain.canonical[n]['parentHash'],chain.canonical[n-1]['hash'])
                self.assertEqual(len(chain.frame['pools']),2)
                for row in bank['pools']:
                    for g in row['state']['getters']:
                        self.assertEqual(chain.rpc('eth_call',[{'to':g['to'],'data':g['data']},chain.anchor()]),g['result'])

    def test_global_wallet_fault_is_consistent_for_both_peers_and_keeps_native_inputs(self):
        chain=self.module.ReplayChain(self.raw,self.thin,'mixed')
        first=next(b for b in self.raw['banks'] if b['namespace']=='mixed')
        original=copy.deepcopy(chain.frames)
        chain.select(first['id'],fault='shared-wallet-underfund')
        token=chain.frame['pools'][0]['multi_asset']['assets'][0]['token']
        key=next(k for k,method in chain.frame['methods'].items() if k[0]==token and method=='balanceOf' and k[1].endswith(chain.vault[2:]))
        value=chain.rpc('eth_call',[{'to':key[0],'data':key[1]},chain.anchor()])
        cash=[int(p['multi_asset']['managed_v2']['token_info'][0]['cash']) for p in chain.frame['pools']]
        self.assertEqual(int(value,16),sum(cash)-1)
        self.assertGreaterEqual(int(value,16),max(cash),'each individual pool could pass; the complete group must fail')
        self.assertGreater(chain.fault_hits,0)
        chain.select(first['id'])
        self.assertEqual(chain.frames,original)

    def test_peer_faults_and_exact_anchor_are_fail_closed(self):
        chain=self.module.ReplayChain(self.raw,self.thin,'active')
        ident=next(iter(chain.frames))
        chain.select(ident,fault='missing-peer-getter')
        peer=chain.frame['pools'][1]['pool_id']
        key=next(k for k,m in chain.frame['methods'].items() if k[0]==peer and m=='getActualSupply')
        with self.assertRaises(ValueError):
            chain.rpc('eth_call',[{'to':key[0],'data':key[1]},chain.anchor()])
        chain.select(ident)
        token=chain.frame['pools'][0]['multi_asset']['assets'][0]['token']
        for anchor in ('latest',{'blockHash':chain.anchor()['blockHash']},
                       {**chain.anchor(),'requireCanonical':False},{**chain.anchor(),'extra':1}):
            with self.assertRaises(ValueError):
                chain.rpc('eth_getCode',[token,anchor])
        with self.assertRaises(ValueError):chain.select(ident,fault='unknown')

    def test_raw_word_fault_targets_an_explicit_scheduled_profile_in_each_mode(self):
        for mode in ('mixed','active'):
            chain=self.module.ReplayChain(self.raw,self.thin,mode)
            ident=next(iter(chain.frames))
            for fault in ('missing-peer-word','reserved-peer-word'):
                chain.select(ident,fault=fault)
                target=chain.fault_evidence['key'][0]
                p=next(p for p in chain.frame['pools'] if p['pool_id']==target)
                self.assertEqual(p['protocol'],'balancer-v2-managed-scheduled', 'legacy static reader does not query schedule words')


class SharedQuoteGuardTests(unittest.TestCase):
    def test_full_reference_and_two_physical_route_conservation_are_required(self):
        import copy
        module=importlib.import_module('managed_v2_scheduled_shared_smoke')
        thin=json.loads((ROOT/'integration/test/contracts/managed-v2-scheduled-shared-pools.json').read_bytes())
        bank=next(iter(thin['banks'].values()))
        pools=[p['pool'] for p in bank['pools']]
        ref={'block':{'hash':bank['header']['hash']},'online_generation':3,'scope_hash':'a'*64,'canonical_epoch':4,'journal_seq':7}
        request={'sellAmount':'5000','sellToken':pools[0]['token0'],'buyToken':pools[0]['token1'],'slippageBps':50}
        routes=[{'amount_in':'2500','amount_out':'100','path':{'tokens':[request['sellToken'],request['buyToken']],'amount_in':'2500','amount_out':'100','edges':[{'pool_id':p['pool_id'],'protocol':p['protocol'],'from_token':request['sellToken'],'to':request['buyToken']}]}} for p in pools]
        q={'snapshotRef':copy.deepcopy(ref),'onlineGeneration':3,'scopeHash':ref['scope_hash'],'verification':{'level':'INDICATIVE','success':False},'consumedIn':'5000','refund':'0','grossBuyAmount':'200','minBuyAmount':'199','routes':routes,'quality':{'candidateScope':[p['pool_id'] for p in pools],'excludedPools':{}}}
        module.check_quote(200,q,request,ref,pools)
        mutations=[lambda x:x['snapshotRef'].update(online_generation=2),lambda x:x.update(onlineGeneration=2),lambda x:x.update(refund='1'),lambda x:x.update(minBuyAmount='200'),lambda x:x.update(grossBuyAmount='201'),lambda x:x['routes'].pop(),lambda x:x['routes'][1]['path']['edges'][0].update(pool_id=pools[0]['pool_id']),lambda x:x['routes'][0].update(amount_in='1'),lambda x:x['verification'].update(level='VERIFIED'),lambda x:x['quality']['candidateScope'].pop()]
        for mutate in mutations:
            value=copy.deepcopy(q)
            mutate(value)
            with self.assertRaises(ValueError):module.check_quote(200,value,request,ref,pools)
        with self.assertRaises(ValueError):module.check_quote(503,{},request,ref,pools)
