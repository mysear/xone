#!/usr/bin/env python3
"""Check the exact R01–R04 plan structure; this never certifies implementation."""
from pathlib import Path
import hashlib,json,re,sys,time
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools'))
from progress_document import load_progress
TASKS={
 'R01.1':('WP02','WP03','WP10','WP11','WP18','WP20'),
 'R01.2':('WP02','WP03','WP10','WP11','WP18','WP20'),
 'R02.1':('WP04','WP05','WP07'),'R02.2':('WP04','WP05','WP07'),'R02.3':('WP04','WP05','WP07'),
 'R03.1':('WP04','WP05','WP07','WP20','WP24'),'R03.2':('WP04','WP05','WP07','WP20','WP24'),
 'R04.1':('WP08','WP09','WP20','WP24'),'R04.2':('WP08','WP09','WP20','WP24'),'R04.3':('WP08','WP09','WP20','WP24'),
 'R01-R04.A':('WP02','WP03','WP04','WP05','WP07','WP08','WP09','WP10','WP11','WP18','WP20','WP24'),
}
DESIGNS={
 '7.1.r01-deployment-evidence':TASKS['R01.1'],'12.14.r01-deployed-source-rebuild':TASKS['R01.2'],
 '7.1.r02-provider-ws':TASKS['R02.1'],'7.2.r02-staging-commit':TASKS['R02.2'],'7.3.r02-raw-dlq-discovery':TASKS['R02.3'],
 '7.r03-yellowstone-wal':TASKS['R03.1'],'28.22.r03-stream-bank-barrier':TASKS['R03.2'],
 '28.26.r04-ensure-pool-scope':TASKS['R04.1'],'28.10.r04-current-baseline-recovery':TASKS['R04.2'],'28.26.r04-history-isolation':TASKS['R04.3'],
 '28.r01-r04-validation':TASKS['R01-R04.A'],
}
TASK_DESIGNS={
 'R01.1':'7.1.r01-deployment-evidence','R01.2':'12.14.r01-deployed-source-rebuild',
 'R02.1':'7.1.r02-provider-ws','R02.2':'7.2.r02-staging-commit','R02.3':'7.3.r02-raw-dlq-discovery',
 'R03.1':'7.r03-yellowstone-wal','R03.2':'28.22.r03-stream-bank-barrier',
 'R04.1':'28.26.r04-ensure-pool-scope','R04.2':'28.10.r04-current-baseline-recovery','R04.3':'28.26.r04-history-isolation',
 'R01-R04.A':'28.r01-r04-validation',
}
def require(value,message):
 if not value:raise ValueError(message)
def select_scope(document):
 # Historical task audits stay anchored when a later development batch starts.
 anchor='plan-2026-11-06-r01-r04'
 matches=[batch for batch in document['batch_history']if batch.get('plan_anchor')==anchor]
 require(len(matches)==1,'missing or duplicate R01–R04 historical task plan')
 return {**document,'current_batch':matches[0]}

def validate(document):
 batch=document['current_batch'];require(batch.get('plan_anchor')=='plan-2026-11-06-r01-r04','current task plan changed')
 tasks=batch['plan_task_progress'];designs=[r for r in document['requirement_traceability']if any('.'+family+'-'in r['design_clause']for family in ['r01','r02','r03','r04'])]
 for rows,key,required in [(tasks,'task_id',TASKS),(designs,'design_clause',DESIGNS)]:
  require(len(rows)==len(required)and {r[key]for r in rows}==set(required),'missing or duplicate R01–R04 task/design')
  for row in rows:
   require(sorted(row['work_packages'])==sorted(required[row[key]]),'wrong work package relationship: '+row[key])
   if key=='design_clause':require(bool(row['paths'])and bool(row['tests']),'missing source/test binding')
 require(all(TASKS[task]==DESIGNS[design]for task,design in TASK_DESIGNS.items()),'task/design work packages drifted')
 structure={'task_design_links':TASK_DESIGNS,'tasks':[{k:r[k]for k in ['task_id','work_packages']}for r in sorted(tasks,key=lambda row:row['task_id'])],
            'design_bindings':[{k:r[k]for k in ['design_clause','work_packages','paths','tests']}for r in sorted(designs,key=lambda row:row['design_clause'])]}
 return {**structure,'structure_sha256':hashlib.sha256(json.dumps(structure,sort_keys=True,separators=(',',':')).encode()).hexdigest(),
         'all_requested_features_complete':False,'completion_authority':'mapping checks never establish real-source or functional acceptance',
         'unclosed_tasks':[r['task_id']for r in tasks if r['status']not in ['LOCAL_SCOPE_VERIFIED','ACCEPTED']]}
def validate_gap_audit(text):
 marker='<a id="r01-r04-gap-audit-2026-11-07"></a>'
 require(text.count(marker)==1,'missing or duplicate R01–R04 gap audit')
 section=re.split(r'<a id="[a-z0-9-]+"></a>',text.split(marker,1)[1],maxsplit=1)[0]
 lines=[line for line in section.splitlines()if line.startswith('|')]
 expected=['任务','当前可验证边界','仍须自行补全的具体功能与配置','真正影响真实环境验收的核对点']
 split=lambda line:[part.strip()for part in re.split(r'(?<!\\)\|',line)[1:-1]]
 require(len(lines)>=3 and split(lines[0])==expected,'invalid gap audit columns')
 rows=[]
 for line in lines[2:]:
  columns=split(line)
  require(len(columns)==4 and all(columns),'empty gap audit engineering/verification boundary')
  rows.append(dict(zip(['task_id','current_boundary','engineering_remaining','environment_verification'],columns)))
 require(len(rows)==len(TASKS)and {r['task_id']for r in rows}==set(TASKS),'missing, duplicate or unknown gap audit task')
 return rows

def main():
 result=validate(select_scope(load_progress(ROOT)));result['gap_audit']=validate_gap_audit((ROOT/'DEVELOPMENT_PROGRESS.md').read_text());report=dict(result,passed=True,scope='exact plan/source/test mapping only',production_ready=False,measured_at_unix_ms=int(time.time()*1000))
 output=ROOT/'integration/artifacts/local/r01-r04-progress-consistency.json';output.write_text(json.dumps(report,ensure_ascii=False,indent=2)+'\n')
 print(json.dumps({'passed':True,'scope':report['scope'],'tasks':len(report['tasks']),'design_bindings':len(report['design_bindings']),'all_requested_features_complete':False,'unclosed_tasks':report['unclosed_tasks']},ensure_ascii=False))
if __name__=='__main__':main()
