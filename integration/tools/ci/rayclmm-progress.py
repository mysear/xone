#!/usr/bin/env python3
"""Verify current CLMM progress against original clauses, tests and fresh evidence."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import json,re,time,hashlib
R=Path(__file__).resolve().parents[3];O=R/'integration/artifacts/local';load=lambda p:json.loads(p.read_text());d=load_progress(R,'rayclmm');c=load_coverage(R,'rayclmm');summary=load(O/'acceptance-summary.json');source=(R/'XONE_SYSTEM_DESIGN.md').read_text();checks={}
checks['original_26_requirements_preserved']=len(d['work_packages'])==26 and all(w['design_requirements_verbatim']in source for w in d['work_packages'])
checks['all_work_package_paths_exist']=all((R/p).exists()for w in d['work_packages']for p in w['paths'])
checks['all_requirement_paths_exist']=all((R/p).exists()for t in d['requirement_traceability']for p in t['paths'])
checks['work_package_exits_not_falsely_accepted']=not any(w['full_acceptance_passed']for w in d['work_packages'])
checks['production_gate_closed']=d['production_ready']is False and summary['production_ready']is False and c['production_ready']is False
checks['protocol_entries_unique']=len(c['entries'])==len(set(e['protocol_id']for e in c['entries']))
checks['other_solana_and_advanced_remain_explicit']=all(e['math']=='NOT_IMPLEMENTED'for e in c['entries']if e['protocol_id']in['raydium-cpmm','orca-whirlpools','raydium-clmm','meteora-dlmm','pump','pancake-infinity'])
checks['rayclmm_static_partial_and_execution_not_claimed']=any(e['protocol_id']=='raydium-clmm-static'and e['parser']=='PARTIAL_LOCAL'and e['math']=='PARTIAL_LOCAL'and e['execution']=='NOT_IMPLEMENTED'for e in c['entries'])
wp={w['id']:w['status']for w in d['work_packages']};checks['human_machine_table_status_matches']=all(l.split('|')[4]==wp[l.split('|')[1].split(' ')[0]]for l in(R/'DEVELOPMENT_PROGRESS.md').read_text().splitlines()if l.startswith('|WP')and l.split('|')[1].split(' ')[0]in wp)
checks['new_scope_verified_with_fresh_evidence']=d['current_batch']['status']=='LOCAL_SCOPE_VERIFIED'and c['current_batch_status']=='LOCAL_SCOPE_VERIFIED'and summary['rayclmm_quote_scope_passed']and load(O/'rayclmm-final-source.json')['passed']
texts='\n'.join(p.read_text()for folder in['chain-parser-go/internal','chain-parser-rust/crates','dex-router/crates']for p in(R/folder).rglob('*')if p.suffix in['.go','.rs']and'target'not in p.parts)
checks['all_new_mapped_test_names_exist']=all(re.search(r'\b(?:func|fn)\s+'+re.escape(n)+r'\s*\(',texts)for t in d['requirement_traceability']if'rayclmm'in t['design_clause']for n in t['tests'])
tasks=d['current_batch']['plan_task_progress'];plan=read_plan(R,'plan-2026-10-27-raydium-clmm-static')
checks['five_plan_tasks_match_written_implementation_plan']=len(tasks)==5 and {t['task_id']for t in tasks}=={f'Task{i}'for i in range(1,6)} and all(re.search(r'^\|'+re.escape(t['task_id'])+r'\|'+re.escape(t['title'])+r'\|',plan,re.M) and t['status']=='LOCAL_SCOPE_VERIFIED'and set(t['work_packages'])<=set(wp)for t in tasks)
checks['all_plan_task_evidence_exists']=all((R/p).exists()for t in tasks for p in t['evidence_refs']if not p.endswith('rayclmm-progress-consistency.json'))
checks['six_functional_rows_map_original_work_packages']=sum(t['design_clause']in {'11.3.rayclmm-bitmap-observation-state','7.3.rayclmm-finalized-pinned-accounts','12.2.rayclmm-static-input-source-rounding','12.9.rayclmm-shared-pool-poststate','12.14.rayclmm-native-original-reference','12.11.rayclmm-execution-boundary'}for t in d['requirement_traceability'])==6 and all(set(t['work_packages'])<=set(wp)for t in d['requirement_traceability'])
checks['all_current_evidence_exists']=all((R/p).exists()for p in d['current_batch']['evidence']if not p.endswith('rayclmm-progress-consistency.json'))
checks['source_hash_matches_progress_matrix_and_verification']=d['current_batch']['source_tree_sha256']==c['source_tree_sha256']==load(O/'verification.json')['source_tree_sha256']==load(O/'rayclmm-final-source.json')['source_tree_sha256']
checks['batch_history_contains_current_evidence']=d['batch_history'][-1]==d['current_batch']
checks['scope_counts_match']=c['verified_math_cases']==480 and c['verified_shared_sequence_count']==12 and c['verified_shared_sequence_steps']==96 and c['verified_process_checks']==19 and c['verified_tickmath_vectors']==2053
assert all(checks.values()),checks
out={'passed':True,'checks':checks,'work_packages':26,'full_work_packages_accepted':0,'protocol_entries':len(c['entries']),'new_traceability_entries':6,'plan_tasks':5,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)};(O/'rayclmm-progress-consistency.json').write_text(json.dumps(out,indent=2)+'\n');print(json.dumps(out,indent=2))
