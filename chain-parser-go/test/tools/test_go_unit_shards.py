"""A growing EVM corpus stays complete when split into bounded processes."""
import importlib.util
from pathlib import Path
import subprocess
import unittest
spec=importlib.util.spec_from_file_location('go_unit_shards',Path(__file__).with_name('go_unit_shards.py'));m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class InventoryTests(unittest.TestCase):
 def test_all_tests_examples_and_fuzz_seeds_have_exactly_one_shard(self):
  names=['TestB','TestA','FuzzDecode','Example','ExampleAmount_width']
  parsed=m.parse_inventory('\n'.join(names)+'\nok xone/internal/chain/evm 0.001s\n')
  shards=m.make_shards(parsed,2)
  self.assertEqual([n for shard in shards for n in shard],sorted(names))
  self.assertEqual(len(set(n for shard in shards for n in shard)),len(names))
  self.assertEqual(shards,[['Example','ExampleAmount_width'],['FuzzDecode','TestA'],['TestB']])
  self.assertEqual(m.run_pattern(shards[0]),'^(Example|ExampleAmount_width)$')
 def test_missing_duplicate_and_invalid_budgets_fail_closed(self):
  for value in ['', 'ok only/package 0.001s\n', 'TestA\nTestA\n']:
   with self.assertRaises(ValueError):m.parse_inventory(value)
  for limit in [0,-1,True]:
   with self.assertRaises(ValueError):m.make_shards(['TestA'],limit)
 def test_failed_inventory_or_shard_stops_without_running_remaining_cases(self):
  calls=[]
  def run(command,**kw):
   calls.append(command)
   if '-list' in command:return subprocess.CompletedProcess(command,0,'TestA\nTestB\nTestC\nok pkg 0.001s\n','')
   return subprocess.CompletedProcess(command,7,'','')
  code=m.run_evm_shards(run,2)
  self.assertEqual(code,7);self.assertEqual(len(calls),2)
  self.assertIn('-race',calls[0]);self.assertIn('-race',calls[1]);self.assertNotIn('-timeout',calls[1]);self.assertEqual(calls[1][-1],'^(TestA|TestB)$')
  calls.clear()
  def failed(command,**kw):calls.append(command);return subprocess.CompletedProcess(command,9,'','')
  self.assertEqual(m.run_evm_shards(failed,2),9);self.assertEqual(len(calls),1)

class AllPackagesTests(unittest.TestCase):
 def test_package_discovery_is_checked_and_no_other_package_is_dropped(self):
  calls=[]
  def run(command,**kw):
   calls.append(command)
   if command==['go','list','-race','./...']:return subprocess.CompletedProcess(command,0,'xone/chain-parser-go\nxone/chain-parser-go/internal/chain/evm\nxone/chain-parser-go/internal/domain\n','')
   if '-list' in command:return subprocess.CompletedProcess(command,0,'TestA\nTestB\nok pkg 0.001s\n','')
   return subprocess.CompletedProcess(command,0,'','')
  self.assertEqual(m.run_all(run,1),0)
  self.assertEqual(calls[1],['go','test','-race','xone/chain-parser-go','xone/chain-parser-go/internal/domain'])
  self.assertEqual(sum(c[0:3]==['go','test','-race'] and '-run' in c for c in calls),2)
  def failed(command,**kw):return subprocess.CompletedProcess(command,8,'','')
  self.assertEqual(m.run_all(failed,1),8)
  def missing(command,**kw):return subprocess.CompletedProcess(command,0,'xone/chain-parser-go/internal/domain\n','')
  with self.assertRaises(ValueError):m.run_all(missing,1)

class CommandTests(unittest.TestCase):
 def test_unknown_arguments_fail_before_any_go_command(self):
  calls=[]
  def run(command,**kw):calls.append(command);raise AssertionError('unexpected execution')
  with self.assertRaises(SystemExit) as error:m.main(['--unknown'],run)
  self.assertEqual(error.exception.code,2);self.assertEqual(calls,[])

class UnicodeAndRaceInventoryTests(unittest.TestCase):
 def test_unicode_go_test_examples_and_fuzz_names_are_never_silently_dropped(self):
  names=['TestASCII','Test中文','Fuzz中文','Example金额']
  self.assertEqual(m.parse_inventory('\n'.join(names)+'\nBenchmarkNotRun\nok pkg 0.001s\n'),sorted(names))
  for bad in ['Test中文-bad','Fuzz中文 broken','Test²']:
   with self.assertRaises(ValueError):m.parse_inventory('TestASCII\n'+bad+'\n')
 def test_package_discovery_uses_same_race_conditions_and_keeps_race_only_package(self):
  calls=[]
  def run(command,**kw):
   calls.append(command)
   if command==['go','list','-race','./...']:return subprocess.CompletedProcess(command,0,'xone/chain-parser-go/internal/chain/evm\nxone/chain-parser-go/internal/raceonly\n','')
   if '-list' in command:return subprocess.CompletedProcess(command,0,'Test中文\nok pkg 0.001s\n','')
   return subprocess.CompletedProcess(command,0,'','')
  self.assertEqual(m.run_all(run,1),0)
  self.assertEqual(calls[0],['go','list','-race','./...'])
  self.assertIn(['go','test','-race','xone/chain-parser-go/internal/raceonly'],calls)
