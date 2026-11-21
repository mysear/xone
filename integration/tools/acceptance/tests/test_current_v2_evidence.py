import base64
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import current_v2_evidence as m

class CurrentV2Evidence(unittest.TestCase):
    def setUp(self):self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.directory.cleanup);self.root=Path(self.directory.name)
    def record(self,method='eth_chainId',response=None):
        body=json.dumps(response or {'jsonrpc':'2.0','id':1,'result':'0x1'},separators=(',',':')).encode()
        observation={'source_kind':'raw-json-rpc-response','method':method,'params':[],'response':base64.b64encode(body).decode()};data=json.dumps(observation,separators=(',',':')).encode();digest=m.sha(data)
        owner={'Chain':'evm:1','Height':0,'Hash':digest,'ProviderID':m.provider_id(m.ENDPOINTS[0])};identity=m.sha(json.dumps(owner,separators=(',',':')).encode())
        payload={'Ref':{'ID':identity,'Digest':digest,'Owner':owner,'Bytes':len(data)},'Data':base64.b64encode(data).decode(),'CapturedAt':'2026-11-18T00:00:00Z'}
        text=json.dumps(payload,separators=(',',':'));path=self.root/(identity+'.raw');path.write_text('{"Digest":"'+m.sha(text.encode())+'","Payload":'+text+'}');return path
    def test_go_envelope_and_embedded_source_identity_are_verified(self):
        path=self.record();provider,method,params,body,identity=m.sealed_record(path)
        self.assertEqual(provider,m.provider_id(m.ENDPOINTS[0]));self.assertEqual(method,'eth_chainId');self.assertEqual(params,[]);self.assertEqual(identity['sha256'],m.sha(path.read_bytes()));self.assertEqual(json.loads(body)['result'],'0x1')
    def test_changed_payload_and_duplicate_keys_are_rejected(self):
        path=self.record();original=path.read_text();path.write_text(original.replace('2026-11-18','2026-11-19'))
        with self.assertRaisesRegex(ValueError,'CAPTURE_ENVELOPE_HASH'):m.sealed_record(path)
        path.write_text(original.replace('{"Digest":','{"Digest":"duplicate","Digest":',1))
        with self.assertRaisesRegex(ValueError,'DUPLICATE_JSON_KEY'):m.sealed_record(path)
    def test_valid_envelope_never_allows_public_write_method_or_symlink(self):
        path=self.record('eth_sendRawTransaction')
        with self.assertRaisesRegex(ValueError,'CAPTURE_METHOD'):m.sealed_record(path)
        link=self.root/'link.raw';link.symlink_to(path)
        with self.assertRaisesRegex(ValueError,'REGULAR_CAPTURE_REQUIRED'):m.sealed_record(link)
    def test_replay_is_provider_scoped_closed_and_raw_variants_are_preserved(self):
        body=b'{"jsonrpc":"2.0","id":1,"result":"0x1"}'
        rpc=m.RecordedRPC([('eth_chainId',[],body),('eth_chainId',[],b'{"result":"0x1","id":1,"jsonrpc":"2.0"}')])
        self.assertEqual(rpc.response('eth_chainId',[]),body);self.assertEqual(rpc.variants,1)
        with self.assertRaisesRegex(ValueError,'UNRECORDED_RPC_REQUEST'):rpc.response('eth_getCode',['0x1'])
        with self.assertRaisesRegex(ValueError,'READ_ONLY_METHOD_REQUIRED'):rpc.response('eth_sendRawTransaction',[])
        with self.assertRaisesRegex(ValueError,'AMBIGUOUS_CAPTURE_RESULT'):m.RecordedRPC([('eth_chainId',[],body),('eth_chainId',[],b'{"jsonrpc":"2.0","id":1,"result":"0x2"}')])
    def test_quote_oracle_checks_exact_reference_and_integer_input_minimum(self):
        pool={'token0':'a','token1':'b','reserve0':'1000000','reserve1':'2000000'};reference={'generation':1};request={'sellToken':'a','kind':'EXACT_IN','sellAmount':'100'}
        expected=100*997*2000000//(1000000*1000+100*997)
        quote={'snapshotRef':reference,'grossBuyAmount':str(expected),'consumedIn':'100','refund':'0','verification':{'level':'INDICATIVE','success':False},'costs':{'unavailableReason':'CHAIN_FEE_MODEL_NOT_IMPLEMENTED'}}
        self.assertTrue(m.check_quote(pool,reference,request,{'status':200,'body':quote})['matches'])
        bad=copy.deepcopy(quote);bad['grossBuyAmount']=str(expected+1)
        with self.assertRaisesRegex(ValueError,'INTEGER_EXACT_IN_ORACLE'):m.check_quote(pool,reference,request,{'status':200,'body':bad})
        request={'sellToken':'a','kind':'EXACT_OUT','buyAmount':'200'};denom=(2000000-200)*997;minimum=(1000000*200*1000+denom-1)//denom;quote['requiredSellAmount']=str(minimum)
        self.assertTrue(m.check_quote(pool,reference,request,{'status':200,'body':quote})['matches'])
        quote['requiredSellAmount']=str(minimum+1)
        with self.assertRaisesRegex(ValueError,'INTEGER_EXACT_OUT_ORACLE'):m.check_quote(pool,reference,request,{'status':200,'body':quote})
    def test_verified_and_changed_reference_never_pass_oracle(self):
        pool={'token0':'a','token1':'b','reserve0':'1','reserve1':'2'};request={'sellToken':'a','kind':'EXACT_IN','sellAmount':'1'};quote={'snapshotRef':{},'verification':{'level':'VERIFIED','success':True}}
        with self.assertRaisesRegex(ValueError,'FALSE_VERIFIED_CLAIM'):m.check_quote(pool,{},request,{'status':200,'body':quote})
        with self.assertRaisesRegex(ValueError,'QUOTE_REFERENCE_CHANGED'):m.check_quote(pool,{'other':1},request,{'status':200,'body':quote})


