"""R01 additive history, metadata and exact executable boundary regressions."""
import importlib.util
import copy
import tempfile
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))
import r01_public_catalog as catalog

class MetadataTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        global metadata
        import r01_public_catalog_metadata as metadata

    def test_cbor_content_address_comes_from_exact_runtime_trailer(self):
        trailer=bytes.fromhex('a2646970667358221220'+'12'*32+'64736f6c6343000813')
        runtime=b'\x60\x00'+trailer+len(trailer).to_bytes(2,'big')
        result=metadata.runtime_metadata(runtime)
        self.assertEqual(result['compiler_version'],'0.8.19')
        self.assertEqual(result['ipfs_cid'],catalog.b58encode(bytes.fromhex('1220'+'12'*32)))
        self.assertEqual(result['metadata_offset'],2)
        for bad in [runtime[:-1], b'\x60\x00\xff\xff',b'\x60\x00\xa0\x00\x01']:
            with self.assertRaises(catalog.CatalogError):metadata.runtime_metadata(bad)

    def test_unknown_cbor_values_and_trailing_objects_are_not_stripped(self):
        raw=bytes.fromhex('a1646970667358221220'+'12'*32)+b'\x00'
        with self.assertRaises(catalog.CatalogError):metadata.runtime_metadata(b'\x60'+raw+len(raw).to_bytes(2,'big'))

class AnchorTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        global history
        import r01_public_catalog_history as history

    def test_same_runtime_can_have_multiple_distinct_immutable_anchors(self):
        first={'chain':'ethereum','protocol':'uniswap-v2','version':'runtime-sha256:'+'ab'*32,'anchor':{'height':1,'hash':'0x'+'11'*32}}
        second=json.loads(json.dumps(first));second['anchor']={'height':2,'hash':'0x'+'22'*32}
        self.assertNotEqual(history.anchor_key(first),history.anchor_key(second))
        self.assertEqual(history.history_scope_errors([first,second],[first,second]),[])
        self.assertIn('DUPLICATE_HISTORY_ANCHOR',str(history.history_scope_errors([first,first],[first])))
        self.assertIn('HISTORY_ANCHOR_MISSING',str(history.history_scope_errors([first],[first,second])))

    def test_reusing_height_with_changed_hash_does_not_replace_prior_anchor(self):
        row={'chain':'ethereum','protocol':'uniswap-v2','version':'runtime-sha256:'+'ab'*32,'anchor':{'height':1,'hash':'0x'+'11'*32}}
        changed=json.loads(json.dumps(row));changed['anchor']['hash']='0x'+'22'*32
        self.assertIn('HISTORY_ANCHOR_MISSING',str(history.history_scope_errors([changed],[row])))


class ForkBoundaryTests(unittest.TestCase):
    def test_public_fork_archive_rejects_every_write_method(self):
        import r01_public_catalog_history as h
        for method in ['eth_sendRawTransaction','eth_sendTransaction','evm_setAccountBalance','personal_unlockAccount']:
            with self.subTest(method=method),self.assertRaisesRegex(catalog.CatalogError,'READ_ONLY_FORK_METHOD'):
                h.pin_request(method,[],100,'0x'+'11'*32)
        self.assertEqual(h.pin_request('eth_getBalance',['0x'+'12'*20,'latest'],100,'0x'+'11'*32)[1],'0x64')
        with self.assertRaises(catalog.CatalogError):h.pin_request('eth_getBalance',['0x'+'12'*20,'0x65'],100,'0x'+'11'*32)
        with self.assertRaises(catalog.CatalogError):h.pin_request('eth_getBlockByNumber',['0x65',False],100,'0x'+'11'*32)

    def test_rpc_transaction_rlp_matches_independent_retained_go_capture(self):
        import subprocess
        root=TOOLS.parents[2];fixture=root/'integration/test/source/ethereum-mainnet/uniswap-v2-26355865-evidence.json'
        script="const h=require(process.argv[1]),x=require(process.argv[2]);for(let i=0;i<x.block.transactions.length;i++){if(h.serializeTransaction(x.block.transactions[i])!==x.raw_transactions[i])throw Error('INDEPENDENT_GO_RLP_MISMATCH:'+i)}console.log(x.block.transactions.length)"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_fork.cjs'),str(fixture)],capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr);self.assertEqual(p.stdout.strip(),'101')

class MPTPrestateTests(unittest.TestCase):
    def test_real_retained_proof_and_balance_code_storage_tamper(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),x=require(process.argv[2]);(async()=>{await h.verifyPrestate(x);for(const change of ['balance','code','storage','root']){let y=structuredClone(x);if(change==='balance')y.deployments[0].proof.balance='0xffff';if(change==='code')y.deployments[0].code='0x6000';if(change==='storage')y.deployments[0].proof.storageProof=[{key:'0x'+'00'.repeat(32),value:'0x1',proof:[]}];if(change==='root')y.block.stateRoot='0x'+'00'.repeat(32);let failed=false;try{await h.verifyPrestate(y)}catch(e){failed=true}if(!failed)throw Error('PROOF_TAMPER_ACCEPTED:'+change)}})().catch(e=>{console.error(e.stack);process.exit(1)})"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_proof.cjs'),str(root/'integration/test/source/ethereum-mainnet/uniswap-v2-26355865-evidence.json')],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stderr)

