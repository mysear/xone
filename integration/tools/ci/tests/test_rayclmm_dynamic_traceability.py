"""Mutation checks for exact design/function/task evidence bindings."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parents[4]/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[4]
SPEC=importlib.util.spec_from_file_location('dynamic_traceability',Path(__file__).resolve().parents[1]/'rayclmm_dynamic_traceability.py')
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)

class DynamicTraceability(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
  self.doc=load_progress(ROOT,'rayclmm-dynamic')
  clauses={k:v for k,v in MODULE.CLAUSES.items()}
  rows=[r for r in self.doc['requirement_traceability']if r['design_clause']in clauses]
  self.assertEqual(len(rows),6)
  for row in rows:
   row['status']='PARTIAL_LOCAL';row['work_packages']=clauses[row['design_clause']]['work_packages'];row['tests']=clauses[row['design_clause']]['tests'];row['paths']=clauses[row['design_clause']]['paths'];row['evidence_refs']=list(MODULE.CLAUSE_EVIDENCE)
  for task in self.doc['current_batch']['plan_task_progress']:
   expected=MODULE.TASKS[task['task_id']];task.update(title=expected['title'],status='LOCAL_SCOPE_VERIFIED',plan=MODULE.PLAN,plan_anchor=MODULE.PLAN_ANCHOR,work_packages=expected['work_packages'],evidence_refs=expected['evidence_refs'])
  paths={MODULE.PLAN}
  for row in rows:paths.update(row['paths']);paths.update(row['evidence_refs'])
  for t in self.doc['current_batch']['plan_task_progress']:paths.update(t['evidence_refs'])
  for name in paths:
   p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);original=ROOT/name
   p.write_bytes(original.read_bytes()if original.is_file()else b'local verification evidence\n')
 def issues(self):return MODULE.validate(self.doc,self.root)
 def row(self):return next(r for r in self.doc['requirement_traceability']if r['design_clause']=='11.3.rayclmm-dynamic-state')
 def test_complete_scope_is_accepted(self):self.assertEqual(self.issues(),[])
 def test_empty_function_tests_and_work_packages_are_rejected(self):
  self.row()['tests']=[];self.row()['work_packages']=[];self.assertTrue(self.issues())
 def test_empty_task_bindings_and_wrong_plan_are_rejected(self):
  for task in self.doc['current_batch']['plan_task_progress']:task.update(work_packages=[],evidence_refs=[],plan='docs/plans/previous-plan.md')
  self.assertTrue(self.issues())
 def test_symbol_elsewhere_does_not_satisfy_declared_source(self):
  self.row()['paths']=['docs/TECHNICAL_REFERENCE.md'];self.assertTrue(self.issues())
 def test_changed_clause_or_task_identity_is_rejected(self):
  self.row()['design_clause']+='-wrong';self.doc['current_batch']['plan_task_progress'][0]['task_id']='Task6';self.assertTrue(self.issues())
 def test_missing_or_unrelated_evidence_is_rejected(self):
  self.row()['evidence_refs']=[];self.doc['current_batch']['plan_task_progress'][-1]['evidence_refs']=['integration/artifacts/local/previous-batch.json'];self.assertTrue(self.issues())
 def test_missing_actual_source_test_is_rejected(self):
  expected=MODULE.CLAUSES['11.3.rayclmm-dynamic-state'];name=expected['tests'][0]
  for path in expected['paths']:
   p=self.root/path
   if p.is_file():p.write_text(p.read_text().replace(name,'renamed_outside_contract'))
  self.assertTrue(self.issues())
 def test_duplicate_or_extra_current_clause_is_rejected(self):
  self.doc['requirement_traceability'].append(copy.deepcopy(self.row()));self.assertTrue(self.issues())

if __name__=='__main__':unittest.main()