class SignedCurrentV2Inputs(unittest.TestCase):
    def setUp(self):
        self.directory=tempfile.TemporaryDirectory();self.addCleanup(self.directory.cleanup);self.root=Path(self.directory.name)
        self.path=self.root/'proof.json';self.execution={'block_number':'26460850','block_hash':'0x'+'12'*32}
        self.path.write_text(json.dumps({'finality':{'data':{'finalized_header':{'execution':self.execution}}}}))
        self.pin={'checkpoint_root':'0x'+'34'*32,'proof_file':str(self.path),'max_checkpoint_age_ms':7*86400000}
        self.endpoints=('https://ethereum-rpc.publicnode.com','https://0xrpc.io/eth')
    def test_signed_configuration_keeps_explicit_pin_and_separate_proof_source(self):
        chain,binding,raw=m.source_configuration(self.endpoints,True,self.pin,self.endpoints[1])
        self.assertEqual(chain['beacon_finality'],self.pin);self.assertEqual(chain['proof_rpc_url'],self.endpoints[1])
        self.assertEqual(binding['execution_number'],26460850);self.assertEqual(binding['execution_hash'],self.execution['block_hash'])
        self.assertEqual(binding['proof_sha256'],m.sha(self.path.read_bytes()));self.assertEqual(raw,self.path.read_bytes())
        chain['beacon_finality']['checkpoint_root']='changed';self.assertEqual(self.pin['checkpoint_root'],'0x'+'34'*32)
    def test_signed_mode_cannot_expand_legacy_scope_or_accept_ambiguous_sources(self):
        with self.assertRaisesRegex(ValueError,'SOURCE_ENDPOINT_SCOPE'):m.source_configuration(self.endpoints,True)
        with self.assertRaisesRegex(ValueError,'PROOF_ENDPOINT_REQUIRES_SIGNED_FINALITY'):m.source_configuration(m.ENDPOINTS,True,proof_endpoint=self.endpoints[1])
        for endpoints,live,proof in [((self.endpoints[0],)*2,True,None)]:
            with self.subTest(endpoints=endpoints,live=live),self.assertRaisesRegex(ValueError,'SIGNED_SOURCE_ENDPOINT_SCOPE'):m.source_configuration(endpoints,live,self.pin,proof)
        for url in ['http://example.org','https://user:secret@example.org','https://example.org/#fragment','https://example.org/ white']:
            with self.subTest(url=url),self.assertRaisesRegex(ValueError,'SIGNED_SOURCE_URL'):m.source_configuration(self.endpoints,True,self.pin,url)
    def test_duplicate_json_symlink_and_unbounded_or_unknown_release_are_rejected(self):
        self.path.write_text('{"finality":{},"finality":{}}')
        with self.assertRaisesRegex(ValueError,'DUPLICATE_JSON_KEY'):m.source_configuration(self.endpoints,True,self.pin)
        link=self.root/'link.json';link.symlink_to(self.path);pin={**self.pin,'proof_file':str(link)}
        with self.assertRaisesRegex(ValueError,'SIGNED_PROOF_FILE'):m.source_configuration(self.endpoints,True,pin)
        with self.assertRaisesRegex(ValueError,'SIGNED_FINALITY_FIELDS'):m.source_configuration(self.endpoints,True,{**self.pin,'untrusted_root':True})
        with self.assertRaisesRegex(ValueError,'SIGNED_CHECKPOINT_AGE'):m.source_configuration(self.endpoints,True,{**self.pin,'max_checkpoint_age_ms':7*86400000+1})
    def test_signed_admission_requires_same_block_and_disclosed_policy_without_verified_claim(self):
        _,binding,_=m.source_configuration(self.endpoints,True,self.pin)
        evidence={'confirmation_tag':'finalized','confirmation_policy':'PINNED_BEACON_FINALITY','consensus_verified':False}
        ref={'block':{'hash':self.execution['block_hash'],'number_or_slot':26460850}}
        m.check_source_binding(evidence,ref,binding)
        for key,value in [('hash','0x'+'56'*32),('number_or_slot',26460849)]:
            changed=copy.deepcopy(ref);changed['block'][key]=value
            with self.subTest(key=key),self.assertRaisesRegex(ValueError,'SIGNED_EXECUTION_BINDING'):m.check_source_binding(evidence,changed,binding)
        with self.assertRaisesRegex(ValueError,'SIGNED_CONFIRMATION_POLICY'):m.check_source_binding({**evidence,'confirmation_policy':'CONFIGURED_PROVIDER_TAG'},ref,binding)
        with self.assertRaisesRegex(ValueError,'NORMAL_SOURCE_DISCLOSURE'):m.check_source_binding({**evidence,'consensus_verified':True},ref,binding)
    def test_retained_signed_urls_are_strict_loopback_and_isolated_from_live_https(self):
        retained=('http://127.0.0.1:19001','http://127.0.0.1:19002');proof='http://127.0.0.1:19003'
        chain,binding,_=m.source_configuration(retained,False,self.pin,proof)
        self.assertEqual(chain['rpc_urls'],list(retained));self.assertEqual(chain['proof_rpc_url'],proof)
        self.assertEqual(binding['execution_hash'],self.execution['block_hash'])
        with self.assertRaisesRegex(ValueError,'SIGNED_SOURCE_URL'):m.source_configuration(retained,True,self.pin,proof)
        for url in [self.endpoints[0],'http://localhost:19001','http://127.0.0.2:19001',
                    'http://127.0.0.1:0','http://127.0.0.1:65536','http://127.0.0.1:019001',
                    'http://127.0.0.1:19001/','http://127.0.0.1:19001?upstream=public',
                    'http://user@127.0.0.1:19001','http://[::1]:19001','http://127.0.0.1:19001#fragment']:
            with self.subTest(url=url):
                with self.assertRaisesRegex(ValueError,'SIGNED_RETAINED_SOURCE_URL'):m.source_configuration((url,retained[1]),False,self.pin,proof)
                with self.assertRaisesRegex(ValueError,'SIGNED_RETAINED_SOURCE_URL'):m.source_configuration(retained,False,self.pin,url)
    def test_rendered_owner_router_freshness_and_source_labels_preserve_legacy_modes(self):
        owner=self.root/'owner-bin';owner.write_bytes(b'owner');router=self.root/'router-bin';router.write_bytes(b'router')
        retained=('http://127.0.0.1:19001','http://127.0.0.1:19002')
        cases=[('legacy-live',True,m.ENDPOINTS,None,None,3600000,'LIVE_PUBLIC_RPC'),
               ('legacy-retained',False,retained,None,None,315360000000,'RETAINED_PUBLIC_BYTES_LOOPBACK_ONLY'),
               ('signed-live',True,self.endpoints,self.pin,self.endpoints[1],3600000,'LIVE_EXPLICIT_SIGNED_RPC'),
               ('signed-retained',False,retained,self.pin,'http://127.0.0.1:19003',3600000,'RETAINED_RECENT_SIGNED_RPC_BYTES')]
        def spawn(args,**kwargs):
            failed=args[0]==str(owner);child=Mock(returncode=1 if failed else 0);child.poll.return_value=1 if failed else None
            if failed:
                kwargs['stdout'].write(json.dumps({'event':'service_start_failed','fields':{'error_code':'TEST_CONFIG_RENDER'}})+'\n');kwargs['stdout'].flush()
            return child
        for name,live,endpoints,pin,proof,age,label in cases:
            with self.subTest(mode=name),patch.object(m,'free_port',side_effect=[18001,18002,18003]), \
                    patch.object(m.subprocess,'run',return_value=Mock(returncode=0)), \
                    patch.object(m.subprocess,'Popen',side_effect=spawn),patch.object(m,'stop'), \
                    patch.object(m,'http',return_value={'status':503,'body':{'error':'CHAIN_NOT_READY'}}):
                output=self.root/name
                result=m.run_processes(output,owner,router,endpoints,live=live,signed_finality=pin,
                    proof_endpoint=proof,reject=True,expected_startup_failure='TEST_CONFIG_RENDER')
                self.assertEqual(result['source_mode'],label);self.assertFalse(result['production_ready']);self.assertFalse(result['consensus_verified'])
                for alias in ['owner','router']:
                    cfg=json.loads((output/(alias+'-config.json')).read_bytes())
                    self.assertEqual(cfg['startup']['max_baseline_age_ms'],age)
                    self.assertEqual(cfg['journal']['request_timeout_ms'],60000)
                    if alias=='router':
                        self.assertEqual(cfg['routing']['max_state_age_ms'],age);self.assertEqual(cfg['chains'][0]['rpc_urls'],[])
                        self.assertNotIn('beacon_finality',cfg['chains'][0]);self.assertNotIn('proof_rpc_url',cfg['chains'][0])
    def test_cli_retained_signed_is_explicit_and_keeps_required_proof_release(self):
        import contextlib
        import io
        base=['current_v2_evidence.py','owner-bin','router-bin',str(self.root/'output'),
              '--beacon-proof',str(self.path),'--checkpoint-root',self.pin['checkpoint_root'],'--rpc-urls']
        for retained in [False,True]:
            urls=['http://127.0.0.1:19001','http://127.0.0.1:19002'] if retained else list(self.endpoints)
            args=base+urls+(['--retained-signed'] if retained else [])
            with self.subTest(retained=retained),patch.object(sys,'argv',args), \
                    patch.object(m,'run_processes',return_value={'passed':True}) as run,contextlib.redirect_stdout(io.StringIO()):
                m.signed_main()
                self.assertEqual(run.call_args.kwargs['live'],not retained)
                self.assertEqual(run.call_args.kwargs['signed_finality']['checkpoint_root'],self.pin['checkpoint_root'])
                self.assertEqual(run.call_args.kwargs['signed_finality']['proof_file'],str(self.path.resolve()))
        with patch.object(sys,'argv',['current_v2_evidence.py','owner','router','output','--retained-signed',
                                    '--rpc-urls','http://127.0.0.1:1','http://127.0.0.1:2']), \
                contextlib.redirect_stderr(io.StringIO()),self.assertRaises(SystemExit):
            m.signed_main()


