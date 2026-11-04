"""Explicit current-batch correspondence; never infer completed scope from empty lists."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from progress_document import read_plan
import re
PLAN='DEVELOPMENT_PROGRESS.md'
PLAN_ANCHOR='plan-2026-10-29-raydium-clmm-custom-seed'
PREFIX='rayclmm-custom-seed-'
CLAUSE_EVIDENCE=['integration/artifacts/local/rayclmm-seed-native-source.json','integration/artifacts/local/rayclmm-seed-differential.json','integration/artifacts/local/rayclmm-seed-three-service-smoke.json']
CLAUSES={
 '11.3.rayclmm-custom-seed-state':{'work_packages':['WP02','WP25'],'paths':['chain-parser-go/internal/domain/rayclmm_seed_test.go','dex-router/crates/domain/tests/rayclmm_seed.rs','common/protoc/proto/dex/v1/state.proto','chain-parser-go/internal/journal/frame_test.go','dex-router/crates/kafka-transport/tests/frames.rs'],'tests':['TestRayCLMMCustomSeedIdentityAndWire','TestRayCLMMCustomSeedRejectsLegacyAndWidth','custom_seed_is_bound_to_pool_and_protobuf_width','independent_protoc_preserves_custom_seed_identity_and_u16_width','TestIndependentRayCLMMCustomSeedProtocIdentity']},
 '7.3.rayclmm-custom-seed-accounts':{'work_packages':['WP04','WP05','WP20'],'paths':['chain-parser-rust/crates/solana-ingest/src/raydium_clmm.rs','chain-parser-rust/crates/solana-ingest/src/rpc.rs','chain-parser-rust/crates/solana-ingest/tests/rayclmm_seed.rs'],'tests':['seed_decodes_all_u16_boundaries_and_static_fee_modes','seed_rejects_zero_wrong_endianness_padding_and_legacy_pool_identity','seed_rpc_fetches_complete_finalized_15_to_19_account_batches','seed_rpc_keeps_same_mint_custom_pools_distinct_in_one_bank_and_scope']},
 '12.2.rayclmm-custom-seed-exact-in':{'work_packages':['WP11','WP20'],'paths':['dex-router/crates/protocol-math/tests/rayclmm_seed_differential.rs'],'tests':['pinned_native_custom_seed_cases_match_complete_post_pool','pinned_native_custom_seed_shared_sequences_match_each_and_final_pool']},
 '12.9.rayclmm-custom-seed-physical-pool':{'work_packages':['WP12','WP13'],'paths':['dex-router/crates/optimizer/tests/rayclmm_seed_shared.rs'],'tests':['shared_rayclmm_custom_seed_sequences_preserve_complete_native_post_pool','distinct_nonzero_seeds_share_mints_but_never_share_physical_pool_state']},
 '12.14.rayclmm-custom-seed-reference':{'work_packages':['WP03','WP11','WP13'],'paths':['integration/tools/reference/raydium-clmm/README.md','integration/tools/ci/rayclmm-seed-golden.py','dex-router/crates/protocol-math/tests/rayclmm_seed_differential.rs'],'tests':['pinned_native_custom_seed_cases_match_complete_post_pool']},
 '12.11.rayclmm-custom-seed-execution-boundary':{'work_packages':['WP14','WP16','WP20'],'paths':['dex-router/crates/execution-plan/tests/plan.rs','integration/tools/acceptance/rayclmm_seed_smoke.py'],'tests':['rayclmm_seed_quote_cannot_be_encoded_as_evm_v2_calldata']},
}
TASKS={
 'Task1':{'title':'固定原seed语义','work_packages':['WP03','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-seed-native-source.json','docs/TECHNICAL_REFERENCE.md']},
 'Task2':{'title':'跨语言state/wire/config先红后绿','work_packages':['WP02','WP25'],'evidence_refs':['integration/artifacts/local/rayclmm-seed-go-green.log','integration/artifacts/local/rayclmm-seed-wire.log','integration/artifacts/local/rayclmm-seed-differential.json']},
 'Task3':{'title':'实际账户/RPC先红后绿','work_packages':['WP04','WP05','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-seed-parser-main.log','integration/artifacts/local/rayclmm-seed-three-service-smoke.json']},
 'Task4':{'title':'原native金额与共享池','work_packages':['WP03','WP11','WP12','WP13','WP20'],'evidence_refs':['integration/artifacts/local/rayclmm-seed-native-source.json','integration/artifacts/local/rayclmm-seed-differential.json']},
 'Task5':{'title':'进程/审查/完整回归/逐项进度','work_packages':['WP02','WP03','WP04','WP05','WP11','WP12','WP13','WP20','WP25'],'evidence_refs':['integration/artifacts/local/rayclmm-seed-three-service-smoke.json','integration/artifacts/local/rayclmm-seed-review.json','integration/artifacts/local/rayclmm-seed-final-source.json','integration/artifacts/local/rayclmm-seed-progress-consistency.json']},
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
   if relative=='integration/artifacts/local/rayclmm-seed-progress-consistency.json':continue
   if not(root/relative).is_file():issues.append(name+': missing evidence '+relative)
 return issues
