'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service');
const receiptPath=path.join(ref,'guard-node-receipt.json');fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const requireThat=(ok,m)=>{if(!ok)throw Error(m);};
function main(){
 const {verify,validateDocument,validateBindings}=require('./infinity-bin-service-check.cjs');
 const f=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/infinity-bin-service.json'))),execution=JSON.parse(fs.readFileSync(path.join(ref,'execution-receipt.json'))),cases=[];
 verify();
 function negative(name,edit,fn=validateDocument,source=f,pattern){const changed=structuredClone(source);edit(changed);let error;try{fn(changed);}catch(e){error=String(e);}requireThat(error,'negative accepted: '+name);if(pattern)requireThat(error.includes(pattern),'wrong failure for '+name+': '+error);cases.push({name,rejected:true,error});}
 const M=f.manager,V=f.vault,T=f.tokens.map(t=>t.address),p=f.pools[0],word=n=>'0x'+BigInt(n).toString(16).padStart(64,'0');
 const managerOffset=1921*2+2;
 negative('forged manager code',d=>{const c=d.RPCbanks[0].code[M];d.RPCbanks[0].code[M]='0x00'+c.slice(4);});
 negative('wrong immutable Vault',d=>{let c=d.RPCbanks[0].code[M];d.RPCbanks[0].code[M]=c.slice(0,managerOffset)+word(1).slice(2)+c.slice(managerOffset+64);},undefined,undefined,'immutable Vault');
 negative('forged Vault runtime',d=>{d.RPCbanks[0].code[V]='0x00'+d.RPCbanks[0].code[V].slice(4);});
 negative('unlimited deployment claim',d=>{d.reference.unlimited_contract_size=true;},undefined,undefined,'EIP170');
 negative('unverified settlement claim',d=>{d.reference.scope.full_manager_swap_settlement_verified=true;},undefined,undefined,'scope');
 negative('immutable metadata offsets',d=>{Object.values(d.reference.immutable_references)[0][0].start++;},undefined,undefined,'immutable reference');
 negative('compiler version',d=>{d.reference.compiler='0.8.25';},undefined,undefined,'compiler');
 negative('wrong pool mapping base',d=>{d.pools[0].pool_base=word(5);},undefined,undefined,'mapping base');
 negative('zero ERC20',d=>{d.tokens[0].address='0x'+'0'.repeat(40);},undefined,undefined,'ERC20');
 negative('unsorted ERC20',d=>{d.tokens.reverse();},undefined,undefined,'ERC20');
 negative('duplicate ERC20',d=>{d.tokens[1].address=d.tokens[0].address;},undefined,undefined,'ERC20');
 negative('hooks nonzero',d=>{d.pools[0].key.hooks=M;},undefined,undefined,'static profile');
 negative('dynamic LP fee flag',d=>{d.pools[0].lp_fee=8388608;},undefined,undefined,'static fee');
 negative('LP fee above cap',d=>{d.pools[0].lp_fee=100001;},undefined,undefined,'static fee');
 negative('bin step below minimum',d=>{d.pools[0].bin_step=0;},undefined,undefined,'bin step');
 negative('bin step above maximum',d=>{d.pools[0].bin_step=101;},undefined,undefined,'bin step');
 negative('protocol direction0 overflow',d=>{d.pools[0].protocol_fee0=4001;},undefined,undefined,'protocol per direction');
 negative('protocol direction1 overflow',d=>{d.pools[0].protocol_fee1=4001;},undefined,undefined,'protocol per direction');
 negative('unregistered app',d=>{const s=Object.keys(d.RPCbanks[0].storage[V]).find(k=>d.RPCbanks[0].storage[V][k]===word(1));requireThat(s,'app slot');d.RPCbanks[0].storage[V][s]=word(0);},undefined,undefined,'registered original app');
 negative('app reserve getter divergence',d=>{d.RPCbanks[0].app_reserves[T[0]]='1';},undefined,undefined,'reserve metadata');
 negative('crosspool global reserve divergence',d=>{d.RPCbanks[2].pools[1].app_reserves[T[0]]='1';},undefined,undefined,'shared by all pools');
 negative('crosspool protocol divergence',d=>{d.RPCbanks[2].pools[1].protocol_fees[T[0]]='1';},undefined,undefined,'shared by all pools');
 negative('missing root tree',d=>{d.RPCbanks[0].pools[0].tree.level0=word(0);},undefined,undefined,'root tree');
 negative('missing L1 branch',d=>{delete d.RPCbanks[0].pools[0].tree.level1[Object.keys(d.RPCbanks[0].pools[0].tree.level1)[0]];},undefined,undefined,'complete level1');
 negative('missing L2 branch',d=>{delete d.RPCbanks[0].pools[0].tree.level2[Object.keys(d.RPCbanks[0].pools[0].tree.level2)[0]];},undefined,undefined,'complete level2');
 negative('missing depleted initialized entry',d=>{delete d.RPCbanks[0].pools[0].bins['8388607'];},undefined,undefined,'complete initialized bins');
 negative('depleted shares erased',d=>{d.RPCbanks[0].pools[0].bins['8388607'].total_shares='0';},undefined,undefined,'retain shares');
 negative('reserve128 overflow',d=>{d.RPCbanks[0].pools[0].bins['8388608'].reserve0=(1n<<128n).toString();},undefined,undefined,'reserve128');
 negative('packed slot mismatch',d=>{d.RPCbanks[0].storage[M][p.pool_base]=word(1);},undefined,undefined,'packed slot0');
 negative('missing extSload evidence',d=>{delete d.RPCbanks[0].callmap[M][Object.keys(d.RPCbanks[0].callmap[M]).find(k=>k.startsWith('0x1e2eaeaf'))];},undefined,undefined,'missing RPC');
 negative('block order',d=>{d.RPCbanks[1].block.num=d.RPCbanks[0].block.num;},undefined,undefined,'block identity');
 negative('invalid block hash',d=>{d.RPCbanks[0].block.hash=word(0);},undefined,undefined,'block identity');
 negative('trade does not consume previous native output',d=>{d.trades[1].amount_in='1';},undefined,undefined,'chained exact input');
 negative('native output tamper',d=>{d.trades[0].expected.amount_out='1';},undefined,undefined,'native deltas');
 negative('native fee cap imposed incorrectly',d=>{d.trades[1].expected.swap_fee=100000;},undefined,undefined,'original swap fee');
 negative('native protocol aggregate tamper',d=>{d.trades[0].expected.bin_fills[0].protocol_amount='0';});
 negative('native full poststate missing bin',d=>{delete d.trades[0].expected.post.bins['8388607'];},undefined,undefined,'complete native poststate');
 negative('native fill price tamper',d=>{d.trades[0].expected.bin_fills[0].price='1';},undefined,undefined,'original fill price');
 negative('native fill reserve tamper',d=>{d.trades[0].expected.bin_fills[0].post.reserve0='1';},undefined,undefined,'native fill reserves');
 negative('native depleted traversal removed',d=>{d.trades[0].expected.bin_fills.splice(1,1);},undefined,undefined,'tree traversal');
 for(const field of ['passed','fixture_sha256','source_metadata_sha256','compile_evidence_sha256'])negative('execution receipt '+field,d=>{d[field]=field==='passed'?false:'0'.repeat(64);},r=>verify({receipt:r}),execution);
 negative('generator input receipt altered',d=>{d.inputs[Object.keys(d.inputs)[0]]='0'.repeat(64);},r=>verify({receipt:r}),execution,'input closure');
 for(const [name,edit,pattern]of [
  ['original source hash',r=>{r.files[0].sha256='0'.repeat(64);},'source manifest'],
  ['source closure omitted',r=>{r.files=[];},'source closure'],
  ['dependency lock',r=>{r.dependency_lock_sha256='0'.repeat(64);},'package lock'],
  ['compiler package',r=>{r.dependencies.solc.package_sha256='0'.repeat(64);},'package binding'],
  ['loaded dependency',r=>{r.loaded_dependencies[0].sha256='0'.repeat(64);},'loaded dependency changed'],
  ['loaded dependency closure omitted',r=>{r.loaded_dependencies=[];},'loaded dependency + binary'],
  ['compiler binary',r=>{r.binary_dependencies[0].sha256='0'.repeat(64);},'loaded dependency changed'],
  ['node binary',r=>{r.binary_dependencies[1].sha256='0'.repeat(64);},'loaded dependency changed'],
  ['manager artifact',r=>{r.artifacts.BinPoolManager.sha256='0'.repeat(64);},'artifact binding'],
  ['Vault artifact',r=>{r.artifacts.Vault.sha256='0'.repeat(64);},'artifact binding']
 ])negative(name,edit,validateBindings,f.reference,pattern);
 return {passed:true,negative_cases:cases.length,cases,inputs:Object.fromEntries(['infinity-bin-service-check.cjs','infinity-bin-service-guards.cjs'].map(n=>['integration/tools/ci/'+n,sha(fs.readFileSync(path.join(__dirname,n)))])),execution_receipt_sha256:sha(fs.readFileSync(path.join(ref,'execution-receipt.json')))};
}
try{const result=main();fs.writeFileSync(receiptPath,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({passed:true,negative_cases:result.negative_cases}));}catch(e){fs.writeFileSync(receiptPath,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}
