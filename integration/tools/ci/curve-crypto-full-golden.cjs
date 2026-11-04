'use strict';
// Receipt invalidation MUST precede dependency loading, pin parsing and compilation.
const fs=require('fs'),path=require('path'),cp=require('child_process'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/curve-crypto-full');
const receiptPath=path.join(ref,'generation.json');
function atomic(file,data){const tmp=file+'.tmp-'+process.pid;fs.writeFileSync(tmp,data);fs.renameSync(tmp,file);}
atomic(receiptPath,JSON.stringify({valid:false,status:'invalidated-before-dependencies'},null,2)+'\n');
let owned;
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const E=10n**18n,MAX=(1n<<256n)-1n;
const keys=['D','price_scale','_price_oracle','last_prices','last_prices_timestamp','xcp_profit','xcp_profit_a','virtual_price','not_adjusted','future_A_gamma_time'];
async function main(){
 const mods=path.join(root,'integration/.runtime/stable-reference/node_modules');
 const {ethers}=require(path.join(mods,'ethers')),ganache=require(path.join(mods,'ganache')),solc=require(path.join(mods,'solc'));
 const pins=JSON.parse(fs.readFileSync(path.join(ref,'sources.json'),'utf8'));
 for(const p of pins){if(sha(fs.readFileSync(path.resolve(ref,p.local)))!==p.sha256)throw Error('source pin mismatch '+p.local);}
 const compile=(compiler,src,version)=>{
  const observed=cp.execFileSync(compiler,['--version'],{encoding:'utf8'}).trim();if(observed!==version&&!observed.startsWith(version+'+'))throw Error('compiler mismatch '+observed);
  const out=cp.execFileSync(compiler,['-f','bytecode,bytecode_runtime,abi,layout',src],{encoding:'utf8',maxBuffer:2**24}).trim().split('\n');
  return {bytecode:out[0],runtime:out[1],abi:JSON.parse(out[2]),layout:JSON.parse(out[3]),compiler:observed,compiler_path:compiler,source_path:src,compiler_arguments:['-f','bytecode,bytecode_runtime,abi,layout',src],compiler_executable_sha256:sha(fs.readFileSync(compiler)),python_dependencies:cp.execFileSync(path.join(path.dirname(compiler),'python'),['-m','pip','freeze'],{encoding:'utf8'}).trim().split('\n')};
 };
 const poolSource=path.join(ref,'CurveCryptoSwap2.vy');
 const pool=compile(path.join(root,'integration/tools/reference/curve-models/.runtime/vyper-0.3.3/bin/vyper'),poolSource,'0.3.3');
 const lp=compile(path.join(ref,'.runtime/vyper-0.3.1/bin/vyper'),path.join(ref,'CurveTokenV4.vy'),'0.3.1');
 const settings={optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode.object']}}};
 const si={language:'Solidity',sources:{'Tokens.sol':{content:fs.readFileSync(path.join(ref,'Tokens.sol'),'utf8')}},settings};
 const so=JSON.parse(solc.compile(JSON.stringify(si)));if((so.errors||[]).some(x=>x.severity==='error'))throw Error(JSON.stringify(so.errors));
 const t=so.contracts['Tokens.sol'].Tokens,ta={abi:t.abi,bytecode:'0x'+t.evm.bytecode.object};
 // Default EIP-170 limit enforced, including the deployed immutable data tail.
 owned=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:false,time:new Date('2026-11-02T00:00:00Z')},miner:{timestampIncrement:0},wallet:{deterministic:true}});
 const provider=new ethers.providers.Web3Provider(owned),signer=provider.getSigner(0),owner=await signer.getAddress(),receiver=await provider.getSigner(1).getAddress();
 const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32);
 const deploy=async(a,args=[])=>{const c=await new ethers.ContractFactory(a.abi,a.bytecode,signer).deploy(...args,{gasLimit:25_000_000});await c.deployed();return c;};
 const lay=pool.layout.storage_layout??pool.layout;
 const slot=async(c,s,v)=>provider.send('evm_setAccountStorageAt',[c.address,word(BigInt(s)),word(BigInt(v))]);
 const read=async(c,s)=>BigInt((await provider.getStorageAt(c.address,Number(s)))||'0x0');
 const storage=async(c,k)=>{const x=await provider.getStorageAt(c.address,Number(lay[k].slot));return BigInt(x==='0x'?'0':x);};
 const metadata={schema_version:1,reference_only:true,source_pins:pins,pool_compiler:pool.compiler,lp_compiler:lp.compiler,pool_compiler_dependencies:pool.python_dependencies,lp_compiler_dependencies:lp.python_dependencies,compiler_invocations:[pool,lp].map(a=>({compiler:a.compiler_path,args:a.compiler_arguments,source:a.source_path,launcher_sha256:a.compiler_executable_sha256})),solc:solc.version(),dependency_versions:Object.fromEntries(['ethers','ganache','solc'].map(n=>[n,require(path.join(mods,n,'package.json')).version])),token_compiler_settings:settings,pool_layout:pool.layout,lp_layout:lp.layout,pool_template_runtime_bytes:(pool.runtime.length-2)/2,pool_template_runtime_sha256:sha(Buffer.from(pool.runtime.slice(2),'hex')),pool_initcode_sha256:sha(Buffer.from(pool.bytecode.slice(2),'hex')),lp_template_runtime_bytes:(lp.runtime.length-2)/2,lp_template_runtime_sha256:sha(Buffer.from(lp.runtime.slice(2),'hex')),eip170_limit:24576,allow_unlimited_contract_size:false,deployments:[]};
 atomic(path.join(ref,'runtime-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
 fs.writeFileSync(path.join(ref,'pool-bytecode-runtime.hex'),pool.runtime+'\n');fs.writeFileSync(path.join(ref,'lp-bytecode-runtime.hex'),lp.runtime+'\n');
 const transaction=r=>({hash:r.transactionHash,block_number:r.blockNumber,status:r.status,gas_used:String(r.gasUsed)});
 const cases=[];
 const immutableLayout=[['token',20814],['coins[0]',20846],['coins[1]',20878],['PRECISIONS[0]',20910],['PRECISIONS[1]',20942]].map(([name,offset])=>({name,offset,bytes:32}));
 metadata.immutable_layout=immutableLayout;
 for(const decimals of [[18,18],[18,6]]){
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
  atomic(path.join(ref,'runtime-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
  const snapshot=async()=>{const state={};for(const k of keys)state[k]=String(await storage(c,k));return {state,balances:await Promise.all([0,1].map(k=>c.balances(k).then(String))),total_supply:String(await lpc.totalSupply()),receiver_balance:String(await lpc.balanceOf(receiver)),raw_balances:await Promise.all(tokens.map(t=>t.balanceOf(c.address).then(String))),trader_balances:await Promise.all(tokens.map(t=>t.balanceOf(owner).then(String)))};};
  let base=await provider.send('evm_snapshot',[]);
  const scenarios=[
   {name:'no-adjust'}, {name:'no-adjust-donations',donation:true},
   {name:'claim-norm-small',claim:true}, {name:'claim-gulp-donations',claim:true,donation:true},
   {name:'claim-receiver-zero',claim:true,zero:true},
   {name:'adjust-accepted',claim:true,oracle:2n*E},
   {name:'adjust-rejected-claim',claim:true,oracle:105n*E/100n,vp:99n*E/100n},
   {name:'auto-adjust-accepted',oracle:2n*E,allowed:0n},
   {name:'direct-claim',direct:true,profit:102n*E/100n},
   {name:'direct-claim-zero-receiver',direct:true,profit:102n*E/100n,zero:true,donation:true},
   {name:'direct-claim-no-profit',direct:true,donation:true},
   {name:'ramp-up',ramp:{initial_amp:'200000',initial_gamma:'100000000000000',offset:-300,end:300}},
   {name:'ramp-down',ramp:{initial_amp:'800000',initial_gamma:'300000000000000',offset:-300,end:300},claim:true},
   {name:'ramp-end',ramp:{initial_amp:'200000',initial_gamma:'100000000000000',offset:-600,end:0}},
   {name:'tiny-input',dx:1n}, {name:'zero-input',dx:0n},
   {name:'overflow-input',dx:MAX}, {name:'loss-revert',vp:2n*E},
   {name:'future-oracle-time',futureOracle:true},
   {name:'ramp-invalid-time',ramp:{initial_amp:'400000',initial_gamma:'145000000000000',offset:20,end:300}},
   {name:'claim-fees-underflow',direct:true,profit:10n*E},
   {name:'reverse-claim',claim:true,i:1},
   {name:'claim-zero-receiver-large-fees',direct:true,profit:10n*E,zero:true},
   {name:'claim-lp-mul-overflow',direct:true,profit:102n*E/100n,supply:MAX},
   {name:'claim-lp-round-to-zero',direct:true,profit:102n*E/100n,supply:1n},
   {name:'no-adjust-tiny-derivative',dx:100001n},
   {name:'claim-admin-fee-zero',direct:true,profit:102n*E/100n,adminfee:0n},
   {name:'adjust-fail-gulp-donation',claim:true,oracle:105n*E/100n,vp:99n*E/100n,donation:true},
   {name:'ramp-up-claim',claim:true,ramp:{initial_amp:'200000',initial_gamma:'100000000000000',offset:-300,end:300}},
   {name:'ramp-end-claim',claim:true,ramp:{initial_amp:'200000',initial_gamma:'100000000000000',offset:-600,end:0}},
  ];
  for(const sc of scenarios){
   await provider.send('evm_revert',[base]);base=await provider.send('evm_snapshot',[]);
   const timestamp=(await provider.getBlock('latest')).timestamp;
   if(sc.supply!==undefined)await slot(lpc,lp.layout.totalSupply.slot,sc.supply);
   if(sc.adminfee!==undefined)await slot(c,lay.admin_fee.slot,sc.adminfee);
   if(sc.claim)await slot(c,lay.not_adjusted.slot,1);
   if(sc.oracle){await slot(c,lay._price_oracle.slot,sc.oracle);await slot(c,lay.last_prices.slot,sc.oracle);}
   if(sc.vp)await slot(c,lay.virtual_price.slot,sc.vp);
   if(sc.profit)await slot(c,lay.xcp_profit.slot,sc.profit);
   if(sc.allowed!==undefined)await slot(c,lay.allowed_extra_profit.slot,sc.allowed);
   if(sc.zero)await slot(c,lay.admin_fee_receiver.slot,0);
   if(sc.futureOracle)await slot(c,lay.last_prices_timestamp.slot,timestamp+600);
   let ramp=null;
   if(sc.ramp){ramp={initial_amp:sc.ramp.initial_amp,initial_gamma:sc.ramp.initial_gamma,initial_time:timestamp+sc.ramp.offset};await slot(c,lay.initial_A_gamma.slot,(BigInt(ramp.initial_amp)<<128n)+BigInt(ramp.initial_gamma));await slot(c,lay.initial_A_gamma_time.slot,ramp.initial_time);await slot(c,lay.future_A_gamma_time.slot,timestamp+sc.ramp.end);}
   if(sc.donation)for(let k=0;k<2;k++)await(await tokens[k].transfer(c.address,String(100n*E/prec[k]))).wait();
   const before=await snapshot(),i=sc.i??0,j=1-i,dx=sc.dx??100n*E/prec[i];
   const row={name:sc.name+'-'+decimals.join('-'),operation:sc.direct?'claim_admin_fees':'exchange',precisions:prec.map(String),params:{...params,...sc.allowed!==undefined?{allowed_extra_profit:String(sc.allowed)}:{},...sc.adminfee!==undefined?{admin_fee:String(sc.adminfee)}:{}},...before,admin_receiver_enabled:!sc.zero,ramp,timestamp,input:String(dx),index_in:i,index_out:j};
   let tx;
   try{
    tx=sc.direct?await c.claim_admin_fees({gasLimit:8_000_000}):await c['exchange(uint256,uint256,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:8_000_000});
    const receipt=await tx.wait(),after=await snapshot();
    const events=receipt.logs.filter(l=>l.address.toLowerCase()===c.address.toLowerCase()).map(l=>c.interface.parseLog(l)).map(e=>({name:e.name,args:e.args.filter(x=>typeof x!=='function').map(String)}));
    const exchange=events.find(e=>e.name==='TokenExchange');
    row.timestamp=(await provider.getBlock(receipt.blockNumber)).timestamp;
    row.expected={...after,transaction:transaction(receipt),amount_out:exchange?exchange.args[4]:'0',minted_admin:String(BigInt(after.total_supply)-BigInt(before.total_supply)),pool_events:events,lp_transfer_events:receipt.logs.filter(l=>l.address.toLowerCase()===lpc.address.toLowerCase()).map(l=>lpc.interface.parseLog(l)).filter(e=>e.name==='Transfer').map(e=>e.args.map(String))};
   }catch(e){
    if(!tx||!e.receipt||e.receipt.status!==0)throw e;
    row.expected={reverted:true,transaction:transaction(e.receipt),after:await snapshot()};
    if(JSON.stringify(row.expected.after)!==JSON.stringify(before))throw Error('rollback mismatch '+row.name);
   }
   if(['adjust-accepted','auto-adjust-accepted'].includes(sc.name)&&(row.expected.reverted||row.expected.state.price_scale===row.state.price_scale))throw Error('expected accepted adjustment '+row.name);
   if(sc.name==='adjust-rejected-claim'&&(row.expected.reverted||row.expected.state.price_scale!==row.state.price_scale||row.expected.minted_admin==='0'))throw Error('expected rejected adjustment then claim '+row.name);
   cases.push(row);
  }
 }
 const text=JSON.stringify({schema_version:1,reference_only:true,profile:'unmodified original CryptoSwap2 + unmodified original CurveTokenV4; nominal exact ERC20; EIP170 enforced',source_pins:pins,cases},null,2)+'\n';
 for(const project of ['integration','dex-router'])atomic(path.join(root,project,'test/contracts/curve-crypto-full.json'),text);
 atomic(receiptPath,JSON.stringify({valid:true,reference_only:true,cases:cases.length,reverts:cases.filter(c=>c.expected.reverted).length,corpus_sha256:sha(text),runtime_metadata_sha256:sha(fs.readFileSync(path.join(ref,'runtime-metadata.json'))),generator_sha256:sha(fs.readFileSync(__filename))},null,2)+'\n');
 console.log(JSON.stringify({cases:cases.length,reverts:cases.filter(c=>c.expected.reverted).length,deployments:metadata.deployments}));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{if(owned)await owned.disconnect();});