class FixedCurrentV2Fixture(unittest.TestCase):
    def fixture(self):return m.ROOT/'integration/test/source/current-v2-public'
    def test_complete_original_capture_unpacks_and_nested_identity_matches(self):
        with tempfile.TemporaryDirectory() as root:
            target=Path(root)/'capture';manifest=m.unpack_fixture(self.fixture(),target);records,inventory=m.capture_index(target)
            self.assertEqual(manifest['record_count'],32);self.assertEqual(len(inventory),32);self.assertEqual(len(records),2);self.assertEqual(manifest['block']['number_or_slot'],26438995)
            self.assertFalse(manifest['qualification']['consensus_verified']);self.assertFalse(manifest['qualification']['independent_execution_balance_delta_verified'])
    def test_changed_blob_or_manifest_membership_never_publishes_partial_capture(self):
        import shutil
        for fault in ['compressed','member-name','decoded-limit']:
            with self.subTest(fault=fault),tempfile.TemporaryDirectory() as root:
                source=Path(root)/'fixture';shutil.copytree(self.fixture(),source);target=Path(root)/'capture';path=source/'manifest.json';manifest=json.loads(path.read_bytes())
                if fault=='compressed':
                    blob=source/'responses.json.gz';raw=bytearray(blob.read_bytes());raw[-1]^=1;blob.write_bytes(raw)
                elif fault=='member-name':manifest['records'][0]['file']='../escape.raw';path.write_text(json.dumps(manifest))
                else:manifest['decoded_bytes']+=1;path.write_text(json.dumps(manifest))
                with self.assertRaises(ValueError):m.unpack_fixture(source,target)
                self.assertFalse(target.exists())
    def test_existing_or_symlink_destination_and_bundle_are_rejected(self):
        import shutil
        with tempfile.TemporaryDirectory() as root:
            source=Path(root)/'fixture';shutil.copytree(self.fixture(),source);existing=Path(root)/'existing';existing.mkdir()
            with self.assertRaisesRegex(ValueError,'FIXTURE_DESTINATION_EXISTS'):m.unpack_fixture(source,existing)
            blob=source/'responses.json.gz';saved=source/'saved';blob.rename(saved);blob.symlink_to(saved)
            with self.assertRaisesRegex(ValueError,'FIXTURE_COMPRESSED_BUDGET'):m.unpack_fixture(source,Path(root)/'target')


