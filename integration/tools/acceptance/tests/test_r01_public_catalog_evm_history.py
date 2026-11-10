"""Multi-chain original-state evidence refuses scope, authority and replay shortcuts."""
import copy
import json
from pathlib import Path
import sys
import unittest

TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))

class MultiChainEvidenceTests(unittest.TestCase):
    def test_unused_capture_header_queries_do_not_weaken_exact_replay_views(self):
        import r01_public_catalog_evm_history as evm
        expected={'a':{'transactions':{'original_count':3,'local_count':0}},'b':{'transactions':{'original_count':0,'local_count':0}}}
        self.assertEqual(evm.verify_simulation_views(expected,{'a':expected['a']}),['b'])
        with self.assertRaises(ValueError):evm.verify_simulation_views(expected,{'a':{'transactions':{'original_count':4,'local_count':0}}})
        with self.assertRaises(ValueError):evm.verify_simulation_views(expected,{'unexpected':{}})

    def test_slow_public_provider_adapter_never_accepts_a_public_write(self):
        import subprocess
        script="const h=require(process.argv[1]),seen=[],remote={send:async(m,p)=>{seen.push(m);return '0x1'}};const source=h.forkSource(remote,'http://127.0.0.1:1','avalanche',123);(async()=>{await source.provider.request({method:'eth_getBalance',params:['0x'+'11'.repeat(20),'0x7b']});for(const method of ['eth_sendRawTransaction','eth_sendTransaction']){let bad=false;try{await source.provider.request({method,params:[]})}catch(e){bad=true}if(!bad)throw Error('PUBLIC_WRITE_ALLOWED')}if(seen.join(',')!=='eth_getBalance')throw Error('PUBLIC_FORWARD_SCOPE')})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_fork.cjs')],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_pinned_public_solidity_token_directory_is_not_a_private_rpc_credential(self):
        from r01_public_catalog import public_url
        url='https://raw.githubusercontent.com/OpenZeppelin/openzeppelin-contracts/7b3e7b7055d6e1226d4920ca7d7c446c0c0bfff5/contracts/token/ERC20/IERC20.sol'
        self.assertEqual(public_url(url),url)
        with self.assertRaises(ValueError):public_url('https://rpc.example/token/private-account')

    def test_chain_native_anchor_does_not_assume_avalanche_header_state_height(self):
        import r01_public_catalog_evm_history as evm
        ordinary={'number':'0x64','stateRoot':'0x'+'11'*32,'hash':'0x'+'22'*32,'timestamp':'0x50'}
        self.assertEqual(evm.state_root_semantics('base',ordinary)['state_height'],100)
        sae=ordinary|{'settledHeight':'0x5e'}
        root=evm.state_root_semantics('avalanche',sae)
        self.assertEqual(root['state_height'],94)
        self.assertFalse(root['proof_for_execution_H_established'])
        with self.assertRaises(ValueError):evm.state_root_semantics('avalanche',ordinary)

    def test_plan_is_finite_and_bound_to_real_catalog_pool_instance(self):
        import r01_public_catalog_evm_history as evm
        from r01_public_catalog import ROOT,load_json
        data=load_json(ROOT/'integration/test/source/deployment-catalog/catalog.json')
        plan=evm.chain_plan(data,'base')
        self.assertEqual(len(plan['cases']),3)
        self.assertEqual({c['kind'] for c in plan['cases']},{'v2-solidly','v3'})
        self.assertTrue(all(c['catalog_deployment_id'].startswith('base-') for c in plan['cases']))
        self.assertEqual(plan['chain_id'],8453)
        bad=copy.deepcopy(data);row=next(r for r in bad['deployments'] if r['id']=='base-uniswap-v3');row['sample_pools'][0]['address']='0x'+'00'*20
        with self.assertRaises(ValueError):evm.chain_plan(bad,'base')

    def test_simulation_header_view_is_explicit_and_cannot_change_commitment(self):
        import r01_public_catalog_evm_history as evm
        original={'number':'0x64','hash':'0x'+'11'*32,'stateRoot':'0x'+'22'*32,'transactions':[{'type':'0x7e','hash':'0x'+'33'*32}]}
        local,changes=evm.simulation_header(original)
        self.assertEqual(local['transactions'],[])
        self.assertEqual(local['stateRoot'],original['stateRoot'])
        self.assertEqual(local['hash'],original['hash'])
        self.assertEqual(len(original['transactions']),1)
        self.assertIn('transactions',changes)
        self.assertFalse(changes['original_native_transactions_replayed'])

