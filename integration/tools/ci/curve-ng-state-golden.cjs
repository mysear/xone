// Original pinned full exchange is the business oracle. Only ERC20s are mocks.
'use strict';
// Only built-in filesystem/path access precedes invalidation. Optional runtime
// dependencies and even source-pin JSON parsing may fail synchronously.
const fs = require('fs'), path = require('path');
const root = path.resolve(__dirname, '../../..');
const ref = path.join(root, 'integration/tools/reference/curve-models');
const output = path.join(root, 'integration/tools/reference/curve-ng-state');
const receipts = ['execution-closure.json', 'metadata-probe.json'];
let stage = 'startup', backend;
function atomicReceipt(name, value) {
  const destination=path.join(output,name), temporary=destination+'.tmp-'+process.pid;
  fs.writeFileSync(temporary,JSON.stringify(value,null,2)+'\n');
  fs.renameSync(temporary,destination);
}
async function closeBackend() {
  if(backend) { const owned=backend; backend=undefined; await owned.disconnect(); }
}
function failed(error) {
  process.exitCode=1;
  for(const name of receipts) {
    try { atomicReceipt(name,{generation_complete:false,status:'FAILED',stage,error:String(error && error.message || error)}); }
    catch(writeError) { console.error('Cannot write failed generation receipt:',writeError); }
  }
  console.error(error);
  void closeBackend().catch(cleanupError=>console.error('Provider cleanup failed:',cleanupError));
}
process.once('uncaughtException',failed);
process.once('unhandledRejection',failed);
for(const name of receipts) atomicReceipt(name,{generation_complete:false,status:'RUNNING',stage});
const cp = require('child_process'), crypto = require('crypto');
stage = 'dependency-initialization';
const modules = path.join(root, 'integration/.runtime/stable-reference/node_modules');
const { ethers } = require(path.join(modules, 'ethers'));
const ganache = require(path.join(modules, 'ganache'));
const solc = require(path.join(modules, 'solc'));
const vyper = path.join(ref, '.runtime/vyper-0.3.10-py313/bin/vyper');
function hashTree(dir) {
  const rows=[];
  function walk(current) { for(const item of fs.readdirSync(current,{withFileTypes:true}).sort((a,b)=>a.name.localeCompare(b.name))) {
    const p=path.join(current,item.name);
    if(item.isDirectory()) { if(!['__pycache__','pip','wheel'].includes(item.name)) walk(p); }
    else if(item.isFile() && !item.name.endsWith('.pyc')) rows.push({path:path.relative(dir,p),sha256:sha(fs.readFileSync(p))});
  }}
  walk(dir); return rows;
}
const sha = data => crypto.createHash('sha256').update(data).digest('hex');
const E = 10n ** 18n, U128 = 2n ** 128n, MAX = 2n ** 256n - 1n;
const pack = (a, b) => a | b << 128n;
stage = 'source-pins';
const pins = JSON.parse(fs.readFileSync(path.join(ref, 'sources.json')));
const pin = pins.find(x => x.local === 'ng/CurveStableSwapNG.vy');
async function main() {
  if (pin.repo !== 'curvefi/stableswap-ng' || pin.path !== 'contracts/main/CurveStableSwapNG.vy' || pin.revision !== '3332cd44656ec64b3c048885e1dd71955254b262' ||
      pin.sha256 !== 'dfe8f8a4dda288c41c630ea4be9bb0e5d61244c50dfdeaf1981a098b55ee12ab') throw Error('Unexpected source pin');
  const source = fs.readFileSync(path.join(ref, pin.local));
  if (sha(source) !== pin.sha256) throw Error('Source hash mismatch');
  stage = 'compiler';
  const compilerPackages=hashTree(path.join(ref,'.runtime/vyper-0.3.10-py313/lib/python3.13/site-packages'));
  const compilerEnv = { ...process.env, PYTHONWARNINGS: 'ignore' };
  const compiler = cp.execFileSync(vyper, ['--version'], { encoding: 'utf8', env: compilerEnv, stdio: ['ignore','pipe','pipe'] }).trim();
  if (!/^0\.3\.10(?:\+|$)/.test(compiler)) throw Error('Expected Vyper 0.3.10');
  const compileArgs = ['-f', 'bytecode,bytecode_runtime,abi,layout', path.join(ref, pin.local)];
  const compileOutput = cp.execFileSync(vyper, compileArgs, { encoding: 'utf8', env: compilerEnv, stdio: ['ignore','pipe','pipe'], maxBuffer: 2 ** 23 }).trim();
  const lines = compileOutput.split('\n');
  const artifact = { bytecode: lines[0], runtime: lines[1], abi: JSON.parse(lines[2]), layout: JSON.parse(lines[3]) };
  fs.writeFileSync(path.join(output, 'compiler-artifact.json'), JSON.stringify(artifact, null, 2) + '\n');
  fs.writeFileSync(path.join(output, 'compiler-layout.json'), JSON.stringify(artifact.layout, null, 2) + '\n');
  // Unmodified helper bodies with visibility-only wrappers, separate from full exchange.
  const text = source.toString();
  const expBody = text.slice(text.indexOf('@internal\n@pure\ndef exp('), text.indexOf('# ---------------------------- ERC20 Utils'));
  const pBody = text.slice(text.indexOf('@internal\n@pure\ndef _get_p('), text.indexOf('@internal\ndef upkeep_oracles'));
  const dBody=text.slice(text.indexOf('@pure\n@internal\ndef get_D('),text.indexOf('@pure\n@internal\ndef get_y_D('));
  const probeSource = '# @version 0.3.10\nMAX_COINS: constant(uint256) = 8\nMAX_COINS_128: constant(int128) = 8\nA_PRECISION: constant(uint256) = 100\nN_COINS: immutable(uint256)\nN_COINS_128: immutable(int128)\n@external\ndef __init__(n: uint256):\n    N_COINS = n\n    N_COINS_128 = convert(n, int128)\n' + expBody.replace('@internal', '@external') + pBody.replace('@internal', '@external') + dBody.replace('@internal','@external');
  const probePath = path.join(output, 'original-helpers.vy');
  fs.writeFileSync(probePath, probeSource);
  const probeCompile = cp.execFileSync(vyper, ['-f', 'bytecode,abi', probePath], { encoding: 'utf8', env: compilerEnv, stdio: ['ignore','pipe','pipe'] }).trim().split('\n');
  const probeArtifact = { bytecode: probeCompile[0], abi: JSON.parse(probeCompile[1]) };
  const tokenSource = 'pragma solidity 0.8.19; contract T { mapping(address=>uint256) public balanceOf; uint8 public immutable decimals; constructor(uint8 d){decimals=d;} function set(address a,uint256 x)external{balanceOf[a]=x;} function transfer(address to,uint256 x)external returns(bool){balanceOf[msg.sender]-=x;balanceOf[to]+=x;return true;} function transferFrom(address from,address to,uint256 x)external returns(bool){balanceOf[from]-=x;balanceOf[to]+=x;return true;} }';
  const solcInput = { language: 'Solidity', sources: { 'T.sol': { content: tokenSource } }, settings: { outputSelection: { '*': { '*': ['abi', 'evm.bytecode.object'] } } } };
  const solcOutput = JSON.parse(solc.compile(JSON.stringify(solcInput)));
  if ((solcOutput.errors || []).some(x => x.severity === 'error')) throw Error(JSON.stringify(solcOutput.errors));
  const token = solcOutput.contracts['T.sol'].T;
  const tokenArtifact = { abi: token.abi, bytecode: token.evm.bytecode.object };
  stage = 'execution';
  backend = ganache.provider({ logging: { quiet: true }, chain: { hardfork: 'shanghai', allowUnlimitedContractSize: true, time: new Date('2026-01-01T00:00:00Z') }, miner: { timestampIncrement: 0 }, wallet: { deterministic: true } });
  const provider = new ethers.providers.Web3Provider(backend);
  provider.pollingInterval = 10;
  const signer = provider.getSigner(0), owner = await signer.getAddress();
  const word = x => ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(), 32);
  const deploy = async (a, args = []) => {
    const contract = await new ethers.ContractFactory(a.abi, a.bytecode, signer).deploy(...args, { gasLimit: 25000000 });
    await contract.deployed(); return contract;
  };
  const storage = artifact.layout.storage_layout;
  const set = async (pool, key, value, index) => provider.send('evm_setAccountStorageAt', [pool.address, word(BigInt(storage[key].slot) + (index === undefined ? 0n : BigInt(index + 1))), word(value)]);
  const raw = async (pool, key, index) => String(BigInt(await provider.getStorageAt(pool.address, Number(storage[key].slot) + (index === undefined ? 0 : index + 1)) || '0x0'));
  const read = async (pool, n, timestamp) => ({
    stored_balances: await Promise.all(Array.from({ length: n }, (_, i) => raw(pool, 'stored_balances', i))),
    admin_balances: await Promise.all(Array.from({ length: n }, async (_, i) => String(await pool.admin_balances(i)))),
    last_prices_packed: await Promise.all(Array.from({ length: n - 1 }, (_, i) => raw(pool, 'last_prices_packed', i))),
    last_d_packed: await raw(pool, 'last_D_packed'), ma_last_time: String(await pool.ma_last_time()),
    ma_exp_time: String(await pool.ma_exp_time()), d_ma_time: String(await pool.D_ma_time()), timestamp,
  });
  const scenarios = [], deployments = [], probes = new Map();
  async function originalMath(n) { if(!probes.has(n)) probes.set(n,await deploy(probeArtifact,[n])); return probes.get(n); }
  for (const config of [
    { name: 'zero-fee', n: 2, fee: 0n, offpeg: 10n**10n },
    { name: 'maximum-fee', n: 3, fee: 5000000000n, offpeg: 10n**10n },
    { name: 'unit-raw-input', n: 3, mixed: true, dx: 1n },
    { name: 'two-sequential', n: 2, b: [1000000n*E,1700000n*E] },
    { name: 'three-mixed-decimals', n: 3, mixed: true },
    { name: 'four', n: 4 }, { name: 'five', n: 5 }, { name: 'six', n: 6 }, { name: 'seven', n: 7 }, { name: 'eight', n: 8 },
    { name: 'independent-oracle-times', n: 3, priceTime: -500, dTime: 500 },
    { name: 'future-oracle-times', n: 2, priceTime: 100000, dTime: 100001 },
    { name: 'max-u64-window', n: 2, maWindow: 2n**64n-1n, dWindow: 2n**64n-1n },
    { name: 'zero-window-unsafe-div', n: 2, maWindow: 0n, dWindow: 0n },
    { name: 'capped-price', n: 2, b: [1000000n*E,100n*E], amp: 100n },
    { name: 'lower-price', n: 2, b: [100n*E,1000000n*E], amp: 100n },
    { name: 'zero-spot', n: 2, b: [10n,10n**22n], amp: 100n, dx: 1n, inputIndex: 0 },
    { name: 'D-square-overflow', n: 2, b: [U128,U128] },
    { name: 'stored-input-overflow', n: 2, rawOverflow: true },
    { name: 'large-admin-balance', n: 2, adminOverflow: true },
    { name: 'xp-product-overflow', n: 2, b: [MAX,MAX] },
  ]) {
    const n = config.n, decimals = Array.from({ length: n }, (_, i) => config.mixed && i % 2 ? 6 : 18);
    const rates = decimals.map(d => 10n ** BigInt(36-d));
    const tokens = await Promise.all(decimals.map(d => deploy(tokenArtifact, [d])));
    const args = ['State reference', 'STATE', 100, '4000000', '20000000000', 866, tokens.map(t => t.address), rates.map(String), Array(n).fill(0), Array(n).fill('0x00000000'), Array(n).fill(ethers.constants.AddressZero)];
    const pool = await deploy(artifact, args);
    const original=await originalMath(n);
    const code = await provider.getCode(pool.address), codeBytes = Buffer.from(code.slice(2), 'hex');
    const immutableLength = Math.max(...Object.values(artifact.layout.code_layout).map(x => x.offset + x.length));
    const start = codeBytes.length - immutableLength;
    const blob = codeBytes.subarray(start);
    if (codeBytes.subarray(0, start).toString('hex') !== artifact.runtime.slice(2)) throw Error('Compiled/deployed runtime prefix mismatch');
    const at = offset => BigInt('0x' + blob.subarray(offset, offset + 32).toString('hex'));
    const c = artifact.layout.code_layout;
    if (at(c.N_COINS.offset) !== BigInt(n) || Number(await pool.N_COINS()) !== n) throw Error('N_COINS suffix mismatch');
    for (const key of ['coins','asset_types','rate_multipliers','rate_oracles','call_amount','scale_factor']) {
      if (at(c[key].offset) !== BigInt(n)) throw Error(key + ' length mismatch');
      for (let i = 0; i < 8; i++) {
        const value = at(c[key].offset + (i + 1) * 32);
        const expected = i >= n ? 0n : key === 'coins' ? BigInt(tokens[i].address) : key === 'rate_multipliers' ? rates[i] : 0n;
        if (value !== expected) throw Error(key + ' immutable item mismatch ' + i);
      }
    }
    if (at(c.pool_contains_rebasing_tokens.offset) !== 0n) throw Error('Rebase flag mismatch');
    const gotCoins = await Promise.all(tokens.map((_, i) => pool.coins(i)));
    const gotRates = (await pool.stored_rates()).map(String);
    if (gotCoins.some((v,i) => v.toLowerCase() !== tokens[i].address.toLowerCase()) || gotRates.some((v,i) => v !== String(rates[i]))) throw Error('Public getter mismatch');
    deployments.push({ scenario: config.name, address: pool.address, constructor_args: args, runtime_bytes: codeBytes.length, runtime_sha256: sha(codeBytes), runtime_keccak256: ethers.utils.keccak256(code), immutable_offset: start, immutable_bytes: immutableLength, immutable_hex: '0x'+blob.toString('hex'), public_n_coins: n, public_coins: gotCoins, public_rates: gotRates });
    fs.writeFileSync(path.join(output, 'runtime-'+config.name+'.hex'), code+'\n');
    const effective = config.b || rates.map((r,k) => BigInt(1000000+k*700000)*E*E/r);
    const admin = effective.map((_,i) => config.rawOverflow && i===0 ? MAX-effective[i] : config.adminOverflow && i===1 ? MAX-effective[i] : BigInt(i+1)*13n);
    for (let i=0; i<n; i++) {
      const stored = effective[i] + admin[i] > MAX ? effective[i] : effective[i] + admin[i];
      // For huge checked-overflow probes keep raw storage itself in uint256.
      if (effective[i] + admin[i] > MAX) admin[i]=0n;
      await set(pool,'stored_balances',stored,i); await set(pool,'admin_balances',admin[i],i);
      await (await tokens[i].set(pool.address,String(stored))).wait();
      await (await tokens[i].set(owner,String(MAX/2n))).wait();
    }
    const amp = config.amp || 12345n;
    const fee=config.fee===undefined?4000000n:config.fee, offpeg=config.offpeg===undefined?20000000000n:config.offpeg;
    await set(pool,'fee',fee); await set(pool,'offpeg_fee_multiplier',offpeg);
    await set(pool,'initial_A',amp); await set(pool,'future_A',amp);
    const base = 1767225600;
    for (let i=0; i<n-1; i++) await set(pool,'last_prices_packed',pack(E+BigInt(i)*123456789n, E-12345n),i);
    await set(pool,'last_D_packed',pack(1234567890123456789012345n,987654321012345678901234n));
    await set(pool,'ma_last_time',pack(BigInt(base+(config.priceTime||0)),BigInt(base+(config.dTime||0))));
    if(config.maWindow!==undefined) await set(pool,'ma_exp_time',config.maWindow);
    if(config.dWindow!==undefined) await set(pool,'D_ma_time',config.dWindow);
    const steps=[];
    const times = config.name.includes('overflow') ? [0] : [0,0,1,866,866,90000,90001,90001];
    for(let k=0;k<times.length;k++) {
      const i=config.inputIndex===undefined?k%n:config.inputIndex, j=(i+1)%n, timestamp=base+times[k];
      await provider.send('evm_setTime',[timestamp*1000]); await provider.send('evm_mine',[]);
      const state=await read(pool,n,timestamp), balances=(await pool.get_balances()).map(String);
      const dx = config.dx===undefined?1000n*E*E/rates[i]:config.dx;
      const preBlock=await provider.getBlock('latest');
      const row={ snapshot:{number:preBlock.number,hash:preBlock.hash,timestamp:preBlock.timestamp},balances, rates:rates.map(String), amp:String(amp),fee:String(fee),admin_fee:'5000000000',offpeg_fee_multiplier:String(offpeg),input:String(dx),index_in:i,index_out:j,state };
      try { row.original_invariant=String(await original.get_D(balances.map((b,k)=>String(BigInt(b)*rates[k]/E)),String(amp))); }
      catch(e) { if(e.code!=='CALL_EXCEPTION')throw e; row.original_invariant=null; }
      row.exchange={sender:owner,receiver:owner,min_dy:'0',calldata:pool.interface.encodeFunctionData('exchange(int128,int128,uint256,uint256,address)',[i,j,String(dx),0,owner])};
      let amount;
      try { amount=String(await pool.callStatic['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner)); }
      catch(e) { if(e.code!=='CALL_EXCEPTION') throw e; row.expected={reverted:true}; }
      if(amount!==undefined) {
        const receipt=await (await pool['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:6000000})).wait();
        const block=await provider.getBlock(receipt.blockNumber);
        if(block.timestamp!==timestamp) throw Error('Timestamp drift');
        const next=await read(pool,n,timestamp);
        if(BigInt(await pool.D_oracle())!==BigInt(next.last_d_packed)/U128) throw Error('Private/public D oracle mismatch');
        for(let p=0;p<n-1;p++) {
          if(BigInt(next.last_prices_packed[p])%U128!==BigInt(await pool.last_price(p)) || BigInt(next.last_prices_packed[p])/U128!==BigInt(await pool.ema_price(p))) throw Error('Private/public price mismatch');
        }
        row.expected={amount_out:amount,next_balances:(await pool.get_balances()).map(String),next_state:next};
        row.execution={transaction_hash:receipt.transactionHash,block_number:receipt.blockNumber,block_hash:block.hash,timestamp,status:1};
      } else {
        let receipt;
        try { await (await pool['exchange(int128,int128,uint256,uint256,address)'](i,j,String(dx),0,owner,{gasLimit:6000000})).wait(); }
        catch(e) { if(!e.receipt || e.receipt.status!==0) throw e; receipt=e.receipt; }
        if(!receipt) throw Error('Expected reverted transaction succeeded');
        const block=await provider.getBlock(receipt.blockNumber);
        if(block.timestamp!==timestamp) throw Error('Revert timestamp drift');
        row.execution={transaction_hash:receipt.transactionHash,block_number:receipt.blockNumber,block_hash:block.hash,timestamp,status:0};
        const next=await read(pool,n,timestamp);
        if(JSON.stringify(next)!==JSON.stringify(state)) throw Error('Revert altered state');
      }
      steps.push(row);
    }
    scenarios.push({name:config.name,pool:{coin_count:n,coins:gotCoins,decimals,asset_types:Array(n).fill(0),pool_contains_rebasing_tokens:false,rate_multipliers:rates.map(String),rate_oracles:Array(n).fill('0'),call_amount:Array(n).fill('0'),scale_factor:Array(n).fill('0'),runtime_sha256:sha(codeBytes),runtime_keccak256:ethers.utils.keccak256(code),immutable_offset:start,immutable_bytes:immutableLength},steps});
    console.log('original NG state',config.name,steps.length);
    if(process.argv.includes('--metadata-only')) break;
  }
  const expCases=[],spotCases=[],invariantCases=[];
  const probe=await originalMath(2);
  for(const x of [...Array.from({length:32},(_,i)=>BigInt(i*2-30)*1234567890123456789n),-41446531673892822314n,-41446531673892822313n,-41446531673892822312n,-E,-1n,0n,1n,E,135305999368893231588n,135305999368893231589n,-(2n**255n),(2n**255n)-1n]) {
    const row={x:String(x)};
    try{row.expected=String(await probe.exp(String(x)));}catch(e){if(e.code!=='CALL_EXCEPTION')throw e;row.reverted=true;}
    expCases.push(row);
  }
  for(const xp of [[1n,2n],[E,10000n*E],[10n,10n**22n],[MAX,1n],[0n,E]]) {
    for(const amp of [100n,MAX]) {
      const row={xp:xp.map(String),amp:String(amp),d:'1000000000000000000'};
      try{row.expected=(await probe._get_p(xp.map(String),String(amp),row.d)).map(String);}catch(e){if(e.code!=='CALL_EXCEPTION')throw e;row.reverted=true;}
      spotCases.push(row);
    }
  }
  for(const xp of [[2n,3n],[2n,3n,5n],[215n,130n,257n],[11n,23n,37n,53n],[1n,0n],[U128,U128]]) {
    const original=await originalMath(xp.length); const row={xp:xp.map(String),amp:'500'};
    try { row.expected=String(await original.get_D(xp.map(String),row.amp)); } catch(e) {if(e.code!=='CALL_EXCEPTION')throw e;row.reverted=true;}
    invariantCases.push(row);
  }
  const fixture={schema_version:1,reference:'unmodified original full CurveStableSwapNG.exchange; original helper bodies visibility-only probes',source_pin:pin,compiler,profile:'asset_type=0, fixed decimal rates, non-rebasing exact ERC20 transfers, u64 timestamp/windows',scenarios,exp_cases:expCases,spot_cases:spotCases,invariant_cases:invariantCases};
  const fixtureText=JSON.stringify(fixture,null,2)+'\n';
  if(!process.argv.includes('--metadata-only')) for(const project of ['integration','dex-router']) fs.writeFileSync(path.join(root,project,'test/contracts/curve-ng-state.json'),fixtureText);
  const moduleClosure=Object.keys(require.cache).filter(p=>p.startsWith(modules+path.sep)).sort().map(p=>({path:path.relative(modules,p),sha256:sha(fs.readFileSync(p))}));
  const closure={generation_complete:true,status:'COMPLETE',source_pin:pin,execution_environment:{hardfork:'shanghai',allow_unlimited_contract_size:true,chain_time:'2026-01-01T00:00:00Z',timestamp_increment:0,deterministic_wallet:true},node_executable_sha256:sha(fs.readFileSync(process.execPath)),node_modules:moduleClosure,node_lock_sha256:sha(fs.readFileSync(path.join(root,'integration/.runtime/stable-reference/package-lock.json'))),compiler,compiler_executable_sha256:sha(fs.readFileSync(vyper)),compiler_packages:compilerPackages,compiler_args:['-f','bytecode,bytecode_runtime,abi,layout','ng/CurveStableSwapNG.vy'],compiler_output_sha256:sha(compileOutput),creation_bytecode_sha256:sha(Buffer.from(artifact.bytecode.slice(2),'hex')),runtime_template_sha256:sha(Buffer.from(artifact.runtime.slice(2),'hex')),runtime_template_bytes:(artifact.runtime.length-2)/2,layout_sha256:sha(JSON.stringify(artifact.layout)),helper_source_sha256:sha(probeSource),helper_scope:'original exp/_get_p/get_D bodies with internal->external visibility only; N_COINS set in constructor',token_source:tokenSource,token_source_sha256:sha(tokenSource),solc_version:solc.version(),solc_input:solcInput,solc_output_sha256:sha(JSON.stringify(solcOutput)),node:process.version,ganache:require(path.join(modules,'ganache/package.json')).version,ethers:ethers.version,deployments,fixture_sha256:sha(fixtureText),helper_creation_sha256:sha(Buffer.from(probeArtifact.bytecode.slice(2),'hex')),generator_sha256:sha(fs.readFileSync(__filename)),scenarios:scenarios.length,steps:scenarios.reduce((a,s)=>a+s.steps.length,0),reverts:scenarios.flatMap(s=>s.steps).filter(s=>s.expected.reverted).length};
  stage = 'publish';
  atomicReceipt('metadata-probe.json',closure);
  if(!process.argv.includes('--metadata-only')) atomicReceipt('execution-closure.json',closure);
  console.log('immutable evidence',JSON.stringify({runtime_bytes:deployments[0].runtime_bytes,immutable_offset:deployments[0].immutable_offset,immutable_bytes:deployments[0].immutable_bytes,public_n_coins:deployments[0].public_n_coins,public_coins:deployments[0].public_coins,public_rates:deployments[0].public_rates}));
}
(async()=>{try{await main();}finally{await closeBackend();}})().catch(failed);
