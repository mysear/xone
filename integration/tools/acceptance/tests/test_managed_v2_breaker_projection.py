"""New breaker observations require full qualification before Pool projection."""
from pathlib import Path
import importlib
import json
import sys
import unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
class ManagedBreakerProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.p=importlib.import_module('managed_v2_breaker_projection')
        cls.bank=cls.p.load(ROOT/'integration/test/contracts/managed-v2-breaker.json')
        cls.value=cls.p.project(cls.bank)
    def test_every_raw_pool_and_bound_getter_is_retained_in_order(self):
        native={b['id']:b for b in self.bank['banks']};count=0
        self.assertEqual(len(self.value['banks']),len(self.bank['banks']))
        for bank in self.value['banks']:
            branch=self.bank['branches'][bank['namespace']]
            self.assertEqual(len(bank['pools']),2)
            for entry in bank['pools']:
                pool=entry['pool'];m=pool['multi_asset']['managed_v2'];p=next(p for p in branch['pools'] if p['name']==entry['name']);s=next(p['state'] for p in native[bank['id']]['pools'] if p['name']==entry['name'])
                self.assertEqual(pool['protocol'],self.p.PROTOCOL);self.assertEqual(pool['adapter_version'],self.p.ADAPTER)
                self.assertEqual([t['token'] for t in m['circuit_breakers']['tokens']],branch['tokens'])
                for i,t in enumerate(m['circuit_breakers']['tokens']):
                    values=next(g['decoded'] for g in s['getters'] if g['to']==p['pool'] and g['method']=='getCircuitBreakerState' and g['args']==[t['token']])
                    self.assertEqual([t[k] for k in self.p.GETTER_FIELDS],values)
                    self.assertEqual(m['token_info'][i]['circuit_breaker_enabled'],int(t['state_word'])!=0)
                count+=1
        self.assertEqual(count,len(self.bank['banks'])*2)
        self.assertFalse(self.value['production_ready']);self.assertFalse(self.value['all_requested_features_complete'])
    def test_unqualified_or_changed_source_and_metadata_cannot_project(self):
        with self.assertRaises(ValueError):self.p.project(dict(self.bank))
        original=self.bank['banks'][0]['header']['hash']
        try:
            self.bank['banks'][0]['header']['hash']='0x'+'f'*64
            with self.assertRaises(ValueError):self.p.project(self.bank)
        finally:self.bank['banks'][0]['header']['hash']=original
        self.assertEqual(self.value,self.p.project(self.bank))
