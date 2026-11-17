// SPDX-License-Identifier: GPL-3.0-or-later
// Actual original Router02 transactions, complete declared pre/post banks; no public network.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=process.cwd(),ref=path.join(root,'integration/tools/reference/evm-execution');
const modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const {ethers}=require(path.join(modules,'ethers')),ganache=require(path.join(modules,'ganache'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),lower=a=>a.toLowerCase();
const need=(v,m)=>{if(!v)throw Error(m);};
const pins=JSON.parse(fs.readFileSync(path.join(ref,'source-pins.json'))),inputs={};
for(const pkg of pins.packages)for(const entry of pkg.files){
 need(!path.isAbsolute(entry.path)&&!entry.path.split('/').includes('..'),'REFERENCE_PATH');
 const p=path.join(ref,entry.path),bytes=fs.readFileSync(p);need(sha(bytes)===entry.sha256,'REFERENCE_IDENTITY');inputs[path.relative(root,p)]=sha(bytes);
}
for(const p of [__filename,path.join(ref,'source-pins.json'),path.join(modules,'ethers/package.json'),path.join(modules,'ganache/package.json')])inputs[path.relative(root,p)]=sha(fs.readFileSync(p));
const artifact=(pkg,name)=>JSON.parse(fs.readFileSync(path.join(ref,`packages/${pkg}/build/${name}.json`)));
const a={factory:artifact('core','UniswapV2Factory'),pair:artifact('core','UniswapV2Pair'),router:artifact('periphery','UniswapV2Router02'),weth:artifact('periphery','WETH9'),token:artifact('periphery','ERC20')};
const miner='0x000000000000000000000000000000000000cafe',recipient='0x000000000000000000000000000000000000beef';
const transport=ganache.provider({logging:{quiet:true},chain:{chainId:31337,hardfork:'shanghai',time:new Date('2026-11-17T00:00:00Z')},miner:{coinbase:miner,timestampIncrement:1,blockGasLimit:30000000},wallet:{deterministic:true}});
const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=20;
const slot=n=>ethers.utils.hexZeroPad(ethers.BigNumber.from(n).toHexString(),32);
const mapping=(address,index)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[address,index]));
const nested=(owner,spender,index)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','bytes32'],[spender,mapping(owner,index)]));
const word=v=>ethers.utils.hexZeroPad(v==='0x'?'0x00':v,32);
(async()=>{try{
 const signer=provider.getSigner(0),sender=lower(await signer.getAddress());
 const deploy=async(art,args=[])=>{const c=await new ethers.ContractFactory(art.abi,art.evm.bytecode.object,signer).deploy(...args,{gasLimit:15000000});await c.deployed();return c;};
 await(await signer.sendTransaction({to:miner,value:ethers.utils.parseEther('1')})).wait();
 const factory=await deploy(a.factory,[sender]),weth=await deploy(a.weth),router=await deploy(a.router,[factory.address,weth.address]);
 const tokens=[];for(let i=0;i<4;i++){const t=await deploy(a.token,['1000000000000000000000000000000']);await(await t.approve(router.address,ethers.constants.MaxUint256)).wait();tokens.push(t);}
 const pairs=[];for(let i=0;i<3;i++){
  await(await factory.createPair(tokens[i].address,tokens[i+1].address)).wait();
  const pair=new ethers.Contract(await factory.getPair(tokens[i].address,tokens[i+1].address),a.pair.abi,signer);
  await(await tokens[i].transfer(pair.address,'1000000000000000000000000')).wait();
  await(await tokens[i+1].transfer(pair.address,'1500000000000000000000000')).wait();await(await pair.mint(sender)).wait();pairs.push(pair);
 }
 const genesis=await provider.send('eth_getBlockByNumber',['0x0',false]);
 const all=[sender,miner,factory.address,weth.address,router.address,...tokens.map(t=>t.address),...pairs.map(p=>p.address)].map(lower);
 const footprints=new Map(all.map(x=>[x,new Set()]));
 for(const p of pairs)for(let i=0;i<=12;i++)footprints.get(lower(p.address)).add(slot(i));
 for(const t of tokens){const slots=footprints.get(lower(t.address));for(let i=0;i<5;i++)slots.add(slot(i));
  for(const owner of [...all,recipient]){slots.add(mapping(owner,1));slots.add(nested(owner,router.address,2));}
 }
 const fspec=footprints.get(lower(factory.address));for(let i=0;i<4;i++)fspec.add(slot(i));
 for(const t of tokens)for(const u of tokens)fspec.add(nested(t.address,u.address,2));
 async function bank(number){const tag=ethers.utils.hexValue(number),accounts=[];
  for(const address of all){const[balance,nonce,code]=await Promise.all([provider.getBalance(address,number),provider.getTransactionCount(address,number),provider.getCode(address,number)]);
   const storage=[];for(const key of [...footprints.get(address)].sort())storage.push({slot:key,value:word(await provider.getStorageAt(address,key,number))});
   accounts.push({address,exists:true,balance:balance.toString(),nonce,code,storage});
  }return accounts;
 }
 async function pools(header){const rows=[];for(const p of pairs){const at={blockTag:header.number};
  const[t0,t1,r,c0,c1]=await Promise.all([p.token0(at),p.token1(at),p.getReserves(at),p.price0CumulativeLast(at),p.price1CumulativeLast(at)]);
  const t0c=new ethers.Contract(t0,a.token.abi,provider),t1c=new ethers.Contract(t1,a.token.abi,provider);
  const raw=await Promise.all([t0c.balanceOf(p.address,at),t1c.balanceOf(p.address,at)]);
  rows.push({chain_key:'evm:31337',pool_id:lower(p.address),protocol:'uniswap-v2-complete',adapter_version:'uniswap-v2/core-1.0.1-full-swap-1',token0:lower(t0),token1:lower(t1),reserve0:r[0].toString(),reserve1:r[1].toString(),fee_numerator:3,fee_denominator:1000,token_behavior:'standard',v2:{factory:lower(factory.address),code_sha256:sha(Buffer.from((await provider.getCode(p.address,header.number)).slice(2),'hex')),swap:{raw_balance0:raw[0].toString(),raw_balance1:raw[1].toString(),block_timestamp_last:Number(r[2]),price0_cumulative_last:c0.toString(),price1_cumulative_last:c1.toString(),unlocked:BigInt(await provider.getStorageAt(p.address,12,header.number))===1n,factory_code_sha256:sha(Buffer.from((await provider.getCode(factory.address,header.number)).slice(2),'hex')),bank_block_number:header.number,bank_block_hash:header.hash,bank_unix_timestamp:header.timestamp}}});
 }return rows;}
 const routerCode=await provider.getCode(router.address),factoryCode=await provider.getCode(factory.address);
 const executor={target:lower(router.address),code_sha256:sha(Buffer.from(routerCode.slice(2),'hex')),factory:lower(factory.address),factory_code_sha256:sha(Buffer.from(factoryCode.slice(2),'hex')),adapter_version:'uniswap-v2-router02/1.1.0-beta.0'};
 const cases=[];let checkpoint=await provider.send('evm_snapshot',[]);
 for(const kind of ['EXACT_IN','EXACT_OUT'])for(const hops of [1,2,3])for(const reverse of [false,true]){
  need(await provider.send('evm_revert',[checkpoint]),'RESET');checkpoint=await provider.send('evm_snapshot',[]);
  const route=tokens.slice(0,hops+1).map(t=>lower(t.address));if(reverse)route.reverse();
  const h=await provider.getBlock('latest'),before=await bank(h.number),prePools=await pools(h);
  const amount='100000000000000000000',deadline=h.timestamp+3600;
  const quoted=kind==='EXACT_IN'?await router.getAmountsOut(amount,route,{blockTag:h.number}):await router.getAmountsIn(amount,route,{blockTag:h.number});
  const input=quoted[0].toString(),output=quoted[quoted.length-1].toString();
  const senderToken=new ethers.Contract(route[0],a.token.abi,provider),recipientToken=new ethers.Contract(route[route.length-1],a.token.abi,provider);
  const sb=await senderToken.balanceOf(sender,{blockTag:h.number}),rb=await recipientToken.balanceOf(recipient,{blockTag:h.number});
  const tx=kind==='EXACT_IN'?await router.swapExactTokensForTokens(input,output,route,recipient,deadline,{gasLimit:7000000,gasPrice:2000000000}):await router.swapTokensForExactTokens(output,input,route,recipient,deadline,{gasLimit:7000000,gasPrice:2000000000});
  const receipt=await tx.wait(),post=await provider.getBlock(receipt.blockNumber),rawHeader=await provider.send('eth_getBlockByNumber',[ethers.utils.hexValue(post.number),false]);
  const after=await bank(post.number),afterPools=await pools(post),sa=await senderToken.balanceOf(sender,{blockTag:post.number}),ra=await recipientToken.balanceOf(recipient,{blockTag:post.number});
  need(receipt.status===1&&sb.sub(sa).toString()===input&&ra.sub(rb).toString()===output,'ORIGINAL_RECEIPT_BALANCES');
  need(post.number===h.number+1&&post.parentHash===h.hash,'BANK_H_H_PLUS_ONE');
  cases.push({id:`${kind}-${hops}-${reverse}`,kind,path:route,amount_in:input,amount_out:output,deadline_ms:deadline*1000,executor,
   fixture:{schema_version:1,state_anchor:{chain_key:'evm:31337',block_number:h.number,block_hash:h.hash,parent_hash:h.parentHash,timestamp:h.timestamp,genesis_hash:genesis.hash},execution_environment:{chain_id:31337,fork:'shanghai',block_number:post.number,block_hash:post.hash,parent_hash:post.parentHash,timestamp:post.timestamp,coinbase:lower(post.miner),gas_limit:post.gasLimit.toNumber(),base_fee:post.baseFeePerGas.toString(),prevrandao:rawHeader.mixHash},accounts:before,block_hashes:[]},pools:prePools,
   transaction_context:{nonce:tx.nonce,gas_limit:tx.gasLimit.toNumber(),gas_price:tx.gasPrice.toString(),chain_id:31337},
   transaction:{chain_key:'evm:31337',from:sender,to:lower(router.address),data:tx.data,value:'0'},
   recipient,expected:{gas_used:receipt.gasUsed.toString(),status:receipt.status,actual_input:sb.sub(sa).toString(),actual_output:ra.sub(rb).toString(),post_accounts:after,post_pools:afterPools,transaction_hash:tx.hash},
   authority:'LOCAL_ORIGINAL_ROUTER_MINED_TRANSACTION'});
 }
 const result={schema_version:1,authority:'CONTROLLED_SHANGHAI_ORIGINAL_ROUTER_H_H_PLUS_ONE_BANKS',inputs,cases,publicTransactionsSubmitted:false,production_ready:false};
 const destinations=process.argv.slice(2);need(destinations.length>0,'OUTPUT_REQUIRED');for(const target of destinations){fs.mkdirSync(path.dirname(target),{recursive:true});fs.writeFileSync(target,JSON.stringify(result,null,2)+'\n');}
 console.log(JSON.stringify({cases:cases.length,inputs:Object.keys(inputs).length,sha256:sha(fs.readFileSync(destinations[0]))}));
}finally{await transport.disconnect();}})().catch(e=>{console.error(e);process.exitCode=1;});
