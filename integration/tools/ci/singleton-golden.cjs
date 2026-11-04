// Independent EVM oracle: only pinned upstream Solidity and local transport harnesses.
'use strict';
const fs = require('node:fs'), path = require('node:path'), crypto = require('node:crypto'), assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const referenceDir = path.join(root, 'integration/tools/reference/singleton-cl');
const runtime = path.join(root, 'integration/.runtime/singleton-reference');
const modules = path.join(runtime, 'node_modules');
const solc = require(path.join(modules, 'solc'));
const ganache = require(path.join(modules, 'ganache'));
const {ethers} = require(path.join(modules, 'ethers'));
const hash = value => crypto.createHash('sha256').update(value).digest('hex');
const clone = value => JSON.parse(JSON.stringify(value));
const manifestBytes = fs.readFileSync(path.join(referenceDir, 'source-metadata.json'));
const manifest = JSON.parse(manifestBytes);
for (const entry of manifest.files) {
  assert.equal(hash(fs.readFileSync(path.join(runtime, 'sources', entry.local))), entry.sha256, 'source hash: ' + entry.local);
}
for (const [name, version] of [['solc','0.8.26'], ['ganache','7.9.2'], ['ethers','5.8.0']]) {
  assert.equal(require(path.join(modules, name, 'package.json')).version, version);
}
assert.ok(solc.version().startsWith('0.8.26+'));
const loaded = new Set();
const sourceInput = {};
for (const file of ['ReferenceTypes.sol', 'V4Reference.sol', 'InfinityReference.sol']) {
  sourceInput[file] = {content: fs.readFileSync(path.join(referenceDir, file), 'utf8')};
}
function compile(evmVersion) {
  const input = {language: 'Solidity', sources: sourceInput, settings: {
    optimizer: {enabled: true, runs: 200}, viaIR: true, evmVersion,
    metadata: {bytecodeHash: 'none', appendCBOR: false},
    outputSelection: {'*': {'*': ['abi', 'evm.bytecode.object', 'evm.deployedBytecode.object']}}
  }};
  const output = JSON.parse(solc.compile(JSON.stringify(input), {import: name => {
    const entry = manifest.files.find(f => f.local === name);
    if (!entry) return {error: 'Import outside pinned manifest: ' + name};
    loaded.add(name);
    return {contents: fs.readFileSync(path.join(runtime, 'sources', name), 'utf8')};
  }}));
  const errors = (output.errors || []).filter(e => e.severity === 'error');
  if (errors.length) throw Error(errors.map(e => e.formattedMessage).join('\n'));
  return {contracts: output.contracts, inputHash: hash(JSON.stringify(input)), warnings: (output.errors || []).map(e => e.formattedMessage)};
}
function forbiddenOpcodes(bytecode) {
  // Decode instructions, skipping PUSH data. Metadata is disabled at compilation.
  const bytes = Buffer.from(bytecode, 'hex'), hits = [];
  for (let pc = 0; pc < bytes.length; pc++) {
    const op = bytes[pc];
    if ([0x5c, 0x5d, 0x5e, 0x49, 0x4a].includes(op)) hits.push({pc, opcode: op});
    if (op >= 0x60 && op <= 0x7f) pc += op - 0x5f;
  }
  return hits;
}
async function main() {
  const builds = {};
  // Both compile the exact original library closure. Shanghai is executable in Ganache.
  for (const version of ['shanghai', 'cancun']) builds[version] = compile(version);
  fs.writeFileSync(path.join(runtime, 'compiled.json'), JSON.stringify(builds));
  if (process.argv.includes('--compile-only')) {
    console.log(JSON.stringify({compiler: solc.version(), source_files: loaded.size, targets: Object.keys(builds)}));
    return;
  }
  const raw = ganache.provider({logging: {quiet: true}, chain: {chainId: 31337, hardfork: 'shanghai'},
    wallet: {deterministic: true}, miner: {blockGasLimit: 100000000},
    database: {dbPath: undefined}});
  const provider = new ethers.providers.Web3Provider(raw), signer = provider.getSigner(0);
  const cases = [], compilation = {}, boundaryChecks = [], sequences = [], allocationCases = [], allocationSequences = [];
  try {
    for (const [protocol, contractName, manager, maxProtocolFee] of [
      ['uniswap-v4', 'V4Reference', '0x0303030303030303030303030303030303030303', 1000],
      ['pancake-infinity-cl', 'InfinityReference', '0x0404040404040404040404040404040404040404', 4000]
    ]) {
      const artifact = builds.shanghai.contracts[contractName + '.sol'][contractName];
      assert.deepEqual(forbiddenOpcodes(artifact.evm.bytecode.object), []);
      assert.deepEqual(forbiddenOpcodes(artifact.evm.deployedBytecode.object), []);
      const contract = await new ethers.ContractFactory(artifact.abi, artifact.evm.bytecode.object, signer).deploy();
      await contract.deployed();
      const deployed = await provider.getCode(contract.address);
      compilation[protocol] = {contract: contractName, deployed_code_sha256: hash(Buffer.from(deployed.slice(2), 'hex')),
        shanghai_creation_sha256: hash(Buffer.from(artifact.evm.bytecode.object, 'hex')),
        cancun_runtime_sha256: hash(Buffer.from(builds.cancun.contracts[contractName + '.sol'][contractName].evm.deployedBytecode.object, 'hex')),
        executed_target: 'shanghai', cancun_only_opcodes_in_executed_bytecode: []};
      const prices = new Map();
      async function price(tick) {
        if (!prices.has(tick)) prices.set(tick, (await contract.priceAtTick(tick)).toString());
        return prices.get(tick);
      }
      for (const spacing of [1, 10, 60, 200, 16383, 32767]) {
        const actual = (await contract.maxLiquidity(spacing)).toString();
        const count = Math.floor(887272 / spacing) - Math.floor(-887272 / spacing) + 1;
        assert.equal(actual, ((2n ** 128n - 1n) / BigInt(count)).toString());
        boundaryChecks.push({protocol, spacing, max_liquidity_per_tick: actual, compressed_tick_count: count});
      }
      async function makePool({fee=3000, p0=0, p1=0, spacing=60, tick=0, boundary=false,
        liquidity=1000000000n, multipliers=[-10,-2,2,10], gap=false, zeroLiquidity=false, fullWords=false}={}) {
        const tickCurrent = tick - (boundary ? 1 : 0);
        const nets = zeroLiquidity ? [] : gap ? [liquidity,-liquidity,2n*liquidity,-2n*liquidity]
          : [liquidity,2n*liquidity,-2n*liquidity,-liquidity];
        const ticks = nets.map((net,i) => ({tick: multipliers[i]*spacing, liquidity_gross: (net<0n?-net:net).toString(), liquidity_net: net.toString()}));
        const active = ticks.filter(t=>t.tick<=tickCurrent).reduce((n,t)=>n+BigInt(t.liquidity_net),0n);
        const currentWord = Math.floor(Math.floor(tickCurrent/spacing)/256);
        const wordPositions = ticks.map(t=>Math.floor((t.tick/spacing)/256));
        const lo = fullWords ? Math.floor(Math.floor(-887272/spacing)/256) : Math.min(-1,currentWord,...wordPositions);
        const hi = fullWords ? Math.floor(Math.floor(887272/spacing)/256) : Math.max(0,currentWord,...wordPositions);
        const words = [];
        for (let w=lo;w<=hi;w++) {
          let bitmap = 0n;
          for (const t of ticks) {
            const compressed = t.tick/spacing;
            if (Math.floor(compressed/256)===w) bitmap |= 1n << BigInt((compressed%256+256)%256);
          }
          words.push({word_pos:w,bitmap:bitmap.toString()});
        }
        const parameters = protocol==='uniswap-v4' ? '' : '0x'+(BigInt(spacing)<<16n).toString(16).padStart(64,'0');
        const token0='0x0101010101010101010101010101010101010101',token1='0x0202020202020202020202020202020202020202',hooks=ethers.constants.AddressZero;
        const encoded = protocol==='uniswap-v4'
          ? ethers.utils.defaultAbiCoder.encode(['address','address','uint24','int24','address'],[token0,token1,fee,spacing,hooks])
          : ethers.utils.defaultAbiCoder.encode(['address','address','address','address','uint24','bytes32'],[token0,token1,hooks,manager,fee,parameters]);
        const id = ethers.utils.keccak256(encoded);
        const managerPath = protocol==='uniswap-v4' ? 'v4/src/PoolManager.sol' : 'infinity/src/pool-cl/CLPoolManager.sol';
        return {chain_key:'evm:31337',pool_id:manager+'/'+id,protocol,token0,token1,reserve0:'0',reserve1:'0',fee_numerator:fee,fee_denominator:1000000,
          adapter_version:protocol==='uniswap-v4'?'uniswap-v4/core-1':'pancake-infinity-cl/cl-1',token_behavior:'standard',
          singleton_cl:{manager,manager_code_sha256:manifest.files.find(f=>f.local===managerPath).sha256,pool_id:id,lp_fee:fee,protocol_fee0:p0,protocol_fee1:p1,hooks,parameters,
            core:{sqrt_price_x96:await price(tick),tick:tickCurrent,liquidity:active.toString(),tick_spacing:spacing,max_steps:512,tick_words:words,ticks}}};
      }
      function seed(pool) {
        const s=pool.singleton_cl,c=s.core;
        return [c.sqrt_price_x96,c.tick,c.liquidity,c.tick_spacing,s.lp_fee,s.protocol_fee0+(s.protocol_fee1<<12),
          c.tick_words.map(w=>[w.word_pos,w.bitmap]),c.ticks.map(t=>[t.tick,t.liquidity_gross,t.liquidity_net])];
      }
      async function limit(pool,zero) {
        const c=pool.singleton_cl.core,words=c.tick_words;
        const t=zero ? Math.max(-887271,words[0].word_pos*256*c.tick_spacing)
          : Math.min(887271,(words.at(-1).word_pos+1)*256*c.tick_spacing-c.tick_spacing);
        return price(t);
      }
      function result(q,input) {
        const consumed=q.consumedIn.toString(),remaining=q.remainingIn.toString();
        assert.equal(BigInt(consumed)+BigInt(remaining),BigInt(input));
        return {reverted:false,amount_out:q.amountOut.toString(),consumed_in:consumed,remaining_in:remaining,
          full_input_consumed:remaining==='0',partial:remaining!=='0',zero_output:q.amountOut.isZero(),
          sqrt_price_x96:q.sqrtPriceX96.toString(),tick:q.tick,liquidity:q.liquidity.toString(),
          next_state:{sqrt_price_x96:q.sqrtPriceX96.toString(),tick:q.tick,liquidity:q.liquidity.toString()},swap_fee:q.swapFee,protocol_amount:q.protocolAmount.toString()};
      }
      function expectation(q,input) {
        if(q.reverted)return {reverted:true};
        if(q.partial || q.zero_output || q.swap_fee>=1000000 || BigInt(input)>(1n<<127n))return {reverted:true};
        return {amount_out:q.amount_out,consumed_in:q.consumed_in,remaining_in:q.remaining_in,
          sqrt_price_x96:q.sqrt_price_x96,tick:q.tick,liquidity:q.liquidity,next_state:q.next_state};
      }
      async function one(name,pool,input,zero,tags=[]) {
        input=String(input);
        const sqrtLimit=await limit(pool,zero);
        let q;
        try {q=result(await contract.callStatic.quote(seed(pool),input,zero,sqrtLimit,{gasLimit:80000000}),input);}
        catch(error) {
          const data=error.error?.data || error.data;
          if(typeof data!=='string' || !/^0x[0-9a-f]+$/i.test(data)) throw error;
          q={reverted:true,revert_data:data};
        }
        const expected=expectation(q,input);
        const rejection=q.reverted?'source-revert':q.partial?'partial-input':q.swap_fee>=1000000?'combined-fee-100-percent':q.zero_output?'zero-output':BigInt(input)>(1n<<127n)?'consumer-input-int128-bound':undefined;
        const entry={name:protocol+'/'+name,pool,input,zero_for_one:zero,tags,expected,reference:{...q,sqrt_price_limit_x96:sqrtLimit,...(rejection?{rejection_reason:rejection}:{})}};
        cases.push(entry);return entry;
      }
      const specs=[
        ['base',{},[]],
        ['directional-fee',{p0:maxProtocolFee,p1:17},['directional-fee']],
        ['reverse-directional-fee',{p0:7,p1:maxProtocolFee},['directional-fee']],
        ['zero-lp-fee',{fee:0,p0:maxProtocolFee,p1:maxProtocolFee},['lp-zero']],
        ['zero-all-fees',{fee:0},['lp-zero']],
        ['high-lp-fee',{fee:999999,p0:maxProtocolFee,p1:0},['high-fee']],
        ['low-fee-small-spacing',{fee:100,spacing:1},['spacing-1']],
        ['boundary-up',{tick:120,boundary:true},['tick-boundary']],
        ['boundary-down',{tick:-120,boundary:true},['tick-boundary']],
        ['crossing-tick',{},['crossing-tick']],
        ['gap',{gap:true},['zero-liquidity','crossing-tick']],
        ['empty-word',{multipliers:[-1000,-500,500,1000]},['empty-word']],
        ['no-liquidity',{zeroLiquidity:true},['zero-liquidity']],
        ['large-liquidity',{liquidity:1234567890123456789012345n},['large-liquidity']]
      ];
      for (const [name,config,tags] of specs) {
        const pool=await makePool(config);
        if(tags.includes('empty-word'))for(const zero of [true,false])for(const amount of ['10000000000','20000000000'])
          await one(`${name}/${zero}/${amount}`,pool,amount,zero,tags);
        for(const zero of [true,false])for(const amount of ['1','2','999','1000000','20000000','1000000000000000000'])
          await one(`${name}/${zero}/${amount}`,pool,amount,zero,tags);
      }
      const largeL=BigInt(await contract.maxLiquidity(32767))/3n;
      for(const fee of [0,3000]) {
        const pool=await makePool({spacing:32767,liquidity:largeL,multipliers:[-26,-20,20,26],fee});
        for(const zero of [true,false])for(const input of [(1n<<127n)-1n,1n<<127n,(1n<<127n)+1n,(1n<<128n)-1n])
          await one(`int128/${fee}/${zero}/${input}`,pool,input,zero,['int128-bound']);
      }
      for(const tick of [-800000,800000]) {
        const pool=await makePool({tick,spacing:16383,liquidity:1000000000000000000000000000000000n,multipliers:[-54,-50,50,54]});
        for(const zero of [true,false])for(const input of ['1','10000000000000000000000000000000000'])
          await one(`sqrt-edge/${tick}/${zero}/${input}`,pool,input,zero,['sqrt-overflow','int128-output-bound']);
      }
      for(const firstZero of [true,false]) {
        const initial=await makePool({p0:maxProtocolFee,p1:17});
        const inputs=['20000000','1000000','3000000'],zeros=[firstZero,!firstZero,firstZero];
        const limits=await Promise.all(zeros.map(z=>limit(initial,z)));
        const native=await contract.callStatic.sequence(seed(initial),inputs,zeros,limits,{gasLimit:80000000});
        let current=clone(initial);const names=[];
        for(let i=0;i<inputs.length;i++) {
          const entry=await one(`sequential/${firstZero}/${i}`,clone(current),inputs[i],zeros[i],['sequential','crossing-tick']);
          assert.deepEqual(entry.reference,{...result(native[i],inputs[i]),sqrt_price_limit_x96:limits[i]});
          assert.ok(!entry.expected.reverted);
          const next=current.singleton_cl.core;
          for(const field of ['sqrt_price_x96','tick','liquidity'])next[field]=entry.reference[field];
          names.push(entry.name);
        }
        sequences.push({protocol,first_zero_for_one:firstZero,cases:names,execution:'one original-pool sequence call compared with independently reseeded per-step calls'});
      }
      for(const zero of [true,false]) {
        const initial=await makePool({p0:maxProtocolFee,p1:17});
        const inputs=['20000000','1000000','3000000'],zeros=inputs.map(()=>zero);
        const limits=await Promise.all(zeros.map(z=>limit(initial,z)));
        const native=await contract.callStatic.sequence(seed(initial),inputs,zeros,limits,{gasLimit:80000000});
        let current=clone(initial);const names=[];
        for(let i=0;i<inputs.length;i++) {
          const entry=await one(`allocation/${zero}/${i}`,clone(current),inputs[i],zero,['allocation','same-direction','crossing-tick']);
          assert.deepEqual(entry.reference,{...result(native[i],inputs[i]),sqrt_price_limit_x96:limits[i]});
          assert.ok(!entry.expected.reverted);
          // Append the allocation cases after all pre-existing cases, preserving their order and contents.
          assert.equal(cases.pop(),entry);
          allocationCases.push(entry);
          const next=current.singleton_cl.core;
          for(const field of ['sqrt_price_x96','tick','liquidity'])next[field]=entry.reference[field];
          names.push(entry.name);
        }
        allocationSequences.push({protocol,zero_for_one:zero,same_direction:true,inputs,cases:names,
          execution:'one original-pool same-direction sequence call compared with independently reseeded per-step calls'});
      }
    }
    cases.push(...allocationCases);
    const reference={method:'unchanged pinned Pool.swap / CLPool.swap in Solidity storage-seeding harnesses',
      revisions:manifest.repositories.filter(r=>!r.local.includes('/lib/')),source_manifest_sha256:hash(manifestBytes),
      source_files_verified:manifest.files.length,compiled_primary_files:[...loaded].sort(),compiler:solc.version(),npm_lock_sha256:hash(fs.readFileSync(path.join(referenceDir,'package-lock.json'))),
      evm:'Ganache 7.9.2 Shanghai; original library closure compiled with solc 0.8.26 to both Shanghai and Cancun; Shanghai artifacts executed',
      expectation_policy:'expected.reverted also means full positive-output quote rejected for partial input, zero output, combined fee >= 1e6, or positive input > 2^127 (2^127 inclusive is source-valid); raw EVM result always retained under reference',
      manager_code_sha256_policy:'synthetic fixture identity: SHA256 of pinned manager SOURCE file, not deployed manager runtime bytecode; never a production code attestation',
      max_steps_policy:'512 is larger than every possible bitmap/tick iteration for these covered word windows; the original library has no configurable max_steps',
      price_limit_policy:'each call is bounded to the supplied continuous bitmap word window; omitted external words are never treated as known empty liquidity',
      limits:['No manager unlock, transient storage, settlement, hooks, token behavior, external RPC, or production deployed-code attestation is executed.',
        'Storage is seeded directly after native initialize; tick/position construction validation is not exercised.',
        'Cancun-target artifacts compile but are not executed; only original-library Shanghai artifacts execute.'],compilation,boundary_checks:boundaryChecks,sequences,allocation_sequences:allocationSequences};
    const fixture={schema_version:1,reference,cases};
    const text=JSON.stringify(fixture,null,2)+'\n';
    for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/singleton-quotes.json'),text);
    const counts={};
    for(const c of cases) {
      const count=counts[c.pool.protocol]??={cases:0,full_positive_quotes:0,source_reverts:0,partial_returns:0,zero_output_returns:0,consumer_rejections:0};
      count.cases++;count.full_positive_quotes+=Number(!c.expected.reverted);count.source_reverts+=Number(c.reference.reverted);
      count.partial_returns+=Number(!!c.reference.partial);count.zero_output_returns+=Number(!!c.reference.zero_output);
      count.consumer_rejections+=Number(!!c.expected.reverted&&!c.reference.reverted);
    }
    const report={cases:cases.length,allocation_cases:allocationCases.length,allocation_sequence_count:allocationSequences.length,
      native_sequence_count:sequences.length+allocationSequences.length,counts,fixture_sha256:hash(text),reference};
    fs.writeFileSync(path.join(root,'integration/artifacts/local/singleton-golden-generation.json'),JSON.stringify(report,null,2)+'\n');
    console.log(JSON.stringify({cases:cases.length,allocation_cases:allocationCases.length,allocation_sequence_count:allocationSequences.length,
      native_sequence_count:sequences.length+allocationSequences.length,counts,fixture_sha256:hash(text),compiler:solc.version(),compiled_primary_files:loaded.size}));
  } finally {await raw.disconnect();}
}
main().catch(error=>{console.error(error);process.exitCode=1;});
