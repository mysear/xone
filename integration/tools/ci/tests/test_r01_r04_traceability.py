from pathlib import Path
import copy,sys,unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools'));sys.path.insert(0,str(ROOT/'integration/tools/ci'))
from progress_document import load_progress
import r01_r04_traceability as check
class R01R04TraceabilityTests(unittest.TestCase):
 def test_current_plan_maps_all_requested_work_without_claiming_completion(self):
  result=check.validate(check.select_scope(load_progress(ROOT)));self.assertEqual(len(result['tasks']),11);self.assertEqual(len(result['design_bindings']),11);self.assertFalse(result['all_requested_features_complete'])
 def test_later_batch_does_not_reassign_historical_audit(self):
  document=load_progress(ROOT)
  scoped=check.select_scope(document)
  self.assertEqual(scoped['current_batch']['plan_anchor'],'plan-2026-11-06-r01-r04')
  self.assertEqual(len(scoped['current_batch']['plan_task_progress']),11)
  bad=copy.deepcopy(document)
  bad['batch_history']=[b for b in bad['batch_history']if b.get('plan_anchor')!='plan-2026-11-06-r01-r04']
  with self.assertRaises(ValueError):check.select_scope(bad)
  duplicate=copy.deepcopy(document);duplicate['batch_history'].append(copy.deepcopy(scoped['current_batch']))
  with self.assertRaises(ValueError):check.select_scope(duplicate)
 def test_deleting_or_duplicate_task_or_design_fails(self):
  doc=check.select_scope(load_progress(ROOT))
  for field in ['tasks','design']:
   for duplicate in [False,True]:
    bad=copy.deepcopy(doc);rows=bad['current_batch']['plan_task_progress']if field=='tasks'else bad['requirement_traceability'];index=next(i for i,r in enumerate(rows)if r.get('design_clause')in check.DESIGNS)if field=='design'else -1
    if duplicate:rows.append(copy.deepcopy(rows[index]))
    else:rows.pop(index)
    with self.assertRaises(ValueError):check.validate(bad)
 def test_wrong_work_package_or_missing_source_test_binding_fails(self):
  doc=check.select_scope(load_progress(ROOT))
  for key in ['work_packages','paths','tests']:
   bad=copy.deepcopy(doc);row=next(r for r in bad['requirement_traceability']if r['design_clause']in check.DESIGNS);row[key]=['WP01']if key=='work_packages'else []
   with self.assertRaises(ValueError):check.validate(bad)
 def test_unknown_related_design_is_not_silently_ignored(self):
  doc=check.select_scope(load_progress(ROOT));extra=copy.deepcopy(next(r for r in doc['requirement_traceability']if r['design_clause']in check.DESIGNS));extra['design_clause']='7.1.r01-unplanned';doc['requirement_traceability'].append(extra)
  with self.assertRaises(ValueError):check.validate(doc)

class R01R04GapAuditTests(unittest.TestCase):
 def test_gap_audit_covers_every_task_and_distinguishes_engineering_from_verification(self):
  text=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text()
  rows=check.validate_gap_audit(text)
  self.assertEqual({r['task_id']for r in rows},set(check.TASKS))
  self.assertEqual(len(rows),11)
  self.assertTrue(all(r['engineering_remaining']and r['environment_verification']for r in rows))
 def test_missing_duplicate_unknown_or_empty_gap_audit_rows_fail(self):
  text=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text()
  section=text.split('<a id="r01-r04-gap-audit-2026-11-07"></a>',1)[1]
  row=next(line for line in section.splitlines()if line.startswith('|R01.1|'))
  bads=[text.replace(row,'',1),text.replace(row,row+'\n'+row,1),text.replace(row,row.replace('|R01.1|','|R99.1|'),1),text.replace(row,'|R01.1|已有| |需核对|',1)]
  for bad in bads:
   with self.assertRaises(ValueError):check.validate_gap_audit(bad)

if __name__=='__main__':unittest.main()
