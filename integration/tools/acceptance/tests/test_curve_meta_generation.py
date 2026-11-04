"""Generator startup/compiler failures invalidate only isolated temporary evidence."""
from pathlib import Path
import json,os,shutil,subprocess,tempfile,unittest
ROOT=Path(__file__).resolve().parents[4]
class MetaGenerationFailures(unittest.TestCase):
 def failure(self,stage):
  with tempfile.TemporaryDirectory(prefix='meta-source-failure-')as temp:
   root=Path(temp);script=root/'integration/tools/ci/curve-meta-golden.cjs';script.parent.mkdir(parents=True);shutil.copyfile(ROOT/'integration/tools/ci/curve-meta-golden.cjs',script)
   ref=root/'integration/tools/reference/curve-meta';ref.mkdir(parents=True);receipt=ref/'generation.json';receipt.write_text('{"generation_complete":true,"stale":true}')
   source=root/'integration/tools/reference/curve-models/old/MetaUSD.vy';source.parent.mkdir(parents=True);shutil.copyfile(ROOT/'integration/tools/reference/curve-models/old/MetaUSD.vy',source)
   shutil.copyfile(ROOT/'integration/tools/reference/curve-meta/source-pin.json',ref/'source-pin.json');shutil.copyfile(ROOT/'integration/tools/reference/curve-meta/Fixtures.sol',ref/'Fixtures.sol')
   env={**os.environ,'CURVE_META_NODE_MODULES':str(ROOT/'integration/.runtime/stable-reference/node_modules'),'CURVE_META_VYPER':str(ROOT/'integration/.runtime/curve-vyper/bin/vyper')}
   if stage=='missing_dependency':env['CURVE_META_NODE_MODULES']=temp
   elif stage=='malformed_pin':(ref/'source-pin.json').write_text('{bad')
   elif stage=='source_hash':source.write_text(source.read_text()+'\n')
   elif stage=='compiler':env['CURVE_META_VYPER']='/usr/bin/false'
   else:raise ValueError(stage)
   result=subprocess.run(['node',str(script)],env=env,capture_output=True,text=True,timeout=45)
   self.assertNotEqual(result.returncode,0);value=json.loads(receipt.read_text());self.assertIs(value['generation_complete'],False);self.assertNotIn('stale',value)
   self.assertFalse((root/'integration/test/contracts/curve-meta-direct.json').exists())
 def test_missing_dependency(self):self.failure('missing_dependency')
 def test_malformed_pin(self):self.failure('malformed_pin')
 def test_source_hash(self):self.failure('source_hash')
 def test_compiler(self):self.failure('compiler')
if __name__=='__main__':unittest.main()
