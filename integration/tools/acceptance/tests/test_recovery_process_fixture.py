import importlib.util
from pathlib import Path
import sys
import unittest
import urllib.request
import json
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
import receipt_witness_smoke as fixture

class RecoveryProcessFixtureTests(unittest.TestCase):
 def test_public_rpc_fixture_does_not_scan_missing_height_range(self):
  chain=fixture.VersionChain();chain.head=216100
  server=chain.provider()
  old=getattr(fixture,'range',None)
  def bounded_range(*args):
   if args and max(args)>1024:raise RuntimeError('fixture scanned the downtime height range')
   return range(*args)
  fixture.range=bounded_range
  def call(method,params):
   payload=json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params}).encode()
   with urllib.request.urlopen(urllib.request.Request(f'http://127.0.0.1:{server.server_port}',data=payload,headers={'Content-Type':'application/json'}),timeout=3)as response:return json.load(response)
  try:
   block=call('eth_getBlockByNumber',['latest',False])['result']
   receipt=call('eth_getTransactionReceipt',[block['transactions'][0]])['result']
   self.assertEqual(receipt['blockHash'],block['hash'])
   pool,_=fixture.profile('uniswap-v2')
   result=call('eth_call',[{'to':pool['id'],'data':'0x0902f1ac'},{'blockHash':block['hash'],'requireCanonical':True}])
   self.assertIn('result',result)
  finally:
   if old is None:del fixture.range
   else:fixture.range=old
   server.shutdown();server.server_close()

if __name__=='__main__':unittest.main()
