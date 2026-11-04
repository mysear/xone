"""Replay captured original V2 complete-state banks, never invent missing slots."""
from pathlib import Path
from functools import lru_cache
import copy,hashlib,json,threading
from evm_execution_smoke import NativeBank,require
ROOT=Path(__file__).resolve().parents[3]
FIXTURE=ROOT/'integration/test/contracts/v2-complete.json'
@lru_cache(maxsize=1)
def corpus():return json.loads(FIXTURE.read_text())
class NativeCompleteBank(NativeBank):
 def __init__(self):
  self.document=corpus();self.lock=threading.RLock();self.reads={};self.fault=None
  self.genesis=self.document['reference']['genesis_header'];self.headers={self.genesis['hash']:self.genesis};self.receipts={};self.codes={};self.slots={};self.calls={}
  records=self.document['reference']['transactions']+[c['evidence'] for c in self.document['cases']]
  def put(mapping,key,value):
   require(key not in mapping or mapping[key]==value,'contradictory captured native source');mapping[key]=value
  for row in records:
   for phase,header_key,account_key,getter_key in [('before','preblock_header','accounts','pre_getters'),('after','execution_block_header','post_accounts','post_getters')]:
    header=row[header_key];digest=header['hash'];put(self.headers,digest,header)
    codes=self.codes.setdefault(digest,{});slots=self.slots.setdefault(digest,{});calls=self.calls.setdefault(digest,{})
    for account in row[account_key]:
     put(codes,account['address'],account['code'])
     for entry in account['storage']:put(slots,(account['address'],int(entry['slot'],16)),entry['value'])
    for bank in row[getter_key]:
     require(bank['block_hash']==digest and bank['block_number']==int(header['number'],16),'native getter bank identity differs')
     for call in bank['calls']:
      require(call['block_hash']==digest and call['block_number']==bank['block_number'],'native call not in recorded bank')
      put(calls,(call['to'],call['data']),call['result'])
     observed=bank['lock_observation'];put(slots,(observed['address'],int(observed['slot'],16)),observed['value'])
   receipt=row['receipt'];put(self.receipts,receipt['transactionHash'],receipt)
  identities=self.document['reference']['runtime_identities'];routers=[r for r in identities if r['artifact'].endswith('/UniswapV2Router02.json')]
  require(len(routers)==1,'original Router identity missing');self.router=routers[0]
  self.select_case(0)
 def select_case(self,index,fault=None):
  with self.lock:
   self.case=copy.deepcopy(self.document['cases'][index]);self.number=self.case['state_anchor']['block_number'];self.timestamp=self.case['state_anchor']['timestamp'];self.header=self.case['evidence']['preblock_header'];self.accounts={a['address']:a for a in self.case['accounts']};self.fault=fault
   require(self.header['hash']==self.case['state_anchor']['block_hash'],'selected native bank differs')
   p=self.case['pool'];self.case['executor']=dict(target=self.router['address'],code_sha256=self.router['runtime_code_sha256'],factory=p['v2']['factory'],factory_code_sha256=p['v2']['swap']['factory_code_sha256'],adapter_version='uniswap-v2-router02/1.1.0-beta.0')
   # Process request expectations originate in the original transaction result.
   self.case['quote_context']={'quoted_buy_amount':self.case['expected']['amount_out']}
 def set_fault(self,fault):
  require(fault in [None,'pool-code','factory-getter','factory-code','lock'],'unknown complete-bank fault')
  with self.lock:self.fault=fault
 def bundle(self):
  c=self.case;known=[{'number':self.number,'hash':self.header['hash']}]
  if self.number:known.append({'number':self.number-1,'hash':self.header['parentHash']})
  fixture={key:copy.deepcopy(c[key]) for key in ['schema_version','state_anchor','execution_environment','accounts']};fixture['block_hashes']=known
  return dict(schema_version=1,fixture=fixture,executor=copy.deepcopy(c['executor']),transaction_context=copy.deepcopy(c['transaction_context']))
 def ancestor(self,number):
  require(0<=number<=self.number,'unrecorded future native block')
  if number==0:return self.genesis
  header=self.header
  while int(header['number'],16)>number:
   require(header['parentHash'] in self.headers,'native ancestor header unavailable');header=self.headers[header['parentHash']]
  require(int(header['number'],16)==number,'native ancestor height missing');return header
 def rpc(self,method,args):
  with self.lock:
   self.reads[method]=self.reads.get(method,0)+1
   if method=='eth_chainId':return hex(self.case['execution_environment']['chain_id'])
   if method=='eth_getBlockByNumber':
    return copy.deepcopy(self.header if args[0] in ['latest','safe','finalized'] else self.ancestor(int(args[0],16)))
   if method=='eth_getTransactionReceipt':
    require(args[0] in self.receipts,'unknown native transaction');receipt=self.receipts[args[0]];require(int(receipt['blockNumber'],16)<=self.number,'transaction beyond selected source bank');return copy.deepcopy(receipt)
   if method=='eth_getBlockReceipts':
    h=self.ancestor(int(args[0],16));return [copy.deepcopy(self.receipts[t]) for t in h['transactions']]
   position=2 if method=='eth_getStorageAt' else 1
   require(method in ['eth_call','eth_getCode','eth_getStorageAt'] and len(args)>position,'unsupported native source method')
   anchor=args[position];require(type(anchor)is dict and set(anchor)=={'blockHash','requireCanonical'} and anchor['requireCanonical']is True,'exact EIP1898 source required')
   digest=anchor['blockHash'];require(digest in self.headers,'unknown native block hash');height=int(self.headers[digest]['number'],16);require(self.ancestor(height)['hash']==digest,'noncanonical selected native branch')
   p=self.case['pool'];active=digest==self.header['hash']
   if method=='eth_getCode':
    if active and ((self.fault=='pool-code'and args[0]==p['pool_id'])or(self.fault=='factory-code'and args[0]==p['v2']['factory'])):return '0x00'
    require(args[0] in self.codes.get(digest,{}),'undeclared native account code');return self.codes[digest][args[0]]
   if method=='eth_getStorageAt':
    key=(args[0],int(args[1],16))
    if active and self.fault=='lock'and key==(p['pool_id'],12):return '0x'+'00'*32
    require(key in self.slots.get(digest,{}),'undeclared native storage slot');return self.slots[digest][key]
   target=args[0];key=(target['to'],target['data'])
   if active and self.fault=='factory-getter'and key==(p['pool_id'],'0xc45a0155'):return '0x'+'00'*32
   require(key in self.calls.get(digest,{}),'unrecorded original native getter');return self.calls[digest][key]
