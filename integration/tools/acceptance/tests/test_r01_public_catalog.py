"""Adversarial tests for the public, finite R01 catalog boundary."""
import copy
import importlib.util
import hashlib
import json
from pathlib import Path
import tempfile
import sys
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location('r01_public_catalog', TOOLS / 'r01_public_catalog.py')
m = importlib.util.module_from_spec(spec)
sys.modules[spec.name] = m
spec.loader.exec_module(m)

class CatalogTests(unittest.TestCase):
    def test_missing_catalog_is_explicit_failure(self):
        with tempfile.TemporaryDirectory() as t:
            with self.assertRaisesRegex(m.CatalogError, 'CATALOG_MISSING'):
                m.load_json(Path(t) / 'absent.json')

    def test_duplicate_json_members_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            p = Path(t)/'a.json'; p.write_text('{"a":1,"a":2}')
            with self.assertRaisesRegex(m.CatalogError, 'DUPLICATE_JSON'):
                m.load_json(p)

    def test_credential_inputs_rejected(self):
        for url in ['https://u:p@a.org', 'https://a.org/?apikey=x', 'http://localhost', 'https://a.org/key/abc']:
            with self.subTest(url=url), self.assertRaises(m.CatalogError):
                m.public_url(url)
        self.assertEqual(m.public_url('https://ethereum-rpc.publicnode.com'), 'https://ethereum-rpc.publicnode.com')
        self.assertEqual(m.public_url('https://sourcify.dev/server/v2/contract/1/0x123?fields=all'),'https://sourcify.dev/server/v2/contract/1/0x123?fields=all')

    def test_blob_escape_and_changed_bytes_rejected(self):
        with tempfile.TemporaryDirectory() as t:
            root=Path(t); p=root/'a';p.write_bytes(b'real bytes')
            ref={'path':'a','sha256':m.sha256(b'real bytes')}
            self.assertEqual(m.read_blob(root,ref),b'real bytes')
            p.write_bytes(b'model')
            with self.assertRaisesRegex(m.CatalogError,'BLOB_HASH'):
                m.read_blob(root,ref)
            with self.assertRaisesRegex(m.CatalogError,'BLOB_PATH'):
                m.read_blob(root,{'path':'../a','sha256':'0'*64})

    def test_minimal_proxy_target_is_decoded_from_runtime(self):
        target='12'*20
        code='0x363d3d373d3d3d363d73'+target+'5af43d82803e903d91602b57fd5bf3'
        self.assertEqual(m.minimal_proxy(code),'0x'+target)
        self.assertIsNone(m.minimal_proxy('0x6000'))

    def test_scope_duplicate_and_omission_fail_closed(self):
        required=[{'chain':'ethereum','protocol':'uniswap-v2','version':'core-1'}]
        entry={**required[0],'id':'eth-v2'}
        self.assertEqual(m.scope_errors([entry],required),[])
        self.assertIn('SCOPE_MISSING',str(m.scope_errors([],required)))
        self.assertIn('DUPLICATE_DEPLOYMENT',str(m.scope_errors([entry,entry],required)))
        self.assertIn('DUPLICATE_SCOPE',str(m.scope_errors([entry],required+required)))

    def test_programdata_strict_binary_decoding(self):
        programdata=m.b58encode(bytes(range(32)))
        program=(2).to_bytes(4,'little')+bytes(range(32))
        info=m.decode_program(program)
        self.assertEqual(info,programdata)
        binary=b'\x7fELF'+b'original SBF bytes'
        data=(3).to_bytes(4,'little')+(123).to_bytes(8,'little')+b'\x01'+bytes(range(32))+binary
        decoded=m.decode_programdata(data)
        self.assertEqual(decoded['upgrade_slot'],123)
        self.assertEqual(decoded['payload_sha256'],m.sha256(binary))
        self.assertEqual(decoded['upgrade_authority'],programdata)
        for corrupt in [data[:44], b'\x00'*45+binary, data[:12]+b'\x02'+data[13:]]:
            with self.assertRaises(m.CatalogError):m.decode_programdata(corrupt)


