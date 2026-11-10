"""Original runtime getters require authenticated storage; no public write or zero-fill."""
from pathlib import Path
import subprocess
import unittest
TOOLS=Path(__file__).resolve().parents[1]
TOOL=TOOLS/'r01_public_catalog_evm_readonly.cjs'

class ProofRuntimeTests(unittest.TestCase):
    def node(self,body):
        self.assertTrue(TOOL.exists(),'proof-backed original runtime read-only source is missing')
        script="const m=require(process.argv[1]),assert=require('node:assert/strict');(async()=>{"+body+"})().catch(e=>{console.error(e.stack);process.exit(1)});"
        p=subprocess.run(['node','-e',script,str(TOOL)],capture_output=True,text=True,timeout=40)
        self.assertEqual(p.returncode,0,p.stderr)
        return p.stdout

    def test_saved_original_account_and_storage_are_authenticated(self):
        self.node("""
        const bank=await m.loadBank('integration/test/source/deployment-catalog/evm-continuation-v2/base-01');
        const a=bank.evidence.deployments.find(d=>d.address==='0xd0b53d9277642d899df5c87a3966a349a798f224');
        const slot=a.proof.storageProof[0];
        const result=await bank.storage(a.address,slot.key);
        assert.equal(BigInt(result.value),BigInt(slot.value));assert.equal(result.root,a.proof.storageHash);
        assert(result.authority.includes('MPT'));assert(result.proof_nodes.length>0);
        """)

    def test_public_fork_write_and_unknown_account_are_rejected(self):
        self.node("""
        const bank=await m.loadBank('integration/test/source/deployment-catalog/evm-continuation-v2/base-01');
        await assert.rejects(bank.request({method:'eth_sendRawTransaction',params:['0x12']}),/READ_ONLY/);
        await assert.rejects(bank.request({method:'evm_setAccountStorageAt',params:[]}),/READ_ONLY/);
        assert.equal(await bank.request({method:'eth_chainId',params:[]}), '0x2105');
        """)

    def test_actual_clone_runtime_and_supplements_still_execute(self):
        import tempfile
        root=TOOLS.parents[2]
        with tempfile.TemporaryDirectory(prefix='r01-public-normal-v4-positive-',dir=root/'integration/artifacts/local') as tmp:
            self.node("const r=await m.probe('integration/test/source/deployment-catalog/evm-normal-v4/base-aerodrome-v2-02/input.json',"+repr(str(Path(tmp)/'getter.json'))+");assert.equal(r.passed,true);assert.equal(r.calls.length,6);")

    def test_declared_H_chain_runtime_and_proxy_cannot_be_relabelled(self):
        import json,copy,tempfile
        root=TOOLS.parents[2]
        good=json.loads((root/'integration/test/source/deployment-catalog/evm-normal-v4/base-aerodrome-v2-02/input.json').read_text())
        cases=[('hash','READONLY_ROW_HASH'),('chain','READONLY_ROW_CHAIN'),('pool','READONLY_ROW_RUNTIME'),('factory','READONLY_ROW_FACTORY_RUNTIME'),('proxy','READONLY_PROXY_TARGET')]
        with tempfile.TemporaryDirectory(prefix='r01-public-normal-v4-negative-',dir=root/'integration/artifacts/local') as tmp:
            for field,error in cases:
                bad=copy.deepcopy(good)
                if field=='hash':bad['row']['anchor']['hash']='0x'+'11'*32
                elif field=='chain':bad['row']['chain_id']=137
                elif field=='pool':bad['row']['pool_runtime_sha256']='11'*32
                elif field=='factory':bad['row']['factory_runtime_sha256']='11'*32
                else:bad['row']['implementation_address']='0x'+'11'*20
                source=Path(tmp)/(field+'.json');source.write_text(json.dumps(bad))
                p=subprocess.run(['node',str(TOOL),str(source),str(Path(tmp)/(field+'-out.json'))],capture_output=True,text=True,timeout=30)
                self.assertNotEqual(p.returncode,0)
                self.assertIn(error,p.stderr)

    def test_proven_absent_account_rpc_sentinel_derives_zero_storage(self):
        self.node("""
        const bank=await m.loadBank('integration/test/source/deployment-catalog/evm-continuation-v2/polygon-01');
        const a=bank.evidence.deployments.find(d=>d.proof.storageHash==='0x'+'00'.repeat(32));assert(a,'real absent sentinel fixture');
        const result=await bank.storage(a.address,'0x123456');assert.equal(BigInt(result.value),0n);assert.equal(result.root,'0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421');
        """)

    def test_missing_actual_initialized_tick_is_never_zero_filled(self):
        self.node("""
        const bank=await m.loadBank('integration/test/source/deployment-catalog/evm-continuation-v2/base-01');
        await assert.rejects(bank.storage('0xd0b53d9277642d899df5c87a3966a349a798f224','0x2e4e897866183ce3c57913da1b7dae1a336dd9dca508e649658deb013114924e'),/READONLY_MPT_STORAGE_MISSING/);
        """)

    def test_clone_getter_storage_uses_pool_and_code_uses_implementation(self):
        import json
        root=TOOLS.parents[2]
        row=json.loads((root/'integration/test/source/deployment-catalog/evm-normal-v4/base-aerodrome-v2-01/input.json').read_text())['row']
        probe=json.loads((root/'integration/test/source/deployment-catalog/evm-normal-v4/base-aerodrome-v2-01/getter-probe-02.json').read_text())
        metadata=next(c for c in probe['calls'] if c['data']=='0x392f37e9')
        self.assertTrue(metadata['sloads'])
        self.assertTrue(all(s['address']==row['pool'] and s['code_address']==row['implementation_address'] for s in metadata['sloads']))
        self.assertIn(row['implementation_address'],metadata['code_addresses'])

    def test_proof_pool_does_not_turn_missing_path_into_zero(self):
        self.node("""
        const {Trie}=require('./integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie');
        const {ethers}=require('./integration/.runtime/stable-reference/node_modules/ethers');
        const tr=new Trie();const a='0x'+'11'.repeat(32),b='0x'+'22'.repeat(32);
        await tr.put(Buffer.from(a.slice(2),'hex'),Buffer.alloc(40,1));await tr.put(Buffer.from(b.slice(2),'hex'),Buffer.alloc(40,2));
        const proof=(await tr.createProof(Buffer.from(a.slice(2),'hex'))).map(x=>'0x'+Buffer.from(x).toString('hex'));
        const root='0x'+Buffer.from(tr.root()).toString('hex');
        assert.throws(()=>m.resolvePath(root,b,proof),/MISSING_NODE/);
        """)

if __name__=='__main__':unittest.main()
