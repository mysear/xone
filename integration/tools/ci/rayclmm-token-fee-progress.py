#!/usr/bin/env python3
"""Check original26 clauses and exact token-fee task/function/evidence correspondence."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import json,re,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
def load(p):return json.loads(p.read_text())
d=load_progress(ROOT,'rayclmm-token-fee');c=load_coverage(ROOT,'rayclmm-token-fee');s=load(OUT/'acceptance-summary.json');design=(ROOT/'XONE_SYSTEM_DESIGN.md').read_text();b=d['current_batch'];checks={}
checks['original_26_requirements_preserved']=len(d['work_packages'])==26 and all(w['design_requirements_verbatim']in design for w in d['work_packages'])
checks['no_full_work_package_falsely_accepted']=not any(w['full_acceptance_passed']for w in d['work_packages'])
checks['all_paths_exist']=all((ROOT/p).exists()for w in d['work_packages']for p in w['paths'])and all((ROOT/p).exists()for t in d['requirement_traceability']for p in t['paths'])
checks['production_and_execution_closed']=d['production_ready']is False and c['production_ready']is False and s['production_ready']is False and any(e['protocol_id']=='raydium-clmm-token-fee'and e['parser']=='PARTIAL_LOCAL'and e['math']=='PARTIAL_LOCAL'and e['execution']=='NOT_IMPLEMENTED'for e in c['entries'])
checks['new_scope_has_fresh_final_evidence']=b['status']=='LOCAL_SCOPE_VERIFIED'and c['current_batch_status']=='LOCAL_SCOPE_VERIFIED'and s['rayclmm_token_fee_quote_scope_passed']and load(OUT/'rayclmm-token-fee-final-source.json')['passed']
checks['source_hashes_match']=b['source_tree_sha256']==c['source_tree_sha256']==load(OUT/'verification.json')['source_tree_sha256']==load(OUT/'rayclmm-token-fee-final-source.json')['source_tree_sha256']
checks['protocol_ids_unique']=len(c['entries'])==len(set(e['protocol_id']for e in c['entries']))
checks['advanced_profiles_still_unimplemented']=all(e['math']=='NOT_IMPLEMENTED'for e in c['entries']if e['protocol_id']in ['raydium-clmm','orca-whirlpools','meteora-dlmm','pump','pancake-infinity'])
checks['scope_counts_match']=c['verified_math_cases']==968 and c['verified_shared_sequence_count']==24 and c['verified_shared_sequence_steps']==192 and c['verified_process_checks']==21
wp={w['id']:w['status']for w in d['work_packages']};checks['human_machine_table_status_matches']=all(line.split('|')[4]==wp[line.split('|')[1].split(' ')[0]]for line in(ROOT/'DEVELOPMENT_PROGRESS.md').read_text().splitlines()if line.startswith('|WP')and line.split('|')[1].split(' ')[0]in wp)
texts='\n'.join(p.read_text()for folder in ['chain-parser-go/internal','chain-parser-rust/crates','dex-router/crates']for p in(ROOT/folder).rglob('*')if p.suffix in ['.go','.rs']and'target'not in p.parts)
rows=[t for t in d['requirement_traceability']if'rayclmm-token-fee'in t['design_clause']]
checks['six_function_rows_and_tests_match']=len(rows)==6 and all(set(t['work_packages'])<=set(wp)and all(re.search(r'\b(?:func|fn)\s+'+re.escape(n)+r'\s*\(',texts)for n in t['tests'])for t in rows)
tasks=b['plan_task_progress'];plan=read_plan(ROOT,'plan-2026-10-28-raydium-clmm-token-fee')
checks['five_plan_tasks_match']=len(tasks)==5 and {t['task_id']for t in tasks}=={f'Task{i}'for i in range(1,6)}and all(re.search(r'^\|'+re.escape(t['task_id'])+r'\|'+re.escape(t['title'])+r'\|',plan,re.M) and t['status']=='LOCAL_SCOPE_VERIFIED'and set(t['work_packages'])<=set(wp)for t in tasks)
refs=b['evidence']+[p for t in tasks for p in t['evidence_refs']]
checks['all_current_evidence_exists']=all((ROOT/p).exists()for p in refs if not p.endswith('rayclmm-token-fee-progress-consistency.json'))
checks['batch_history_matches']=d['batch_history'][-1]==b
assert all(checks.values()),checks
report={'passed':True,'checks':checks,'work_packages':26,'full_work_packages_accepted':0,'protocol_entries':len(c['entries']),'new_traceability_entries':6,'plan_tasks':5,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)};(OUT/'rayclmm-token-fee-progress-consistency.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