class TransportTests(unittest.TestCase):
    def test_probe_rejects_transactions_and_unbounded_budget(self):
        probe_spec=importlib.util.spec_from_file_location('r01_public_catalog_capture',TOOLS/'r01_public_catalog_capture.py')
        probe=importlib.util.module_from_spec(probe_spec);probe_spec.loader.exec_module(probe)
        with self.assertRaisesRegex(m.CatalogError,'READ_ONLY_METHOD'):
            probe.fetch_rpc('https://ethereum-rpc.publicnode.com','eth_sendRawTransaction',['0x'],Path('/absent'))
        with self.assertRaisesRegex(m.CatalogError,'RESPONSE_BUDGET'):
            probe.fetch_rpc('https://ethereum-rpc.publicnode.com','eth_chainId',[],Path('/absent'),budget=1024**3)

class ReplayAuthorityTests(unittest.TestCase):
    def test_model_report_cannot_supply_original_execution(self):
        import subprocess
        script="const h=require(process.argv[1]);try{h.validateExecution({passed:true,cases:[{expected:{amount_out:'42'}}]},'00'.repeat(32));process.exit(2)}catch(e){console.error(e.message);process.exit(1)}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_replay.cjs')],capture_output=True,text=True)
        self.assertEqual(p.returncode,1)
        self.assertIn('EXECUTION_',p.stderr)

class SampleBoundaryTests(unittest.TestCase):
    def test_factory_cannot_claim_pool_scope_without_real_pool(self):
        self.assertIn('SWAP_POOL_RUNTIME_SCOPE_MISSING',m.sample_gaps({'role':'factory','sample_pools':[]}))
        self.assertEqual(m.sample_gaps({'role':'pool'}),[])

class PublicCompilerTests(unittest.TestCase):
    def test_rebuild_never_strips_different_metadata(self):
        import subprocess
        script="const h=require(process.argv[1]);try{h.compareRuntime('0x6000aabb','0x6000ccdd');process.exit(2)}catch(e){console.error(e.message);process.exit(1)}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_sources.cjs')],capture_output=True,text=True)
        self.assertEqual(p.returncode,1);self.assertIn('SOURCE_RUNTIME_MISMATCH',p.stderr)

class RetainedDeploymentTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.root=TOOLS.parents[2]
        cls.catalog=m.load_json(cls.root/'integration/test/source/deployment-catalog/catalog.json')
        cls.targets=m.load_json(cls.root/'integration/config/chain-targets.json')
        cls.rows={r['id']:r for r in cls.catalog['deployments']}

    def evm(self,row):return m.validate_evm(self.root,row,self.targets['targets'][row['chain']])
    def solana(self,row):return m.validate_solana(self.root,row,self.targets['targets']['solana'])

    def test_all_eight_chains_have_real_nonempty_runtimes(self):
        self.assertEqual(set(r['chain'] for r in self.rows.values()),set(self.targets['targets']))
        self.assertGreaterEqual(len(self.rows),16)
        for row in self.rows.values():
            result=self.solana(row) if row['chain']=='solana' else self.evm(row)
            self.assertTrue(result['runtime_observed'],row['id'])
            m.validate_sources(self.root,row)

    def test_chain_height_hash_runtime_version_tampering(self):
        original=self.rows['ethereum-uniswap-v2']
        for mutate in [lambda r:r.update(chain_key='evm:56'),lambda r:r.update(genesis_hash='0x'+'00'*32),
                       lambda r:r['anchor'].update(height=r['anchor']['height']+1),lambda r:r['anchor'].update(hash='0x'+'00'*32),
                       lambda r:r['runtime'].update(sha256='00'*32),lambda r:r.update(version='model/config-sha256')]:
            row=copy.deepcopy(original);mutate(row)
            with self.subTest(row=row['version']),self.assertRaises(m.CatalogError):self.evm(row)

    def test_actual_clone_implementation_cannot_be_omitted_or_changed(self):
        original=self.rows['base-aerodrome-v2']
        self.assertTrue(any(s.get('implementation') for s in original['sample_pools']))
        for change in ['omit','target','digest']:
            row=copy.deepcopy(original);sample=next(s for s in row['sample_pools'] if s.get('implementation'))
            if change=='omit':del sample['implementation']
            elif change=='target':sample['implementation']['address']='0x'+'11'*20
            else:sample['implementation']['sha256']='00'*32
            with self.subTest(change=change),self.assertRaises(m.CatalogError):self.evm(row)

    def test_custom_factory_dependency_getter_and_code_are_bound(self):
        original=self.rows['base-aerodrome-v2']
        row=copy.deepcopy(original);row['upgrade']['dependencies'][0]['address']='0x'+'11'*20
        with self.assertRaises(m.CatalogError):self.evm(row)
        row=copy.deepcopy(original);row['upgrade']['dependencies'][0]['sha256']='00'*32
        with self.assertRaises(m.CatalogError):self.evm(row)

    def test_programdata_upgrade_authority_slot_context_and_runtime_are_bound(self):
        original=self.rows['solana-orca-whirlpools']
        for mutate in [lambda r:r['upgrade'].update(programdata=m.b58encode(bytes(32))),
                       lambda r:r['upgrade'].update(slot=r['upgrade']['slot']+1),
                       lambda r:r['upgrade'].update(authority=m.b58encode(bytes(32))),
                       lambda r:r['anchor'].update(height=r['anchor']['height']-1),
                       lambda r:r['anchor'].update(hash=m.b58encode(bytes(32))),
                       lambda r:r['runtime'].update(sha256='00'*32),
                       lambda r:r.update(version='assumed-source-commit')]:
            row=copy.deepcopy(original);mutate(row)
            with self.subTest(mutate=mutate),self.assertRaises(m.CatalogError):self.solana(row)

    def test_rehashed_wrong_program_owner_and_proxy_slot_are_rejected(self):
        from unittest.mock import patch
        original=m.json_blob
        row=self.rows['solana-orca-whirlpools'];target=row['runtime']['observation']
        def changed(root,ref):
            data=original(root,ref)
            if ref==target:data['response']['result']['value'][0]['owner']='11111111111111111111111111111111'
            return data
        with patch.object(m,'json_blob',side_effect=changed),self.assertRaisesRegex(m.CatalogError,'PROGRAM_OWNER'):
            self.solana(row)
        row=self.rows['ethereum-uniswap-v2'];target=row['upgrade']['slots'][m.IMPLEMENTATION_SLOT]
        def changed(root,ref):
            data=original(root,ref)
            if ref==target:data['response']['result']='0x'+'00'*12+'11'*20
            return data
        with patch.object(m,'json_blob',side_effect=changed),self.assertRaises(m.CatalogError):self.evm(row)

    def test_compiler_source_proof_and_runtime_reports_cannot_be_substituted(self):
        original=self.rows['ethereum-uniswap-v2']
        for field in ['compiler','artifact','evidence']:
            row=copy.deepcopy(original);row['rebuild']['inputs'][field]['sha256']='00'*32
            with self.subTest(field=field),self.assertRaises(m.CatalogError):m.preflight_rebuild(self.root,row)
        row=copy.deepcopy(original);row['rebuild']['inputs']['source_files'].pop(next(iter(row['rebuild']['inputs']['source_files'])))
        with self.assertRaisesRegex(m.CatalogError,'SOURCE_INVENTORY'):m.preflight_rebuild(self.root,row)
        row=copy.deepcopy(original);row['anchor']['hash']='0x'+'11'*32
        with self.assertRaisesRegex(m.CatalogError,'PROOF_ANCHOR'):m.preflight_rebuild(self.root,row)

    def test_replayed_execution_receipt_and_balance_tamper_fails(self):
        import subprocess
        fixture=self.rows['ethereum-uniswap-v2']['execution']['fixture']
        script="const fs=require('node:fs'),h=require(process.argv[1]);let x=JSON.parse(fs.readFileSync(process.argv[2]));x.cases[0].expected.recipient_buy_after='0';try{h.validateExecution(x,process.argv[3]);process.exit(2)}catch(e){console.error(e.message);process.exit(1)}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_replay.cjs'),str(self.root/fixture['path']),self.rows['ethereum-uniswap-v2']['runtime']['sha256']],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,1)
        self.assertNotIn("Cannot find module '"+str(TOOLS/'r01_public_catalog_replay.cjs'),p.stderr)
        self.assertIn('Expected values',p.stderr)

    def test_no_report_only_qualification_and_scope_omission(self):
        scope=m.load_json(self.root/'integration/test/source/deployment-catalog/scope.json')
        report=m.validate_catalog(self.root,self.catalog,scope,self.targets,replay=False)
        self.assertTrue(report['structurally_valid'],report['errors'])
        self.assertFalse(report['qualified'])
        self.assertTrue(all(r['status']=='NOT_QUALIFIED' for r in report['deployments']))
        partial=copy.deepcopy(self.catalog);partial['deployments'].pop()
        report=m.validate_catalog(self.root,partial,scope,self.targets,replay=False)
        self.assertFalse(report['structurally_valid']);self.assertIn('SCOPE_MISSING',str(report['errors']))

