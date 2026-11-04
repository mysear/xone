// Execute unmodified pinned Vyper pools. Mocks supply only token transfers/base rate.
const fs=require('fs'),path=require('path'),cp=require('child_process'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/curve-models');
const mods=path.join(root,'integration/.runtime/stable-reference/node_modules');
const {ethers}=require(path.join(mods,'ethers')),ganache=require(path.join(mods,'ganache')),solc=require(path.join(mods,'solc'));
const vyper=path.join(root,'integration/.runtime/curve-vyper/bin/vyper');
cp.execFileSync('python3',[path.join(ref,'verify_sources.py')]);
if(!cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim().match(/^0\.3\.1(?:\+|$)/))throw Error('Expected Vyper 0.3.1');
const E=10n**18n,F=4000000n,AF=5000000000n;
const provider=new ethers.providers.Web3Provider(ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai'},wallet:{deterministic:true}}));
const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32);
async function slot(c,s,v){await provider.send('evm_setAccountStorageAt',[c.address,word(BigInt(s)),word(v)]);}
function artifact(name){const lines=cp.execFileSync(vyper,['-f','bytecode,abi,layout',path.join(ref,'old',name+'.vy')],{encoding:'utf8',maxBuffer:2**22}).trim().split('\n');return {bytecode:lines[0],abi:JSON.parse(lines[1]),layout:JSON.parse(lines[2])};}
async function deploy(a,signer){const c=await new ethers.ContractFactory(a.abi,a.bytecode,signer).deploy();await c.deployed();return c;}
(async()=>{const signer=provider.getSigner(0),owner=await signer.getAddress();
 const si={language:'Solidity',sources:{'T.sol':{content:'pragma solidity 0.8.19; contract T { uint256 public rate=1e18; function set(uint256 r) external {rate=r;} function get_virtual_price() external view returns(uint256){return rate;} function transfer(address,uint256)external pure returns(bool){return true;} function transferFrom(address,address,uint256)external pure returns(bool){return true;} }'}},settings:{outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}};
 const a=JSON.parse(solc.compile(JSON.stringify(si))).contracts['T.sol'].T;const token=await deploy({abi:a.abi,bytecode:a.evm.bytecode.object},signer);
 const base='0xbEbc44782C7dB0a1A60Cb6fe97d0b483032FF1C7';await provider.send('evm_setAccountCode',[base,await provider.getCode(token.address)]);const baseContract=new ethers.Contract(base,a.abi,signer);
 const cases=[],layouts={};
 for(const name of ['Plain3Basic','Plain4Basic','MetaUSD']){
  const a=artifact(name),pool=await deploy(a,signer),n=name==='Plain3Basic'?3:name==='Plain4Basic'?4:2;layouts[name]=a.layout;
  const set=async(key,value,k=0)=>slot(pool,a.layout[key].slot+k,value);
  for(let k=0;k<n;k++)await set('coins',BigInt(token.address),k);
  for(const imbalance of [false,true])for(const amp of [100n,10000n,123456n])for(const scaled of [false,true]){
   let rates=Array.from({length:n},(_,k)=>scaled&&k%2===1?10n**30n:E);if(n===2)rates[1]=1234567890123456789n;
   let b=rates.map((r,k)=>(k===n-1&&imbalance?1700000n:1000000n)*E*E/r);
   for(let k=0;k<n;k++){await set('balances',b[k],k);if(n>2)await set('rate_multipliers',rates[k],k);}if(n===2){await set('rate_multiplier',rates[0]);await (await baseContract.set(rates[1])).wait();}
   await set('fee',F);await set('initial_A',amp);await set('future_A',amp);await set('initial_A_time',0n);await set('future_A_time',0n);
   if((await pool.A_precise()).toString()!==amp.toString() || (await pool.admin_fee()).toString()!==AF.toString())throw Error('getter mismatch');
   for(let i=0;i<n;i++)for(let j=0;j<n;j++){if(i===j)continue;
    for(const h of [0n,1n,10000n]){
     for(let k=0;k<n;k++)await set('balances',b[k],k);
     const dx=h===1n?1n:h*E*E/rates[i];
     const row={model:name,balances:b.map(String),rates:rates.map(String),amp:String(amp),fee:String(F),admin_fee:String(AF),input:String(dx),index_in:i,index_out:j};
     try{const out=await pool.callStatic['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner);await (await pool['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:6000000})).wait();row.expected={amount_out:String(out),next_balances:await Promise.all(b.map((_,k)=>pool.balances(k).then(String)))};}
     catch(e){row.expected={reverted:true};}
     cases.push(row);
    }
   }
  }
  console.log(name,cases.length);
 }
 const text=JSON.stringify({schema_version:1,reference:'original unmodified full Vyper exchange; deterministic storage state; mock transfers and supplied base virtual price',source_pins:JSON.parse(fs.readFileSync(path.join(ref,'sources.json'))).filter(x=>x.local.startsWith('old/')),compiler:cp.execFileSync(vyper,['--version'],{encoding:'utf8'}).trim(),cases},null,2)+'\n';
 for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/curve-multi-quotes.json'),text);
 fs.writeFileSync(path.join(ref,'evidence/multi-layouts.json'),JSON.stringify(layouts,null,2)+'\n');
 fs.writeFileSync(path.join(ref,'evidence/multi-generation.json'),JSON.stringify({cases:cases.length,reverts:cases.filter(x=>x.expected.reverted).length,sha256:crypto.createHash('sha256').update(text).digest('hex')},null,2)+'\n');
 console.log('cases',cases.length);provider.provider.disconnect();
})().catch(e=>{console.error(e);provider.provider.disconnect();process.exitCode=1;});
