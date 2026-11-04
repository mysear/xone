#!/usr/bin/env node
'use strict';
// Only the original pinned Vyper implementation performs liquidity/swap math.
const fs = require('fs'), path = require('path'), cp = require('child_process'), crypto = require('crypto');
const ROOT = path.resolve(__dirname, '../../..');
const REF = path.join(ROOT, 'integration/tools/reference/curve-meta');
const receiptPath = path.join(REF, 'generation.json');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function requireThat(ok, message) { if (!ok) throw Error(message); }
function atomic(file, value) {
  const temp = file + '.tmp-' + process.pid;
  fs.writeFileSync(temp, typeof value === 'string' ? value : JSON.stringify(value, null, 2) + '\n');
  fs.renameSync(temp, file);
}
// Must precede ALL optional dependency loading, pin parsing, and compilation.
atomic(receiptPath, {generation_complete: false, stage: 'invalidated'});
let engine;
async function main() {
  const mods = process.env.CURVE_META_NODE_MODULES || path.join(ROOT, 'integration/.runtime/stable-reference/node_modules');
  const {ethers} = require(path.join(mods, 'ethers'));
  const ganache = require(path.join(mods, 'ganache'));
  const solc = require(path.join(mods, 'solc'));
  const pinPath = process.env.CURVE_META_SOURCE_PIN || path.join(REF, 'source-pin.json');
  const pin = JSON.parse(fs.readFileSync(pinPath));
  requireThat(pin.repo === 'curvefi/curve-factory' && pin.revision === '5272fd2b1199f185d9cd94822be9262c387dc2c2' &&
    pin.path === 'contracts/implementations/meta/MetaUSD.vy' &&
    pin.local === 'integration/tools/reference/curve-models/old/MetaUSD.vy' &&
    pin.sha256 === 'edfb3da7c748edb873465e60d471f49b6fd67be2972c97878948e9b9c2c98388' &&
    pin.compiler === '0.3.1' && pin.runtime_bytes === 20060 &&
    pin.runtime_sha256 === 'c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d', 'Original source pin mismatch');
  const source = path.join(ROOT, pin.local);
  requireThat(sha(fs.readFileSync(source)) === pin.sha256, 'Original source digest mismatch');
  const vyper = process.env.CURVE_META_VYPER || path.join(ROOT, 'integration/.runtime/curve-vyper/bin/vyper');
  const compiler = cp.execFileSync(vyper, ['--version'], {encoding:'utf8'}).trim();
  requireThat(/^0\.3\.1(?:\+|$)/.test(compiler), 'Expected Vyper 0.3.1');
  const fields = ['bytecode','bytecode_runtime','abi','layout'];
  const lines = cp.execFileSync(vyper, ['-f', fields.join(','), source], {encoding:'utf8', maxBuffer:8*1024*1024}).trim().split('\n');
  requireThat(lines.length === 4, 'Compiler artifact count');
  const original = {bytecode:lines[0], runtime:lines[1], abi:JSON.parse(lines[2]), layout:JSON.parse(lines[3])};
  // Vyper 0.3.1 gas hints omit Shanghai cold access and clone delegatecall costs.
  const executionABI = original.abi.map(({gas, ...entry}) => entry);
  const hexBytes = hex => Buffer.from(hex.replace(/^0x/, ''), 'hex');
  requireThat(hexBytes(original.runtime).length === 20060 && sha(hexBytes(original.runtime)) === pin.runtime_sha256, 'Compiled runtime mismatch');
  for (const [key, slot] of Object.entries({coins:2, balances:4, fee:6, rate_multiplier:11}))
    requireThat(original.layout[key].slot === slot, 'Original layout mismatch: ' + key);
  requireThat(solc.version().startsWith('0.8.19+'), 'Expected mock compiler Solidity 0.8.19');
  const mockSource = fs.readFileSync(path.join(REF, 'Fixtures.sol'), 'utf8');
  const solidityInput = {language:'Solidity', sources:{'Fixtures.sol':{content:mockSource}}, settings:{optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode.object']}}}};
  const compiled = JSON.parse(solc.compile(JSON.stringify(solidityInput)));
  requireThat(!(compiled.errors || []).some(x => x.severity === 'error'), JSON.stringify(compiled.errors));
  const fixtures = compiled.contracts['Fixtures.sol'];
  atomic(path.join(REF,'artifacts.json'), {original, solidity_input:solidityInput, solidity_output:compiled, solidity_compiler:solc.version()});
  atomic(path.join(REF,'layout.json'), original.layout);
  atomic(path.join(REF,'implementation-runtime.hex'), original.runtime + '\n');
  const E = 10n**18n, MAX = (1n<<256n)-1n, BASE_TIME = 1767225600;
  const BASE = '0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7';
  const LP = '0x6c3f90f043a72fa612cbac8115ee7e52bde6e490';
  const BASE_COINS = ['0x6b175474e89094c44da98b954eedeac495271d0f','0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48','0xdac17f958d2ee523a2206206994597c13d831ec7'];
  const VP = [E,E,1234567890123456789n,1234567890123456789n,987654321012345678n,987654321012345678n,1100000000000000000n,E];
  engine = ganache.provider({logging:{quiet:true}, chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false,time:new Date(BASE_TIME*1000)}, miner:{timestampIncrement:0,blockGasLimit:30000000},wallet:{deterministic:true}});
  const provider = new ethers.providers.Web3Provider(engine);
  provider.pollingInterval = 10;
  const signer = provider.getSigner(0), owner = (await signer.getAddress()).toLowerCase();
  const wait = async tx => (await tx).wait();
  const deploy = async (abi, bytecode, args=[]) => {
    const c = await new ethers.ContractFactory(abi,bytecode.startsWith('0x')?bytecode:'0x'+bytecode,signer).deploy(...args,{gasLimit:15000000});
    await c.deployed(); return c;
  };
  const deployFixture = name => deploy(fixtures[name].abi,fixtures[name].evm.bytecode.object);
  const tokenABI = fixtures.BookkeepingERC20.abi;
  const install = async (address,name) => { await provider.send('evm_setAccountCode',[address,'0x'+fixtures[name].evm.deployedBytecode.object]); return new ethers.Contract(address,fixtures[name].abi,signer); };
  const base = await install(BASE,'SuppliedVirtualPrice');
  for (let k=0;k<3;k++) { const c = await install(BASE_COINS[k],'BookkeepingERC20'); await wait(c.configure(k===0?18:6)); }
  const lp = await install(LP,'BookkeepingERC20'); await wait(lp.configure(18));
  await wait(base.setVirtualPrice(String(E)));
  const implementation = await deploy(executionABI,original.bytecode);
  const implReceipt = await provider.getTransactionReceipt(implementation.deployTransaction.hash);
  const implRuntime = await provider.getCode(implementation.address);
  requireThat(implRuntime.toLowerCase() === original.runtime.toLowerCase(), 'Actual implementation differs from compiler output');
  requireThat((await implementation.fee()).toString() === '31337', 'Implementation constructor lock missing');
  let locked = false;
  try { await implementation.callStatic.initialize('locked','LOCK',LP,String(E),123,4000000); } catch (_) { locked = true; }
  requireThat(locked, 'Implementation initializer unexpectedly usable');
  const factory = await deployFixture('CloneFactory');
  // Exercise EIP-170 itself: this initcode returns 24577 runtime bytes.
  const size = 24577;
  const overInit = '0x61'+size.toString(16).padStart(4,'0')+'80600c6000396000f3'+'00'.repeat(size);
  let sizeReceipt;
  try { const tx = await signer.sendTransaction({data:overInit,gasLimit:10000000}); sizeReceipt = await tx.wait(); }
  catch (e) { sizeReceipt = e.receipt; }
  requireThat(sizeReceipt && sizeReceipt.status === 0, 'EIP170 enforcement probe did not revert');
  const txInfo = async receipt => {
    const b = await provider.getBlock(receipt.blockHash);
    return {transaction_hash:receipt.transactionHash,block_number:receipt.blockNumber,block_hash:receipt.blockHash,timestamp:b.timestamp,status:receipt.status,gas_used:receipt.gasUsed.toString()};
  };
  const codeHash = hex => sha(hexBytes(hex));
  const reference = {revision:pin.revision,compiler,runtime_sha256:pin.runtime_sha256,runtime_bytes:20060,source_pin:pin,
    protocol:'curve-metausd-direct',adapter_version:'curve-metausd-direct/5272fd2b-fixed-base-1',
    hardfork:'shanghai',chain_id:31337,allow_unlimited_contract_size:false,base_pool_isolated_mock:true,
    timestamp_policy:'evm_setTime per phase, timestampIncrement=0; isolated-chain rewinds between scenarios',
    phase_virtual_prices:VP.map(String),base_timestamp:BASE_TIME,
    eip170_enforcement:await txInfo(sizeReceipt),implementation_constructor_fee:'31337',implementation_initialize_reverted:locked};
  const metadata = {reference,implementation:implementation.address.toLowerCase(),implementation_deployment:await txInfo(implReceipt),
    implementation_runtime_hex:implRuntime,implementation_runtime_sha256:codeHash(implRuntime),
    factory:factory.address.toLowerCase(),factory_runtime_hex:await provider.getCode(factory.address),
    base_pool:BASE,base_pool_runtime_hex:await provider.getCode(BASE),base_pool_code_sha256:codeHash(await provider.getCode(BASE)),
    token1:LP,base_coins:BASE_COINS,layout:original.layout,deployments:[]};
  atomic(path.join(REF,'metadata.json'),metadata);
  const scenarios=[], probes=[];
  const state = async (pool,token) => {
    const balances=await Promise.all([0,1].map(k=>pool.balances(k).then(String)));
    const raw=await Promise.all([token,lp].map(t=>t.balanceOf(pool.address).then(String)));
    const admin=await Promise.all([0,1].map(k=>pool.admin_balances(k).then(String)));
    for(let k=0;k<2;k++) requireThat(BigInt(raw[k])-BigInt(balances[k])===BigInt(admin[k]),'Native raw/effective/admin mismatch');
    return {balances,stored_balances:raw,admin_balances:admin,wallet_balances:await Promise.all([token,lp].map(t=>t.balanceOf(owner).then(String)))};
  };
  const verifyLayout = async (pool,token,rate,fee) => {
    const expected = {1:BigInt(factory.address),2:BigInt(token.address),3:BigInt(LP),6:BigInt(fee),7:12300n,8:12300n,9:0n,10:0n,11:rate};
    const storage={};
    for(const [slot,value] of Object.entries(expected)) {
      const got=await provider.getStorageAt(pool.address,Number(slot));
      requireThat(BigInt(got==='0x'?'0x0':got)===value,'Initialized storage mismatch: '+slot); storage[slot]=got;
    }
    for(const address of BASE_COINS) {
      const t=new ethers.Contract(address,tokenABI,signer);
      requireThat((await t.allowance(pool.address,BASE)).toString()===String(MAX),'Initializer base approval missing');
    }
    requireThat((await pool.coins(0)).toLowerCase()===token.address.toLowerCase() && (await pool.coins(1)).toLowerCase()===LP,'Native coin order');
    return storage;
  };
  async function execute(pool,token,row,receiver=owner,minOut='0') {
    Object.assign(row,await state(pool,token));
    const b=await provider.getBlock('latest'); Object.assign(row,{timestamp:b.timestamp,block_number:b.number,block_hash:b.hash});
    const args=[row.index_in,row.index_out,row.input,minOut,receiver];
    const method='exchange(int128,int128,uint256,uint256,address)';
    let quoted, staticRevert=false;
    try { quoted=String(await pool.callStatic[method](...args)); } catch (_) { staticRevert=true; }
    let receipt;
    try { receipt=await wait(pool[method](...args,{gasLimit:6000000})); } catch(e) { receipt=e.receipt; }
    requireThat(receipt,'Missing native transaction receipt');
    const after=await state(pool,token);
    row.expected={...await txInfo(receipt),reverted:receipt.status===0,next_balances:after.balances,next_stored_balances:after.stored_balances,next_admin_balances:after.admin_balances,next_wallet_balances:after.wallet_balances};
    requireThat(row.timestamp===row.expected.timestamp,'Execution timestamp changed');
    requireThat(staticRevert===(receipt.status===0),'Static and transaction outcomes disagree');
    if(receipt.status===0) {
      requireThat(receipt.logs.length===0,'Reverted transaction emitted logs');
      requireThat(JSON.stringify(after)===JSON.stringify({balances:row.balances,stored_balances:row.stored_balances,admin_balances:row.admin_balances,wallet_balances:row.wallet_balances}),'Native revert changed token or pool state');
    } else {
      row.expected.amount_out=quoted;
      const i=row.index_in,j=row.index_out;
      requireThat(BigInt(after.stored_balances[i])===BigInt(row.stored_balances[i])+BigInt(row.input),'Input transfer mismatch');
      requireThat(BigInt(after.stored_balances[j])===BigInt(row.stored_balances[j])-BigInt(quoted),'Output transfer mismatch');
      requireThat(BigInt(after.wallet_balances[i])===BigInt(row.wallet_balances[i])-BigInt(row.input) && BigInt(after.wallet_balances[j])===BigInt(row.wallet_balances[j])+BigInt(quoted),'Wallet transfer mismatch');
      const events=receipt.logs.filter(l=>l.address.toLowerCase()===pool.address.toLowerCase()).map(l=>pool.interface.parseLog(l));
      requireThat(events.length===1 && events[0].name==='TokenExchange' && events[0].args.tokens_bought.toString()===quoted,'Original TokenExchange event mismatch');
      row.expected.token_exchange={sold_id:String(events[0].args.sold_id),tokens_sold:String(events[0].args.tokens_sold),bought_id:String(events[0].args.bought_id),tokens_bought:String(events[0].args.tokens_bought)};
    }
    return row;
  }
  for(const [scenarioIndex,decimals] of [0,6,8,18].entries()) {
    await provider.send('evm_setTime',[BASE_TIME*1000]);
    await wait(base.setVirtualPrice(String(E)));
    const token=await deployFixture('BookkeepingERC20'); await wait(token.configure(decimals));
    const rate=10n**BigInt(36-decimals),unit=10n**BigInt(decimals),fee=[4000000,10000000,50000000,100000000][scenarioIndex];
    await wait(token.mint(owner,String(100000000n*unit))); await wait(lp.mint(owner,String(100000000n*E)));
    const initialization=implementation.interface.encodeFunctionData('initialize',['Original reference','META',token.address,String(rate),123,fee]);
    const created=await wait(factory.create(implementation.address,initialization,{gasLimit:6000000}));
    const event=created.logs.filter(l=>l.address.toLowerCase()===factory.address.toLowerCase()).map(l=>factory.interface.parseLog(l))[0];
    const pool=new ethers.Contract(event.args.pool,executionABI,signer);
    const cloneRuntime=await provider.getCode(pool.address);
    const wanted='0x363d3d373d3d3d363d73'+implementation.address.slice(2).toLowerCase()+'5af43d82803e903d91602b57fd5bf3';
    requireThat(cloneRuntime.toLowerCase()===wanted,'Actual EIP1167 runtime mismatch');
    const storage=await verifyLayout(pool,token,rate,fee);
    let repeatReverted=false; try { await pool.callStatic.initialize('again','AGAIN',token.address,String(rate),123,fee); } catch(_){repeatReverted=true;}
    requireThat(repeatReverted,'Clone initialized twice');
    await wait(token.approve(pool.address,String(MAX))); await wait(lp.approve(pool.address,String(MAX)));
    const seedAmounts=[String(1000000n*unit),String(1300000n*E)];
    const seedReceipt=await wait(pool['add_liquidity(uint256[2],uint256,address)'](seedAmounts,'0',owner,{gasLimit:6000000}));
    requireThat((await pool.totalSupply()).gt(0),'Original add_liquidity failed to mint LP shares');
    const s={name:'meta-'+decimals,meta_decimals:decimals,implementation:implementation.address.toLowerCase(),pool:pool.address.toLowerCase(),
      token0:token.address.toLowerCase(),token1:LP,base_pool:BASE,implementation_code_sha256:codeHash(implRuntime),pool_code_sha256:codeHash(cloneRuntime),implementation_runtime_hex:implRuntime,pool_runtime_hex:cloneRuntime,
      token0_runtime_hex:await provider.getCode(token.address),token1_runtime_hex:await provider.getCode(LP),base_runtime_hex:await provider.getCode(BASE),
      base_pool_code_sha256:metadata.base_pool_code_sha256,initialize:{...await txInfo(created),calldata:initialization,storage,repeat_initialize_reverted:repeatReverted},
      seed:{method:'add_liquidity(uint256[2],uint256,address)',amounts:seedAmounts,...await txInfo(seedReceipt),...await state(pool,token)},steps:[]};
    const common={meta_rate:String(rate),amp:'12300',fee:String(fee),admin_fee:'5000000000'};
    for(let phase=0;phase<8;phase++) {
      await provider.send('evm_setTime',[(BASE_TIME+phase)*1000]);
      await wait(base.setVirtualPrice(String(VP[phase])));
      const i=phase%2,dx=BigInt([1000,1100,70000,65000,1,1,250000,240000][phase])*(i===0?unit:E);
      const row=await execute(pool,token,{...common,phase,base_virtual_price:String(VP[phase]),input:String(dx),index_in:i,index_out:1-i});
      requireThat(!row.expected.reverted,'Live scenario swap reverted'); s.steps.push(row);
    }
    scenarios.push(s); metadata.deployments.push({...s,steps:undefined});
    atomic(path.join(REF,'metadata.json'),metadata);
    const corpus={schema_version:1,reference,scenarios,probes};
    for(const project of ['integration','dex-router']) atomic(path.join(ROOT,project,'test/contracts/curve-meta-direct.json'),corpus);
    console.log('scenario ready:',s.name,s.pool,'steps',s.steps.length);
    if(process.env.CURVE_META_FAIL_STAGE==='execution') throw Error('Injected execution failure');
    if(decimals===18) {
      const tests=[['zero-meta','0',0,'arithmetic'],['zero-lp','0',1,'arithmetic'],['dust-meta','1',0,'arithmetic'],['dust-lp','1',1,'arithmetic'],
        ['uint256-overflow',String(MAX),0,'arithmetic'],['minimum-output',String(E),0,'minimum-output'],['input-transfer-rollback',String(E),0,'input-transfer'],['output-transfer-rollback',String(E),0,'output-transfer']];
      for(const [name,input,i,kind] of tests) {
        const snapshot=await provider.send('evm_snapshot',[]);
        if(kind==='input-transfer') await wait(token.approve(pool.address,'0'));
        const row=await execute(pool,token,{...common,name,scenario:s.name,kind,base_virtual_price:String(E),input,index_in:i,index_out:1-i},kind==='output-transfer'?ethers.constants.AddressZero:owner,kind==='minimum-output'?String(MAX):'0');
        if(['minimum-output','input-transfer','output-transfer'].includes(kind)) requireThat(row.expected.reverted,'Expected rollback probe success');
        probes.push(row); requireThat(await provider.send('evm_revert',[snapshot]),'Probe snapshot restore failed');
      }
    }
  }
  const corpus={schema_version:1,reference,scenarios,probes};
  const text=JSON.stringify(corpus,null,2)+'\n';
  for(const project of ['integration','dex-router']) atomic(path.join(ROOT,project,'test/contracts/curve-meta-direct.json'),text);
  atomic(path.join(REF,'deployments.json'),metadata);
  // Hash loaded JS files and every distribution in the Vyper environment.
  const nodeModules=Object.keys(require.cache).filter(p=>p.startsWith(mods+path.sep)&&fs.statSync(p).isFile()).sort().map(p=>({path:path.relative(mods,p),sha256:sha(fs.readFileSync(p))}));
  const pyCode='import importlib.metadata as m,json,pathlib,hashlib,sys; rows=[]\nfor d in m.distributions():\n for f in d.files or []:\n  p=pathlib.Path(d.locate_file(f)).resolve()\n  if p.is_file() and p.suffix != ".pyc": rows.append({"path":str(p),"sha256":hashlib.sha256(p.read_bytes()).hexdigest()})\nprint(json.dumps({"python":sys.executable,"files":sorted(rows,key=lambda x:x["path"])}))';
  const compilerPackages=JSON.parse(cp.execFileSync(path.join(path.dirname(vyper),'python'),['-c',pyCode],{encoding:'utf8',maxBuffer:32*1024*1024}));
  const files=['integration/tools/ci/curve-meta-golden.cjs','integration/tools/ci/curve-meta-verify.py','integration/tools/ci/curve-meta-test.py',
    'integration/tools/reference/curve-meta/source-pin.json','integration/tools/reference/curve-meta/Fixtures.sol',
    'integration/tools/reference/curve-meta/artifacts.json','integration/tools/reference/curve-meta/layout.json','integration/tools/reference/curve-meta/implementation-runtime.hex',
    'integration/tools/reference/curve-meta/metadata.json','integration/tools/reference/curve-meta/deployments.json',
    'integration/test/contracts/curve-meta-direct.json','dex-router/test/contracts/curve-meta-direct.json','dex-router/crates/protocol-math/tests/curve_meta_reference.rs'];
  const closure={generation_complete:true,scenarios:4,steps:32,probes:8,reverted_probes:probes.filter(x=>x.expected.reverted).length,
    source_pin:pin,corpus_sha256:sha(text),compiler,compiler_executable:{path:vyper,sha256:sha(fs.readFileSync(vyper))},
    python_executable:{path:fs.realpathSync(compilerPackages.python),sha256:sha(fs.readFileSync(fs.realpathSync(compilerPackages.python)))},
    node_executable:{path:process.execPath,sha256:sha(fs.readFileSync(process.execPath))},
    node_lock:{path:path.join(path.dirname(mods),'package-lock.json'),sha256:sha(fs.readFileSync(path.join(path.dirname(mods),'package-lock.json')))},
    javascript_compiler:{path:path.join(mods,'solc/soljson.js'),sha256:sha(fs.readFileSync(path.join(mods,'solc/soljson.js'))),version:solc.version()},
    node_modules_root:mods,node_modules:nodeModules,compiler_packages:compilerPackages.files,
    files:files.map(p=>({path:p,sha256:sha(fs.readFileSync(path.join(ROOT,p)))}))};
  atomic(receiptPath,closure);
  console.log('completed: 4 scenarios, 32 original exchange transactions, 8 probes; corpus sha256',sha(text));
}
main().catch(e=>{console.error(e.stack||e);process.exitCode=1;}).finally(async()=>{if(engine) await engine.disconnect();});
