'use strict';
// New oracle inputs; no service source imports and no copied quote outputs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'), ref=path.join(root,'integration/tools/reference/singleton-advanced');
const receiptPath=path.join(root,'integration/artifacts/local/singleton-advanced-generation.json');fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const runtime=path.join(root,'integration/.runtime/singleton-reference'),mods=path.join(runtime,'node_modules');
const solc=require(path.join(mods,'solc')),ganache=require(path.join(mods,'ganache')),{ethers}=require(path.join(mods,'ethers'));
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const manifestBytes=fs.readFileSync(path.join(ref,'../singleton-cl/source-metadata.json')),manifest=JSON.parse(manifestBytes);
for(const f of manifest.files) assert.equal(sha(fs.readFileSync(path.join(runtime,'sources',f.local))),f.sha256,f.local);
for(const [name,version] of [['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']])assert.equal(require(path.join(mods,name,'package.json')).version,version);
const oracleFiles=['integration/tools/ci/singleton-advanced-golden.cjs','integration/tools/reference/singleton-cl/source-metadata.json',...['ReferenceTypes.sol','V4AdvancedReference.sol','InfinityAdvancedReference.sol','InfinityBinReference.sol'].map(n=>'integration/tools/reference/singleton-advanced/'+n)];
const oracleInputs=Object.fromEntries(oracleFiles.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
const sources={},closure=new Set();
for(const name of ['ReferenceTypes.sol','V4AdvancedReference.sol','InfinityAdvancedReference.sol','InfinityBinReference.sol'])sources[name]={content:fs.readFileSync(path.join(ref,name),'utf8')};
const input={language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',metadata:{bytecodeHash:'none',appendCBOR:false},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode.object']}}}};
const out=JSON.parse(solc.compile(JSON.stringify(input),{import:name=>{const f=manifest.files.find(f=>f.local===name);if(!f)return {error:'Unpinned '+name};closure.add(name);return {contents:fs.readFileSync(path.join(runtime,'sources',name),'utf8')};}}));
assert.deepEqual((out.errors||[]).filter(e=>e.severity==='error'),[]);
async function main(){
 const raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337},wallet:{deterministic:true},miner:{blockGasLimit:100000000}});
 try{
 const provider=new ethers.providers.Web3Provider(raw),signer=provider.getSigner();
 const cases=[],capabilities=[],builds={};
 for(const [protocol,name,managerHint,max]of [['uniswap-v4','V4AdvancedReference','0x0303030303030303030303030303030303030303',1000],['pancake-infinity-cl','InfinityAdvancedReference','0x0404040404040404040404040404040404040404',4000]]){
  const v4=protocol==='uniswap-v4',artifact=out.contracts[name+'.sol'][name];
  const contract=await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer).deploy();await contract.deployed();
  const manager=contract.address.toLowerCase();
  builds[protocol]={runtime_sha256:sha(Buffer.from((await provider.getCode(contract.address)).slice(2),'hex'))};
  // A real registration getter and reverting callback fallback prove the accepted dispatch boundary.
  const hook='0x0000000000000000000000000000000000010000';
  await provider.send('evm_setAccountCode',[hook,'0x'+out.contracts['ReferenceTypes.sol'].SnapshotOnlyHook.evm.deployedBytecode.object]);
  const currency1='0x0202020202020202020202020202020202020202';
  for(const native of [false,true])for(const dynamic of [false,true])for(const lp of [0,1,3000,999999,1000000])for(const zero of [false,true]){
   const currency0=native?ethers.constants.AddressZero:'0x0101010101010101010101010101010101010101',keyFee=dynamic?0x800000:lp;
   const hooks=dynamic?hook:ethers.constants.AddressZero,spacing=60,parameters=v4?'':'0x'+(BigInt(spacing)<<16n).toString(16).padStart(64,'0');
   const key=v4?[currency0,currency1,keyFee,spacing,hooks]:[currency0,currency1,hooks,manager,keyFee,parameters];
   assert.equal(await contract.callStatic.dispatch(key,'0x'),true);
   const id=await contract.keyId(key),p0=max,p1=17;
   const ticks=[[-120,1000000000000n,1000000000000n],[-60,500000000000n,500000000000n],[60,500000000000n,-500000000000n],[120,1000000000000n,-1000000000000n]];
   const words=[{word_pos:-1,bitmap:((1n<<254n)|(1n<<255n)).toString()},{word_pos:0,bitmap:'6'}];
   const core={sqrt_price_x96:(await contract.priceAtTick(0)).toString(),tick:0,liquidity:'1500000000000',tick_spacing:spacing,max_steps:512,tick_words:words,ticks:ticks.map(([tick,gross,net])=>({tick,liquidity_gross:String(gross),liquidity_net:String(net)}))};
   const snapshot={protocol,currency0,currency1,key_fee:keyFee,snapshot_block_hash:'0x'+'ab'.repeat(32),snapshot:{manager,manager_code_sha256:builds[protocol].runtime_sha256,pool_id:id,lp_fee:lp,protocol_fee0:p0,protocol_fee1:p1,hooks,parameters,core}};
   for(const amount of ['1','1000000','6000000000']){
    const seed=[core.sqrt_price_x96,0,core.liquidity,spacing,lp,p0+(p1<<12),words.map(w=>[w.word_pos,w.bitmap]),ticks.map(t=>t.map(String))];
    const limit=await contract.priceAtTick(zero?-120:120);
    const q=await contract.callStatic.quoteAdvanced(seed,amount,zero,limit,{gasLimit:80000000});
    const expected={amount_out:q.amountOut.toString(),consumed_in:q.consumedIn.toString(),remaining_in:q.remainingIn.toString(),sqrt_price_x96:q.sqrtPriceX96.toString(),tick:q.tick,liquidity:q.liquidity.toString(),swap_fee:q.swapFee,protocol_amount:q.protocolAmount.toString()};
    cases.push({name:[protocol,native,dynamic,lp,zero,amount].join('/'),snapshot,input:amount,zero_for_one:zero,expected});
   }
  }
  for(const permission of (v4?[0x80,0x40,0x08,0x04]:[0x40,0x80,0x400,0x800])){
   const hooks=v4?'0x'+(0x10000+permission).toString(16).padStart(40,'0'):hook;
   const parameters=v4?'':'0x'+((60n<<16n)|BigInt(permission)).toString(16).padStart(64,'0');
   const key=v4?[ethers.constants.AddressZero,currency1,0x800000,60,hooks]:[ethers.constants.AddressZero,currency1,hooks,manager,0x800000,parameters];
   await assert.rejects(contract.callStatic.dispatch(key,'0x'));
   capabilities.push({protocol,permission,rejected:true});
  }
 }
 const binArtifact=out.contracts['InfinityBinReference.sol'].InfinityBinReference;
 const binContract=await new ethers.ContractFactory(binArtifact.abi,binArtifact.evm.bytecode.object,signer).deploy();await binContract.deployed();
 const binCases=[];
 for(const offset of [-100,-1,0,1,100])for(const step of [1,25])for(const lp of [0,3000,100000])for(const zero of [false,true])for(const amount of ['1','1000000','1000000000000000']){
  const state={active_id:(1<<23)+offset,bin_step:step,reserve0:'1000000000',reserve1:'2000000000',lp_fee:lp,protocol_fee0:4000,protocol_fee1:17};
  let expected;
  try{const q=await binContract.callStatic.quote([state.active_id,step,state.reserve0,state.reserve1,lp,4000+(17<<12)],amount,zero,{gasLimit:80000000});
   expected={amount_out:q.amountOut.toString(),protocol_amount:q.protocolAmount.toString(),swap_fee:q.swapFee,reserve0:q.reserve0.toString(),reserve1:q.reserve1.toString(),price_x128:q.priceX128.toString()};
  }catch(e){assert.ok(e.error?.data||e.data,'missing source revert');expected={reverted:true};}
  binCases.push({name:[offset,step,lp,zero,amount].join('/'),state,input:amount,zero_for_one:zero,expected});
 }
 const binBytes=JSON.stringify({schema:'infinity-single-bin/1',cases:binCases},null,2)+'\n';
 for(const base of ['integration','dex-router'])fs.writeFileSync(path.join(root,base,'test/contracts/singleton-advanced-bin-quotes.json'),binBytes);
 const fixture={schema:'singleton-advanced-cl/1',reference:{compiler:solc.version(),ganache:'7.9.2',manager_runtime_kind:'library-harness',native_currency_evidence:'original PoolId key and currency-denominated core quote; settlement not executed',source_manifest_sha256:sha(manifestBytes),compile_input_sha256:sha(JSON.stringify(input)),builds,capabilities},cases};
 const bytes=JSON.stringify(fixture,null,2)+'\n';
 for(const base of ['integration','dex-router','chain-parser-go','chain-parser-rust'])fs.writeFileSync(path.join(root,base,'test/contracts/singleton-advanced-quotes.json'),bytes);
 const pins={schema:1,upstream_manifest_sha256:sha(manifestBytes),compiler:solc.version(),files:[...closure].sort().map(name=>manifest.files.find(f=>f.local===name)),harnesses:Object.entries(sources).map(([name,s])=>({name,sha256:sha(s.content)})),fixture_sha256:sha(bytes),bin_fixture_sha256:sha(binBytes)};
 fs.writeFileSync(path.join(ref,'source-metadata.json'),JSON.stringify(pins,null,2)+'\n');
 if(oracleFiles.some(p=>sha(fs.readFileSync(path.join(root,p)))!==oracleInputs[p]))throw Error('oracle input changed during execution');
 fs.writeFileSync(receiptPath,JSON.stringify({passed:true,oracle_inputs:oracleInputs,source_metadata_sha256:sha(JSON.stringify(pins,null,2)+'\n'),cases:cases.length,bin_cases:binCases.length,fixture_sha256:sha(bytes),bin_fixture_sha256:sha(binBytes)},null,2)+'\n');
 console.log(JSON.stringify({cases:cases.length,bin_cases:binCases.length,capabilities:capabilities.length,fixture_sha256:sha(bytes),source_files:closure.size}));
 }finally{await raw.disconnect();}
}
main().catch(e=>{fs.writeFileSync(receiptPath,JSON.stringify({passed:false,error:String(e)})+'\n');console.error(e);process.exitCode=1;});
