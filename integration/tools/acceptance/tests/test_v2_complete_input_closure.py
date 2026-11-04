import hashlib,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from weighted_evidence import ROOT,snapshot_inputs
class NativeClosure(unittest.TestCase):
 def test_real_collector_contains_all_original_scripts_packages_traces_and_service_copies(self):
  v=json.loads((ROOT/'integration/test/contracts/v2-complete.json').read_text());actual=snapshot_inputs();rows=list(v['reference']['scripts'])+list(v['reference']['loaded_dependencies'])
  rows.append({'path':v['reference']['node']['executable'],'sha256':v['reference']['node']['sha256']})
  for package in v['source_pins']['packages']:
   rows.extend({'path':'integration/tools/reference/evm-execution/'+r['path'],'sha256':r['sha256']} for r in package['files'])
  traces={}
  for transaction in v['reference']['transactions']:
   trace=transaction['trace'];path='integration/tools/reference/v2-complete/traces/'+trace['file'];traces[path]=trace['gzip_sha256']
  self.assertEqual(len(traces),79);rows.extend({'path':path,'sha256':digest} for path,digest in traces.items())
  for service in ['integration','chain-parser-go','dex-router']:
   path=service+'/test/contracts/v2-complete.json';rows.append({'path':path,'sha256':hashlib.sha256((ROOT/path).read_bytes()).hexdigest()})
  for row in rows:self.assertEqual(actual.get(row['path']),row['sha256'],row['path'])
  for output in ['verified.json','file-manifest.json']:self.assertNotIn('integration/tools/reference/v2-complete/'+output,actual)
 def test_independent_wire_copies_and_cpp_protoc_binary_are_inputs(self):
  actual=snapshot_inputs();wire=json.loads((ROOT/'integration/test/contracts/v2-complete-wire.json').read_text());tool=wire['protoc']
  self.assertEqual(actual.get(tool['executable']),tool['sha256'])
  for service in ['chain-parser-go','chain-parser-rust','dex-router']:
   path=service+'/test/contracts/v2-complete-wire.json';self.assertEqual(actual.get(path),hashlib.sha256((ROOT/path).read_bytes()).hexdigest(),path)
if __name__=='__main__':unittest.main()
