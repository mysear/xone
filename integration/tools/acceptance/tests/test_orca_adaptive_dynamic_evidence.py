import copy
import sys
import unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import orca_adaptive_dynamic_evidence as m

class AdaptiveDynamicEvidence(unittest.TestCase):
    def report(self):
        return dict(passed=True,state='PASSED',source_binding={'s':'current'},input_hashes={'fixture':'current'},inputs_unchanged=True,started_at_unix_ms=1000,completed_at_unix_ms=2000,checks=list(m.CHECKS),quote_checks=20,successful_quotes=16,native_rejections=4,pool_count=2,phase_count=5,account_batches=1,block_reads=1,native_plan_steps=16,test_quote_budget=m.BUDGET,verified=False,production_ready=False,all_requested_features_complete=False,trust=m.TRUST)
    def test_all_current_inputs_and_normal_source_recovery_checks_are_required(self):
        m.validate_process(self.report(),{'s':'current'},{'fixture':'current'},3000)
        for key,value in [('passed',False),('source_binding',{}),('input_hashes',{}),('inputs_unchanged',False),('checks',m.CHECKS[:-1]),('native_rejections',0),('native_plan_steps',0),('account_batches',0),('verified',True),('trust','LIVE_SBF'),('completed_at_unix_ms',4000)]:
            p=self.report();p[key]=value
            with self.assertRaises(ValueError,msg=key):m.validate_process(p,{'s':'current'},{'fixture':'current'},3000)
        with self.assertRaises(ValueError):m.validate_process(self.report(),{'s':'current'},{'fixture':'current'},3_603_000)
    def test_complete_original_reward_banks_and_poststates_match_projection(self):
        m.check_projection()
    def test_reviews_must_bind_core_and_integration_tools_with_independent_agents(self):
        inputs={'fixture':'current'}
        r=dict(scope=m.SCOPE,source_tree_sha256='current',input_sha256=m.input_digest(inputs),input_files=1,reviews={key:dict(passed=True,agent_id=key,open_p1_p2=0)for key in ['spec','quality']})
        m.validate_review(r,{'source_tree_sha256':'current'},inputs)
        for bad in ['missing-input','stale-input','count','same-agent','missing-review','open']:
            v=copy.deepcopy(r)
            if bad=='missing-input':del v['input_sha256']
            elif bad=='stale-input':v['input_sha256']='old'
            elif bad=='count':v['input_files']=0
            elif bad=='same-agent':v['reviews']['quality']['agent_id']='spec'
            elif bad=='missing-review':del v['reviews']['quality']
            else:v['reviews']['spec']['open_p1_p2']=1
            with self.assertRaises(ValueError,msg=bad):m.validate_review(v,{'source_tree_sha256':'current'},inputs)

if __name__=='__main__':unittest.main()
