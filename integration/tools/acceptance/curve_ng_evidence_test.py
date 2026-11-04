#!/usr/bin/env python3
"""Bounded Curve NG evidence and startup authorization regression tests."""
import builtins
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch
sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).parent))
import curve_ng_evidence as E
import curve_ng_regression as R


PROCESS_CHECKS = [
    'seven-ng-pools-fixed-original-runtime-and-historical-rpc',
    'complete-ng-balances-admin-and-oracles-in-live-protobuf',
    'all-56-original-quotes-match-both-router-replicas',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'invalid-ng-dependencies-revoke-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-ng-bank',
    'verified-and-build-remain-disabled',
]


class Startup(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.out = Path(self.tmp.name)
        for module in (E, R):
            p = patch.object(module, 'OUT', self.out)
            p.start()
            self.addCleanup(p.stop)

    def seed(self, names):
        for name in names:
            (self.out / name).write_text(json.dumps({'passed': True}))

    def assert_failed(self, name):
        self.assertIs(json.loads((self.out / name).read_text())['passed'], False)

    def broken_import(self, name, *args, **kwargs):
        if name in ('weighted_evidence', 'weighted_regression', 'curve_ng_evidence'):
            raise ImportError('injected unavailable helper ' + name)
        return self.original_import(name, *args, **kwargs)

    def test_callable_evidence_invalidates_before_import(self):
        for action, target in [(E.bind_runtime, 'runtime-binding'), (E.seal_scopes, 'scope-execution'), (E.run, 'acceptance'), (E.validate, 'acceptance')]:
            with self.subTest(action=action.__name__):
                names=['curve-ng-'+target+'.json', 'curve-ng-acceptance.json']
                self.seed(names)
                self.original_import=builtins.__import__
                with patch('builtins.__import__', self.broken_import):
                    with self.assertRaises(ImportError): action()
                for name in names:self.assert_failed(name)

    def test_callable_regression_invalidates_all_scopes_before_import(self):
        names=['curve-ng-regression-command.json','curve-ng-acceptance.json']+E.SCOPES
        self.seed(names)
        self.original_import=builtins.__import__
        with patch('builtins.__import__', self.broken_import):
            with self.assertRaises(ImportError): R.run()
        for name in names:self.assert_failed(name)

    def test_bundle_helpers_are_reused(self):
        common=types.SimpleNamespace(bind_bundle=lambda prefix:{'passed':True,'prefix':prefix},seal_bundle=lambda prefix,scopes:{'passed':True,'scopes':scopes})
        with patch.dict(sys.modules,weighted_evidence=common):
            self.assertEqual(E.bind_runtime()['prefix'],'curve-ng')
            self.assertEqual(E.seal_scopes()['scopes'],E.SCOPES)

    def test_regression_reuses_weighted_runner(self):
        calls=[]
        def execute(evidence,producers,prefix):
            calls.append((evidence,producers,prefix));return ['executed']
        with patch.dict(sys.modules,weighted_regression=types.SimpleNamespace(run=execute)):
            self.assertEqual(R.run(),['executed'])
        self.assertEqual(calls,[(E,E.PRODUCERS,'curve-ng')])

    def test_shared_runner_receives_its_complete_evidence_interface(self):
        def execute(evidence, producers, prefix):
            for attribute in ['load','require','runtime_fingerprint','snapshot_inputs','sha','write_result']:
                self.assertTrue(callable(getattr(evidence,attribute)), attribute)
            return []
        with patch.dict(sys.modules,weighted_regression=types.SimpleNamespace(run=execute)):
            R.run()

    def test_cli_failure_invalidates_even_with_python_optimize_two(self):
        for script,args,names in [
            (E.__file__,[],['curve-ng-acceptance.json']),
            (E.__file__,['--bind-runtime'],['curve-ng-runtime-binding.json','curve-ng-acceptance.json']),
            (E.__file__,['--seal-scopes'],['curve-ng-scope-execution.json','curve-ng-acceptance.json']),
            (E.__file__,['--invalid-option'],['curve-ng-acceptance.json']),
            (R.__file__,[],['curve-ng-regression-command.json','curve-ng-acceptance.json']+E.SCOPES),
        ]:
            self.seed(names)
            code='''import builtins,os,runpy,sys
from pathlib import Path
original=builtins.__import__
def broken(name,*args,**kwargs):
 if name in ('weighted_evidence','weighted_regression','curve_ng_evidence'):raise ImportError('injected cli helper failure')
 return original(name,*args,**kwargs)
builtins.__import__=broken
script,out,*arguments=sys.argv[1:]
sys.argv=[script]+arguments
# Supply an alternate ROOT via source execution; production CLI has no path override.
ns={'__file__':script,'__name__':'__main__'}
source=Path(script).read_text().replace("OUT = ROOT / 'integration/artifacts/local'",'OUT = Path('+repr(out)+')')
exec(compile(source,script,'exec'),ns)
'''
            result=subprocess.run([sys.executable,'-OO','-c',code,script,str(self.out)]+args,capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            for name in names:self.assert_failed(name)


class Gate(unittest.TestCase):
    def test_no_assertion_can_disable_require_under_optimization(self):
        for module in (E,R):
            ns={'__file__':module.__file__,'__name__':'optimized_sidecar'}
            exec(compile(Path(module.__file__).read_text(),module.__file__,'exec',optimize=2),ns)
            if 'require' in ns:
                with self.assertRaises(ValueError):ns['require'](False,'required')


class Acceptance(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.out=Path(self.tmp.name)
        patcher=patch.object(E,'OUT',self.out);patcher.start();self.addCleanup(patcher.stop)
        self.runtime={'source_tree_sha256':'current','binaries':{'router':'binary'}}
        self.inputs={'bound':'digest'}
        self.common=types.SimpleNamespace(validate_bundle=lambda prefix,scopes:(copy.deepcopy(self.runtime),copy.deepcopy(self.inputs)),
                                         runtime_fingerprint=lambda:copy.deepcopy(self.runtime),
                                         snapshot_inputs=lambda:copy.deepcopy(self.inputs),sha=E.sha)
        self.review={'source_tree_sha256':'current','input_hashes':self.inputs.copy(),'spec_review':{'status':'CLOSED','open_important_findings':0},'quality_review':{'status':'CLOSED','open_important_findings':0},'production_ready':False,'all_requested_features_complete':False}
        for name,result in [('_validate_oracle',{'steps':147,'successful_steps':144,'reverts':3}),('_validate_service',{'cases':56}),('_validate_process',{'checks':PROCESS_CHECKS.copy()})]:
            patcher=patch.object(E,name,return_value=result);patcher.start();self.addCleanup(patcher.stop)
        patcher=patch.dict(sys.modules,weighted_evidence=self.common);patcher.start();self.addCleanup(patcher.stop)
        for name in ['verification.json','curve-ng-runtime-binding.json','curve-ng-scope-execution.json']+E.SCOPES:
            (self.out/name).write_text(json.dumps({'passed':True,'fixture':name}))
        self.update()

    def update(self):
        (self.out/'curve-ng-review.json').write_text(json.dumps(self.review))

    def test_bounded_success_keeps_all_status_claims_false(self):
        value=E.run()
        self.assertIs(value['passed'],True)
        for key in ['verified','all_requested_features_complete','production_ready']:
            self.assertIs(value[key],False)
        self.assertEqual(value['fresh_scope_reports'],E.SCOPES)

    def test_open_stale_or_incomplete_review_rejected_and_old_success_revoked(self):
        original=copy.deepcopy(self.review)
        for stage in ['spec_review','quality_review']:
            for field,value in [('status','OPEN'),('open_important_findings',1),('open_important_findings',False)]:
                self.review=copy.deepcopy(original);self.review[stage][field]=value;self.update()
                E.write_result(self.out/'curve-ng-acceptance.json',{'passed':True})
                with self.assertRaises(ValueError):E.run()
                self.assertIs(json.loads((self.out/'curve-ng-acceptance.json').read_text())['passed'],False)
        for field,value in [('source_tree_sha256','old'),('input_hashes',{'old':'input'}),('production_ready',True),('all_requested_features_complete',True),('verified',True)]:
            self.review=copy.deepcopy(original);self.review[field]=value;self.update()
            with self.assertRaises(ValueError):E.run()

    def test_oracle_service_process_or_bundle_failure_cannot_reuse_success(self):
        for function in ['_validate_oracle','_validate_service','_validate_process']:
            with self.subTest(function=function):
                E.write_result(self.out/'curve-ng-acceptance.json',{'passed':True})
                with patch.object(E,function,side_effect=ValueError('injected gate failure')):
                    with self.assertRaises(ValueError):E.run()
                self.assertIs(json.loads((self.out/'curve-ng-acceptance.json').read_text())['passed'],False)
        self.common.validate_bundle=lambda *args:(_ for _ in ()).throw(ValueError('stale shared bundle'))
        with self.assertRaises(ValueError):E.run()


class PublicationRace(unittest.TestCase):
    setUp=Acceptance.setUp
    update=Acceptance.update

    def assert_revoked(self):
        self.assertIs(json.loads((self.out/'curve-ng-acceptance.json').read_text())['passed'],False)

    def replacement(self,name,value):
        path=self.out/name
        temporary=path.with_suffix('.replacement')
        temporary.write_text(json.dumps(value));temporary.replace(path)

    def test_scope_replaced_after_bundle_validation_is_rejected(self):
        def oracle(*args):
            self.replacement(E.SCOPES[0],{'passed':False,'error':'replaced scope'})
            return {'steps':147}
        with patch.object(E,'_validate_oracle',side_effect=oracle):
            with self.assertRaises(ValueError):E.run()
        self.assert_revoked()

    def test_review_replaced_immediately_after_parse_is_rejected(self):
        original=json.loads;fired=[]
        def loads(data,*args,**kwargs):
            value=original(data,*args,**kwargs)
            if isinstance(value,dict) and 'spec_review' in value and not fired:
                fired.append(True)
                changed=copy.deepcopy(value);changed['quality_review']['status']='OPEN'
                self.replacement('curve-ng-review.json',changed)
            return value
        with patch.object(E.json,'loads',side_effect=loads):
            with self.assertRaises(ValueError):E.run()
        self.assertEqual(fired,[True]);self.assert_revoked()

    def test_review_or_scope_replaced_after_staging_before_publish_is_rejected(self):
        original=E.write_result
        for name in ['curve-ng-review.json',E.SCOPES[-1],'curve-ng-scope-execution.json']:
            with self.subTest(name=name):
                # A fresh stable starting point for every actual replacement.
                if name=='curve-ng-review.json':self.update()
                fired=[]
                def write(path,value,*args,**kwargs):
                    if Path(path).name=='curve-ng-acceptance.json' and value.get('passed') is True and not fired:
                        fired.append(True);self.replacement(name,{'passed':False,'status':'OPEN'})
                    return original(path,value,*args,**kwargs)
                with patch.object(E,'write_result',side_effect=write):
                    with self.assertRaises(ValueError):E.run()
                self.assertEqual(fired,[True]);self.assert_revoked()
                self.update()

    def test_replace_during_success_rename_is_detected_and_revoked(self):
        original=Path.replace;fired=[]
        def replace(path,target):
            value=json.loads(path.read_bytes())
            if Path(target)==self.out/'curve-ng-acceptance.json' and value.get('passed') is True and not fired:
                fired.append(True)
                changed=copy.deepcopy(self.review);changed['spec_review']['status']='OPEN'
                self.replacement('curve-ng-review.json',changed)
            return original(path,target)
        with patch.object(Path,'replace',replace):
            with self.assertRaises(ValueError):E.run()
        self.assertEqual(fired,[True]);self.assert_revoked()

    def test_runtime_or_inputs_changed_at_publication_are_rejected(self):
        original=E.write_result
        for which in ['runtime','inputs']:
            with self.subTest(which=which):
                fired=[]
                def write(path,value,*args,**kwargs):
                    if value.get('passed') is True and not fired:
                        fired.append(True)
                        if which=='runtime':self.runtime={'source_tree_sha256':'changed','binaries':{}}
                        else:self.inputs={'changed':'input'}
                    return original(path,value,*args,**kwargs)
                with patch.object(E,'write_result',side_effect=write):
                    with self.assertRaises(ValueError):E.run()
                self.assertEqual(fired,[True]);self.assert_revoked()
                self.runtime={'source_tree_sha256':'current','binaries':{'router':'binary'}}
                self.inputs={'bound':'digest'}

    def test_identical_byte_replacement_or_change_then_restore_is_rejected(self):
        name='curve-ng-review.json';original=(self.out/name).read_bytes()
        for aba in [False,True]:
            with self.subTest(change_then_restore=aba):
                def oracle(*args):
                    if aba:self.replacement(name,{'quality_review':{'status':'OPEN'}})
                    self.replacement(name,json.loads(original))
                    return {'steps':147}
                with patch.object(E,'_validate_oracle',side_effect=oracle):
                    with self.assertRaises(ValueError):E.run()
                self.assert_revoked()

    def test_replacement_while_capture_reads_is_rejected(self):
        original=Path.read_bytes;fired=[]
        def read(path):
            raw=original(path)
            if path==self.out/'curve-ng-review.json' and not fired:
                fired.append(True);self.replacement('curve-ng-review.json',{'status':'OPEN'})
            return raw
        with patch.object(Path,'read_bytes',read):
            with self.assertRaises(ValueError):E.run()
        self.assertEqual(fired,[True]);self.assert_revoked()

    def test_scope_changed_inside_final_bundle_revalidation_is_rejected(self):
        calls=[]
        def validate(*args):
            calls.append(True)
            if len(calls)==2:self.replacement(E.SCOPES[-1],{'passed':False})
            return copy.deepcopy(self.runtime),copy.deepcopy(self.inputs)
        self.common.validate_bundle=validate
        with self.assertRaises(ValueError):E.run()
        self.assertEqual(len(calls),2);self.assert_revoked()

    def test_published_hashes_describe_exact_bytes_validated(self):
        expected={name:E.sha(self.out/name) for name in
                  ['verification.json','curve-ng-runtime-binding.json','curve-ng-scope-execution.json',
                   'curve-ng-review.json']+E.SCOPES}
        value=E.run()
        self.assertEqual(value['validated_evidence_sha256'],expected)
        self.assertEqual(value['review_sha256'],expected['curve-ng-review.json'])
        self.assertEqual(value['scope_execution_sha256'],expected['curve-ng-scope-execution.json'])


class ProcessContract(unittest.TestCase):
    def setUp(self):
        self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
        self.out=Path(self.tmp.name)
        patcher=patch.object(E,'OUT',self.out);patcher.start();self.addCleanup(patcher.stop)
        self.checks=PROCESS_CHECKS.copy()
        self.producer=types.SimpleNamespace(PROCESS_CHECKS=self.checks)
        patcher=patch.dict(sys.modules,curve_ng_smoke=self.producer);patcher.start();self.addCleanup(patcher.stop)
        self.report={'passed':True,'production_ready':False,'verified':False,'all_requested_features_complete':False,'pool_count':7,'checks':self.checks.copy(),
                     'service_steps':56,'quote_checks':112,
                     'fault_names':['accounting','oracle','immutable','template'],
                     'historical_fixture_replay':True,'test_freshness_ms':315360000000}
        for name in E.SCOPES[:-1]:(self.out/name).write_text(json.dumps({'passed':True,'production_ready':False}))
        self.update()

    def update(self):
        (self.out/'curve-ng-three-service-smoke.json').write_text(json.dumps(self.report))

    def test_direct_bounded_report_contract(self):
        self.assertEqual(list(E.REQUIRED_PROCESS_CHECKS),PROCESS_CHECKS)
        value=E._validate_process(None)
        self.assertEqual(value['checks'],self.checks)
        self.assertEqual((value['pool_count'],value['service_steps'],value['quote_checks']),(7,56,112))
        self.assertEqual(value['fault_names'],['accounting','oracle','immutable','template'])
        self.assertIs(value['historical_fixture_replay'],True)
        self.assertEqual(value['test_freshness_ms'],315360000000)

    def test_matching_arbitrary_producer_and_report_is_rejected(self):
        arbitrary=['unrelated-first-check','unrelated-second-check']
        self.producer.PROCESS_CHECKS=arbitrary
        self.report['checks']=arbitrary.copy();self.update()
        with self.assertRaises(ValueError):E._validate_process(None)

    def test_each_fixed_semantic_is_required_even_if_both_lists_remove_it(self):
        for index in range(len(PROCESS_CHECKS)):
            with self.subTest(missing=PROCESS_CHECKS[index]):
                shortened=PROCESS_CHECKS[:index]+PROCESS_CHECKS[index+1:]
                self.producer.PROCESS_CHECKS=shortened
                self.report['checks']=shortened.copy();self.update()
                with self.assertRaises(ValueError):E._validate_process(None)

    def test_matching_reordered_or_arbitrary_eight_check_contracts_are_rejected(self):
        for changed in [list(reversed(PROCESS_CHECKS)),['unrelated-'+str(i) for i in range(8)]]:
            self.producer.PROCESS_CHECKS=changed
            self.report['checks']=changed.copy();self.update()
            with self.assertRaises(ValueError):E._validate_process(None)

    def test_replay_counts_faults_and_historical_freshness_are_required(self):
        original=copy.deepcopy(self.report)
        for field,value in [('service_steps',55),('service_steps',56.0),
                            ('quote_checks',111),('quote_checks',112.0),
                            ('fault_names',['accounting','oracle','immutable']),
                            ('fault_names',['accounting','oracle','immutable','other']),
                            ('historical_fixture_replay',False),('historical_fixture_replay',1),
                            ('test_freshness_ms',315360000001),('test_freshness_ms',315360000000.0)]:
            with self.subTest(field=field,value=value):
                self.report=copy.deepcopy(original);self.report[field]=value;self.update()
                with self.assertRaises(ValueError):E._validate_process(None)
        for field in ['service_steps','quote_checks','fault_names','historical_fixture_replay','test_freshness_ms']:
            self.report=copy.deepcopy(original);del self.report[field];self.update()
            with self.assertRaises(ValueError):E._validate_process(None)

    def test_missing_semantics_counts_and_unsafe_flags_rejected(self):
        original=copy.deepcopy(self.report)
        for field,value in [('passed',False),('checks',self.checks[:-1]),('checks',self.checks+['unreviewed-extra']),('pool_count',6),('production_ready',True),('verified',True),('all_requested_features_complete',True)]:
            self.report=copy.deepcopy(original);self.report[field]=value;self.update()
            with self.assertRaises(ValueError):E._validate_process(None)
        self.report=original;self.update();self.producer.PROCESS_CHECKS=[]
        with self.assertRaises(ValueError):E._validate_process(None)


class OriginalInputs(unittest.TestCase):
    def test_real_original_verifier_and_service_56_wire_copies(self):
        import weighted_evidence as common
        inputs=common.snapshot_inputs()
        result=E._validate_oracle(common,inputs)
        self.assertEqual((result['deployments_verified'],result['steps'],result['successful_steps'],result['reverts']),(21,147,144,3))
        self.assertEqual(E._validate_service(common,inputs)['cases'],56)

    def test_node_executable_must_be_bound_and_match_original_closure(self):
        import shutil
        import weighted_evidence as common
        node=Path(shutil.which('node'))
        inputs=common.snapshot_inputs()
        inputs.pop(str(node),None)
        with self.assertRaises(ValueError):E._validate_oracle(common,inputs)
        inputs[str(node)]=common.sha(node)
        real_sha=common.sha
        with patch.object(common,'sha',side_effect=lambda path:'changed-node' if Path(path)==node else real_sha(path)):
            with self.assertRaises(ValueError):E._validate_oracle(common,inputs)

    def test_unbound_or_altered_reference_is_rejected(self):
        import weighted_evidence as common
        with self.assertRaises(ValueError):E._validate_oracle(common,{})
        with self.assertRaises(ValueError):E._validate_service(common,{})


if __name__ == '__main__':unittest.main()