class RealMultiChainProofTests(unittest.TestCase):
    def test_required_proof_scalars_and_account_codec_cannot_be_omitted(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),e=require(process.argv[2]);(async()=>{for(const key of ['nonce','balance','codeHash','storageHash']){const x=structuredClone(e);delete x.deployments.find(d=>BigInt(d.proof.nonce)===0n).proof[key];let bad=false;try{await h.verifyPrestate({...x,chain_id:'1'})}catch(e){bad=true}if(!bad)throw Error('MISSING_PROOF_FIELD_ACCEPTED:'+key)}let bad=false;try{await h.verifyPrestate({...e,chain_id:'1'},[],{accountEncoding:'unknown-native-account'})}catch(e){bad=true}if(!bad)throw Error('UNKNOWN_ACCOUNT_CODEC_ACCEPTED')})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_proof.cjs'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/base-01/prestate-evidence.json')],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_coreth_fifth_account_field_is_authenticated_and_never_treated_as_ethereum(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const fs=require('node:fs'),path=require('node:path'),h=require(process.argv[1]),p=require(process.argv[2]),x=require(process.argv[3]),idx=require(process.argv[4]),root=process.argv[5];const code=idx.records.map(r=>require(path.join(root,r.path))).find(r=>r.method==='eth_getCode'&&r.params[0]==='0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7'&&r.response?.result)?.response.result,a=h.verifySAEStateAnchor(x.headers,x.target_height),header=x.headers.find(b=>parseInt(b.number,16)===x.target_height),e={chain_id:'1',block:{...header,stateRoot:a.state_root},expected_block_hash:header.hash,deployments:[{address:x.state_proof.address,code,proof:x.state_proof}]};(async()=>{await p.verifyPrestate(e,[],{accountEncoding:'coreth-account-held-xchain-rlp/v1'});let bad=false;try{await p.verifyPrestate(e)}catch(e){bad=true}if(!bad)throw Error('CORETH_AS_ETHEREUM_ACCEPTED')})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs'),str(TOOLS/'r01_public_catalog_proof.cjs'),str(root/'integration/test/source/chain-receipt-vectors/avalanche-current/settlement-fresh.json'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-03/rpc-index.json'),str(root)],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_coreth_sae_uses_successor_state_root_and_native_parent_chain(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),x=require(process.argv[2]);const a=h.verifySAEStateAnchor(x.headers,x.target_height),own=x.headers.find(b=>parseInt(b.number,16)===x.target_height);if(a.state_root===own.stateRoot||a.height!==x.target_height)throw Error('STALE_ROOT_ACCEPTED');for(const kind of ['settled','gap']){let headers=structuredClone(x.headers);if(kind==='settled')headers.at(-1).settledHeight='0x1';else headers.splice(1,1);let bad=false;try{h.verifySAEStateAnchor(headers,x.target_height)}catch(e){bad=true}if(!bad)throw Error('SAE_TAMPER_ACCEPTED:'+kind)}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs'),str(root/'integration/test/source/chain-receipt-vectors/avalanche-current/settlement-fresh.json')],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_proven_absence_covers_any_zero_storage_read_without_fabricating_key_proofs(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const fs=require('node:fs'),path=require('node:path'),h=require(process.argv[1]),d=process.argv[2],root=process.argv[3],e=require(path.join(d,'prestate-evidence.json')),i=require(path.join(d,'rpc-index.json'));const records=i.records.map(x=>require(path.join(root,x.path)));(async()=>{await h.verifyEvidence(e,records);let r=structuredClone(records);const absent=new Set(e.deployments.filter(x=>x.code==='0x'&&BigInt(x.proof.balance)===0n&&BigInt(x.proof.nonce)===0n).map(x=>x.address));let x=r.find(x=>x.method==='eth_getStorageAt'&&absent.has(x.params[0])&&x.response?.result);if(!x)throw Error('ABSENT_READ_TEST_MISSING');x.response.result='0x1';let failed=false;try{await h.verifyEvidence(e,r)}catch(e){failed=true}if(!failed)throw Error('NONZERO_ABSENT_STORAGE_ACCEPTED')})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/bsc-05'),str(root)],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_absence_rpc_zero_hash_sentinel_requires_a_real_absence_proof(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),e=require(process.argv[2]);(async()=>{await h.verifyEvidence(e,[]);let bad=structuredClone(e);let a=bad.deployments.find(d=>d.proof.codeHash==='0x'+'00'.repeat(32));if(!a)throw Error('REAL_ABSENCE_SENTINEL_MISSING');a.code='0x6000';let failed=false;try{await h.verifyEvidence(bad,[])}catch(e){failed=true}if(!failed)throw Error('ABSENCE_CODE_ACCEPTED')})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/polygon-01/prestate-evidence.json')],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_native_base_body_binds_deposit_encoding_order_hash_and_chain(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),a=require(process.argv[2]),fs=require('node:fs'),p=require('node:path');const b=require(p.join(process.argv[3],a.header.path)).response.result;(async()=>{await h.verifyBody('base',b);for(const kind of ['omit','chain','position']){let x=structuredClone(b);if(kind==='omit')x.transactions.pop();if(kind==='chain')x.transactions.find(t=>t.type==='0x2').chainId='0x1';if(kind==='position')x.transactions[0].transactionIndex='0x1';let bad=false;try{await h.verifyBody('base',x)}catch(e){bad=true}if(!bad)throw Error('BODY_TAMPER_ACCEPTED:'+kind)}})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/base-01/anchor.json'),str(root)],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

    def test_failed_transport_records_do_not_poison_an_offline_success_cache(self):
        import r01_public_catalog_evm_history as evm
        from r01_public_catalog import ROOT,load_json
        d=ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2/base-01'
        a=load_json(d/'anchor.json');archive=evm.EvmArchive(a['anchor'],d,a['endpoints'],replay=True)
        result=archive.request('eth_chainId',[])
        self.assertEqual(result['result'],'0x2105')

    def test_real_base_prestate_with_proven_empty_storage_trie(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),e=require(process.argv[2]);h.verifyPrestate({...e,chain_id:'1'}).then(x=>console.log(x.accounts)).catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_proof.cjs'),str(root/'integration/test/source/deployment-catalog/evm-continuation-v2/base-01/prestate-evidence.json')],capture_output=True,text=True,timeout=45)
        self.assertEqual(p.returncode,0,p.stderr)

if __name__=='__main__':unittest.main()
