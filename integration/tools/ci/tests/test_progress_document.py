"""The progress ledger stays readable; tools parse its ordinary Markdown tables."""
from pathlib import Path
import sys
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'integration/tools'))
from progress_document import load_progress,load_coverage,read_plan

class ProgressDocument(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
 def write(self,text):(self.root/'DEVELOPMENT_PROGRESS.md').write_text(text)
 def minimal(self):
  return '''# 开发进度
<a id="work-packages"></a>
## 工作包
|工作包|功能|方案|状态|已完成范围|待完成范围|
|---|---|---|---|---|---|
|WP01|基础|§28.3|PARTIAL_LOCAL|已做\\|保留分隔符|待验收|
<a id="protocol-coverage"></a>
## 协议
|协议|适配版本|链族|设计条款|工作包|解析|数学|执行|范围与剩余功能|
|---|---|---|---|---|---|---|---|---|
|sample|sample/1|EVM|12.2|WP01|PARTIAL_LOCAL|NOT_IMPLEMENTED|NOT_IMPLEMENTED|待验证|
<a id="design-traceability"></a>
## 功能
|设计功能|工作包|状态|已完成|剩余|验证|
|---|---|---|---|---|---|
|12.2.sample|WP01|IN_PROGRESS|解析|数学|[报告](integration/artifacts/local/sample.json)|
<a id="batch-history"></a>
## 批次
|序号|批次|结果|验证|
|---|---|---|---|
|1|样例|LOCAL_SCOPE_VERIFIED|[报告](integration/artifacts/local/sample.json)|
'''
 def test_reads_ordinary_markdown_without_json(self):
  self.write(self.minimal());p=load_progress(self.root);c=load_coverage(self.root)
  self.assertEqual(p['work_packages'][0]['implemented'],'已做|保留分隔符');self.assertEqual(c['entries'][0]['protocol_id'],'sample')
  self.assertEqual(p['requirement_traceability'][0]['design_clause'],'12.2.sample')
 def test_missing_or_duplicate_section_is_rejected(self):
  for text in ['# no progress tables',self.minimal().replace('<a id="work-packages"></a>','')]:
   self.write(text)
   with self.assertRaises(ValueError):load_progress(self.root)
 def test_duplicate_work_package_and_invalid_status_are_rejected(self):
  row='|WP01|基础|§28.3|PARTIAL_LOCAL|已做|待验收|\n'
  for text in [self.minimal().replace('<a id="protocol-coverage">',row+'<a id="protocol-coverage">'),self.minimal().replace('PARTIAL_LOCAL','UNKNOWN_STATUS')]:
   self.write(text)
   with self.assertRaises(ValueError):load_progress(self.root)
 def test_completed_function_requires_a_real_validation_binding(self):
  self.write(self.minimal().replace('|12.2.sample|WP01|IN_PROGRESS|','|12.2.unknown|WP01|LOCAL_SCOPE_VERIFIED|'))
  with self.assertRaises(ValueError):load_progress(self.root)
 def test_readable_evidence_cannot_be_swapped_for_an_unrelated_report(self):
  text=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text().replace('(integration/artifacts/local/rayclmm-dynamic-native-source.json)','(integration/artifacts/local/unrelated.json)')
  self.write(text)
  with self.assertRaises(ValueError):load_progress(self.root)
 def test_plan_reader_returns_only_its_named_section(self):
  self.write('<a id="plan-one"></a>\n## 一个任务\n|Task1|预期|\n<a id="plan-two"></a>\n## 另一个任务\n|Task2|无关|')
  self.assertIn('预期',read_plan(self.root,'plan-one'));self.assertNotIn('无关',read_plan(self.root,'plan-one'))
  with self.assertRaises(ValueError):read_plan(self.root,'missing')
 def test_historical_scope_reads_its_own_task_table_and_evidence_counts(self):
  expected={'rayclmm':(480,2053),'rayclmm-token-fee':(968,0),'rayclmm-bitmap':(364,0),'rayclmm-seed':(464,0),'rayclmm-dynamic':(348,0)}
  for scope,(cases,ticks) in expected.items():
   document=load_progress(ROOT,scope);coverage=load_coverage(ROOT,scope);tasks=document['current_batch']['plan_task_progress']
   self.assertEqual(len(tasks),5);self.assertEqual(coverage['verified_math_cases'],cases)
   self.assertEqual(coverage['verified_tickmath_vectors'],ticks)
   self.assertEqual(document['batch_history'][-1],document['current_batch'])
   self.assertIn(tasks[0]['title'],read_plan(ROOT,tasks[0]['plan_anchor']))
 def test_appending_unrelated_history_does_not_shift_historical_scopes(self):
  original=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text();first='<a id="plan-2026-10-27-raydium-clmm-static"></a>'
  next_id=len(load_progress(ROOT)['batch_history'])+1
  modified=original.replace(first,f'|{next_id}|EVM receipt integrity|LOCAL_SCOPE_VERIFIED|[报告](integration/artifacts/local/receipt-integrity-acceptance.json)|\n\n'+first)
  self.write(modified)
  for scope in ['rayclmm','rayclmm-token-fee','rayclmm-bitmap','rayclmm-seed','rayclmm-dynamic']:
   expected=load_progress(ROOT,scope)['current_batch']
   actual=load_progress(self.root,scope)['current_batch']
   self.assertEqual(actual['name'],expected['name'])
   self.assertEqual(actual['plan_task_progress'],expected['plan_task_progress'])
 def test_unrelated_new_batch_uses_its_own_verification_fingerprint(self):
  import json
  original=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text();first='<a id="plan-2026-10-27-raydium-clmm-static"></a>';next_id=len(load_progress(ROOT)['batch_history'])+1
  self.write(original.replace(first,f'|{next_id}|Receipt guard|LOCAL_SCOPE_VERIFIED|[报告](integration/artifacts/local/receipt-integrity-acceptance.json)|\n\n'+first))
  reports=self.root/'integration/artifacts/local';reports.mkdir(parents=True)
  (reports/'receipt-integrity-acceptance.json').write_text(json.dumps({'source_tree_sha256':'new-verification'}))
  (reports/'rayclmm-dynamic-final-source.json').write_text(json.dumps({'source_tree_sha256':'old-unrelated-verification'}))
  batch=load_progress(self.root)['current_batch'];self.assertEqual(batch['name'],'Receipt guard');self.assertEqual(batch['source_tree_sha256'],'new-verification')
  coverage=load_coverage(self.root);self.assertEqual(coverage['source_tree_sha256'],batch['source_tree_sha256']);self.assertEqual(coverage['verified_math_cases'],0)
 def test_main_document_is_only_readable_progress(self):
  text=(ROOT/'DEVELOPMENT_PROGRESS.md').read_text()
  for token in ['```json','xone-progress-data:','<details>','source_files_sha256','design_requirements_verbatim']:self.assertNotIn(token,text)
 def test_scope_and_design_mappings_remain_complete(self):
  p=load_progress(ROOT);c=load_coverage(ROOT)
  self.assertEqual(len(p['work_packages']),26);self.assertGreaterEqual(len(c['entries']),31);self.assertGreaterEqual(len(p['requirement_traceability']),76)
  self.assertFalse(any(w['full_acceptance_passed']for w in p['work_packages']))
  self.assertTrue(all(w['design_requirements_verbatim']in(ROOT/'XONE_SYSTEM_DESIGN.md').read_text()for w in p['work_packages']))
 def test_docs_has_one_technical_reference_and_no_fragmented_progress(self):
  files=sorted(str(path.relative_to(ROOT/'docs'))for path in (ROOT/'docs').rglob('*')if path.is_file()and path.suffix in ['.md','.json'])
  self.assertEqual(files,['TECHNICAL_REFERENCE.md'])
  technical=(ROOT/'docs/TECHNICAL_REFERENCE.md').read_text()
  for anchor in ['service-communication','raydium-clmm-dynamic-quotes','normalized-facts-and-analytics']:
   self.assertIn('<a id="'+anchor+'"></a>',technical)
  self.assertTrue((ROOT/'integration/tools/reference/protocol-sources/source-metadata.json').is_file())
 def test_remaining_priorities_cover_all_original_work_packages(self):
  import re
  text=read_plan(ROOT,'remaining-priorities')
  rows=[line for line in text.splitlines()if re.match(r'^\|R\d{2}\|',line)]
  self.assertEqual(len(rows),21)
  self.assertEqual({package for line in rows for package in re.findall(r'WP\d{2}',line)},{f'WP{index:02}'for index in range(1,27)})
  self.assertIn('WP18/WP22',text)
  self.assertIn('RunJournalJob',text)
 def test_technical_section_links_preserve_file_evidence_bindings(self):
  for scope in ['rayclmm-seed','rayclmm-dynamic']:
   tasks=load_progress(ROOT,scope)['current_batch']['plan_task_progress']
   self.assertIn('docs/TECHNICAL_REFERENCE.md',tasks[0]['evidence_refs'])
   self.assertTrue(all((ROOT/path).exists()for path in tasks[0]['evidence_refs']))
 def test_live_local_markdown_links_exist(self):
  import re
  for path in [ROOT/'README.md',ROOT/'DEVELOPMENT_PROGRESS.md',ROOT/'integration/README.md']+list((ROOT/'docs').rglob('*.md')):
   text=re.sub(r'```.*?```','',path.read_text(),flags=re.S)
   for match in re.finditer(r'\]\(([^\s)]+)\)',text):
    url=match[1]
    if re.match(r'^[a-zA-Z]+:',url)or url.startswith('#'):continue
    self.assertTrue((path.parent/url.split('#',1)[0]).exists(),str(path)+': '+url)
if __name__=='__main__':unittest.main()
