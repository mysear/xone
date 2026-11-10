from pathlib import Path
import subprocess,unittest,json,tempfile
ROOT=Path(__file__).resolve().parents[4]
class DeploymentRebuildTests(unittest.TestCase):
 def test_rebuilt_runtime_and_exact_source_inventory_must_both_match(self):
  script=r'''
const assert=require('node:assert/strict');const {verifyRebuild}=require('./integration/tools/acceptance/evm_deployment_rebuild.cjs');
const expected={compiler:{version:'0.5.16+commit.9c3226ce'},sources:{'A.sol':{keccak256:'0xabc'}},settings:{optimizer:{enabled:true,runs:999999},evmVersion:'istanbul'}};
const compiled={metadata:JSON.stringify(expected),evm:{deployedBytecode:{object:'6000'}}};
assert.equal(verifyRebuild(expected,compiled,'0x6000'),true);
for(const bad of [{...compiled,evm:{deployedBytecode:{object:'6001'}}},{...compiled,metadata:JSON.stringify({...expected,sources:{}})},{...compiled,metadata:JSON.stringify({...expected,compiler:{version:'other'}})},{...compiled,metadata:JSON.stringify({...expected,settings:{...expected.settings,optimizer:{enabled:true,runs:200}}})}])assert.throws(()=>verifyRebuild(expected,bad,'0x6000'));
'''
  p=subprocess.run(['node','-e',script],cwd=ROOT,capture_output=True,text=True);self.assertEqual(p.returncode,0,p.stderr)
 def test_entry_rejects_unproven_deployment_before_reporting_bound_address(self):
  original=ROOT/'integration/test/source/ethereum-mainnet/uniswap-v2-26355865-evidence.json'
  self.assertTrue(original.is_file(),'retained real deployment fixture missing')
  evidence=json.loads(original.read_text());evidence.pop('block',None);evidence.pop('raw_transactions',None);evidence['expected_block_hash']='0x'+'0'*64;evidence['deployments'][0].pop('proof',None);evidence['deployments'][0]['address']='0x'+'1'*40
  with tempfile.TemporaryDirectory()as folder:
   source=Path(folder)/'bad.json';output=Path(folder)/'report.json';source.write_text(json.dumps(evidence));artifact=ROOT/'integration/.runtime/v3-sdk-oracle/node_modules/@uniswap/v2-core/build/UniswapV2Pair.json'
   p=subprocess.run(['node',str(ROOT/'integration/tools/acceptance/evm_deployment_rebuild.cjs'),str(source),str(artifact),str(output)],cwd=ROOT,capture_output=True,text=True,timeout=60)
   self.assertNotEqual(p.returncode,0,'unproved address/header accepted as deployed-source rebuild');self.assertIn('PROVENANCE_HEADER',p.stderr);self.assertFalse(output.exists(),'failed evidence left a successful report')
if __name__=='__main__':unittest.main()