class PublicExecutionFixture(unittest.TestCase):
    def test_execution_fixture_keeps_exact_separate_anchor_and_extra_projection_scope(self):
        source=m.ROOT/'integration/test/source/current-v2-execution-public'
        with tempfile.TemporaryDirectory() as temporary:
            manifest=m.unpack_fixture(source,Path(temporary)/'raw')
        self.assertEqual(manifest['block']['number_or_slot'],26439090)
        self.assertEqual(manifest['record_count'],33)
        self.assertFalse(manifest['execution_extras']['full_http_envelope_retained'])
        extra=(source/manifest['execution_extras']['file']).read_bytes()
        self.assertEqual(m.sha(extra),manifest['execution_extras']['sha256'])
        self.assertEqual(len(json.loads(extra)),2)
        self.assertEqual(manifest['quote_request']['sellToken'],manifest['pool']['token1'])
    def test_changed_execution_extra_proofs_never_restore_or_relabel_base_fixture(self):
        import restore_current_v2_fixture as restore
        with tempfile.TemporaryDirectory() as temporary:
            bad=Path(temporary)/'extra.json';bad.write_text('[]')
            source=m.ROOT/'integration/test/source/current-v2-execution-public'
            with self.assertRaisesRegex(ValueError,'PINNED_EXTRA_PROOF_HASH'):
                restore.restore(bundle=source/'responses.json.gz',profile='execution',extra_proofs=bad)
        with self.assertRaisesRegex(ValueError,'FIXTURE_PROFILE'):
            restore.restore(bundle=source/'responses.json.gz',profile='unknown')

if __name__=='__main__':unittest.main()
