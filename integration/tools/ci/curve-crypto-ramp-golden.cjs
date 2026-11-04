'use strict';
// Receipt invalidation MUST precede dependency loading, pin parsing and compilation.
const fs=require('fs'),path=require('path'),cp=require('child_process'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/curve-crypto-full');
const outputRef=path.join(root,'integration/tools/reference/curve-crypto-ramp');
fs.mkdirSync(outputRef,{recursive:true});
const receiptPath=path.join(outputRef,'generation.json');
function atomic(file,data){const tmp=file+'.tmp-'+process.pid;fs.writeFileSync(tmp,data);fs.renameSync(tmp,file);}
atomic(receiptPath,JSON.stringify({valid:false,status:'invalidated-before-dependencies'},null,2)+'\n');
let owned;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const E=10n**18n,MAX=(1n<<256n)-1n;
const keys=['D','price_scale','_price_oracle','last_prices','last_prices_timestamp','xcp_profit','xcp_profit_a','virtual_price','not_adjusted','future_A_gamma_time'];
async function main(){
 const inventory=()=>JSON.parse(cp.execFileSync('python3',['-I','-S','-B',path.join(root,'integration/tools/ci/curve-crypto-ramp-inventory.py')],{encoding:'utf8',maxBuffer:2**24}));
 const originalInputs=inventory();
 const compilerEnv={...process.env,PYTHONDONTWRITEBYTECODE:'1',PYTHONNOUSERSITE:'1',PYTHONPATH:'',PYTHONOPTIMIZE:'0',SETUPTOOLS_USE_DISTUTILS:'stdlib'};delete compilerEnv.PYTHONHOME;
 const mods=path.join(root,'integration/.runtime/stable-reference/node_modules');
 const {ethers}=require(path.join(mods,'ethers')),ganache=require(path.join(mods,'ganache')),solc=require(path.join(mods,'solc'));
 const pins=JSON.parse(fs.readFileSync(path.join(ref,'sources.json'),'utf8'));
 for(const p of pins){if(sha(fs.readFileSync(path.resolve(ref,p.local)))!==p.sha256)throw Error('source pin mismatch '+p.local);}
 const compile=(compiler,src,version)=>{
  const observed=cp.execFileSync(compiler,['--version'],{encoding:'utf8',env:compilerEnv}).trim();if(observed!==version&&!observed.startsWith(version+'+'))throw Error('compiler mismatch '+observed);
  const out=cp.execFileSync(compiler,['-f','bytecode,bytecode_runtime,abi,layout',src],{encoding:'utf8',maxBuffer:2**24,env:compilerEnv}).trim().split('\n');
  return {bytecode:out[0],runtime:out[1],abi:JSON.parse(out[2]),layout:JSON.parse(out[3]),compiler:observed,compiler_path:compiler,source_path:src,compiler_arguments:['-f','bytecode,bytecode_runtime,abi,layout',src],compiler_executable_sha256:sha(fs.readFileSync(compiler)),python_dependencies:cp.execFileSync(path.join(path.dirname(compiler),'python'),['-m','pip','freeze'],{encoding:'utf8',env:compilerEnv}).trim().split('\n')};
 };
 const poolSource=path.join(ref,'CurveCryptoSwap2.vy');
 const pool=compile(path.join(root,'integration/tools/reference/curve-models/.runtime/vyper-0.3.3/bin/vyper'),poolSource,'0.3.3');
 const lp=compile(path.join(ref,'.runtime/vyper-0.3.1/bin/vyper'),path.join(ref,'CurveTokenV4.vy'),'0.3.1');
 const settings={optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode.object']}}};
 const si={language:'Solidity',sources:{'Tokens.sol':{content:fs.readFileSync(path.join(ref,'Tokens.sol'),'utf8')}},settings};
 const so=JSON.parse(solc.compile(JSON.stringify(si)));if((so.errors||[]).some(x=>x.severity==='error'))throw Error(JSON.stringify(so.errors));
 const t=so.contracts['Tokens.sol'].Tokens,ta={abi:t.abi,bytecode:'0x'+t.evm.bytecode.object};
 // Default EIP-170 limit enforced, including the deployed immutable data tail.
 owned=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:false,time:new Date('2026-10-01T00:00:00Z')},miner:{timestampIncrement:0},wallet:{deterministic:true}});
 const provider=new ethers.providers.Web3Provider(owned);provider.pollingInterval=20;
 const signer=provider.getSigner(0),owner=await signer.getAddress(),receiver=await provider.getSigner(1).getAddress();
 const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32);
 const deploy=async(a,args=[])=>{const c=await new ethers.ContractFactory(a.abi,a.bytecode,signer).deploy(...args,{gasLimit:25_000_000});await c.deployed();return c;};
 const lay=pool.layout.storage_layout??pool.layout;
 const slot=async(c,s,v)=>provider.send('evm_setAccountStorageAt',[c.address,word(BigInt(s)),word(BigInt(v))]);
 const read=async(c,s)=>BigInt((await provider.getStorageAt(c.address,Number(s)))||'0x0');
 const storage=async(c,k)=>{const x=await provider.getStorageAt(c.address,Number(lay[k].slot));return BigInt(x==='0x'?'0':x);};
 const metadata={schema_version:1,reference_only:true,source_pins:pins,pool_compiler:pool.compiler,lp_compiler:lp.compiler,pool_compiler_dependencies:pool.python_dependencies,lp_compiler_dependencies:lp.python_dependencies,compiler_invocations:[pool,lp].map(a=>({compiler:a.compiler_path,args:a.compiler_arguments,source:a.source_path,launcher_sha256:a.compiler_executable_sha256})),solc:solc.version(),dependency_versions:Object.fromEntries(['ethers','ganache','solc'].map(n=>[n,require(path.join(mods,n,'package.json')).version])),token_compiler_settings:settings,pool_layout:pool.layout,lp_layout:lp.layout,pool_template_runtime_bytes:(pool.runtime.length-2)/2,pool_template_runtime_sha256:sha(Buffer.from(pool.runtime.slice(2),'hex')),pool_initcode_sha256:sha(Buffer.from(pool.bytecode.slice(2),'hex')),lp_template_runtime_bytes:(lp.runtime.length-2)/2,lp_template_runtime_sha256:sha(Buffer.from(lp.runtime.slice(2),'hex')),eip170_limit:24576,allow_unlimited_contract_size:false,deployments:[]};
 atomic(path.join(outputRef,'runtime-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
 
 const transaction=r=>({hash:r.transactionHash,block_number:r.blockNumber,status:r.status,gas_used:String(r.gasUsed)});
 const cases=[];
 let chainBase=await provider.send('evm_snapshot',[]);
 const immutableLayout=[['token',20814],['coins[0]',20846],['coins[1]',20878],['PRECISIONS[0]',20910],['PRECISIONS[1]',20942]].map(([name,offset])=>({name,offset,bytes:32}));
 metadata.immutable_layout=immutableLayout;
 for(const decimals of [[18,18],[18,6]]){
  // Each precision pair is an independent branch of the same trusted local baseline.
  await provider.send('evm_revert',[chainBase]);chainBase=await provider.send('evm_snapshot',[]);
  const tokens=await Promise.all(decimals.map(d=>deploy(ta,[d]))),lpc=await deploy(lp,['Curve LP reference','CRVREF']);
  const params={amp:'400000',gamma:'145000000000000',mid_fee:'3000000',out_fee:'30000000',fee_gamma:'230000000000000',allowed_extra_profit:'1000000000000000000',adjustment_step:'146000000000000',admin_fee:'5000000000',ma_half_time:'600'};
  const poolArgs=[owner,receiver,params.amp,params.gamma,params.mid_fee,params.out_fee,params.allowed_extra_profit,params.fee_gamma,params.adjustment_step,params.admin_fee,params.ma_half_time,String(E),lpc.address,tokens.map(t=>t.address)];
  const c=await deploy(pool,poolArgs);await(await lpc.set_minter(c.address)).wait();
  const prec=decimals.map(d=>10n**BigInt(18-d)),balances=prec.map(p=>1000000n*E/p);
  for(let k=0;k<2;k++){await(await tokens[k].mint(owner,MAX/4n)).wait();await(await tokens[k].approve(c.address,ethers.constants.MaxUint256)).wait();}
  await(await c['add_liquidity(uint256[2],uint256,address)'](balances.map(String),0,owner,{gasLimit:6_000_000})).wait();
  const code=await provider.getCode(c.address),lpcode=await provider.getCode(lpc.address),tail=code.slice(2+pool.runtime.length-2);
  const words=[lpc.address,...tokens.map(t=>t.address),...prec.map(String)].map(x=>word(x).slice(2).toLowerCase()).join('');
  if(code.slice(0,pool.runtime.length).toLowerCase()!==pool.runtime.toLowerCase()||tail!==words)throw Error('immutable layout mismatch');
  if(lpcode.toLowerCase()!==lp.runtime.toLowerCase())throw Error('LP deployed runtime mismatch');
  if((await lpc.minter()).toLowerCase()!==c.address.toLowerCase())throw Error('LP minter mismatch');
  if((code.length-2)/2>24576)throw Error('EIP170 deployed runtime too large');
  metadata.deployments.push({decimals,pool:c.address,lp:lpc.address,coins:tokens.map(t=>t.address),pool_runtime_bytes:(code.length-2)/2,pool_runtime_sha256:sha(Buffer.from(code.slice(2),'hex')),lp_runtime_sha256:sha(Buffer.from(lpcode.slice(2),'hex')),immutable_tail_hex:tail,immutable_tail_bytes:tail.length/2,immutable_layout:immutableLayout,lp_minter:await lpc.minter(),constructor_arguments:poolArgs});
  atomic(path.join(outputRef,'runtime-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
  const snapshot=async()=>{const state={};for(const k of keys)state[k]=String(await storage(c,k));return {state,balances:await Promise.all([0,1].map(k=>c.balances(k).then(String))),total_supply:String(await lpc.totalSupply()),receiver_balance:String(await lpc.balanceOf(receiver)),raw_balances:await Promise.all(tokens.map(t=>t.balanceOf(c.address).then(String))),trader_balances:await Promise.all(tokens.map(t=>t.balanceOf(owner).then(String)))};};

  for(const direction of ['up','down']){
   const started=(await provider.getBlock('latest')).timestamp;
   const end=started+172800;
   const target=direction==='up'?{amp:'800000',gamma:'300000000000000'}:{amp:'200000',gamma:'100000000000000'};
   const schedule=await(await c.ramp_A_gamma(target.amp,target.gamma,end,{gasLimit:2_000_000})).wait();
   for(const [step,offset]of [1,43201,86401,172799,172800,172801].entries()){
    await provider.send('evm_setTime',[new Date((started+offset)*1000)]);await provider.send('evm_mine',[]);
    const timestamp=(await provider.getBlock('latest')).timestamp;
    const initial=await storage(c,'initial_A_gamma'),future=await storage(c,'future_A_gamma');
    const ramp={initial_amp:String(initial>>128n),initial_gamma:String(initial&((1n<<128n)-1n)),initial_time:Number(await storage(c,'initial_A_gamma_time'))};
    const before=await snapshot(),i=step%2,j=1-i,dx=100n*E/prec[i];
    const row={name:`ramp-${direction}-step${step}-${decimals.join('-')}`,sequence:direction+'-'+decimals.join('-'),step,operation:'exchange',schedule_transaction:transaction(schedule),precisions:prec.map(String),params:{...params,amp:String(future>>128n),gamma:String(future&((1n<<128n)-1n))},...before,admin_receiver_enabled:true,ramp,timestamp,input:String(dx),index_in:i,index_out:j};
    const tx=await c['exchange(uint256,uint256,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:8_000_000}),receipt=await tx.wait(),after=await snapshot();
    row.timestamp=(await provider.getBlock(receipt.blockNumber)).timestamp;
    const events=receipt.logs.filter(l=>l.address.toLowerCase()===c.address.toLowerCase()).map(l=>c.interface.parseLog(l)).map(e=>({name:e.name,args:e.args.filter(x=>typeof x!=='function').map(String)}));
    const exchange=events.find(e=>e.name==='TokenExchange');if(!exchange)throw Error('missing original exchange');
    row.expected={...after,transaction:transaction(receipt),amount_out:exchange.args[4],minted_admin:String(BigInt(after.total_supply)-BigInt(before.total_supply)),pool_events:events,lp_transfer_events:receipt.logs.filter(l=>l.address.toLowerCase()===lpc.address.toLowerCase()).map(l=>lpc.interface.parseLog(l)).filter(e=>e.name==='Transfer').map(e=>e.args.map(String))};
    cases.push(row);
   }
  }
  // stop_ramp writes equal start/end timestamps and packed current endpoints.
  const start=(await provider.getBlock('latest')).timestamp;
  const rampTx=await(await c.ramp_A_gamma('800000','300000000000000',start+172800,{gasLimit:2_000_000})).wait();
  await provider.send('evm_setTime',[new Date((start+86400)*1000)]);await provider.send('evm_mine',[]);
  const stopped=await(await c.stop_ramp_A_gamma({gasLimit:2_000_000})).wait();
  for(let step=0;step<2;step++){
   const timestamp=(await provider.getBlock('latest')).timestamp,initial=await storage(c,'initial_A_gamma'),future=await storage(c,'future_A_gamma');
   const ramp={initial_amp:String(initial>>128n),initial_gamma:String(initial&((1n<<128n)-1n)),initial_time:Number(await storage(c,'initial_A_gamma_time'))};
   const before=await snapshot(),i=step,j=1-i,dx=100n*E/prec[i];
   const row={name:`ramp-stop-step${step}-${decimals.join('-')}`,sequence:'stop-'+decimals.join('-'),step,operation:'exchange',schedule_transaction:transaction(rampTx),stop_transaction:transaction(stopped),precisions:prec.map(String),params:{...params,amp:String(future>>128n),gamma:String(future&((1n<<128n)-1n))},...before,admin_receiver_enabled:true,ramp,timestamp,input:String(dx),index_in:i,index_out:j};
   const receipt=await(await c['exchange(uint256,uint256,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:8_000_000})).wait(),after=await snapshot();
   const events=receipt.logs.filter(l=>l.address.toLowerCase()===c.address.toLowerCase()).map(l=>c.interface.parseLog(l)).map(e=>({name:e.name,args:e.args.filter(x=>typeof x!=='function').map(String)}));
   row.timestamp=(await provider.getBlock(receipt.blockNumber)).timestamp;
   row.expected={...after,transaction:transaction(receipt),amount_out:events.find(e=>e.name==='TokenExchange').args[4],minted_admin:String(BigInt(after.total_supply)-BigInt(before.total_supply)),pool_events:events,lp_transfer_events:receipt.logs.filter(l=>l.address.toLowerCase()===lpc.address.toLowerCase()).map(l=>lpc.interface.parseLog(l)).filter(e=>e.name==='Transfer').map(e=>e.args.map(String))};cases.push(row);
  }
 }
 // Re-enumerate the complete independent filesystem scope after execution.
 if(JSON.stringify(inventory())!==JSON.stringify(originalInputs))throw Error('original source/runtime changed during reference execution');
 metadata.input_inventory_kind='complete-fixed-node-and-compiler-filesystem-v1';
 metadata.input_inventory=originalInputs;
 atomic(path.join(outputRef,'runtime-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
 const text=JSON.stringify({schema_version:1,reference_only:true,profile:'original public ramp_A_gamma/stop_ramp_A_gamma and exchange on original deployed pool + V4 LP; EIP170 enforced; synthetic tokens',source_pins:pins,cases},null,2)+'\n';
 for(const project of ['integration','dex-router'])atomic(path.join(root,project,'test/contracts/curve-crypto-ramp-native.json'),text);
 atomic(receiptPath,JSON.stringify({valid:true,reference_only:true,cases:cases.length,reverts:0,corpus_sha256:sha(text),runtime_metadata_sha256:sha(fs.readFileSync(path.join(outputRef,'runtime-metadata.json'))),generator_sha256:sha(fs.readFileSync(__filename))},null,2)+'\n');
 console.log(JSON.stringify({cases:cases.length,sequences:6,public_admin_calls:true}));
}
main().then(async()=>{if(owned)await owned.disconnect();}).catch(async e=>{try{atomic(receiptPath,JSON.stringify({valid:false,status:'failed',error:String(e)},null,2)+'\n');}finally{if(owned)await owned.disconnect();}console.error(e);process.exitCode=1;});