class SBFBoundaryTests(unittest.TestCase):
    def test_executable_length_requires_an_actual_build_file_not_guessing_tail(self):
        import r01_public_catalog_solana as s
        import struct
        blob=bytearray(192);blob[:16]=b'\x7fELF\x02\x01\x01'+bytes(9)
        struct.pack_into('<HHIQQQIHHHHHH',blob,16,3,247,1,0,64,0,0,64,56,1,64,0,0)
        struct.pack_into('<IIQQQQQQ',blob,64,1,5,120,0,0,32,32,8);blob[120:152]=bytes(range(32))
        artifact=bytes(blob);payload=artifact+bytes(4096)
        bounds=s.elf_bounds(payload);self.assertEqual(bounds['declared_file_extent'],152)
        result=s.compare_programdata_build(payload,artifact)
        self.assertEqual(result['allocation_padding_bytes'],4096)
        self.assertEqual(result['executable_file_bytes'],192)
        with self.assertRaises(catalog.CatalogError):s.compare_programdata_build(payload[:-1]+b'\x01',artifact)
        with self.assertRaises(catalog.CatalogError):s.compare_programdata_build(payload,b'\x7fELF')
        changed=bytearray(artifact);changed[121]^=1
        with self.assertRaises(catalog.CatalogError):s.compare_programdata_build(payload,bytes(changed))

class V3TemplateInstanceTests(unittest.TestCase):
    def test_real_pools_have_different_runtime_hashes_but_one_template(self):
        import subprocess
        root=TOOLS.parents[2]
        script="const h=require(process.argv[1]),t=require(process.argv[2]),e=require(process.argv[3]);const a=e.deployments.filter(x=>['0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640','0xc2e9f25be6257c210d7adf0d4cd6e3e881ba25f8'].includes(x.address.toLowerCase()));const b=a.map(x=>h.bindV3(t,x.code,x.address));if(b.length!==2||b[0].actual_runtime_sha256===b[1].actual_runtime_sha256||b[0].runtime_template_sha256!==b[1].runtime_template_sha256)throw Error('MULTI_POOL_IDENTITY');for(const x of a){let bad='0x61'+x.code.slice(4);let failed=false;try{h.bindV3(t,bad,x.address)}catch(e){failed=true}if(!failed)throw Error('NONIMMUTABLE_TAMPER_ACCEPTED');try{h.bindV3(t,x.code,'0x'+'11'.repeat(20));throw Error('SELF_BINDING_ACCEPTED')}catch(e){if(e.message==='SELF_BINDING_ACCEPTED')throw e}}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_history_bind.cjs'),str(root/'integration/test/source/deployment-catalog/v3-pool-template/template.json'),str(root/'integration/test/source/deployment-catalog/history/ethereum-fresh-04/prestate-evidence.json')],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stderr)

class ForkArchiveAccountingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import r01_public_catalog_history as h
        cls.h=h
        source=catalog.ROOT/'integration/test/source/deployment-catalog/evm-fresh-v5/arbitrum-refresh-05'
        cls.anchor=catalog.load_json(source/'anchor.json')['anchor']
        rows=[json.loads(catalog.read_blob(catalog.ROOT,ref))
              for ref in catalog.load_json(source/'rpc-index.json')['records']]
        # Real retained original-state cache values and proof/chunk references;
        # tests exercise accounting only, without claiming VM qualification.
        cls.derived=[r for r in rows if r.get('source_kind')==h.DERIVED_PRESTATE and
                     r['params'][0]=='0xc6962004f452be9203591991d15f6b388e09e8d0']
        if len(cls.derived)<=450:raise RuntimeError('retained large Arbitrum seed required')

    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name).resolve();self.directory=self.root/'archive'
        (self.directory/'rpc').mkdir(parents=True)
        self.refs=[];self.sources={};self.sequence=0
        self.addCleanup(patch.stopall)
        patch.object(self.h,'ROOT',self.root).start()
        patch.object(self.h,'reference',side_effect=self.reference).start()
        self.fetch=patch.object(self.h,'fetch_rpc',side_effect=self.fetch_success).start()

    def reference(self,path):
        return {'path':str(Path(path).resolve().relative_to(self.root)),
                'sha256':catalog.sha256(Path(path).read_bytes())}

    def source(self,ref):
        key=(ref['path'],ref['sha256'])
        if key not in self.sources:
            row=json.loads(catalog.read_blob(catalog.ROOT,ref))
            if 'sources' in row:row['sources']=[self.source(r) for r in row['sources']]
            path=self.root/'sources'/f'{len(self.sources):03}.json'
            path.parent.mkdir(exist_ok=True)
            path.write_text(json.dumps(row))
            self.sources[key]=self.reference(path)
        return self.sources[key]

    def derived_row(self,index=0):
        row=copy.deepcopy(self.derived[index])
        row['proof_source']=self.source(row['proof_source'])
        if row.get('code_bytes_source') is not None:
            row['code_bytes_source']=self.source(row['code_bytes_source'])
        return row

    def add(self,row,sequence=None,indexed=True):
        self.sequence=self.sequence+1 if sequence is None else sequence
        path=self.directory/'rpc'/f'{self.sequence:04}-{row["method"]}.json'
        path.write_text(json.dumps(row))
        ref=self.reference(path)
        if indexed:self.refs.append(ref)
        return ref

    def index(self,version=1):
        (self.directory/'rpc-index.json').write_text(json.dumps(
            {'schema_version':version,'anchor':self.anchor,'records':self.refs}))

    def archive(self,replay=False):
        return self.h.ForkArchive(self.anchor,self.directory,
                                 ['https://arb1.arbitrum.io/rpc'],replay=replay)

    def actual(self,failed=False,**fields):
        row={'endpoint':'https://arb1.arbitrum.io/rpc','method':'eth_getBalance',
             'params':['0x'+'12'*20,hex(self.anchor['height'])],
             'response':{'jsonrpc':'2.0','id':1,'result':'0x1'}}
        if failed:
            row.pop('response');row['error']='retained HTTP failure';row['http_status']=503
        return row|fields

    def fetch_success(self,endpoint,method,params,path,budget):
        row={'endpoint':endpoint,'method':method,'params':params,
             'response':{'jsonrpc':'2.0','id':1,'result':{
                 'eth_chainId':'0xa4b1','net_version':'42161',
                 'web3_clientVersion':'bounded-test'}[method]}}
        self.h.write_new(path,(json.dumps(row)+'\n').encode())
        return self.reference(path),row['response']['result']

    def seed(self,count=455):
        for i in range(count):self.add(self.derived_row(i))

    def test_more_than_450_real_derived_rows_allow_next_actual_attempt(self):
        self.seed();self.index();before={r['path']:r['sha256'] for r in self.refs}
        archive=self.archive()
        self.assertEqual((archive.count,archive.public_attempts),(455,0))
        self.assertEqual(archive.request('eth_chainId',[])['result'],'0xa4b1')
        self.assertEqual((archive.count,archive.public_attempts),(456,1))
        self.fetch.assert_called_once()
        self.assertEqual(Path(self.fetch.call_args.args[3]).name,'0456-eth_chainId.json')
        for path,digest in before.items():
            self.assertEqual(catalog.sha256((self.root/path).read_bytes()),digest)

    def test_schema2_seed_index_uses_same_anchor_record_and_budget_gates(self):
        self.seed();self.index(version=2)
        archive=self.archive()
        self.assertEqual((archive.count,archive.public_attempts),(455,0))
        self.assertEqual(self.archive(replay=True).public_attempts,0)
        archive.request('eth_chainId',[])
        self.assertEqual((archive.count,archive.public_attempts),(456,1))
        self.fetch.assert_called_once()
        with patch.object(self.h,'FORK_ARCHIVE_RECORDS',454):
            with self.assertRaisesRegex(catalog.CatalogError,'FORK_ARCHIVE_RECORD_BUDGET'):
                self.archive()
        manifest=catalog.load_json(self.directory/'rpc-index.json')
        manifest['anchor']['height']+=1
        (self.directory/'rpc-index.json').write_text(json.dumps(manifest))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_ARCHIVE_ANCHOR'):self.archive()

    def test_schema2_still_checks_record_digests_and_rejects_unknown_versions(self):
        self.add(self.actual());self.index(version=2)
        manifest=catalog.load_json(self.directory/'rpc-index.json')
        manifest['records'][0]['sha256']='0'*64
        (self.directory/'rpc-index.json').write_text(json.dumps(manifest))
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        for version in (0,3,True,'2',None):
            self.index(version=version)
            with self.subTest(version=version),self.assertRaisesRegex(catalog.CatalogError,'FORK_ARCHIVE_SCHEMA'):
                self.archive()
        self.fetch.assert_not_called()

    def test_450_prior_actual_attempts_including_errors_block_before_sending(self):
        for i in range(450):self.add(self.actual(failed=i%2==0))
        self.index();archive=self.archive()
        self.assertEqual(archive.public_attempts,450)
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_REQUEST_BUDGET'):
            archive.request('eth_chainId',[])
        self.assertEqual(archive.count,450)
        self.assertEqual(archive.public_attempts,450);self.fetch.assert_not_called()
        self.assertEqual(archive.request('eth_getBalance',self.actual()['params'])['result'],'0x1')

    def test_mixed_restart_replay_and_frozen_height_preserve_both_counters(self):
        self.seed()
        self.add(self.actual(failed=True));self.add(self.actual())
        self.index();archive=self.archive()
        self.assertEqual((archive.count,archive.public_attempts),(457,2))
        archive.request('eth_chainId',[])
        archive.request('eth_blockNumber',[])
        self.assertEqual((archive.count,archive.public_attempts),(459,3))
        archive.save();self.assertTrue((self.directory/'rpc-index-before-459.json').exists())
        restarted=self.archive();self.assertEqual((restarted.count,restarted.public_attempts),(459,3))
        restarted.request('eth_chainId',[]);self.assertEqual(self.fetch.call_count,1)
        snapshots={str(p):catalog.sha256(p.read_bytes()) for p in self.directory.rglob('*') if p.is_file()}
        replay=self.archive(replay=True)
        replay.request('eth_chainId',[]);replay.request('eth_blockNumber',[])
        with self.assertRaisesRegex(catalog.CatalogError,'OFFLINE_FORK_READ_NOT_RECORDED'):
            replay.request('net_version',[])
        replay.save();self.assertEqual(self.fetch.call_count,1)
        self.assertEqual(snapshots,{str(p):catalog.sha256(p.read_bytes()) for p in self.directory.rglob('*') if p.is_file()})

    def test_failed_fallback_attempts_are_retained_charged_and_restartable(self):
        def failure(endpoint,method,params,path,budget):
            row={'endpoint':endpoint,'method':method,'params':params,
                 'response':{'jsonrpc':'2.0','id':1,'error':{'code':-1,'message':'failed'}}}
            self.h.write_new(path,json.dumps(row).encode());return self.reference(path),None
        self.fetch.side_effect=failure
        archive=self.h.ForkArchive(self.anchor,self.directory,['a','b'])
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_READ_FAILED'):
            archive.request('eth_chainId',[])
        self.assertEqual((archive.count,archive.public_attempts,len(archive.records)),(2,2,2))
        archive.save();self.assertEqual(self.archive().public_attempts,2)

    def test_failed_last_attempt_blocks_fallback_and_stays_charged_on_restart(self):
        for _ in range(449):self.add(self.actual(failed=True))
        self.index()
        def failure(endpoint,method,params,path,budget):
            self.h.write_new(path,json.dumps({'endpoint':endpoint,'method':method,
                'params':params,'error':'last allowed transport attempt failed'}).encode())
            return self.reference(path),None
        self.fetch.side_effect=failure
        archive=self.h.ForkArchive(self.anchor,self.directory,['a','b'])
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_REQUEST_BUDGET'):
            archive.request('eth_chainId',[])
        self.fetch.assert_called_once()
        self.assertEqual((archive.count,archive.public_attempts),(450,450))
        self.assertEqual(self.archive().public_attempts,450)

    def test_raised_transport_error_is_persisted_and_charged_without_save(self):
        self.fetch.side_effect=OSError('interrupted transport')
        archive=self.archive()
        with self.assertRaisesRegex(OSError,'interrupted transport'):
            archive.request('eth_chainId',[])
        self.assertEqual((archive.public_attempts,len(archive.records)),(1,1))
        row=catalog.load_json(self.root/archive.records[0]['path'])
        self.assertIn('interrupted transport',row['error'])
        self.assertEqual(self.archive().public_attempts,1)

    def test_unindexed_actual_files_are_recovered_and_charged(self):
        for i in range(450):self.add(self.actual(failed=True),indexed=False)
        archive=self.archive()
        self.assertEqual((archive.count,archive.public_attempts,len(archive.records)),(450,450,450))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_REQUEST_BUDGET'):
            archive.request('eth_chainId',[])
        self.fetch.assert_not_called()

    def test_unknown_derived_tags_and_success_flags_do_not_free_attempts(self):
        for _ in range(450):
            row=self.actual();row.pop('endpoint')
            row.update(source_kind='unknown-derived-proof',passed=True)
            self.add(row)
        self.index();archive=self.archive()
        self.assertEqual(archive.public_attempts,450)
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_REQUEST_BUDGET'):
            archive.request('eth_chainId',[])
        self.fetch.assert_not_called()

    def test_relabeling_an_actual_envelope_cannot_bypass_cap(self):
        for _ in range(449):self.add(self.actual(failed=True))
        row=self.derived_row(1);row['endpoint']='https://arb1.arbitrum.io/rpc'
        self.add(row);self.index();archive=self.archive()
        self.assertEqual(archive.public_attempts,450)
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_PUBLIC_REQUEST_BUDGET'):
            archive.request('eth_chainId',[])
        self.fetch.assert_not_called()

    def test_fake_known_derived_record_without_provenance_fails_closed(self):
        self.add({'method':'eth_getBalance','params':['0x'+'12'*20,hex(self.anchor['height'])],
                  'response':{'jsonrpc':'2.0','id':1,'result':'0x1'},
                  'source_kind':self.h.DERIVED_PRESTATE})
        self.index()
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_DERIVED_SOURCE_REF'):
            self.archive()
        self.fetch.assert_not_called()

    def test_derived_value_anchor_and_reference_digest_must_match(self):
        row=self.derived_row(1)
        for change,error in [('value','FORK_DERIVED_ACCOUNT_VALUE'),('anchor','FORK_DERIVED_ANCHOR'),
                             ('hash','BLOB_HASH'),('outside','BLOB_PATH')]:
            bad=copy.deepcopy(row)
            if change=='value':bad['response']['result']='0xffffffffffff'
            elif change=='anchor':bad['params'][-1]='0x1'
            elif change=='hash':bad['proof_source']['sha256']='0'*64
            else:bad['proof_source']['path']='../outside'
            self.refs=[]
            for p in (self.directory/'rpc').glob('*.json'):p.unlink()
            self.add(bad);self.index()
            with self.subTest(change=change),self.assertRaisesRegex(catalog.CatalogError,error):
                self.archive()
        self.fetch.assert_not_called()

    def test_union_provenance_and_code_bytes_are_authenticated(self):
        row=self.derived_row(0)
        path=self.root/row['proof_source']['path'];source=catalog.load_json(path)
        self.assertGreater(len(source['sources']),1)
        source['sources']=[];path.write_text(json.dumps(source))
        row['proof_source']=self.reference(path)
        self.add(row);self.index()
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_DERIVED_UNION_SOURCES'):
            self.archive()
        self.fetch.assert_not_called()

    def test_code_bytes_reference_digest_is_checked(self):
        row=self.derived_row(3)
        self.assertEqual(row['method'],'eth_getCode')
        row['code_bytes_source']['sha256']='0'*64
        self.add(row);self.index()
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        self.fetch.assert_not_called()

    def test_sparse_sequence_and_archive_anchor_mismatch_are_not_counter_resets(self):
        self.add(self.actual(failed=True),sequence=9000);self.index()
        archive=self.archive();self.assertEqual((archive.count,archive.public_attempts),(9000,1))
        archive.request('eth_chainId',[])
        self.assertEqual((archive.count,archive.public_attempts),(9001,2))
        manifest=catalog.load_json(self.directory/'rpc-index.json');manifest['anchor']['height']+=1
        (self.directory/'rpc-index.json').write_text(json.dumps(manifest))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_ARCHIVE_ANCHOR'):self.archive()

    def test_record_and_provenance_memory_budgets_fail_closed(self):
        self.add(self.derived_row(1));self.index()
        for name,error in [('FORK_ARCHIVE_RECORDS','FORK_ARCHIVE_RECORD_BUDGET'),
                           ('FORK_ARCHIVE_BYTES','FORK_ARCHIVE_BYTE_BUDGET'),
                           ('FORK_SOURCE_FILES','FORK_DERIVED_SOURCE_BUDGET'),
                           ('FORK_SOURCE_BYTES','FORK_DERIVED_SOURCE_BUDGET')]:
            with self.subTest(name=name),patch.object(self.h,name,0):
                with self.assertRaisesRegex(catalog.CatalogError,error):self.archive()
        self.fetch.assert_not_called()

class CorethArchiveBindingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import r01_public_catalog_history as h
        cls.h=h
        cls.relative=Path('integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-07')
        source=catalog.ROOT/cls.relative
        cls.files={}
        def keep(ref,recurse=False):
            if ref['path'] not in cls.files:
                cls.files[ref['path']]=catalog.read_blob(catalog.ROOT,ref)
                if recurse:
                    for child in json.loads(cls.files[ref['path']]).get('sources',[]):keep(child,True)
        def direct(path):
            ref={'path':str(path.relative_to(catalog.ROOT)),'sha256':catalog.sha256(path.read_bytes())}
            keep(ref)
        for name in ('anchor.json','rpc-index.json','prestate-evidence.json'):direct(source/name)
        for path in (source/'storage-key-bindings').glob('*.json'):direct(path)
        keep(h.CORETH_BINDING_INVENTORY)
        cls.index_data=catalog.load_json(source/'rpc-index.json')
        for ref in cls.index_data['records']:
            keep(ref);row=json.loads(cls.files[ref['path']])
            if row.get('proof_source'):keep(row['proof_source'],True)
            if row.get('code_bytes_source'):keep(row['code_bytes_source'])
        cls.anchor=catalog.load_json(source/'anchor.json')['anchor']
        cls.address='0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7'

    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name).resolve()
        for path,data in self.files.items():
            target=self.root/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        self.directory=self.root/self.relative
        self.addCleanup(patch.stopall)
        patch.object(self.h,'ROOT',self.root).start()
        self.fetch=patch.object(self.h,'fetch_rpc',side_effect=AssertionError('network forbidden')).start()
        self.write=patch.object(self.h,'write_new',side_effect=AssertionError('archive writes forbidden')).start()

    def archive(self,anchor=None):
        from r01_public_catalog_evm_history import EvmArchive
        return EvmArchive(self.anchor if anchor is None else anchor,self.directory,[],replay=True)

    def binding_path(self):return self.directory/'storage-key-bindings'/(self.address+'.json')

    def record(self,method='eth_getStorageAt'):
        for ref in self.index_data['records']:
            row=json.loads(self.files[ref['path']])
            if row['method']==method and row.get('source_kind')==self.h.DERIVED_PRESTATE and row['params'][0]==self.address:
                if method!='eth_getStorageAt' or row['params'][1].startswith('0x11'):
                    return copy.deepcopy(ref),row
        raise AssertionError('retained Coreth cache row missing')

    def replace_row(self,ref,row):
        path=self.root/ref['path'];path.write_text(json.dumps(row))
        manifest=catalog.load_json(self.directory/'rpc-index.json')
        for target in manifest['records']:
            if target['path']==ref['path']:target['sha256']=catalog.sha256(path.read_bytes())
        (self.directory/'rpc-index.json').write_text(json.dumps(manifest))

    def test_typed_pinned_logical_mapping_preserves_physical_proof_keys(self):
        archive=self.archive();ref,row=self.record()
        logical=row['params'][1];physical='0x10'+logical[4:]
        self.assertEqual(archive._storage_maps[self.address][logical],physical)
        self.assertEqual(archive.public_attempts,6)
        source_path=self.root/row['proof_source']['path'];before=source_path.read_bytes()
        proof=json.loads(before)['response']['result']
        self.assertIn(physical,{slot['key'] for slot in proof['storageProof']})
        self.assertNotIn(logical,{slot['key'] for slot in proof['storageProof']})
        self.assertEqual(archive.request(row['method'],row['params']),row['response'])
        self.assertEqual(source_path.read_bytes(),before)
        self.fetch.assert_not_called();self.write.assert_not_called()

    def test_explicit_encoding_cannot_apply_to_wrong_chain(self):
        anchor=copy.deepcopy(self.anchor);anchor.update(chain_key='evm:42161',chain_id=42161)
        manifest=catalog.load_json(self.directory/'rpc-index.json');manifest['anchor']=anchor
        (self.directory/'rpc-index.json').write_text(json.dumps(manifest))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_STORAGE_KEY_CHAIN'):self.archive(anchor)
        self.fetch.assert_not_called()

    def test_unknown_encoding_and_missing_typed_selector_are_not_inferred(self):
        path=self.directory/'prestate-evidence.json';evidence=json.loads(path.read_bytes())
        evidence['storage_key_encoding']='coreth-partitioned-storage/v999';path.write_text(json.dumps(evidence))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_STORAGE_KEY_ENCODING'):self.archive()
        evidence.pop('storage_key_encoding');path.write_text(json.dumps(evidence))
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_DERIVED_PROOF_VALUE'):self.archive()
        self.fetch.assert_not_called()

    def test_binding_and_evidence_bytes_require_independent_digest_pins(self):
        path=self.binding_path();original=path.read_bytes();path.write_bytes(original+b' ')
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        path.write_bytes(original)
        evidence=self.directory/'prestate-evidence.json';evidence.write_bytes(evidence.read_bytes()+b' ')
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        self.fetch.assert_not_called()

    def test_unlisted_logical_key_is_not_auto_transformed(self):
        ref,row=self.record();logical=row['params'][1];physical='0x10'+logical[4:]
        binding=json.loads(self.binding_path().read_bytes())
        self.assertNotIn(physical,{r['logical_slot'] for r in binding['mappings']})
        row['params'][1]=physical;self.replace_row(ref,row)
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_STORAGE_MAPPING_UNKNOWN'):self.archive()
        self.fetch.assert_not_called()

    def test_binding_proof_source_must_match_exact_record_reference(self):
        ref,row=self.record();original=self.root/row['proof_source']['path']
        alias=self.directory/'same-proof-different-source.json';alias.write_bytes(original.read_bytes())
        row['proof_source']={'path':str(alias.relative_to(self.root)),'sha256':catalog.sha256(alias.read_bytes())}
        self.replace_row(ref,row)
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_STORAGE_BINDING_SCOPE'):self.archive()
        self.fetch.assert_not_called()

    def test_binding_rule_address_height_and_exact_bit_transform(self):
        archive=self.archive();_,row=self.record()
        account,storage=archive._proof(row['proof_source'])
        path=str(self.binding_path().relative_to(self.root));ref=archive._storage_refs[path]
        original=archive._source(ref);read_source=archive._source
        cases=[('rule','unknown','FORK_STORAGE_BINDING_SCOPE'),
               ('address','0x'+'12'*20,'FORK_STORAGE_BINDING_SCOPE'),
               ('height',self.anchor['height']+1,'FORK_STORAGE_BINDING_SCOPE'),
               ('physical',None,'FORK_STORAGE_MAPPING_TRANSFORM'),
               ('coverage',None,'FORK_STORAGE_MAPPING_KEYS')]
        for field,value,error in cases:
            changed=copy.deepcopy(original)
            if field=='physical':changed['mappings'][0]['physical_slot']=changed['mappings'][0]['logical_slot']
            elif field=='coverage':changed['mappings'].pop()
            else:changed[field]=value
            archive._storage_maps.pop(self.address,None)
            def source(selected):return changed if selected==ref else read_source(selected)
            with self.subTest(field=field),patch.object(archive,'_source',side_effect=source):
                with self.assertRaisesRegex(catalog.CatalogError,error):
                    archive._storage_mapping(row,account,storage)
        self.fetch.assert_not_called();self.write.assert_not_called()


class BatchProjectionArchiveTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import r01_public_catalog_history as h
        cls.h=h
        cls.relative=Path('integration/test/source/deployment-catalog/evm-continuation-v2/bsc-04')
        directory=catalog.ROOT/cls.relative
        cls.files={}
        def keep(ref,recurse=False):
            if ref['path'] not in cls.files:
                data=catalog.read_blob(catalog.ROOT,ref);cls.files[ref['path']]=data
                if recurse:
                    row=json.loads(data)
                    for child in row.get('sources',[]):keep(child,True)
                    if row.get('batch_source'):keep(row['batch_source'],True)
        for name in ('anchor.json','rpc-index.json','prestate-evidence.json'):
            path=directory/name
            keep({'path':str(path.relative_to(catalog.ROOT)),'sha256':catalog.sha256(path.read_bytes())})
        saved=catalog.load_json(directory/'anchor.json');cls.anchor=saved['anchor']
        cls.header_ref=saved['header'];keep(cls.header_ref,True)
        cls.index_data=catalog.load_json(directory/'rpc-index.json');cls.rows=[]
        for ref in cls.index_data['records']:
            keep(ref);row=json.loads(cls.files[ref['path']]);cls.rows.append(row)
            if row.get('proof_source'):keep(row['proof_source'],True)
            if row.get('code_bytes_source'):keep(row['code_bytes_source'])
        cls.proof_ref=next(row['proof_source'] for row in cls.rows if row.get('proof_source'))

    def setUp(self):
        temp=tempfile.TemporaryDirectory();self.addCleanup(temp.cleanup)
        self.root=Path(temp.name).resolve();self.directory=self.root/self.relative
        for path,data in self.files.items():
            target=self.root/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data)
        self.addCleanup(patch.stopall)
        patch.object(self.h,'ROOT',self.root).start()
        self.fetch=patch.object(self.h,'fetch_rpc',side_effect=AssertionError('network forbidden')).start()
        self.write=patch.object(self.h,'write_new',side_effect=AssertionError('writes forbidden')).start()

    def archive(self):
        from r01_public_catalog_evm_history import EvmArchive
        return EvmArchive(self.anchor,self.directory,[],replay=True)

    def context(self):
        archive=self.archive();wire=copy.deepcopy(archive._source(self.proof_ref))
        batch=copy.deepcopy(archive._source(wire['batch_source']))
        frozen=copy.deepcopy(archive._source(self.header_ref))
        return archive,wire,batch,frozen

    def test_retained_schema2_batch_latest_constructor_binds_frozen_header(self):
        archive,wire,batch,frozen=self.context()
        self.assertEqual(self.index_data['schema_version'],2)
        self.assertEqual((len(archive.records),archive.public_attempts),(86,0))
        self.assertEqual(len(archive._batches),1)
        self.assertEqual(wire['params'][-1],'latest')
        self.assertEqual(frozen['response']['result']['hash'],self.anchor['hash'])
        self.assertEqual(frozen['response'],next(r for r in batch['response'] if r['id']==1))
        self.fetch.assert_not_called();self.write.assert_not_called()

    def test_batch_and_projection_bytes_both_require_digest_authentication(self):
        wire=json.loads(self.files[self.proof_ref['path']]);path=self.root/wire['batch_source']['path']
        original=path.read_bytes();path.write_bytes(original+b' ')
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        path.write_bytes(original)
        projection=self.root/self.proof_ref['path'];projection.write_bytes(projection.read_bytes()+b' ')
        with self.assertRaisesRegex(catalog.CatalogError,'BLOB_HASH'):self.archive()
        self.fetch.assert_not_called()

    def test_latest_or_projection_tag_without_exact_batch_is_not_bound(self):
        archive,wire,_,_=self.context();read_source=archive._source
        for case,error in [('untagged','FORK_DERIVED_PROOF_ANCHOR'),
                           ('missing-batch','FORK_DERIVED_SOURCE_REF'),
                           ('unknown-tag','FORK_DERIVED_PROOF_ANCHOR')]:
            changed=copy.deepcopy(wire)
            if case=='untagged':changed.pop('source_kind')
            elif case=='missing-batch':changed['batch_source']=None
            else:changed['source_kind']='untrusted-projection';changed['passed']=True
            archive._proofs.clear();archive._batches.clear()
            def source(ref):return changed if ref==self.proof_ref else read_source(ref)
            with self.subTest(case=case),patch.object(archive,'_source',side_effect=source):
                with self.assertRaisesRegex(catalog.CatalogError,error):archive._proof(self.proof_ref)
        self.fetch.assert_not_called()

    def test_projection_request_response_id_endpoint_and_value_are_exact(self):
        archive,wire,_,_=self.context()
        for case,error in [('params','FORK_BATCH_PROJECTION'),('id','FORK_BATCH_PROJECTION'),
                           ('value','FORK_BATCH_PROJECTION'),('endpoint','FORK_BATCH_ENDPOINT')]:
            changed=copy.deepcopy(wire)
            if case=='params':changed['params'][-1]='pending'
            elif case=='id':changed['response']['id']=999
            elif case=='value':changed['response']['result']['balance']='0xffffffffffff'
            else:changed['endpoint']='https://arb1.arbitrum.io/rpc'
            with self.subTest(case=case),self.assertRaisesRegex(catalog.CatalogError,error):
                archive._batch_projection(changed)
        self.fetch.assert_not_called()

    def test_ambiguous_ids_and_unknown_batch_format_fail_closed(self):
        archive,wire,batch,_=self.context();read_source=archive._source
        for case,error in [('request-id','FORK_BATCH_REQUEST_ID'),
                           ('response-id','FORK_BATCH_RESPONSE_ID'),
                           ('missing-response','FORK_BATCH_RESPONSE_IDS'),
                           ('source-kind','FORK_BATCH_SOURCE_KIND'),
                           ('budget','FORK_BATCH_REQUEST_BUDGET')]:
            changed=copy.deepcopy(batch)
            if case=='request-id':changed['requests'][1]['id']=changed['requests'][0]['id']
            elif case=='response-id':changed['response'][1]['id']=changed['response'][0]['id']
            elif case=='missing-response':changed['response'].pop()
            elif case=='source-kind':changed['source_kind']='claimed-success';changed['passed']=True
            else:changed['max_methods']=257
            archive._batches.clear()
            def source(ref):return changed if ref==wire['batch_source'] else read_source(ref)
            with self.subTest(case=case),patch.object(archive,'_source',side_effect=source):
                with self.assertRaisesRegex(catalog.CatalogError,error):archive._batch_projection(wire)
        self.fetch.assert_not_called()

    def test_joint_batch_header_must_match_actual_frozen_anchor_identity(self):
        archive,wire,batch,frozen=self.context();read_source=archive._source
        for field,value in [('number',hex(self.anchor['height']+1)),('hash','0x'+'ab'*32),
                            ('stateRoot','0x'+'ab'*32),('timestamp',hex(self.anchor['timestamp']+1))]:
            changed=copy.deepcopy(batch);header=next(r for r in changed['response'] if r['id']==1)
            header['result'][field]=value
            changed_frozen=copy.deepcopy(frozen);changed_frozen['response']=copy.deepcopy(header)
            archive._batches.clear()
            def source(ref):
                if ref==wire['batch_source']:return changed
                if ref==self.header_ref:return changed_frozen
                return read_source(ref)
            with self.subTest(field=field),patch.object(archive,'_source',side_effect=source):
                with self.assertRaisesRegex(catalog.CatalogError,'FORK_BATCH_FROZEN_ANCHOR'):
                    archive._batch_projection(wire)
        self.fetch.assert_not_called()

    def test_frozen_header_projection_must_reference_this_exact_batch(self):
        archive,wire,_,frozen=self.context();read_source=archive._source
        changed=copy.deepcopy(frozen);changed['batch_source']['sha256']='0'*64
        archive._batches.clear()
        def source(ref):return changed if ref==self.header_ref else read_source(ref)
        with patch.object(archive,'_source',side_effect=source):
            with self.assertRaisesRegex(catalog.CatalogError,'FORK_BATCH_FROZEN_PROJECTION'):
                archive._batch_projection(wire)
        self.fetch.assert_not_called()

    def test_response_order_and_retained_helper_proof_superset(self):
        archive,wire,batch,_=self.context();read_source=archive._source
        reordered=copy.deepcopy(batch);reordered['response'].reverse();archive._batches.clear()
        def source(ref):return reordered if ref==wire['batch_source'] else read_source(ref)
        with patch.object(archive,'_source',side_effect=source):archive._batch_projection(wire)
        helper=next(row for row in self.rows if row['method']=='eth_getProof' and not row['params'][1]
                    and row['response']['result']['storageProof'])
        self.assertTrue(archive._derived(helper))
        changed=copy.deepcopy(helper);changed['params'][1]=['0x'+'fe'*32]
        with self.assertRaisesRegex(catalog.CatalogError,'FORK_DERIVED_PROOF_VALUE'):
            archive._derived(changed)
        self.fetch.assert_not_called();self.write.assert_not_called()


