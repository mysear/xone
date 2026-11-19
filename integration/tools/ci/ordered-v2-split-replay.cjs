// Offline differential replay with pinned original V2 artifacts and one atomic transaction.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../..');
const deps = name => path.join(root, 'integration/.runtime/stable-reference/node_modules', name);
const { ethers } = require(deps('ethers'));
const ganache = require(deps('ganache'));
const solc = require(path.join(root, 'integration/.runtime/singleton-reference/node_modules/solc'));
const hash = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const src = fs.readFileSync(path.join(root, 'dex-router/evm/OrderedV2Split.sol'), 'utf8');
const ref = path.join(root, 'integration/tools/reference/evm-execution');
const pins = JSON.parse(fs.readFileSync(path.join(ref, 'source-pins.json'))).packages.flatMap(p => p.files);
function original(name) {
  const file = `packages/${name}.json`, bytes = fs.readFileSync(path.join(ref, file));
  assert.equal(hash(bytes), pins.find(p => p.path === file).sha256);
  return JSON.parse(bytes);
}
async function replay(transport, captureBank) {
  assert.equal(solc.version(), '0.8.26+commit.8a97fa7a.Emscripten.clang');
  const build = JSON.parse(solc.compile(JSON.stringify({language:'Solidity', sources:{'OrderedV2Split.sol':{content:src}},
    settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}})));
  assert.deepEqual((build.errors || []).filter(e => e.severity === 'error'), []);
  const provider = new ethers.providers.Web3Provider(transport);
  provider.pollingInterval = 15;
  const signer = provider.getSigner(0), caller = await signer.getAddress(), recipient = await provider.getSigner(1).getAddress();
  const deploy = async (art, ...args) => (await new ethers.ContractFactory(art.abi, art.evm.bytecode.object, signer).deploy(...args)).deployed();
  const factory = await deploy(original('core/build/UniswapV2Factory'), caller);
  const weth = await deploy(original('periphery/build/WETH9'));
  const router = await deploy(original('periphery/build/UniswapV2Router02'), factory.address, weth.address);
  const batch = await deploy(build.contracts['OrderedV2Split.sol'].OrderedV2Split, router.address);
  const token = original('periphery/build/ERC20'), pair = original('core/build/UniswapV2Pair');
  const unit = ethers.utils.parseEther, max = ethers.constants.MaxUint256;
  const mined = async tx => (await tx).wait();
  const tokens = [], pairs = [];
  for (let i=0; i<4; i++) tokens.push(await deploy(token, unit('10000000')));
  for (const [i,j] of [[0,1],[1,2],[0,2],[2,3]]) {
    await mined(factory.createPair(tokens[i].address,tokens[j].address));
    const p = new ethers.Contract(await factory.getPair(tokens[i].address,tokens[j].address),pair.abi,signer);
    for (const t of [tokens[i],tokens[j]]) await mined(t.transfer(p.address,unit('10000')));
    await mined(p.mint(caller)); pairs.push(p);
  }
  const sell=tokens[0], buy=tokens[3];
  await mined(sell.approve(router.address,max));
  const approval=await mined(sell.approve(batch.address,max));
  const state = async () => ({balances:await Promise.all([sell.balanceOf(caller),buy.balanceOf(recipient),
    sell.balanceOf(batch.address),sell.allowance(batch.address,router.address)].map(async n => String(await n))),
    reserves:await Promise.all(pairs.map(async p => (await p.getReserves()).slice(0,2).map(String)))});
  let snap = await provider.send('evm_snapshot',[]);
  async function restore() {
    assert.equal(await provider.send('evm_revert',[snap]),true); snap=await provider.send('evm_snapshot',[]);
  }
  const base=await provider.getBlock('latest'), deadline=base.timestamp+3600;
  const lower = a => a.toLowerCase();
  const runtime = async a => hash(Buffer.from((await provider.getCode(a,base.number)).slice(2),'hex'));
  const at = {blockTag:base.number};
  const factoryHash = await runtime(factory.address);
  const pools = await Promise.all(pairs.map(async p => {
    const [t0,t1,r,c0,c1] = await Promise.all([p.token0(at),p.token1(at),p.getReserves(at),p.price0CumulativeLast(at),p.price1CumulativeLast(at)]);
    const balance = async addr => String(await new ethers.Contract(addr,token.abi,provider).balanceOf(p.address,at));
    return {chain_key:'evm:1337',pool_id:lower(p.address),protocol:'uniswap-v2-complete',adapter_version:'uniswap-v2/core-1.0.1-full-swap-1',token0:lower(t0),token1:lower(t1),reserve0:String(r[0]),reserve1:String(r[1]),fee_numerator:3,fee_denominator:1000,token_behavior:'standard',v2:{factory:lower(factory.address),code_sha256:await runtime(p.address),swap:{raw_balance0:await balance(t0),raw_balance1:await balance(t1),block_timestamp_last:Number(r[2]),price0_cumulative_last:String(c0),price1_cumulative_last:String(c1),unlocked:BigInt(await provider.getStorageAt(p.address,12,base.number))===1n,factory_code_sha256:factoryHash,bank_block_number:base.number,bank_block_hash:base.hash,bank_unix_timestamp:base.timestamp}}};
  }));
  const binding = {pools,block:{chain_key:'evm:1337',number_or_slot:base.number,hash:base.hash,parent_hash:base.parentHash,timestamp_ms:base.timestamp*1000},
    executor:{target:lower(batch.address),code_sha256:await runtime(batch.address),router:{target:lower(router.address),code_sha256:await runtime(router.address),factory:lower(factory.address),factory_code_sha256:factoryHash,adapter_version:'uniswap-v2-router02/1.1.0-beta.0'}},
    caller:lower(caller),recipient:lower(recipient),deadlineMs:deadline*1000,cases:[]};
  const bank = captureBank ? await splitBank(provider,base,{caller,recipient,batch,router,factory,weth,tokens,pairs}) : null;
  const cases=[];
  for (const exactOut of [false,true]) for (const reverse of [false,true]) {
    const legs = [[unit('110'),[0,1,2,3]],[unit('70'),[0,2,3]]].map(([amount,ids]) => [amount,ids.map(i=>tokens[i].address)]);
    if (reverse) legs.reverse();
    const before = await state();
    const quote = async ([amount,route]) => exactOut ? (await router.getAmountsIn(amount,route))[0] : (await router.getAmountsOut(amount,route)).at(-1);
    const staleSecond = await quote(legs[1]);
    let orderedSecond;
    for (const [i,leg] of legs.entries()) {
      if (i===1) orderedSecond=await quote(leg);
      const [n,route]=leg;
      const gas = {gasLimit:3000000};
      await mined(exactOut ? router.swapTokensForExactTokens(n,max,route,recipient,deadline,gas) : router.swapExactTokensForTokens(n,0,route,recipient,deadline,gas));
    }
    assert.notEqual(String(orderedSecond),String(staleSecond));
    const expected=await state();
    const input=BigInt(before.balances[0])-BigInt(expected.balances[0]);
    const output=BigInt(expected.balances[1])-BigInt(before.balances[1]);
    assert.equal(exactOut?output:input,BigInt(unit('180')));
    await restore();
    const fees = captureBank ? {maxFeePerGas:2000000000,maxPriorityFeePerGas:100000000} : {};
    const execute = (cap,min) => batch.execute(exactOut,recipient,cap,min,deadline,legs,{gasLimit:3000000,...fees});
    const receipt=await mined(execute(exactOut?input+123n:input,output));
    assert.deepEqual(await state(), expected);
    const swaps=receipt.logs.filter(l=>l.topics[0]===pairs[0].interface.getEventTopic('Swap')).map(l=>l.address);
    const order = reverse ? [2,3,0,1,3] : [0,1,3,2,3];
    assert.deepEqual(swaps,order.map(i=>pairs[i].address));
    if (bank) await bank.record(receipt);
    const tx = await provider.getTransaction(receipt.transactionHash);
    binding.cases.push({exactOut,input:String(input),output:String(output),budget:String(exactOut?input+123n:input),
      legs:legs.map(([n,route])=>({amount:String(n),tokens:route.map(lower)})),poolOrder:swaps.map(lower),data:tx.data});
    assert.equal(expected.balances[2],'0'); assert.equal(expected.balances[3],'0');
    await restore();
    // Fail after at least one successful leg; no pair or token state may escape.
    const failure = await execute(exactOut?input-1n:input,exactOut?output:output+1n);
    await assert.rejects(failure.wait(), e => e.receipt?.status===0 && e.receipt.logs.length===0);
    if (bank) await bank.record(await provider.getTransactionReceipt(failure.hash));
    assert.deepEqual(await state(),before);
    cases.push({kind:exactOut?'EXACT_OUT':'EXACT_IN',reverse,input:String(input),output:String(output),
      tx:receipt.transactionHash,gasUsed:String(receipt.gasUsed),effectiveGasPrice:String(receipt.effectiveGasPrice),
      nativeExecutionFee:String(receipt.gasUsed.mul(receipt.effectiveGasPrice)),failedTx:failure.hash,
      sharedPool: pairs[3].address,staleSecond:String(staleSecond),orderedSecond:String(orderedSecond)});
    await restore();
  }
  return {authority:'LOCAL_ORIGINAL_ROUTER_DIFFERENTIAL',bankHash:base.hash,sourceSha256:hash(src),compiler:solc.version(),
    runtimeSha256:hash(Buffer.from((await provider.getCode(batch.address)).slice(2),'hex')),priorApprovalGas:String(approval.gasUsed),
    normalQuoteConnected:false,completeNetCost:false,cases,binding,executionBank:bank?.result};
}

