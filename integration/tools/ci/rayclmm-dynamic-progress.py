#!/usr/bin/env python3
"""Check original26 clauses and exact dynamic-fee task/function/evidence correspondence."""
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import json,re,time
from rayclmm_dynamic_traceability import validate as validate_traceability
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
def load(p):return json.loads(p.read_text())
d=load_progress(ROOT,'rayclmm-dynamic');c=load_coverage(ROOT,'rayclmm-dynamic');s=load(OUT/'acceptance-summary.json');design=(ROOT/'XONE_SYSTEM_DESIGN.md').read_text();b=d['current_batch'];checks={}
checks['original_26_requirements_preserved']=len(d['work_packages'])==26 and all(w['design_requirements_verbatim']in design for w in d['work_packages'])
checks['no_full_work_package_falsely_accepted']=not any(w['full_acceptance_passed']for w in d['work_packages'])
checks['all_paths_exist']=all((ROOT/p).exists()for w in d['work_packages']for p in w['paths'])and all((ROOT/p).exists()for t in d['requirement_traceability']for p in t['paths'])
checks['production_and_execution_closed']=d['production_ready']is False and c['production_ready']is False and s['production_ready']is False and any(e['protocol_id']=='raydium-clmm-dynamic'and e['parser']=='PARTIAL_LOCAL'and e['math']=='PARTIAL_LOCAL'and e['execution']=='NOT_IMPLEMENTED'for e in c['entries'])
checks['new_scope_has_fresh_final_evidence']=b['status']=='LOCAL_SCOPE_VERIFIED'and c['current_batch_status']=='LOCAL_SCOPE_VERIFIED'and s['rayclmm_dynamic_quote_scope_passed']and load(OUT/'rayclmm-dynamic-final-source.json')['passed']
checks['source_hashes_match']=b['source_tree_sha256']==c['source_tree_sha256']==load(OUT/'verification.json')['source_tree_sha256']==load(OUT/'rayclmm-dynamic-final-source.json')['source_tree_sha256']
checks['protocol_ids_unique']=len(c['entries'])==len(set(e['protocol_id']for e in c['entries']))
checks['advanced_profiles_still_unimplemented']=all(e['math']=='NOT_IMPLEMENTED'for e in c['entries']if e['protocol_id']in ['raydium-clmm','orca-whirlpools','meteora-dlmm','pump','pancake-infinity'])
checks['scope_counts_match']=c['verified_math_cases']==load(OUT/'rayclmm-dynamic-native-source.json')['counts']['cases'] and c['verified_shared_sequence_count']==load(OUT/'rayclmm-dynamic-native-source.json')['counts']['sequences'] and c['verified_shared_sequence_steps']==load(OUT/'rayclmm-dynamic-native-source.json')['counts']['sequence_steps'] and c['verified_process_checks']==41
wp={w['id']:w['status']for w in d['work_packages']};checks['human_machine_table_status_matches']=all(line.split('|')[4]==wp[line.split('|')[1].split(' ')[0]]for line in(ROOT/'DEVELOPMENT_PROGRESS.md').read_text().splitlines()if line.startswith('|WP')and line.split('|')[1].split(' ')[0]in wp)
work={w['id']:w for w in d['work_packages']}
def human_scope_matches(line):
 parts=re.split(r'(?<!\\)\|',line);identity=parts[1].split(' ')[0]
 if identity not in work:return True
 encode=lambda text:text.replace('|','\\|').replace('\n','<br>')
 return len(parts)>=8 and parts[5]==encode(work[identity]['implemented'])and parts[6]==encode(work[identity]['remaining'])
checks['human_machine_scope_descriptions_match']=all(human_scope_matches(line)for line in(ROOT/'DEVELOPMENT_PROGRESS.md').read_text().splitlines()if line.startswith('|WP'))

traceability_issues=validate_traceability(d,ROOT)
checks['exact_six_function_and_five_task_bindings']=not traceability_issues
if traceability_issues:print(json.dumps({'traceability_errors':traceability_issues},ensure_ascii=False))
tasks=b['plan_task_progress']
refs=b['evidence']+[p for t in tasks for p in t['evidence_refs']]
checks['all_current_evidence_exists']=all((ROOT/p).exists()for p in refs if not p.endswith('rayclmm-dynamic-progress-consistency.json'))
checks['batch_history_matches']=d['batch_history'][-1]==b
assert all(checks.values()),checks
report={'passed':True,'checks':checks,'work_packages':26,'full_work_packages_accepted':0,'protocol_entries':len(c['entries']),'new_traceability_entries':6,'plan_tasks':5,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)};(OUT/'rayclmm-dynamic-progress-consistency.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
