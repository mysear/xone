// Full original StableSwapNG exchange, dynamic fee and LP/admin balance execution.
const fs=require('fs'),path=require('path'),cp=require('child_process'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/curve-models');
const mods=path.join(root,'integration/.runtime/stable-reference/node_modules');
const {ethers}=require(path.join(mods,'ethers')),ganache=require(path.join(mods,'ganache')),solc=require(path.join(mods,'solc'));
const vyper=path.join(ref,'.runtime/vyper-0.3.10-py313/bin/vyper');
cp.execFileSync('python3',[path.join(ref,'verify_sources.py')]);
if(!cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim().match(/^0\.3\.10(?:\+|$)/))throw Error('Expected Vyper 0.3.10');
function compile(name){const ls=cp.execFileSync(vyper,['-f','bytecode,abi,layout',path.join(ref,'ng',name+'.vy')],{encoding:'utf8',maxBuffer:2**23}).trim().split('\n');const lay=JSON.parse(ls[2]);return{bytecode:ls[0],abi:JSON.parse(ls[1]),layout:lay.storage_layout??lay};}
const E=10n**18n,F=4000000n;
const provider=new ethers.providers.Web3Provider(ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:true},wallet:{deterministic:true}}));
const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32);
async function slot(c,s,v){await provider.send('evm_setAccountStorageAt',[c.address,word(BigInt(s)),word(v)]);}
async function deploy(a,signer,args=[]){const c=await new ethers.ContractFactory(a.abi,a.bytecode,signer).deploy(...args,{gasLimit:20000000});await c.deployed();return c;}
(async()=>{const signer=provider.getSigner(0),owner=await signer.getAddress();
 const si={language:'Solidity',sources:{'T.sol':{content:'pragma solidity 0.8.19; contract T {mapping(address=>uint256)public balanceOf; uint256 public rate=1e18; function decimals()external pure returns(uint8){return 18;} function asset()external view returns(address){return address(this);} function convertToAssets(uint256 x)external view returns(uint256){return x*rate/1e18;} function setRate(uint256 r)external{rate=r;} function set(address a,uint256 x)external{balanceOf[a]=x;} function transfer(address to,uint256 x)external returns(bool){balanceOf[msg.sender]-=x;balanceOf[to]+=x;return true;} function transferFrom(address,address to,uint256 x)external returns(bool){balanceOf[to]+=x;return true;} }'}},settings:{outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}};
 const a=JSON.parse(solc.compile(JSON.stringify(si))).contracts['T.sol'].T,ta={abi:a.abi,bytecode:a.evm.bytecode.object};
 const pa=compile('CurveStableSwapNG'),ma=compile('CurveStableSwapNGMath'),math=await deploy(ma,signer);
 const cases=[];let ratesCases=[];
 const mathCases=[];
 for(const xp of [[2n,3n,5n],[7n,11n,13n],[11n,23n,37n,53n],[12345n,23456n,34567n],[1000000000000000000000001n,1700000000000000000000003n,800000000000000000000007n]]){
  try{mathCases.push({xp:xp.map(String),amp:'12345',expected:String(await math.get_D(xp.map(String),'12345',xp.length))});}catch(e){mathCases.push({xp:xp.map(String),amp:'12345',reverted:true});}
 }
 mathCases.push({xp:['215','130','257'],amp:'500',expected:String(await math.get_D(['215','130','257'],'500',3))});
 if(process.argv.includes('--math-only')){
  const existing=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/curve-ng-quotes.json')));existing.math_cases=mathCases;
  const text=JSON.stringify(existing,null,2)+'\n';for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/curve-ng-quotes.json'),text);
  const reportPath=path.join(ref,'evidence/ng-generation.json');const report=JSON.parse(fs.readFileSync(reportPath));report.sha256=crypto.createHash('sha256').update(text).digest('hex');fs.writeFileSync(reportPath,JSON.stringify(report,null,2)+'\n');
  console.log('Original NGMath probes',mathCases.length);provider.provider.disconnect();return;
 }
 for(const n of [2,3,4,8])for(const lending of [false,true]){
  if(lending&&n!==3)continue;
  const tokens=await Promise.all(Array.from({length:n},()=>deploy(ta,signer)));
  const rates=Array.from({length:n},(_,i)=>i%2===1?10n**30n:E),assetTypes=Array.from({length:n},(_,i)=>lending&&i===1?3:0);
  const pool=await deploy(pa,signer,['Reference','REF',100,String(F),'20000000000',866,tokens.map(t=>t.address),rates.map(String),assetTypes,Array(n).fill('0x00000000'),Array(n).fill(ethers.constants.AddressZero)]);
  const set=async(key,value,k)=>slot(pool,pa.layout[key].slot+(k===undefined?0:k+1),value);
  if(lending)await (await tokens[1].setRate(1234567890123456789n.toString())).wait();
  const effective=await pool.stored_rates();const rs=effective.map(x=>BigInt(x.toString()));
  if(lending)ratesCases.push({multipliers:rates.map(String),asset_types:assetTypes,converted_assets:Array.from({length:n},(_,i)=>String(i===1?1234567890123456789n:0n)),scale_factors:Array(n).fill('1'),expected:rs.map(String)});
  const pairs=n===8?[[0,7],[7,0],[3,5]]:Array.from({length:n},(_,i)=>Array.from({length:n},(_,j)=>[i,j])).flat().filter(([i,j])=>i!==j);
  for(const imbalance of [false,true])for(const multiplier of [10000000000n,20000000000n,1000000000000n])for(const [i,j]of pairs)for(const human of [0n,1n,10000n]){
   const b=rs.map((r,k)=>(k===n-1&&imbalance?1700000n:1000000n)*E*E/r),amp=imbalance?12345n:10000n;
   for(let k=0;k<n;k++){await set('stored_balances',b[k],k);await set('admin_balances',0n,k);await (await tokens[k].set(pool.address,String(b[k]))).wait();}
   await set('initial_A',amp);await set('future_A',amp);await set('offpeg_fee_multiplier',multiplier);
   const dx=human===1n?1n:human*E*E/rs[i];
   const xp=b.map((b,k)=>b*rs[k]/E);let D;try{D=String(await math.get_D(xp.map(String),String(amp),n));}catch(e){D=null;}
   const row={balances:b.map(String),rates:rs.map(String),amp:String(amp),fee:String(F),admin_fee:'5000000000',offpeg_fee_multiplier:String(multiplier),input:String(dx),index_in:i,index_out:j,invariant:D,asset_types:assetTypes};
   try{const out=await pool.callStatic['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner);await (await pool['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:6000000})).wait();row.expected={amount_out:String(out),next_balances:(await pool.get_balances()).map(String),admin_fee_amount:String(await pool.admin_balances(j))};}
   catch(e){row.expected={reverted:true};}
   cases.push(row);
  }
  console.log('NG',n,lending,cases.length);
 }
 const text=JSON.stringify({schema_version:1,reference:'original full StableSwapNG.exchange + original NGMath.get_D; ERC20 transfers and explicit ERC4626 convertToAssets provider',source_pins:JSON.parse(fs.readFileSync(path.join(ref,'sources.json'))).filter(x=>x.local.startsWith('ng/')),compiler:cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim(),cases,rate_cases:ratesCases,math_cases:mathCases},null,2)+'\n';
 for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/curve-ng-quotes.json'),text);
 fs.writeFileSync(path.join(ref,'evidence/ng-layout.json'),JSON.stringify(pa.layout,null,2)+'\n');
 fs.writeFileSync(path.join(ref,'evidence/ng-generation.json'),JSON.stringify({cases:cases.length,reverts:cases.filter(x=>x.expected.reverted).length,sha256:crypto.createHash('sha256').update(text).digest('hex')},null,2)+'\n');
 provider.provider.disconnect();
})().catch(e=>{console.error(e);provider.provider.disconnect();process.exitCode=1;});