async function splitBank(provider,base,{caller,recipient,batch,router,factory,weth,tokens,pairs}) {
  const lower = a => a.toLowerCase();
  const addr = v => lower(v.address ?? v);
  const word = n => ethers.utils.hexZeroPad(n === '0x' ? '0x00' : ethers.BigNumber.from(n).toHexString(),32);
  const mapping = (key,slot) => ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','bytes32'],[addr(key),word(slot)]));
  const slots = new Map([caller,recipient,batch,router,factory,weth,base.miner,...tokens,...pairs].map(x=>[addr(x),new Set()]));
  for (const p of pairs) for(let i=0;i<=12;i++) slots.get(addr(p)).add(word(i));
  for (const t of tokens) {
    const rows=slots.get(addr(t));
    for(const owner of slots.keys()) rows.add(mapping(owner,1));
    // Direct and batch candidates execute against the same pre-transaction bank.
    rows.add(mapping(router,mapping(caller,2)));
    rows.add(mapping(batch,mapping(caller,2))); rows.add(mapping(router,mapping(batch,2)));
  }
  for(const t of tokens) for(const u of tokens) if(t!==u) slots.get(addr(factory)).add(mapping(u,mapping(t,2)));
  async function accounts(n) {
    return Promise.all([...slots].map(async ([address,keys])=>{
      const [balance,nonce,code]=await Promise.all([provider.getBalance(address,n),provider.getTransactionCount(address,n),provider.getCode(address,n)]);
      const storage=await Promise.all([...keys].sort().map(async slot=>({slot,value:word(await provider.getStorageAt(address,slot,n))})));
      assert.ok(!balance.isZero() || nonce>0 || code!=='0x','expected funded or deployed account: '+address);
      return {address,exists:true,balance:String(balance),nonce,code,storage};
    }));
  }
  const genesis=await provider.getBlock(0);
  const result={state_anchor:{chain_key:'evm:1337',block_number:base.number,block_hash:base.hash,parent_hash:base.parentHash,timestamp:base.timestamp,genesis_hash:genesis.hash,base_fee_per_gas:String(base.baseFeePerGas)},accounts:await accounts(base.number),runs:[]};
  async function record(receipt) {
    const h=await provider.send('eth_getBlockByHash',[receipt.blockHash,false]);
    assert.equal(h.parentHash,base.hash); assert.equal(Number(h.number),base.number+1);
    assert.deepEqual(h.transactions,[receipt.transactionHash]);
    const tx=await provider.getTransaction(receipt.transactionHash);
    const post=await accounts(Number(h.number));
    result.runs.push({execution_environment:{chain_id:tx.chainId,fork:'shanghai',block_number:Number(h.number),block_hash:h.hash,parent_hash:h.parentHash,timestamp:Number(h.timestamp),coinbase:lower(h.miner),gas_limit:Number(h.gasLimit),base_fee:String(BigInt(h.baseFeePerGas)),prevrandao:h.mixHash},
      transaction_context:{nonce:tx.nonce,gas_limit:tx.gasLimit.toNumber(),gas_price:String(tx.maxFeePerGas),max_priority_fee_per_gas:String(tx.maxPriorityFeePerGas),chain_id:tx.chainId},
      from:lower(tx.from),to:lower(tx.to),data:tx.data,value:String(tx.value),status:receipt.status,gasUsed:String(receipt.gasUsed),effectiveGasPrice:String(receipt.effectiveGasPrice),
      logs:receipt.logs.map(l=>({address:lower(l.address),topics:l.topics,data:l.data})),post_accounts:post});
  }
  return {result,record};
}
async function main() {
  const transport=ganache.provider({logging:{quiet:true},wallet:{deterministic:true},chain:{hardfork:'shanghai',time:new Date('2026-11-17Z')},miner:{timestampIncrement:1}});
  try {
    const captureBank=process.argv[3]==='--bank';
    const report=await replay(transport,captureBank);
    assert.equal(hash(JSON.stringify(report.binding)),'544d55501ed280378a67b9ab45b77a4cef8e0cd1bafeee2ba48f1ffbd60732be','Mined binding corpus changed');
    if (process.argv[2]) fs.writeFileSync(process.argv[2],JSON.stringify(report,null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify(process.argv[2]?{...report,binding:undefined,executionBank:undefined}:report));
  } finally { await transport.disconnect(); }
}
main().catch(e=>{console.error(e);process.exitCode=1;});
