"""Bounded normal-process input rejects unproven getter results and wrong references."""
import importlib
from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))

class NormalBoundTests(unittest.TestCase):
    def module(self):
        self.assertTrue((TOOLS/'r01_public_catalog_evm_normal.py').exists(),'bound normal consumer missing')
        return importlib.import_module('r01_public_catalog_evm_normal')

    def test_process_timeout_is_explicit_bounded_and_checked_before_file_reads(self):
        m=self.module()
        for value in [0,-1,181,True,35.0,'90',None]:
            with self.subTest(value=value), self.assertRaisesRegex(ValueError,'NORMAL_PROCESS_TIMEOUT_BUDGET'):
                m.execute(Path('/missing-case'),Path('/missing-probe'),Path('/missing-output'),timeout_seconds=value)

    def test_raw_success_label_cannot_replace_mpt_getter_execution(self):
        m=self.module()
        with self.assertRaises(ValueError):m.validate_probe({'passed':True,'calls':[],'missing':[]})
        with self.assertRaises(ValueError):m.validate_probe({'passed':True,'calls':[{'error':None,'all_code_and_storage_MPT_bound':False,'steps':1}],'missing':[]})
        with self.assertRaises(ValueError):m.validate_probe({'passed':True,'calls':[{'error':None,'all_code_and_storage_MPT_bound':True,'steps':0}],'missing':[]})

    def test_normal_cache_and_config_must_match_actual_getter_bytes(self):
        import copy
        m=self.module();d=m.BASE/'base-aerodrome-v2-01';p=m.load_json(d/'getter-probe-02.json');c=m.load_json(d/'capture.json')
        self.assertTrue(hasattr(m,'validate_cache'),'normal cache lacks executed-getter binding')
        m.validate_cache(c,p)
        bad=copy.deepcopy(c);bad['pool_config']['fee_numerator']+=1
        with self.assertRaises(ValueError):m.validate_cache(bad,p)
        values={m.call_key(x['to'],x['data']):x['value'] for x in p['calls']};values[next(iter(values))]='0x'+'00'*32
        with self.assertRaises(ValueError):m.validate_cache(c,p,values)

    def test_recorded_getter_rejection_cannot_be_promoted_to_positive(self):
        m=self.module();probe=m.load_json(m.BASE/'polygon-uniswap-v3-01/getter-probe-01.json')
        m.validate_rejection(probe)
        with self.assertRaises(ValueError):m.validate_probe(probe)
        with self.assertRaises(ValueError):m.validate_rejection(m.load_json(m.BASE/'base-aerodrome-v2-01/getter-probe-02.json'))

    def test_compiler_tick_plan_covers_actual_failed_slot_and_all_members(self):
        import r01_public_catalog_evm_supplement as supplement
        m=self.module();d=m.BASE/'base-uniswap-v3-01';plan=supplement.storage_plan(d/'input.json',d/'getter-probe-02.json')
        self.assertEqual(plan['tick_layout']['member_slots'],[0,1,2,3])
        self.assertEqual(plan['tick_layout']['initialized_tick_count'],256)
        self.assertEqual(len(plan['accounts']['0xd0b53d9277642d899df5c87a3966a349a798f224']),1024)
        self.assertIn('0x2e4e897866183ce3c57913da1b7dae1a336dd9dca508e649658deb013114924e',plan['accounts']['0xd0b53d9277642d899df5c87a3966a349a798f224'])

    def test_existing_aero_sample_getters_supply_config_only_not_typed_pool(self):
        m=self.module();d=m.BASE/'base-aerodrome-v2-01';p=m.load_json(d/'getter-probe-02.json');row=m.load_json(d/'input.json')['row']
        cfg=m.pool_config(row,p)
        self.assertEqual(cfg['protocol'],'aerodrome-v2-volatile')
        self.assertEqual(cfg['id'],row['pool'])
        self.assertNotIn('reserve0',cfg);self.assertNotIn('stable_state',cfg)

if __name__=='__main__':unittest.main()
