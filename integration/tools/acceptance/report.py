#!/usr/bin/env python3
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import json,subprocess,time
root=Path(__file__).resolve().parents[3];out=root/'integration/artifacts/local';verification=json.loads((out/'verification.json').read_text());smoke=json.loads((out/'three-service-smoke.json').read_text());matrix=load_progress(root);schema=subprocess.run(['python3',str(root/'integration/tools/ci/log-schema.py')],capture_output=True,text=True)
report={'schema_version':1,'design_version':matrix['design_version'],'implemented_scope_passed':verification['passed'] and smoke['passed'] and schema.returncode==0,'production_ready':False,'full_work_packages_accepted':sum(w['full_acceptance_passed'] for w in matrix['work_packages']),'work_package_count':len(matrix['work_packages']),'verification':'verification.json','three_process_evidence':'three-service-smoke.json','log_schema':{'exit_code':schema.returncode,'output':schema.stdout+schema.stderr},'release_blockers':[{'id':w['id'],'remaining':w['remaining']} for w in matrix['work_packages'] if not w['full_acceptance_passed']],'container_validation':json.loads((out/'container-validation.json').read_text()) if (out/'container-validation.json').exists() else {'image_build_executed':False},'measured_at_unix_ms':int(time.time()*1000)}
for name,key in [('journal-integration.json','real_journal_integration'),('kafka-three-service-smoke.json','real_kafka_multi_replica'),('reorg-smoke.json','real_journal_reorg'),('reorg-http-smoke.json','http_reorg'),('analytics-journal-smoke.json','normalized_analytics'),('v3-differential.json','v3_differential'),('v3-three-service-smoke.json','v3_process'),('multidex-differential.json','multidex_differential'),('multidex-three-service-smoke.json','multidex_process'),('stable-differential.json','stable_differential'),('stable-three-service-smoke.json','stable_process'),('singleton-differential.json','singleton_differential'),('singleton-three-service-smoke.json','singleton_process'),('raydium-differential.json','raydium_differential'),('raydium-three-service-smoke.json','raydium_process'),('orca-differential.json','orca_differential'),('orca-three-service-smoke.json','orca_process'),('rayclmm-differential.json','rayclmm_differential'),('rayclmm-three-service-smoke.json','rayclmm_process'),('rayclmm-token-fee-differential.json','rayclmm_token_fee_differential'),('rayclmm-token-fee-three-service-smoke.json','rayclmm_token_fee_process'),('rayclmm-bitmap-differential.json','rayclmm_bitmap_differential'),('rayclmm-bitmap-three-service-smoke.json','rayclmm_bitmap_process'),('rayclmm-seed-differential.json','rayclmm_seed_differential'),('rayclmm-seed-three-service-smoke.json','rayclmm_seed_process'),('rayclmm-dynamic-differential.json','rayclmm_dynamic_differential'),('rayclmm-dynamic-three-service-smoke.json','rayclmm_dynamic_process'),('receipt-witness-three-service-smoke.json','receipt_witness_process'),('source-wal-three-service-smoke.json','source_wal_process'),('receipt-root-three-service-smoke.json','receipt_root_process')]:
 path=out/name
 report[key]=json.loads(path.read_text()) if path.exists() else {'passed':False,'status':'NOT_RUN'}
report['durable_journal_scope_passed']=all(report[k].get('passed',False) for k in ['real_journal_integration','real_kafka_multi_replica'])
report['reorg_scope_passed']=report['real_journal_reorg'].get('passed',False) and report['http_reorg'].get('passed',False) and verification['passed']
report['analytics_journal_scope_passed']=report['normalized_analytics'].get('passed',False) and report['durable_journal_scope_passed'] and verification['passed']
report['v3_quote_scope_passed']=report['v3_differential'].get('passed',False) and report['v3_process'].get('passed',False) and verification['passed']
report['multidex_quote_scope_passed']=report['multidex_differential'].get('passed',False) and report['multidex_process'].get('passed',False) and verification['passed']
report['stable_quote_scope_passed']=report['stable_differential'].get('passed',False) and report['stable_process'].get('passed',False) and verification['passed']
report['singleton_quote_scope_passed']=report['singleton_differential'].get('passed',False) and report['singleton_process'].get('passed',False) and verification['passed']
report['raydium_quote_scope_passed']=report['raydium_differential'].get('passed',False) and report['raydium_process'].get('passed',False) and verification['passed']
report['orca_quote_scope_passed']=report['orca_differential'].get('passed',False) and report['orca_process'].get('passed',False) and verification['passed']
report['rayclmm_quote_scope_passed']=report['rayclmm_differential'].get('passed',False) and report['rayclmm_process'].get('passed',False) and verification['passed']
report['rayclmm_token_fee_quote_scope_passed']=report['rayclmm_token_fee_differential'].get('passed',False) and report['rayclmm_token_fee_process'].get('passed',False) and verification['passed']
report['rayclmm_bitmap_quote_scope_passed']=report['rayclmm_bitmap_differential'].get('passed',False) and report['rayclmm_bitmap_process'].get('passed',False) and verification['passed']
report['rayclmm_seed_quote_scope_passed']=report['rayclmm_seed_differential'].get('passed',False) and report['rayclmm_seed_process'].get('passed',False) and verification['passed']
report['rayclmm_dynamic_quote_scope_passed']=report['rayclmm_dynamic_differential'].get('passed',False) and report['rayclmm_dynamic_process'].get('passed',False) and verification['passed']
report['receipt_witness_scope_passed']=report['receipt_witness_process'].get('passed',False) and verification['passed']
report['source_wal_scope_passed']=report['source_wal_process'].get('passed',False) and verification['passed']
report['receipt_root_scope_passed']=report['receipt_root_process'].get('passed',False) and verification['passed']
(out/'acceptance-summary.json').write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
text='# 生产准入状态\n\n**未准入。** 本轮可验证的是离线开发范围，不具备完整生产能力。\n\n'
for item in report['release_blockers']:text+=f"- {item['id']}: {item['remaining']}\n"
(out/'production-readiness.md').write_text(text);print(json.dumps({k:v for k,v in report.items() if k!='release_blockers'},ensure_ascii=False,indent=2));raise SystemExit(0 if report['implemented_scope_passed'] else 1)
