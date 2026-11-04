"""Explicit current-batch correspondence; never infer completed scope from empty lists."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from progress_document import read_plan
import re
PLAN='DEVELOPMENT_PROGRESS.md'
PLAN_ANCHOR='plan-2026-10-29-raydium-clmm-dynamic'
PREFIX='rayclmm-dynamic-'
CLAUSE_EVIDENCE=['integration/artifacts/local/rayclmm-dynamic-native-source.json','integration/artifacts/local/rayclmm-dynamic-differential.json','integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']
CLAUSES={'11.3.rayclmm-dynamic-state': {'work_packages': ['WP02', 'WP25'], 'paths': ['chain-parser-go/internal/domain/rayclmm_dynamic_test.go', 'dex-router/crates/domain/tests/rayclmm_dynamic.rs', 'common/protoc/proto/dex/v1/state.proto', 'chain-parser-go/internal/journal/frame_test.go', 'dex-router/crates/kafka-transport/tests/frames.rs'], 'tests': ['TestRayCLMMDynamicStateProfileAndWire', 'TestRayCLMMDynamicParametersAndOldProfileRejection', 'dynamic_state_profile_width_and_wire_are_explicit', 'independent_protoc_preserves_dynamic_signed_reference_and_rejects_bad_profile', 'TestIndependentRayCLMMDynamicProtocParametersAndAdmission']}, '7.3.rayclmm-dynamic-accounts': {'work_packages': ['WP04', 'WP05', 'WP20'], 'paths': ['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs', 'chain-parser-rust/crates/solana-ingest/src/rpc.rs', 'chain-parser-rust/crates/solana-ingest/tests/rayclmm_dynamic.rs'], 'tests': ['dynamic_actual_packed_offsets_signed_index_le_seed_and_all_fee_modes', 'dynamic_padding_default_and_old_profiles_fail_closed', 'dynamic_parameter_and_time_boundaries', 'original_dynamic_account_fixture_matches_full_pool_and_next_bank']}, '12.2.rayclmm-dynamic-exact-in': {'work_packages': ['WP11', 'WP20'], 'paths': ['dex-router/crates/protocol-math/tests/rayclmm_dynamic.rs', 'dex-router/crates/protocol-math/tests/rayclmm_dynamic_differential.rs'], 'tests': ['pinned_native_dynamic_cases_match_complete_post_pool', 'pinned_native_dynamic_shared_sequences_match_each_and_final_pool', 'dynamic_reference_time_windows_include_equalities_and_future_saturation', 'dynamic_budget_counts_spacing_iterations_including_no_price_movement']}, '12.9.rayclmm-dynamic-physical-pool': {'work_packages': ['WP12', 'WP13'], 'paths': ['dex-router/crates/optimizer/tests/rayclmm_dynamic_shared.rs'], 'tests': ['shared_rayclmm_dynamic_sequences_preserve_volatility_fees_and_complete_native_post_pool']}, '12.14.rayclmm-dynamic-reference': {'work_packages': ['WP03', 'WP11', 'WP13'], 'paths': ['integration/tools/reference/raydium-clmm/README.md', 'integration/tools/ci/rayclmm-dynamic-golden.py', 'dex-router/crates/protocol-math/tests/rayclmm_dynamic_differential.rs'], 'tests': ['pinned_native_dynamic_cases_match_complete_post_pool', 'native_dynamic_conservative_admission_is_distinct_from_actual_step_budget']}, '12.11.rayclmm-dynamic-execution-boundary': {'work_packages': ['WP14', 'WP16', 'WP20'], 'paths': ['dex-router/crates/execution-plan/tests/plan.rs', 'integration/tools/acceptance/rayclmm_dynamic_smoke.py'], 'tests': ['rayclmm_dynamic_quote_cannot_be_encoded_as_evm_v2_calldata']}}
TASKS={
 'Task1':{'title':'固定原动态费语义','work_packages':['WP03','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-dynamic-native-source.json','docs/TECHNICAL_REFERENCE.md']},
 'Task2':{'title':'跨语言state/wire/config先红后绿','work_packages':['WP02','WP25'],'evidence_refs':['integration/artifacts/local/rayclmm-dynamic-go-green.log','integration/artifacts/local/rayclmm-dynamic-wire.log','integration/artifacts/local/rayclmm-dynamic-differential.json']},
 'Task3':{'title':'实际账户/RPC先红后绿','work_packages':['WP04','WP05','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-dynamic-parser-main.log','integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json']},
 'Task4':{'title':'独立动态数学与原program参考先红后绿','work_packages':['WP03','WP11','WP12','WP13','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-dynamic-native-source.json','integration/artifacts/local/rayclmm-dynamic-differential.json']},
 'Task5':{'title':'进程/审查/完整回归/逐项进度','work_packages':['WP02','WP03','WP04','WP05','WP11','WP12','WP13','WP20','WP25'],'evidence_refs':['integration/artifacts/local/rayclmm-dynamic-three-service-smoke.json','integration/artifacts/local/rayclmm-dynamic-review.json','integration/artifacts/local/rayclmm-dynamic-clock-red.log','integration/artifacts/local/rayclmm-dynamic-clock-green.log','integration/artifacts/local/rayclmm-dynamic-resume-green.log','integration/artifacts/local/full-local-acceptance-resume.json','integration/artifacts/local/raydium-lease-recovery-audit.json','integration/artifacts/local/rayclmm-dynamic-final-source.json','integration/artifacts/local/rayclmm-dynamic-progress-consistency.json']},
}

def bound_list(actual,expected):
 return isinstance(actual,list)and bool(actual)and len(actual)==len(set(actual))and set(expected)<=set(actual)

def validate(document,root):
 root=Path(root);issues=[]
 work={w['id']for w in document['work_packages']}
 rows=[r for r in document['requirement_traceability']if PREFIX in r.get('design_clause','')]
 names=[r.get('design_clause')for r in rows]
 if len(names)!=len(CLAUSES)or set(names)!=set(CLAUSES):issues.append('exact functional clause identities required')
 for row in rows:
  name=row.get('design_clause');expected=CLAUSES.get(name)
  if expected is None:continue
  if not bound_list(row.get('work_packages'),expected['work_packages'])or not set(row.get('work_packages',[]))<=work:issues.append(name+': work packages')
  if not bound_list(row.get('tests'),expected['tests']):issues.append(name+': test bindings')
  if not bound_list(row.get('paths'),expected['paths']):issues.append(name+': declared source paths')
  texts=[]
  for relative in row.get('paths',[]):
   path=root/relative
   if not path.is_file():issues.append(name+': missing source '+relative)
   elif path.suffix in ['.go','.rs']:texts.append(path.read_text())
  for symbol in row.get('tests',[]):
   if not any(re.search(r'\b(?:func|fn)\s+'+re.escape(symbol)+r'\s*\(',text)for text in texts):issues.append(name+': test not in declared source '+symbol)
  if not bound_list(row.get('evidence_refs'),CLAUSE_EVIDENCE):issues.append(name+': evidence bindings')
  for relative in row.get('evidence_refs',[]):
   if not(root/relative).is_file():issues.append(name+': missing evidence '+relative)
 tasks=document['current_batch'].get('plan_task_progress',[]);identities=[t.get('task_id')for t in tasks]
 if len(identities)!=len(TASKS)or set(identities)!=set(TASKS):issues.append('exact implementation task identities required')
 try:plan_text=read_plan(root,PLAN_ANCHOR)
 except (OSError,ValueError):plan_text=''
 for task in tasks:
  name=task.get('task_id');expected=TASKS.get(name)
  if expected is None:continue
  if task.get('plan')!=PLAN or task.get('plan_anchor')!=PLAN_ANCHOR or task.get('title')!=expected['title']or not re.search(r'^\|'+re.escape(name)+r'\|'+re.escape(expected['title'])+r'\|',plan_text,re.M):issues.append(name+': exact plan and heading')
  if task.get('status')!='LOCAL_SCOPE_VERIFIED':issues.append(name+': task not verified')
  if not bound_list(task.get('work_packages'),expected['work_packages'])or not set(task.get('work_packages',[]))<=work:issues.append(name+': work packages')
  if not bound_list(task.get('evidence_refs'),expected['evidence_refs']):issues.append(name+': task-specific evidence')
  for relative in task.get('evidence_refs',[]):
   # This audit emits its own report only after all other gates passed.
   if relative=='integration/artifacts/local/rayclmm-dynamic-progress-consistency.json':continue
   if not(root/relative).is_file():issues.append(name+': missing evidence '+relative)
 return issues
