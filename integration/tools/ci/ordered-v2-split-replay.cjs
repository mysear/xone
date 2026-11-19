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
const src = fs.readFileSync(path.join(root, 'dex-router/contracts/OrderedV2Split.sol'), 'utf8');
const ref = path.join(root, 'integration/tools/reference/evm-execution');
const pins = JSON.parse(fs.readFileSync(path.join(ref, 'source-pins.json'))).packages.flatMap(p => p.files);
function original(name) {
  const file = `packages/${name}.json`, bytes = fs.readFileSync(path.join(ref, file));
  assert.equal(hash(bytes), pins.find(p => p.path === file).sha256);
  return JSON.parse(bytes);
}
async function replay(transport) {
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
    const execute = (cap,min) => batch.execute(exactOut,recipient,cap,min,deadline,legs,{gasLimit:3000000});
    const receipt=await mined(execute(exactOut?input+123n:input,output));
    assert.deepEqual(await state(), expected);
    const swaps=receipt.logs.filter(l=>l.topics[0]===pairs[0].interface.getEventTopic('Swap')).map(l=>l.address);
    const order = reverse ? [2,3,0,1,3] : [0,1,3,2,3];
    assert.deepEqual(swaps,order.map(i=>pairs[i].address));
    assert.equal(expected.balances[2],'0'); assert.equal(expected.balances[3],'0');
    await restore();
    // Fail after at least one successful leg; no pair or token state may escape.
    const failure = await execute(exactOut?input-1n:input,exactOut?output:output+1n);
    await assert.rejects(failure.wait(), e => e.receipt?.status===0 && e.receipt.logs.length===0);
    assert.deepEqual(await state(),before);
    cases.push({kind:exactOut?'EXACT_OUT':'EXACT_IN',reverse,input:String(input),output:String(output),
      tx:receipt.transactionHash,gasUsed:String(receipt.gasUsed),effectiveGasPrice:String(receipt.effectiveGasPrice),
      nativeExecutionFee:String(receipt.gasUsed.mul(receipt.effectiveGasPrice)),failedTx:failure.hash,
      sharedPool: pairs[3].address,staleSecond:String(staleSecond),orderedSecond:String(orderedSecond)});
    await restore();
  }
  return {authority:'LOCAL_ORIGINAL_ROUTER_DIFFERENTIAL',bankHash:base.hash,sourceSha256:hash(src),compiler:solc.version(),
    runtimeSha256:hash(Buffer.from((await provider.getCode(batch.address)).slice(2),'hex')),priorApprovalGas:String(approval.gasUsed),
    normalQuoteConnected:false,completeNetCost:false,cases};
}
async function main() {
  const transport=ganache.provider({logging:{quiet:true},wallet:{deterministic:true},chain:{hardfork:'shanghai',time:new Date('2026-11-17Z')},miner:{timestampIncrement:1}});
  try {
    const report=await replay(transport);
    if (process.argv[2]) fs.writeFileSync(process.argv[2],JSON.stringify(report,null,2)+'\n',{flag:'wx'});
    console.log(JSON.stringify(report));
  } finally { await transport.disconnect(); }
}
main().catch(e=>{console.error(e);process.exitCode=1;});
