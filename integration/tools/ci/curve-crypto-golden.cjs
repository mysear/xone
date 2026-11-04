// Native unmodified original CryptoSwap2 (gamma invariant, oracle and profit state).
const fs=require('fs'),path=require('path'),cp=require('child_process'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/curve-models'),mods=path.join(root,'integration/.runtime/stable-reference/node_modules');
const{ethers}=require(path.join(mods,'ethers')),ganache=require(path.join(mods,'ganache')),solc=require(path.join(mods,'solc'));
const vyper=path.join(ref,'.runtime/vyper-0.3.3/bin/vyper');const E=10n**18n;
const provider=new ethers.providers.Web3Provider(ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:true},wallet:{deterministic:true}}));
const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32);
cp.execFileSync('python3',[path.join(ref,'verify_sources.py')]);
if(!cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim().match(/^0\.3\.3(?:\+|$)/))throw Error('Expected Vyper 0.3.3');
async function slot(c,s,v){await provider.send('evm_setAccountStorageAt',[c.address,word(BigInt(s)),word(v)]);}
async function deploy(a,s,args=[]){const c=await new ethers.ContractFactory(a.abi,a.bytecode,s).deploy(...args,{gasLimit:20000000});await c.deployed();return c;}
const stateKeys=['D','price_scale','_price_oracle','last_prices','last_prices_timestamp','xcp_profit','xcp_profit_a','virtual_price','not_adjusted','future_A_gamma_time'];
(async()=>{const signer=provider.getSigner(0),owner=await signer.getAddress();
 const si={language:'Solidity',sources:{'T.sol':{content:'pragma solidity 0.8.19; contract T {uint8 public decimals;uint256 public totalSupply;mapping(address=>uint256)public balanceOf;constructor(uint8 d){decimals=d;}function mint(address a,uint256 x)external returns(bool){balanceOf[a]+=x;totalSupply+=x;return true;}function mint_relative(address a,uint256 f)external returns(uint256){uint256 x=totalSupply*f/1e18;balanceOf[a]+=x;totalSupply+=x;return x;}function transfer(address a,uint256 x)external returns(bool){balanceOf[msg.sender]-=x;balanceOf[a]+=x;return true;}function transferFrom(address,address a,uint256 x)external returns(bool){balanceOf[a]+=x;return true;} }'}},settings:{outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}};
 const ta=JSON.parse(solc.compile(JSON.stringify(si))).contracts['T.sol'].T,t={abi:ta.abi,bytecode:ta.evm.bytecode.object};
 const ls=cp.execFileSync(vyper,['-f','bytecode,abi,layout',path.join(ref,'crypto/CurveCryptoSwap2.vy')],{encoding:'utf8',maxBuffer:2**22}).trim().split('\n');const a={bytecode:ls[0],abi:JSON.parse(ls[1]),layout:JSON.parse(ls[2])};const lay=a.layout.storage_layout??a.layout;
 async function read(c,key){const v=await provider.getStorageAt(c.address,Number(lay[key].slot));return BigInt(v==='0x'?'0':v);}
 async function snapshot(c){const obj={};for(const key of stateKeys)obj[key]=String(await read(c,key));return obj;}
 const cases=[];
 for(const decs of [[18,18],[18,6]])for(const gamma of [10000000000n,145000000000000n,20000000000000000n])for(const price of [E,2000n*E]){
  const tokens=await Promise.all(decs.map(d=>deploy(t,signer,[d]))),lp=await deploy(t,signer,[18]);
  const params={amp:'400000',gamma:String(gamma),mid_fee:'3000000',out_fee:'30000000',fee_gamma:'230000000000000',allowed_extra_profit:'1000000000000000000',adjustment_step:'146000000000000',admin_fee:'5000000000',ma_half_time:'600'};
  const p=await deploy(a,signer,[owner,ethers.constants.AddressZero,params.amp,params.gamma,params.mid_fee,params.out_fee,params.allowed_extra_profit,params.fee_gamma,params.adjustment_step,params.admin_fee,params.ma_half_time,String(price),lp.address,tokens.map(c=>c.address)]);
  const prec=decs.map(d=>10n**BigInt(18-d)),balances=[1000000n*E/prec[0],1000000n*E*E/price/prec[1]];
  await(await p['add_liquidity(uint256[2],uint256,address)'](balances.map(String),0,owner,{gasLimit:6000000})).wait();
  const initial=await snapshot(p),supply=String(await lp.totalSupply());
  for(const i of [0,1])for(const human of [0n,1n,100n,10000n])for(const elapsed of [0,61,600]){
   // Restore all pricing/profit state; explicit snapshots isolate every vector.
   for(const key of stateKeys)await slot(p,lay[key].slot,BigInt(initial[key]));
   for(let k=0;k<2;k++){await slot(p,Number(lay.balances.slot)+k,balances[k]);const existing=BigInt(String(await tokens[k].balanceOf(p.address)));if(existing<balances[k])await(await tokens[k].mint(p.address,String(balances[k]-existing))).wait();}
   // Simulated prior oracle observation with nontrivial EMA: keep oracle close enough
   // to avoid adjustment, and use explicit last-time/block-time in evidence.
   await slot(p,lay._price_oracle.slot,price*1001n/1000n);await slot(p,lay.last_prices.slot,price*999n/1000n);
   const block=await provider.getBlock('latest'),timestamp=Number(block.timestamp);await slot(p,lay.last_prices_timestamp.slot,BigInt(timestamp-elapsed));
   const s=await snapshot(p),dx=human===1n?1n:human*E/prec[i];
   const row={balances:balances.map(String),precisions:prec.map(String),params,total_supply:supply,state:s,input:String(dx),index_in:i,index_out:1-i,timestamp};
   try{row.get_dy=String(await p.get_dy(i,1-i,String(dx)));}catch(e){row.get_dy_reverted=true;}
   try{const out=await p.callStatic['exchange(uint256,uint256,uint256,uint256,address)'](i,1-i,String(dx),0,owner);const receipt=await(await p['exchange(uint256,uint256,uint256,uint256,address)'](i,1-i,String(dx),0,owner,{gasLimit:6000000})).wait();row.timestamp=(await provider.getBlock(receipt.blockNumber)).timestamp;row.expected={amount_out:String(out),next_balances:await Promise.all([0,1].map(k=>p.balances(k).then(String))),state:await snapshot(p)};}
   catch(e){row.expected={reverted:true};}
   cases.push(row);
  }
  console.log('Crypto',decs,gamma.toString(),price.toString(),cases.length);
 }
 const text=JSON.stringify({schema_version:1,reference:'unmodified full original CryptoSwap2 add_liquidity/get_dy/exchange; standard token/LP bookkeeping mocks',compiler:cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim(),source_pins:JSON.parse(fs.readFileSync(path.join(ref,'sources.json'))).filter(x=>x.local.startsWith('crypto/')),cases},null,2)+'\n';
 for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/curve-crypto-quotes.json'),text);
 fs.writeFileSync(path.join(ref,'evidence/crypto-layout.json'),JSON.stringify(lay,null,2)+'\n');fs.writeFileSync(path.join(ref,'evidence/crypto-generation.json'),JSON.stringify({cases:cases.length,reverts:cases.filter(x=>x.expected.reverted).length,sha256:crypto.createHash('sha256').update(text).digest('hex')},null,2)+'\n');
 provider.provider.disconnect();
})().catch(e=>{console.error(e);provider.provider.disconnect();process.exitCode=1;});
