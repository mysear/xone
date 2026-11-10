"""Finite EVM reference scope and saved-output tampering must fail before replay."""
import copy
import json
from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
from r01_public_catalog import ROOT,load_json,CatalogError
BASE=ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2'

class ReferenceScopeTests(unittest.TestCase):
    def test_reference_scope_refuses_missing_duplicate_or_replaced_H_version(self):
        import r01_public_catalog_evm_acceptance as a
        row={'id':'base-uniswap-v3','chain':'base','protocol':'uniswap-v3','factory':'0x'+'11'*20,'pool':'0x'+'22'*20,'anchor':{'height':1,'hash':'0x'+'33'*32},'factory_runtime_sha256':'44'*32,'pool_runtime_sha256':'55'*32,'implementation_runtime_sha256':None}
        self.assertEqual(a.scope_errors([row],[row]),[])
        self.assertTrue(a.scope_errors([row,row],[row]));self.assertTrue(a.scope_errors([],[row]));self.assertTrue(a.scope_errors([row],[]))
        for field in ['height','hash','pool_runtime_sha256','implementation_runtime_sha256']:
            bad=copy.deepcopy(row)
            if field in ['height','hash']:bad['anchor'][field]=2 if field=='height' else '0x'+'66'*32
            else:bad[field]='66'*32
            self.assertTrue(a.scope_errors([bad],[row]))

    def test_saved_success_reports_never_qualify_without_actual_replay(self):
        import r01_public_catalog_evm_acceptance as a
        catalog=load_json(BASE/'catalog-qualified-v2.json');scope=load_json(BASE/'scope-qualified-v2.json')
        for row in catalog['entries']:row['qualification']='QUALIFIED';row['passed']=True
        report=a.verify_catalog(catalog,scope,False)
        self.assertTrue(report['structurally_valid'],report['errors'])
        self.assertFalse(report['qualified_declared_reference_scope'])
        self.assertTrue(all(r['status']=='NOT_QUALIFIED' for r in report['entries']))

    def test_another_directory_cannot_supply_the_actual_execution_binding(self):
        import r01_public_catalog_evm_acceptance as a
        catalog=load_json(BASE/'catalog-qualified-v2.json');scope=load_json(BASE/'scope-qualified-v2.json')
        catalog['entries'][0]['directory']='integration/test/source/deployment-catalog/evm-continuation-v2/optimism-01'
        report=a.verify_catalog(catalog,scope,False)
        self.assertFalse(report['structurally_valid'])
        self.assertIn('BUNDLE_INPUT_PATH_BINDING',str(report['errors']))

    def test_real_execution_requires_exact_actor_nonce_receipt_and_net_recipient(self):
        import r01_public_catalog_evm_acceptance as a
        data=load_json(BASE/'base-01/fork-execution.json');case=data['cases'][0]
        row={'id':case['catalog_deployment_id'],'chain':'base','chain_id':8453,'factory':case['factory'],'pool':case['pool'],'anchor':data['anchor'],'pool_runtime_sha256':case['pool_runtime_sha256']}
        a.validate_execution(row,data)
        for kind in ['receipt','net','H','override','nonce','actor']:
            bad=copy.deepcopy(data)
            if kind=='receipt':bad['cases'][0]['transactions'][-1]['receipt']['status']='0x0'
            elif kind=='net':bad['cases'][0]['actual_net_recipient']='1'
            elif kind=='H':bad['anchor']['height']+=1
            elif kind=='override':bad['balance_overrides']=[{'address':data['sender'],'value':'0x1'}]
            elif kind=='nonce':bad['cases'][0]['transactions'][1]['transaction']['nonce']='0xffff'
            else:bad['cases'][0]['transactions'][-1]['transaction']['from']='0x'+'11'*20
            with self.subTest(kind=kind),self.assertRaises(CatalogError):a.validate_execution(row,bad)

if __name__=='__main__':unittest.main()