class FailClosedCatalogTests(unittest.TestCase):
    def test_empty_scope_and_catalog_are_not_an_acceptance(self):
        with self.assertRaisesRegex(m.CatalogError,'EMPTY_SCOPE_CATALOG'):
            m.validate_catalog(Path('.'),{'schema_version':1,'deployments':[]},{'schema_version':1,'required':[]},{'targets':{}})

    def test_untrusted_site_cannot_call_itself_an_official_directory(self):
        with self.assertRaisesRegex(m.CatalogError,'OFFICIAL_DIRECTORY_ORIGIN'):
            m.official_directory_url({'protocol':'uniswap-v3'},'https://attacker.example/UniswapV3Factory')


class ImmutableCompilerBoundaryTests(unittest.TestCase):
    def test_only_compiler_declared_zero_immutable_slots_may_be_linked(self):
        import subprocess
        script="const h=require(process.argv[1]);let b=h.bindImmutables('6000000060','6012340060',{'7':[{start:1,length:2}]});if(b.code!=='6012340060')process.exit(2);try{h.bindImmutables('6000000060','6012340061',{'7':[{start:1,length:2}]});process.exit(3)}catch(e){if(!e.message.includes('SOURCE_RUNTIME_MISMATCH'))throw e}"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_sources.cjs')],capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr)

class LargePublicCompilerInputTests(unittest.TestCase):
    def test_heap_transfer_preserves_large_standard_json_input(self):
        import subprocess
        compiler=TOOLS.parents[2]/'integration/artifacts/local/r01-public-compilers/soljson-v0.7.6+commit.7338295f.js'
        if not compiler.exists():self.skipTest('restore the public pinned 0.7.6 compiler cache to run large-input regression')
        script="const h=require(process.argv[1]),s=require(process.argv[2]);const input=JSON.stringify({language:'Solidity',sources:{'A.sol':{content:'/*'+'x'.repeat(2500000)+'*/ pragma solidity =0.7.6; contract A {}'}},settings:{outputSelection:{'A.sol':{'A':['evm.deployedBytecode.object']}}}});const out=JSON.parse(h.compileStandardHeap(s,input));if(!out.contracts['A.sol'].A.evm.deployedBytecode.object)process.exit(2)"
        p=subprocess.run(['node','-e',script,str(TOOLS/'r01_public_catalog_sources.cjs'),str(compiler)],capture_output=True,text=True,timeout=30)
        self.assertEqual(p.returncode,0,p.stderr)

class PublicSourceReportTests(unittest.TestCase):
    setUpClass=classmethod(RetainedDeploymentTests.setUpClass.__func__)
    def test_public_source_runtime_compiler_settings_and_report_tamper(self):
        from unittest.mock import patch
        row=self.rows['ethereum-uniswap-v3'];original=m.json_blob
        manifest=m.json_blob(self.root,row['public_source_compilation']['input_manifest'])
        entry=next(e for e in manifest['entries'] if e['id']==row['id'])
        for change in ['compiler','settings','runtime','report']:
            def changed(root,ref):
                d=original(root,ref)
                if ref==entry['source']['blob']:
                    if change=='compiler':d['compilation']['compilerVersion']='0.8.19+commit.7dd6d404'
                    if change=='settings':d['compilation']['compilerSettings']['optimizer']['runs']+=1
                    if change=='runtime':d['runtimeBytecode']['onchainBytecode']='0x6000'
                if change=='report' and ref==row['public_source_compilation']['report']:
                    target=next(r for r in d['entries'] if r['id']==row['id']);target['source_files']['fake.sol']='00'*32
                return d
            with self.subTest(change=change),patch.object(m,'json_blob',side_effect=changed),self.assertRaises(m.CatalogError):
                m.preflight_public_source(self.root,row)


