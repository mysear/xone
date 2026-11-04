#!/usr/bin/env python3
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[3]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import json,sys
root=Path(__file__).resolve().parents[3]
ids=sys.argv[1:];matrix=load_progress(root);rows=[w for w in matrix['work_packages'] if w['id'] in ids]
if len(rows)!=len(ids):raise SystemExit('Unknown work package')
print(json.dumps({'accepted':all(r['full_acceptance_passed'] for r in rows),'work_packages':[{'id':r['id'],'status':r['status'],'remaining':r['remaining']} for r in rows]},ensure_ascii=False,indent=2))
raise SystemExit(0 if all(r['full_acceptance_passed'] for r in rows) else 2)