class ExistingEvmArchiveProducerTests(unittest.TestCase):
    def test_actual_avalanche07_constructor_does_not_fetch_save_or_rewrite(self):
        import r01_public_catalog_history as h
        from r01_public_catalog_evm_history import EvmArchive
        directory=catalog.ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2/avalanche-07'
        saved=catalog.load_json(directory/'anchor.json')
        paths=[directory/name for name in ('anchor.json','rpc-index.json','prestate-evidence.json')]
        paths+=list((directory/'storage-key-bindings').glob('*.json'))
        before={str(p):catalog.sha256(p.read_bytes()) for p in paths}
        with patch.object(h,'fetch_rpc',side_effect=AssertionError('network forbidden')) as fetch,patch.object(h,'write_new',side_effect=AssertionError('write forbidden')) as write,patch.object(h.ForkArchive,'save',side_effect=AssertionError('save forbidden')) as save:
            archive=EvmArchive(saved['anchor'],directory,saved['endpoints'])
            self.assertEqual(len(archive.records),91)
            self.assertEqual(len(archive._storage_maps),13)
            self.assertEqual(archive.public_attempts,6)
            fetch.assert_not_called();write.assert_not_called();save.assert_not_called()
        self.assertEqual(before,{str(p):catalog.sha256(p.read_bytes()) for p in paths})

    def test_native_mpt_still_rejects_avalanche04_and_accepts_typed_avalanche07(self):
        import subprocess
        script="""const fs=require('node:fs'),assert=require('node:assert/strict');
        const {verifyEvidence}=require(process.argv[1]);
        const root='integration/test/source/deployment-catalog/evm-continuation-v2/';
        (async()=>{const old=JSON.parse(fs.readFileSync(root+'avalanche-04/prestate-evidence.json'));
          assert.equal(old.storage_key_encoding,undefined);
          await assert.rejects(verifyEvidence(old,[]),/PRESTATE_STORAGE_VALUE/);
          const typed=JSON.parse(fs.readFileSync(root+'avalanche-07/prestate-evidence.json'));
          assert.equal(typed.storage_key_encoding,'coreth-partitioned-storage/v1');
          assert.equal((await verifyEvidence(typed,[])).passed,true);
          console.log('native MPT: Avalanche04 rejected; Avalanche07 passed');
        })().catch(e=>{console.error(e.stack);process.exitCode=1;});"""
        run=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_evm_commitments.cjs')],
                           cwd=catalog.ROOT,capture_output=True,text=True,timeout=40)
        self.assertEqual(run.returncode,0,run.stderr)
        self.assertIn('Avalanche04 rejected; Avalanche07 passed',run.stdout)

if __name__=='__main__':unittest.main()
