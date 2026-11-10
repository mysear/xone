// SPDX-License-Identifier: GPL-3.0-or-later
// Execute pinned upstream functions in a local EVM; no service math is imported.
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../..');
const reference = path.join(root, 'integration/tools/reference/velodrome-v2');
const modules = path.join(root, 'integration/.runtime/stable-reference/node_modules');
const solc = require(path.join(modules, 'solc'));
const ganache = require(path.join(modules, 'ganache'));
const {ethers} = require(path.join(modules, 'ethers'));
const source = fs.readFileSync(path.join(reference, 'Pool.sol'), 'utf8');
const metadata = JSON.parse(fs.readFileSync(path.join(reference, 'source-metadata.json')));
const sha = value => crypto.createHash('sha256').update(value).digest('hex');
if (metadata.commit !== 'b3065d8b6702b14b094f9f6046b752cc9f78c43b' || sha(source) !== metadata.sha256)
  throw Error('VELODROME_PINNED_SOURCE_CHANGED');
if (!solc.version().startsWith('0.8.19+')) throw Error('VELODROME_COMPILER_VERSION');
function extract(text, name) {
  const start = text.indexOf('function '+name+'(');
  if (start < 0) throw Error('MISSING_ORIGINAL_FUNCTION:'+name);
  let end = text.indexOf('{', start)+1, depth = 1;
  while (depth && end < text.length) {
    if (text[end] === '{') depth++;
    if (text[end] === '}') depth--;
    end++;
  }
  if (depth) throw Error('UNTERMINATED_ORIGINAL_FUNCTION');
  return text.slice(start, end);
}
const functions = ['_f','_d','_get_y','_getAmountOut','_k'].map(n => extract(source,n)).join('\n');
const harness = `// SPDX-License-Identifier: GPL-3.0-or-later
pragma solidity 0.8.19;
contract VeloReference {
 uint256 decimals0; uint256 decimals1; bool stable; address token0=address(1);
 ${functions}
 function quote(uint256[2] memory b,uint256[2] memory d,uint256 fee,uint256 input,bool zero,bool st)
 external returns(uint256 out,uint256[2] memory next) {
  decimals0=d[0];decimals1=d[1];stable=st;
  require(input>0 && fee<=300,'input');
  uint256 effective=input-input*fee/10000;
  out=_getAmountOut(effective,zero?address(1):address(2),b[0],b[1]);
  uint i=zero?0:1;uint j=1-i;require(out>0 && out<b[j],'output');
  next=b;next[i]+=effective;next[j]-=out;
  require(_k(next[0],next[1])>=_k(b[0],b[1]),'K');
 }
}`;
const compiled = JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources:{'Reference.sol':{content:harness}},settings:{optimizer:{enabled:true,runs:200},evmVersion:'paris',outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}})));
const errors = (compiled.errors||[]).filter(e=>e.severity==='error');
if (errors.length) throw Error(errors.map(e=>e.formattedMessage).join('\n'));
const artifact = compiled.contracts['Reference.sol'].VeloReference;
const transport = ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai'},wallet:{deterministic:true}});
const provider = new ethers.providers.Web3Provider(transport);
const E = 10n**18n, address = n=>'0x'+n.toString(16).padStart(40,'0');
function pool(st,b,d,fee) {
 const name='velodrome-v2-'+(st?'stable':'volatile');
 return {chain_key:'evm:10',pool_id:address(3),protocol:name,token0:address(1),token1:address(2),reserve0:String(b[0]),reserve1:String(b[1]),fee_numerator:fee,fee_denominator:10000,adapter_version:name+'/b3065d8b-1',token_behavior:'standard',stable:{code_sha256:'aa'.repeat(32),factory:address(4),factory_code_sha256:'bb'.repeat(32),rate0:String(d[0]),rate1:String(d[1]),amplification:'0',admin_fee:0,max_iterations:255}};
}
(async()=>{
 try {
  const contract = await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,provider.getSigner(0)).deploy();await contract.deployed();
  const cases=[];
  for(const st of [false,true])for(const d of [[E,E],[E,1000000n],[1000000n,E]])for(const imbalance of [1n,3n])for(const zero of [true,false])for(const fee of [0,4,300])for(const human of [1n,10000n,1000000000n]) {
   const b=[1000000n*d[0]*imbalance,1000000n*d[1]],input=human===1n?1n:human*d[zero?0:1]/1000n;
   const item={pool:pool(st,b,d,fee),input:String(input),zero_for_one:zero};
   try { const q=await contract.callStatic.quote(b.map(String),d.map(String),fee,String(input),zero,st);item.expected={amount_out:q.out.toString(),next_balances:q.next.map(String)}; }
   catch(e) {if(e.code!=='CALL_EXCEPTION')throw e;item.expected={reverted:true};}
   cases.push(item);
  }
  const aeroSource=fs.readFileSync(path.join(root,'integration/.runtime/stable-reference/sources/aero/Pool.sol'),'utf8');
  const aeroFunctions=['_f','_d','_get_y','_getAmountOut','_k'].map(n=>extract(aeroSource,n)).join('\n');
  const aeroHarness=harness.replace(functions,aeroFunctions);
  const aeroOutput=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources:{'Reference.sol':{content:aeroHarness}},settings:{optimizer:{enabled:true,runs:200},evmVersion:'paris',outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}})));
  if((aeroOutput.errors||[]).some(e=>e.severity==='error'))throw Error('AERODROME_CONTROL_COMPILE_FAILED');
  const aa=aeroOutput.contracts['Reference.sol'].VeloReference;
  const control=await new ethers.ContractFactory(aa.abi,aa.evm.bytecode.object,provider.getSigner(0)).deploy();await control.deployed();
  async function observed(c,b,d,input) {
   try {const q=await c.callStatic.quote(b.map(String),d.map(String),4,String(input),true,true);return {amount_out:q.out.toString(),next_balances:q.next.map(String)};}
   catch(e) {if(e.code!=='CALL_EXCEPTION')throw e;return {reverted:true};}
  }
  const roundingCases=[];
  for(const reserve of [100n,101n,1000n,1001n,10000n])for(const input of [1n,1000n,100000n,260000n,333333n,500000n]) {
   const b=[1000000n,reserve],d=[1000000n,E];
   roundingCases.push({pool:pool(true,b,d,4),input:String(input),zero_for_one:true,expected:await observed(contract,b,d,input),aerodrome_control:await observed(control,b,d,input)});
  }
  const sequences=[];
  for(const st of [false,true])for(const zero of [true,false])for(const reverse of [false,true]) {
   const d=[E,1000000n],initial=[1000000n*d[0],1000000n*d[1]],p=pool(st,initial,d,4),steps=[];
   let b=initial;
   const human=reverse?[30000n,10000n,7000n]:[7000n,10000n,30000n];
   for (const quantity of human) {
    const input=quantity*d[zero?0:1];
    const q=await contract.callStatic.quote(b.map(String),d.map(String),4,String(input),zero,st);
    steps.push({input:String(input),amount_out:q.out.toString(),next_balances:q.next.map(String)});b=q.next.map(v=>BigInt(v.toString()));
   }
   sequences.push({pool:p,zero_for_one:zero,steps});
  }
  const result={schema_version:1,reference:{...metadata,compiler:solc.version(),harness_sha256:sha(harness),compiled_bytecode_sha256:sha(Buffer.from(artifact.evm.bytecode.object,'hex')),authority:'original extracted Solidity functions executed locally; modeled fee transfer and invariant; not full Pool.swap or deployment qualification'},cases,sequences,rounding_cases:roundingCases,aerodrome_control:{commit:'1ba30815bba620f7e9faa34769ffd00c214c9b82',source_sha256:sha(aeroSource)}};
  const text=JSON.stringify(result,null,2)+'\n';
  for(const project of ['integration','dex-router','chain-parser-go'])fs.writeFileSync(path.join(root,project,'test/contracts/velodrome-v2-quotes.json'),text);
  fs.writeFileSync(path.join(reference,'Reference.sol'),harness);
  const localSources=path.join(root,'dex-router/test/fixtures/velodrome-v2');fs.mkdirSync(localSources,{recursive:true});
  fs.writeFileSync(path.join(localSources,'Pool.sol'),source);fs.writeFileSync(path.join(localSources,'Reference.sol'),harness);
  console.log(JSON.stringify({rounding_cases:roundingCases.length,rounding_differences:roundingCases.filter(c=>JSON.stringify(c.expected)!==JSON.stringify(c.aerodrome_control)).length,cases:cases.length,reverts:cases.filter(c=>c.expected.reverted).length,sequences:sequences.length,steps:sequences.reduce((n,s)=>n+s.steps.length,0),sha256:sha(text)}));
 } finally {await transport.disconnect();}
})().catch(error=>{console.error(error);process.exitCode=1;});