class CredentialFreeRPCTests(unittest.TestCase):
    def test_required_rpc_cannot_hide_a_key_in_provider_path(self):
        for url in ['https://eth-mainnet.g.alchemy.com/v2/opaquevalue','https://private.example/rpc/abc','https://ethereum-rpc.publicnode.com/?token=x']:
            with self.subTest(url=url),self.assertRaises(m.CatalogError):m.public_rpc_endpoint(url)
        self.assertEqual(m.public_rpc_endpoint('https://1rpc.io/eth'),'https://1rpc.io/eth')


class InventoryTests(unittest.TestCase):
    def test_inventory_detects_missing_extra_and_changed_source_bytes(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'data').mkdir();file=root/'data/a.json';file.write_bytes(b'{}')
            manifest={'schema_version':1,'data_directory':'data','files':[{'path':'data/a.json','sha256':m.sha256(b'{}')}],'tools':[]}
            self.assertTrue(m.check_inventory(root,manifest))
            file.write_bytes(b'{"model":1}')
            with self.assertRaisesRegex(m.CatalogError,'BLOB_HASH'):m.check_inventory(root,manifest)
            file.write_bytes(b'{}');extra=root/'data/b.json';extra.write_bytes(b'{}')
            with self.assertRaisesRegex(m.CatalogError,'INVENTORY_MEMBERSHIP'):m.check_inventory(root,manifest)
            extra.unlink();file.unlink()
            with self.assertRaisesRegex(m.CatalogError,'INVENTORY_MEMBERSHIP'):m.check_inventory(root,manifest)

class StreamedInventoryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.directory=tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.directory.cleanup)
        cls.root=Path(cls.directory.name)
        (cls.root/'data').mkdir()
        cls.large=cls.root/'data/retained.bin'
        block=bytes(range(256))*256
        digest=hashlib.sha256()
        with cls.large.open('wb') as stream:
            for _ in range(33*1024*1024//len(block)):
                stream.write(block);digest.update(block)
        cls.ref={'path':'data/retained.bin','sha256':digest.hexdigest()}
        cls.manifest={'schema_version':1,'data_directory':'data','files':[cls.ref],'tools':[]}

    def test_33mib_opaque_retained_file_streams_in_bounded_chunks(self):
        self.assertEqual(self.large.stat().st_size,33*1024*1024)
        original_open=Path.open;reads=[]
        class ObservedFile:
            def __init__(self,stream):self.stream=stream
            def __enter__(self):self.stream.__enter__();return self
            def __exit__(self,*args):return self.stream.__exit__(*args)
            def fileno(self):return self.stream.fileno()
            def read(self,size):
                reads.append(size)
                return self.stream.read(size)
        def opened(path,*args,**kwargs):
            stream=original_open(path,*args,**kwargs)
            return ObservedFile(stream) if path==self.large.resolve() and args==('rb',) else stream
        with patch.object(Path,'open',opened),patch.object(Path,'read_bytes',side_effect=AssertionError('unbounded read')),patch.object(m,'read_blob',side_effect=AssertionError('opaque file decoded')):
            self.assertTrue(m.check_inventory(self.root,self.manifest))
        self.assertGreater(len(reads),500)
        self.assertTrue(all(0<n<=64*1024 for n in reads))

    def test_same_33mib_file_still_rejected_for_selected_content(self):
        for reader in (m.read_blob,m.json_blob):
            with self.subTest(reader=reader.__name__),self.assertRaisesRegex(m.CatalogError,'BLOB_SIZE_LIMIT'):
                reader(self.root,self.ref)

    def test_large_file_wrong_missing_or_malformed_hash_is_rejected(self):
        for digest in ('0'*64,'bad',None,self.ref['sha256'].upper()):
            ref=self.ref.copy()
            if digest is None:ref.pop('sha256')
            else:ref['sha256']=digest
            manifest=self.manifest|{'files':[ref]}
            with self.subTest(digest=digest),self.assertRaisesRegex(m.CatalogError,'BLOB_HASH'):
                m.check_inventory(self.root,manifest)

    def test_changed_and_truncated_retained_files_fail_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'data').mkdir();file=root/'data/retained.bin'
            original=b'original retained evidence'*4096
            ref={'path':'data/retained.bin','sha256':m.sha256(original)}
            manifest={'schema_version':1,'data_directory':'data','files':[ref],'tools':[]}
            for content in (b'X'+original[1:],original[:-1]):
                file.write_bytes(content)
                with self.subTest(size=len(content)),self.assertRaisesRegex(m.CatalogError,'BLOB_HASH'):
                    m.check_inventory(root,manifest)

    def test_reference_paths_and_symlinks_remain_inside_checked_root(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);root=base/'root';root.mkdir()
            outside=base/'outside';outside.write_bytes(b'evidence')
            (root/'link').symlink_to(outside)
            for path in ('../outside',str(outside),'link'):
                with self.subTest(path=path),self.assertRaisesRegex(m.CatalogError,'BLOB_PATH'):
                    m._verify_inventory_blob(root,{'path':path,'sha256':m.sha256(b'evidence')},512*1024*1024)
            for ref in (None,{'path':1}):
                with self.subTest(ref=ref),self.assertRaisesRegex(m.CatalogError,'BLOB_REF'):
                    m._verify_inventory_blob(root,ref,512*1024*1024)
            with self.assertRaisesRegex(m.CatalogError,'BLOB_MISSING'):
                m._verify_inventory_blob(root,{'path':'absent','sha256':'0'*64},512*1024*1024)
            (root/'data').mkdir();(root/'data/escape').symlink_to(outside)
            manifest={'schema_version':1,'data_directory':'data','files':[{'path':'data/escape','sha256':m.sha256(b'evidence')}],'tools':[]}
            with self.assertRaisesRegex(m.CatalogError,'BLOB_PATH'):
                m.check_inventory(root,manifest)

    def test_original_symlink_reference_cannot_escape_during_hashing(self):
        with tempfile.TemporaryDirectory() as directory:
            base=Path(directory);root=base/'root';root.mkdir()
            original=b'evidence'*32768
            target=root/'target';target.write_bytes(original)
            link=root/'link';link.symlink_to(target)
            outside=base/'outside';outside.write_bytes(original)
            ref={'path':'link','sha256':m.sha256(original)}
            digest=hashlib.sha256();first=True
            class RedirectingDigest:
                def update(self,block):
                    nonlocal first
                    digest.update(block)
                    if first:
                        first=False;link.unlink();link.symlink_to(outside)
                def hexdigest(self):return digest.hexdigest()
            with patch.object(m.hashlib,'sha256',return_value=RedirectingDigest()):
                with self.assertRaisesRegex(m.CatalogError,'BLOB_PATH'):
                    m._verify_inventory_blob(root,ref,m.INVENTORY_MAX_TOTAL_BYTES)

    def test_explicit_64mib_file_limit_rejects_before_reading(self):
        self.assertEqual(m.INVENTORY_MAX_FILE_BYTES,64*1024*1024)
        self.assertEqual(m.INVENTORY_MAX_TOTAL_BYTES,512*1024*1024)
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'data').mkdir();file=root/'data/too-large'
            with file.open('wb') as stream:stream.truncate(64*1024*1024+1)
            manifest={'schema_version':1,'data_directory':'data','files':[{'path':'data/too-large','sha256':'0'*64}],'tools':[]}
            with patch.object(Path,'open',side_effect=AssertionError('oversize file read')):
                with self.assertRaisesRegex(m.CatalogError,'INVENTORY_FILE_SIZE_LIMIT'):
                    m.check_inventory(root,manifest)

    def test_total_budget_is_shared_across_files_and_includes_tools(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'data').mkdir()
            files=[]
            for name in ('a','b'):
                (root/'data'/name).write_bytes(b'x'*65536)
                files.append({'path':'data/'+name,'sha256':m.sha256(b'x'*65536)})
            manifest={'schema_version':1,'data_directory':'data','files':files,'tools':[]}
            with patch.object(m,'INVENTORY_MAX_TOTAL_BYTES',131072):
                self.assertTrue(m.check_inventory(root,manifest))
                with patch.object(Path,'open',side_effect=AssertionError('budget overflow read')):
                    with self.assertRaisesRegex(m.CatalogError,'INVENTORY_TOTAL_SIZE_LIMIT'):
                        m._verify_inventory_blob(root,files[0],65535)
            with patch.object(m,'INVENTORY_MAX_TOTAL_BYTES',131071):
                with self.assertRaisesRegex(m.CatalogError,'INVENTORY_TOTAL_SIZE_LIMIT'):
                    m.check_inventory(root,manifest)
            tools=root/'integration/tools/acceptance';tools.mkdir(parents=True)
            tool=tools/'r01_public_catalog.py';tool.write_bytes(b'original tool')
            manifest['tools']=[{'path':str(tool.relative_to(root)),'sha256':m.sha256(b'original tool')}]
            with patch.object(m,'INVENTORY_MAX_TOTAL_BYTES',131072):
                with self.assertRaisesRegex(m.CatalogError,'INVENTORY_TOTAL_SIZE_LIMIT'):
                    m.check_inventory(root,manifest)
            with patch.object(m,'INVENTORY_MAX_TOTAL_BYTES',131072+len(b'original tool')),patch.object(m,'read_blob',wraps=m.read_blob) as content_reader:
                self.assertTrue(m.check_inventory(root,manifest))
                content_reader.assert_called_once_with(root,manifest['tools'][0])

    def test_membership_duplicates_and_tool_set_stay_exact(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory);(root/'data').mkdir();(root/'data/a').write_bytes(b'a')
            ref={'path':'data/a','sha256':m.sha256(b'a')}
            manifest={'schema_version':1,'data_directory':'data','files':[ref],'tools':[]}
            with self.assertRaisesRegex(m.CatalogError,'INVENTORY_DUPLICATE'):
                m.check_inventory(root,manifest|{'files':[ref,ref]})
            tools=root/'integration/tools/acceptance';tools.mkdir(parents=True)
            tool=tools/'r01_public_catalog.py';tool.write_bytes(b'tool')
            tool_ref={'path':str(tool.relative_to(root)),'sha256':m.sha256(b'tool')}
            for refs in ([],[tool_ref,tool_ref],[tool_ref,ref]):
                with self.subTest(refs=refs),self.assertRaisesRegex(m.CatalogError,'INVENTORY_TOOL_MEMBERSHIP'):
                    m.check_inventory(root,manifest|{'tools':refs})
            self.assertTrue(m.check_inventory(root,manifest|{'tools':[tool_ref]}))
            with self.assertRaisesRegex(m.CatalogError,'BLOB_HASH'):
                m.check_inventory(root,manifest|{'tools':[tool_ref|{'sha256':'0'*64}]})

    def test_detects_midstream_mutation_truncation_growth_and_replacement(self):
        # Mutate the actual source after a real chunk has been consumed. In the
        # same-size case the consumed bytes still hash correctly: stat checks
        # must reject it independently of the checksum.
        for change in ('same-size','truncate','grow','replace','outside-link'):
            with self.subTest(change=change),tempfile.TemporaryDirectory() as directory:
                base=Path(directory);root=base/'root';root.mkdir()
                file=root/'retained';original=b'evidence'*32768
                file.write_bytes(original)
                ref={'path':'retained','sha256':m.sha256(original)}
                digest=hashlib.sha256();first=True
                class MutatingDigest:
                    def update(self,block):
                        nonlocal first
                        digest.update(block)
                        if not first:return
                        first=False
                        if change=='same-size':
                            with file.open('r+b') as stream:stream.write(b'X')
                        elif change=='truncate':
                            with file.open('r+b') as stream:stream.truncate(65536)
                        elif change=='grow':
                            with file.open('ab') as stream:stream.write(b'X')
                        elif change=='replace':
                            replacement=root/'replacement';replacement.write_bytes(original)
                            replacement.replace(file)
                        else:
                            outside=base/'outside';outside.write_bytes(original)
                            file.unlink();file.symlink_to(outside)
                    def hexdigest(self):return digest.hexdigest()
                with patch.object(m.hashlib,'sha256',return_value=MutatingDigest()):
                    with self.assertRaisesRegex(m.CatalogError,'INVENTORY_FILE_CHANGED'):
                        m._verify_inventory_blob(root,ref,m.INVENTORY_MAX_TOTAL_BYTES)

if __name__=='__main__':unittest.main()
