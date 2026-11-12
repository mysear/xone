#!/usr/bin/env python3
"""Close finite same-source checks and authentic captured-plan/original-state links."""
from pathlib import Path
from collections import Counter
import copy,json
from weighted_evidence import ROOT,OUT,sha,source_hash,write_result
from validate_solana_expanded import helper_inputs
from solana_expanded_reference import verify_family,verify_pump,verify_migration,delta_token
VALIDATION=OUT/'core-solana-expanded-validation-20261111.json';TARGET=OUT/'core-solana-expanded-acceptance-20261111.json'
FAMILIES=['bitmap','rewards','orders','token2022','pump-amm']
def require(ok,why):
 if not ok:raise ValueError('SOLANA_EXPANDED_ACCEPTANCE:'+why)
def artifact(item):
 path=(ROOT/item['path']).resolve();require(path.is_relative_to(ROOT)and path.is_file()and sha(path)==item['sha256'],'artifact-binding');return path
def freeze(path):
 destination=OUT/f'solana-final-frozen-{path.stem[:35]}-{sha(path)}.json'
 if not destination.exists():destination.write_bytes(path.read_bytes())
 return dict(path=str(destination.relative_to(ROOT)),sha256=sha(destination))
def main():
 v=json.loads(VALIDATION.read_bytes());source=source_hash();require(v.get('passed')is True and v['state']=='LOCAL_SCOPE_VERIFIED'and source==v['source_tree_sha256']==v['source_tree_sha256_after'],'same-source-validation');require(v['helper_sha256']==helper_inputs()and v['tool_sha256']==sha(Path(__file__).with_name('validate_solana_expanded.py')),'validation-helper-binding');require(len(v['checks'])==33 and len({c['name']for c in v['checks']})==33 and all(c['exit_code']==0 for c in v['checks']),'finite-check-coverage')
 for c in v['checks']:artifact(c['log'])
 require(set(v['normal_processes'])==set(FAMILIES),'process-family-coverage');total=0;receipts={}
 for family in FAMILIES:
  path=artifact(v['normal_processes'][family]);r=json.loads(path.read_bytes());require(r.get('passed')is True and source==r['source_tree_sha256']==r['source_tree_sha256_after']and r['family']==family,'normal-source-and-family');require(r['normal_exact_out_exercised']is False,'normal-exact-out-authority');require(r['api_records']==r['original_sbf_swaps']==9 and len(r['selected_indices'])==len(set(r['selected_indices']))==4,'normal-coverage');require(r['tool_sha256']==sha(Path(__file__).with_name('solana_expanded_smoke.py')),'normal-tool-binding')
  for name,digest in (r['helper_sha256']|r['binaries']).items():require(sha(ROOT/name)==digest,'normal-dependency-changed:'+name)
  fixture_path=ROOT/'integration/test/contracts'/('pump-amm.json'if family=='pump-amm'else'meteora-'+family+'.json');original_path=OUT/('core-pump-amm-original-20261111.json'if family=='pump-amm'else'core-meteora-'+family+'-original-20261111.json');require(sha(fixture_path)==r['reference_sha256']and sha(original_path)==r['original_sha256'],'normal-original-input-binding');f=json.loads(fixture_path.read_bytes());e=json.loads(original_path.read_bytes());captures={k:json.loads(artifact(a).read_bytes())for k,a in r['artifacts'].items()};require(set(captures)=={'captured_plans','model_projection','original_capture'}and all(len(c['records'])==9 for c in captures.values()),'captured-record-coverage');model=captures['model_projection'];require(model.get('passed')is True and model['input_sha256']==r['artifacts']['captured_plans']['sha256']and model['original_program_execution_claimed']is False,'production-projection-input-binding');public=captures['captured_plans']['records'];counts=Counter({index:3 if index==r['selected_indices'][-1]else 2 for index in r['selected_indices']});require(Counter(c['case_index']for c in public)==counts,'distinct-original-bank-coverage')
  require('router-checkpoint-tail-restart'in r['checks']and all('fault-withdraw-and-recovery:'+fault in r['checks']for fault in ['program-code','clock-slot']),'process-fault-and-restart-coverage')
  for record,projected,original in zip(public,model['records'],captures['original_capture']['records']):
   require(record['case_index']==projected['case_index']==original['case_index'],'case-alignment');require(record['execution_plan']['plan_hash']==projected['plan_hash']==original['plan_hash'],'plan-alignment');require(len(projected['steps'])==1,'single-physical-pool-template');step=projected['steps'][0];n=original['native'];pinned=e['records'][record['case_index']]['native'];require(n['passed']and n['before']==pinned['before']and n['after']==pinned['after'],'original-complete-bank');require(step['before']==record['snapshot']['pools'][0],'model-source-bank');require(step['amount_in']==record['request']['sellAmount']and step['amount_out']==record['quote']['grossBuyAmount'],'public-model-amounts');paid=-delta_token(n,record['request']['sellToken'],record['request']['caller']);received=delta_token(n,record['request']['buyToken'],record['request']['recipient']);require(str(paid)==step['amount_in']and str(received)==step['amount_out'],'actual-user-token-funds')
   name='pump'if family=='pump-amm'else'meteora'
   for key in ['before','after']:
    expected=copy.deepcopy(f['cases'][record['case_index']]['steps'][0]['pool'if key=='before'else'after']);actual=step[key];expected['chain_key']=actual['chain_key'];expected[name].update(slot=actual[name]['slot'],timestamp=actual[name]['timestamp']);
    if key=='after':expected[name]['derived']=True
    require(expected==actual,'complete-model-state:'+family)
  total+=9;receipts[family]=freeze(path)
 refs=dict(dlmm={name:verify_family(name)for name in FAMILIES if name!='pump-amm'},pump={name:verify_pump(name)for name in ['amm','exact-out','sell-exact-out']},migration=verify_migration());require(source_hash()==source,'source-changed-during-acceptance');result=dict(schema_version=1,passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256=source,validation=freeze(VALIDATION),normal_processes=receipts,reference_results=refs,finite_checks=33,rust_test_executions=v['rust_test_executions'],actual_normal_api_plans=total,actual_plan_original_sbf_swaps=total,source_fault_recovery_scenarios=10,router_restart_scenarios=5,normal_exact_out_exercised=False,normal_bitmap_negative_boundary_exercised=False,original_migration_cpis=1,migration_repeated_noop=1,actual_migrated_account_swaps=3,original_real_quote_bound_controls=1,dlmm_strategy_cross_products_implemented=False,arbitrary_token2022_extensions_implemented=False,automatic_migration_discovery_implemented=False,final_tool_sha256=sha(__file__),all_requested_features_complete=False,production_ready=False,authority='finite fixed profiles, controlled original programs/banks and explicit original created-account handoff; no public atomic builder, trusted public sourcebank, automatic migration discovery, chain-cost or global optimum qualification');write_result(TARGET,result);print(json.dumps({k:result[k]for k in ['passed','state','finite_checks','rust_test_executions','actual_normal_api_plans']}))
if __name__=='__main__':main()
