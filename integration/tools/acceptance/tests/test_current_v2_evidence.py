import base64
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
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

if __name__=='__main__':unittest.main()
