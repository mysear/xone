"""Explicit native proof paths preserve commitments and genuine absence boundaries."""
import json
from pathlib import Path
import subprocess
import unittest

TOOLS = Path(__file__).resolve().parents[1]
ROOT = TOOLS.parents[2]
TOOL = TOOLS / 'r01_public_catalog_evm_trie.cjs'


class NativeTriePathTests(unittest.TestCase):
    def run_node(self, body):
        self.assertTrue(TOOL.is_file(), 'explicit authenticated trie path diagnostic is missing')
        script = "const t=require(process.argv[1]);const assert=require('node:assert/strict');(async()=>{" + body + "})().catch(e=>{console.error(e.stack);process.exit(1)});"
        result = subprocess.run(['node', '-e', script, str(TOOL)], capture_output=True, text=True, timeout=30)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_member_and_divergent_leaf_keep_distinct_results(self):
        self.run_node("""
        const {Trie}=require('./integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie');
        const tr=new Trie();await tr.put(Buffer.from('1234','hex'),Buffer.from('81aa','hex'));
        const proof=await tr.createProof(Buffer.from('1234','hex'));
        const root='0x'+Buffer.from(tr.root()).toString('hex'),nodes=proof.map(x=>'0x'+Buffer.from(x).toString('hex'));
        const a=t.traceProof(root,'0x1234',nodes),b=t.traceProof(root,'0x1235',nodes);
        assert.equal(a.status,'MEMBER');assert.equal(a.value,'0x81aa');
        assert.equal(b.status,'ABSENT_DIVERGENT_PATH');assert.equal(b.value,null);
        assert.equal(b.nodes[0].hash,root);assert.equal(b.nodes[0].matched_nibbles,3);
        """)

    def test_wrong_root_or_missing_node_never_becomes_absence(self):
        self.run_node("""
        const {Trie}=require('./integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie');
        const tr=new Trie();await tr.put(Buffer.from('1234','hex'),Buffer.alloc(40,1));
        const p=(await tr.createProof(Buffer.from('1234','hex'))).map(x=>'0x'+Buffer.from(x).toString('hex'));
        assert.throws(()=>t.traceProof('0x'+'11'.repeat(32),'0x1234',p),/MISSING_NODE/);
        assert.throws(()=>t.traceProof('0x'+Buffer.from(tr.root()).toString('hex'),'0x1234',[]),/MISSING_NODE/);
        """)

    def test_branch_missing_child_and_inline_nodes(self):
        self.run_node("""
        const {Trie}=require('./integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie');
        const tr=new Trie();for(const key of ['1234','1256'])await tr.put(Buffer.from(key,'hex'),Buffer.from('01','hex'));
        const root='0x'+Buffer.from(tr.root()).toString('hex');
        const p=key=>tr.createProof(Buffer.from(key,'hex')).then(v=>v.map(x=>'0x'+Buffer.from(x).toString('hex')));
        assert.equal(t.traceProof(root,'0x1234',await p('1234')).value,'0x01');
        assert.equal(t.traceProof(root,'0x1244',await p('1244')).status,'ABSENT_EMPTY_BRANCH');
        """)

    def test_real_coreth_account_absence_and_storage_contradictions(self):
        output=self.run_node("""
        const fs=require('node:fs');
        const e=JSON.parse(fs.readFileSync('integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-04/prestate-evidence.json'));
        const r=await t.auditEvidence(e);assert.equal(r.anchor.height,99656725);assert.equal(r.anchor.settling_height,99656733);
        assert.equal(r.accounts.filter(x=>x.account.status.startsWith('ABSENT_')).length,3);
        assert.equal(r.storage_contradictions.length,5);assert.equal(r.qualified,false);
        for(const c of r.storage_contradictions){assert.equal(c.raw_slot.length,66);assert.equal(c.secure_key.length,66);assert.equal(c.path.value,null);assert(c.path.status.startsWith('ABSENT_'));}
        console.log(JSON.stringify(r.accounts.filter(x=>x.account.status.startsWith('ABSENT_')).map(x=>x.address)));
        """)
        self.assertIn('0x000000000000000000000000000000000000cafe', json.loads(output))

    def test_coreth_partition_binds_logical_read_to_proven_physical_slot(self):
        self.run_node("""
        const {Trie}=require('./integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie');
        const {ethers}=require('./integration/.runtime/stable-reference/node_modules/ethers');
        const v=require('./integration/tools/acceptance/r01_public_catalog_proof.cjs');
        const logical='0xd7'+'11'.repeat(31),physical='0xd6'+'11'.repeat(31);
        assert.equal(v.storageTrieKey(logical,'coreth-partitioned-storage/v1'),physical);
        assert.equal(v.storageTrieKey(logical,'ethereum-storage/v1'),logical);
        const address='0x'+'12'.repeat(20),st=new Trie(),at=new Trie();
        await st.put(Buffer.from(ethers.utils.keccak256(physical).slice(2),'hex'),Buffer.from(ethers.utils.RLP.encode('0x2a').slice(2),'hex'));
        const storageHash='0x'+Buffer.from(st.root()).toString('hex'),codeHash=ethers.utils.keccak256('0x');
        await at.put(Buffer.from(ethers.utils.keccak256(address).slice(2),'hex'),Buffer.from(ethers.utils.RLP.encode(['0x','0x',storageHash,codeHash,'0x']).slice(2),'hex'));
        const hex=x=>'0x'+Buffer.from(x).toString('hex');
        const proof={address,nonce:'0x0',balance:'0x0',codeHash,storageHash,accountProof:(await at.createProof(Buffer.from(ethers.utils.keccak256(address).slice(2),'hex'))).map(hex),storageProof:[{key:physical,value:'0x2a',proof:(await st.createProof(Buffer.from(ethers.utils.keccak256(physical).slice(2),'hex'))).map(hex)}]};
        const e={chain_id:'1',block:{number:'0x1',stateRoot:hex(at.root())},deployments:[{address,code:'0x',proof}]};
        const reads=[{method:'eth_getStorageAt',params:[address,logical,'0x1'],response:{result:'0x2a'}}];
        const opts={accountEncoding:'coreth-account-held-xchain-rlp/v1',storageKeyEncoding:'coreth-partitioned-storage/v1'};
        assert.equal((await v.verifyPrestate(e,reads,opts)).passed,true);
        await assert.rejects(v.verifyPrestate(e,reads,{accountEncoding:opts.accountEncoding}),/PUBLIC_STORAGE_READ_NOT_PROVEN/);
        await assert.rejects(v.verifyPrestate(e,[{...reads[0],response:{result:'0x2b'}}],opts),/PUBLIC_STORAGE_PROOF_DIFFERENCE/);
        """)

    def test_next_anchor_seed_recovers_actual_lazy_boundary_without_rewriting_old_bundle(self):
        import sys
        sys.path.insert(0,str(TOOLS))
        import r01_public_catalog_evm_seed as seed
        self.assertTrue(hasattr(seed,'previous_boundary'),'seed requires boundary from actual successful reads')
        boundary=seed.previous_boundary(ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2/bsc-05')
        self.assertIn('0x16b9a82891338f9ba80e2d6970fdda79d1eb0dae',boundary)
        self.assertTrue(any(int(k,16)==8 for k in boundary['0x16b9a82891338f9ba80e2d6970fdda79d1eb0dae']))

    def test_python_capture_partition_is_explicit_and_idempotent(self):
        import sys
        sys.path.insert(0,str(TOOLS))
        import r01_public_catalog_evm_history as history
        self.assertTrue(hasattr(history,'storage_trie_key'),'capture lacks native physical key rule')
        odd='0xdb'+'00'*31;even='0xda'+'00'*31
        self.assertEqual(history.storage_trie_key('avalanche',odd),even)
        self.assertEqual(history.storage_trie_key('avalanche',even),even)
        self.assertEqual(history.storage_trie_key('base',odd),odd)
        with self.assertRaises(ValueError):history.storage_trie_key('unknown',odd)
        for bad in ['0x+1','0x 1','0x1_1','0x'+'1'*65]:
            with self.subTest(key=bad),self.assertRaises(ValueError):history.storage_trie_key('avalanche',bad)

    def test_raw_key_is_not_accepted_as_secure_key(self):
        self.run_node("""
        const fs=require('node:fs');const e=JSON.parse(fs.readFileSync('integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-04/prestate-evidence.json'));
        const a=e.deployments.find(x=>x.address==='0xb97ef9ef8734c71904d8002f8b6bc66dd9c48a6e');
        const s=a.proof.storageProof.find(x=>x.key.startsWith('0xd794'));
        assert.throws(()=>t.traceProof(a.proof.storageHash,s.key,s.proof),/TRACE_MISSING_NODE/);
        """)


if __name__ == '__main__':
    unittest.main()
