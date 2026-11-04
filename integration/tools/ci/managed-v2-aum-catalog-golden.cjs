#!/usr/bin/env node
'use strict';
// T34.8 original-bytecode catalog producer. Fixed fixture clock; no model math.
const fs = require('node:fs'), path = require('node:path'), assert = require('node:assert/strict');
const ROOT = fs.realpathSync(process.env.XONE_SOURCE_ROOT || path.resolve(__dirname, '../../..'));
const PRODUCER = 'integration/tools/ci/managed-v2-aum-catalog-golden.cjs';
const installed = fs.realpathSync(__filename) === path.join(ROOT, PRODUCER);
const defaults = ['integration', 'chain-parser-go', 'dex-router'].map(p =>
  path.join(ROOT, p, 'test/contracts/managed-v2-aum-catalog.json'));
const FIXTURE_DATE = '2026-01-01T00:00:00.000Z';
const REQUESTED_NODE = '/usr/local/bin/node25.3.0';
const FALLBACK_NODE = '/usr/local/bin/node'; // exact pinned binary, only if requested path is absent
const REF = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-aum');
const V = require(path.join(REF, 'validate.cjs'));
const BANK_SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66';
const ADAPTER = 'balancer-v2/20230411-managed-aum-3';
const same = (a,b,m) => assert.equal(V.canonical(a),V.canonical(b),m);
const clone = x => JSON.parse(JSON.stringify(x));
const lower = x => x.toLowerCase();
const number = x => {const n=Number(BigInt(x)); assert(Number.isSafeInteger(n));return n;};
// Match the original environment's explicit output declarations at their exact
// reference-root paths. file-manifest.json is bound separately as an INPUT.
const CHECKER = 'integration/tools/ci/managed-v2-aum-catalog-check.cjs';
const REFERENCE_PREFIX = 'integration/tools/reference/balancer-managed-v2-aum/';
const MANIFEST = REFERENCE_PREFIX+'file-manifest.json';
const REFERENCE_OUTPUTS = Object.freeze(['generation.json','verified.json',
  'output-0.pending','output-1.pending','output-2.pending'].map(n=>REFERENCE_PREFIX+n));
function sourcePath(logical) {
  if(logical===PRODUCER)return __filename;
  if(logical===CHECKER)return path.join(__dirname,path.basename(CHECKER));
  return path.join(ROOT,logical);
}
function qualifiedOriginalInputs() {
  const E=require(path.join(REF,'environment.cjs')), inputs=E.verifyInputs();
  const bytes=fs.readFileSync(path.join(ROOT,'integration/test/contracts/managed-v2-aum.json'));
  assert.equal(bytes.length,7905624,'frozen bank byte length');
  assert.equal(V.sha(bytes),BANK_SHA,'frozen bank SHA256');
  same(inputs,JSON.parse(bytes).reference.inputs,'exact original manifest SHA and members');
  // E.files() (called by verifyInputs) authenticates all pinned sources. Its
  // legacy name-based exclusions must not admit a new nested file of that name.
  const inputsSet=new Set(inputs.files.map(f=>f.path));
  function walk(dir) {
    for(const e of fs.readdirSync(dir,{withFileTypes:true})) {
      assert(!e.isSymbolicLink(),'reference symlink');
      const absolute=path.join(dir,e.name),logical=path.relative(ROOT,absolute);
      if(e.isDirectory())walk(absolute);
      else assert(logical===MANIFEST || REFERENCE_OUTPUTS.includes(logical) || inputsSet.has(logical),
        'unknown reference file outside exact original input/output boundary: '+logical);
    }
  }
  walk(REF);
  return inputs;
}
function preservation() {
  const inputs=qualifiedOriginalInputs(), paths=new Set(inputs.files.map(f=>f.path));
  for(const f of [MANIFEST,'integration/test/contracts/managed-v2-aum.json',
    'integration/test/contracts/managed-v2-aum-pools.json',
    'integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs',PRODUCER,CHECKER])paths.add(f);
  return [...paths].sort().map(logical=>{
    const f=sourcePath(logical);assert(!fs.lstatSync(f).isSymbolicLink(),'input symlink');
    return {path:logical,bytes:fs.statSync(f).size,sha256:V.sha(fs.readFileSync(f))};
  });
}
function toolInputs() {
  const inputs=qualifiedOriginalInputs();
  return [...inputs.files.map(f=>({...f,role:f.path.startsWith('integration/tools/ci/')?
    'frozen-original-producer':'immutable-original-reference'})),
    ...[[MANIFEST,'immutable-original-manifest'],
      ['integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs','original-template-producer'],
      ['integration/test/contracts/managed-v2-aum-pools.json','authenticated-original-public-template'],
      [PRODUCER,'catalog-producer'],[CHECKER,'catalog-authenticator']].map(([p,role])=>
        ({path:p,role,sha256:V.sha(fs.readFileSync(sourcePath(p)))}))];
}
function referenceOutputPolicy() {
  return {authority:'mutable reference outputs; never input provenance or qualification',
    paths:[...REFERENCE_OUTPUTS],manifest_is_input:MANIFEST,
    observations:'presence/bytes/SHA are emitted only in CLI log metadata, outside deterministic bank'};
}
function observedReferenceOutputs() {
  return REFERENCE_OUTPUTS.map(logical=>{
    try {const bytes=fs.readFileSync(path.join(ROOT,logical));
      return {path:logical,present:true,bytes:bytes.length,sha256:V.sha(bytes),qualifies_input:false};
    } catch(e) {if(e.code!=='ENOENT')throw e;return {path:logical,present:false,qualifies_input:false};}
  });
}
async function generate({collect30d=true}={}) {
  const preservationBefore = preservation();
  const bytes = fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum.json'));
  assert.equal(bytes.length, 7905624, 'frozen bank byte length');
  assert.equal(V.sha(bytes), BANK_SHA, 'frozen bank SHA256');
  const bank = V.validate(JSON.parse(bytes));
  const R = (ref, kind) => V.resolveObject(bank.objects, ref.ref, kind);
  const E = require(path.join(REF, 'environment.cjs'));
  E.verifyInputs();
  same(E.nodeClosure(), bank.reference.node_closure, 'exact original runtime pins');
  const A = require(path.join(REF, 'artifacts.cjs'));
  const {builds} = A.load();
  const {ethers} = E.rr('ethers');
  const ganache = E.rr('ganache');
  // Authenticate the thin projection's original reference before reusing only
  // its identity/code/link metadata. All live public values are overwritten.
  const thin = JSON.parse(fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum-pools.json')));
  assert.equal(thin.original_bank.sha256, V.sha(bytes), 'thin original digest');
  assert.equal(thin.original_bank.bytes, bytes.length);
  assert.equal(thin.original_bank.filename, 'managed-v2-aum.json');
  same(thin.original_bank.reference, bank.reference, 'thin complete original reference');
  const sourceSnapshot = R(bank.cases[0].execution.before, 'snapshot');
  const originalState = sourceSnapshot.state;
  const template = clone(thin.cases[0].source_pool);
  assert.equal(template.pool_id, originalState.pool);
  const identities = bank.reference.identities.map(ref => ({...R(ref, 'identity'), ref}));
  const ids = new Map(identities.map(i => [i.address, i]));
  const labels = new Map(identities.map(i => [i.label, i]));
  const iface = new Map();
  const runtime = new Map();
  const identityProofs = new Map();
  for (const i of identities) {
    const a = A.locate(builds, i.artifact.build, i.artifact.name);
    assert.equal(a.source, i.artifact.source);
    iface.set(i.address, new ethers.utils.Interface(a.c.abi));
    const code = R(i.code, 'code').hex;
    runtime.set(i.address, code);
    const normalized = Buffer.from(code.slice(2), 'hex');
    const d = a.c.evm.deployedBytecode, encoded = d.object.split('');
    const spans = [];
    if (a.library) spans.push({start: 1, length: 20});
    for (const libs of Object.values(d.linkReferences || {}))
      for (const ss of Object.values(libs)) spans.push(...ss);
    for (const ss of Object.values(d.immutableReferences || {})) spans.push(...ss);
    for (const s of spans) { normalized.fill(0, s.start, s.start+s.length); encoded.fill('0', s.start*2, (s.start+s.length)*2); }
    assert.equal(normalized.toString('hex'), encoded.join(''), 'original runtime template');
    identityProofs.set(i.address, {code_sha256: V.sha(Buffer.from(code.slice(2), 'hex')), template_sha256: V.sha(normalized)});
  }
  const multi = template.multi_asset, m = multi.managed_v2;
  function identity(address, codeHash, templateHash) {
    const proof = identityProofs.get(address); assert(proof, 'known original identity');
    assert.equal(codeHash, proof.code_sha256, 'thin code identity');
    if (templateHash !== undefined) assert.equal(templateHash, proof.template_sha256, 'thin compiler/link template');
  }
  identity(template.pool_id, multi.code_sha256, m.pool_runtime_template_sha256);
  identity(multi.vault, multi.vault_code_sha256);
  for (const k of ['factory', 'fee_provider', 'protocol_collector']) identity(m[k], m[k+'_code_sha256']);
  for (const dep of m.dependencies) identity(dep.address, dep.code_sha256, dep.template_sha256);
  for (const asset of multi.assets) identity(asset.token, asset.code_sha256);
  assert.equal(multi.vault, originalState.vault);
  assert.equal(m.factory, labels.get('ManagedPoolFactory').address);
  assert.equal(m.fee_provider, labels.get('ProtocolFeePercentagesProvider').address);
  assert.equal(m.protocol_collector, labels.get('ProtocolFeesCollector').address);
  same(m.dependencies.map(d => [d.role, d.address]), [
    ['circuit-breaker-lib', 'CircuitBreakerLib'], ['managed-pool-add-remove-token-lib', 'ManagedPoolAddRemoveTokenLib'],
    ['managed-pool-amm-lib', 'ManagedPoolAmmLib'], ['external-weighted-math', 'ExternalWeightedMath'],
    ['recovery-mode-helper', 'RecoveryModeHelper']].map(([role, label]) => [role, labels.get(label).address]), 'original link roles');
  same(multi.assets.map(a => a.token), originalState.underlying.map(ref => R(ref, 'token_state').address), 'original token order');
  const options = bank.reference.chain_options;
  assert.equal(new Date(options.chain.time).toISOString(), FIXTURE_DATE, 'fixed original fixture date');
  assert.equal(options.chain.hardfork, 'shanghai');
  assert.equal(options.chain.allowUnlimitedContractSize, false);
  assert.equal(options.chain.allowUnlimitedInitCodeSize, false);
  const raw = ganache.provider({...options, chain: {...options.chain, time: new Date(options.chain.time)}});
  const rpc = (method, params=[]) => raw.request({method, params});
  try {
    const provider = new ethers.providers.Web3Provider(raw);
    const wallets = [1, 3, 4].map(k => new ethers.Wallet('0x'+k.toString(16).padStart(64, '0'), provider));
    const [admin, lp, trader] = wallets;
    assert.equal(lower(admin.address), originalState.roles.admin);
    assert.equal(lower(trader.address), originalState.roles.trader);
    const originalHeaders = new Map(Object.values(bank.objects).filter(o => o.kind === 'header').map(o => [number(o.value.number), o.value]));
    const getHeader = async (tag='latest') => rpc('eth_getBlockByNumber', [tag, false]);
    const checkOriginalHeader = async height => {
      const h = await getHeader('0x'+height.toString(16));
      same(h, originalHeaders.get(height), 'original complete header '+height); return h;
    };
    await checkOriginalHeader(0);
    let height = 0;
    const prefix = [];
    for (const ref of bank.reference.transactions) {
      const t = R(ref, 'transaction'), h = R(t.execution_header, 'header');
      if (number(h.number) > 28) break;
      while (height+1 < number(h.number)) {
        const next = originalHeaders.get(height+1); assert(next, 'original empty block');
        await rpc('evm_setTime', [number(next.timestamp)*1000]); await rpc('evm_mine'); await checkOriginalHeader(++height);
      }
      same(await checkOriginalHeader(height), R(t.pre_header, 'header'), 'original tx source header');
      await rpc('evm_setTime', [number(h.timestamp)*1000]);
      const hash = await rpc('eth_sendRawTransaction', [t.signed_transaction]);
      assert.equal(hash, t.transaction.hash, 'original signed prefix hash');
      same(await rpc('eth_getTransactionReceipt', [hash]), t.receipt, 'original full prefix receipt');
      same(await rpc('eth_getTransactionByHash', [hash]), t.transaction, 'original full prefix transaction');
      height = number(h.number); await checkOriginalHeader(height); prefix.push(hash);
    }
    assert.equal(prefix.length, 28); assert.equal(height, 28);
    const headers = [await getHeader()], receipts = [], setupTransactions = [], setupGetters = [];
    const codeChecks = [];
    const activeIds = identities.filter(i => number(R(R(i.creation_transaction, 'transaction').execution_header, 'header').number) <= 28);

    const originalInputs = E.verifyInputs();
    const MAX = ethers.constants.MaxUint256.toString(), Z = ethers.constants.AddressZero;
    const vault = originalState.vault, factory = m.factory;
    const txs = [], prefixRecords = bank.reference.transactions.slice(0,28).map(r=>clone(R(r,'transaction')));
    const specs = JSON.parse(fs.readFileSync(path.join(REF,'matrix.json'))).slice(0,4);
    const scenarios = [], banks = {}, cases = [], proofs = new Map(), codeSeen = new Set();
    const linkAddresses = Object.fromEntries([...labels].map(([label,i])=>[label,i.address]));
    const artifact = (build,name)=>A.locate(builds,build,name);
    const contractArtifact = new Map(activeIds.map(i=>[i.address,artifact(i.artifact.build,i.artifact.name)]));
    const poolArtifact = artifact('ManagedPoolFactory','ManagedPool');
    function normalizeProof(address,a,code) {
      assert(code!=='0x','nonempty native runtime');
      const buf=Buffer.from(code.slice(2),'hex'), normalized=Buffer.from(buf), d=a.c.evm.deployedBytecode;
      assert(buf.length<=24576,'EIP170 runtime limit');
      const expected=d.object.split(''), spans=[], links=[], immutables=[];
      if(a.library)spans.push({start:1,length:20});
      for(const [source,libs]of Object.entries(d.linkReferences||{}))for(const [name,ss]of Object.entries(libs))for(const s of ss){
        const value='0x'+buf.subarray(s.start,s.start+s.length).toString('hex');
        assert.equal(value,linkAddresses[name],'actual native library link '+name);
        links.push({source,name,...s,address:value});spans.push(s);
      }
      for(const [id,ss]of Object.entries(d.immutableReferences||{})) {
        let name=null,type=null;
        function walk(x){if(x&&typeof x==='object'){if(String(x.id)===id&&x.nodeType==='VariableDeclaration'){name=x.name;type=x.typeDescriptions.typeString;}for(const v of Object.values(x))walk(v);}}
        walk(builds[a.build].output.sources);
        const values=ss.map(s=>'0x'+buf.subarray(s.start,s.start+s.length).toString('hex'));
        assert(values.every(x=>x===values[0]),'immutable repeated-span consistency');
        immutables.push({id,name,type,value:values[0],spans:ss});spans.push(...ss);
      }
      for(const s of spans){normalized.fill(0,s.start,s.start+s.length);expected.fill('0',s.start*2,(s.start+s.length)*2);}
      assert.equal(normalized.toString('hex'),expected.join(''),'actual native runtime versus original compiled template');
      const proof={address,artifact:{build:a.build,name:a.name,source:a.source},code_hex:code,runtime_bytes:buf.length,
        code_sha256:V.sha(buf),template_sha256:V.sha(normalized),links,immutables};
      return proof;
    }
    async function nativeProof(address,h) {
      const code=await rpc('eth_getCode',[address,h.number]);
      const prior=proofs.get(address);
      if(prior)assert.equal(code,prior.code_hex,'immutable runtime unchanged at fixed H');
      else proofs.set(address,normalizeProof(address,contractArtifact.get(address),code));
      const key=h.number+':'+address;
      if(!codeSeen.has(key)){codeSeen.add(key);codeChecks.push({address,block_tag:h.number,block_hash:h.hash,code_sha256:proofs.get(address).code_sha256});}
      return proofs.get(address);
    }
    async function refreshCodeHashes(pool,h) {
      const v=pool.multi_asset,s=v.managed_v2;
      v.code_sha256=(await nativeProof(pool.pool_id,h)).code_sha256;
      s.pool_runtime_template_sha256=proofs.get(pool.pool_id).template_sha256;
      v.vault_code_sha256=(await nativeProof(v.vault,h)).code_sha256;
      for(const [addr,field]of [[s.factory,'factory_code_sha256'],[s.fee_provider,'fee_provider_code_sha256'],[s.protocol_collector,'protocol_collector_code_sha256']])s[field]=(await nativeProof(addr,h)).code_sha256;
      for(const d of s.dependencies){const p=await nativeProof(d.address,h);d.code_sha256=p.code_sha256;d.template_sha256=p.template_sha256;}
      for(const a of v.assets)a.code_sha256=(await nativeProof(a.token,h)).code_sha256;
    }
    async function send(wallet,to,data,purpose) {
      const before=await getHeader();
      const from=lower(wallet.address), nonce=number(await rpc('eth_getTransactionCount',[from,before.number]));
      const signed=await wallet.signTransaction({type:0,chainId:31337,nonce,...(to?{to}:{}),data,gasLimit:55000000,gasPrice:2000000000});
      const hash=await rpc('eth_sendRawTransaction',[signed]);
      const receipt=await rpc('eth_getTransactionReceipt',[hash]), transaction=await rpc('eth_getTransactionByHash',[hash]), execution=await getHeader();
      assert.equal(number(receipt.status),1,'native successful receipt '+purpose);
      assert.equal(execution.parentHash,before.hash);assert.equal(number(execution.number),number(before.number)+1);
      assert.equal(execution.timestamp,before.timestamp,'timestampIncrement0');
      assert.equal(transaction.hash,ethers.utils.keccak256(signed));assert.equal(transaction.from,from);
      assert.equal(receipt.blockHash,execution.hash);assert.equal(transaction.blockHash,execution.hash);
      headers.push(execution);receipts.push(receipt);
      const tx={purpose,signed_transaction:signed,transaction,receipt,pre_header:before,execution_header:execution};txs.push(tx);return tx;
    }
    async function op(wallet,to,method,args,purpose=method) {
      const tx=await send(wallet,to,lower(iface.get(to).encodeFunctionData(method,args)),purpose);
      tx.method=method;tx.args=clone(args);return tx;
    }
    async function deployToken(spec,index,decimals) {
      const a=spec.n===50?artifact('TestBalancerToken','TestBalancerToken'):artifact('Authorizer','TestToken');
      const name='T34 current '+spec.name+' '+index, args=[lower(admin.address),name,'T34C'+index,...(a.name==='TestToken'?[decimals]:[])];
      const data=new ethers.ContractFactory(a.c.abi,A.link(a,linkAddresses)).getDeployTransaction(...args).data;
      const tx=await send(admin,null,data,spec.name+':deploy-token-'+index), address=lower(tx.receipt.contractAddress);
      assert.equal(address,lower(ethers.utils.getContractAddress({from:admin.address,nonce:tx.transaction.nonce})));
      iface.set(address,new ethers.utils.Interface(a.c.abi));contractArtifact.set(address,a);
      const p=await nativeProof(address,tx.execution_header);
      if(a.name==='TestBalancerToken') {
        assert.equal(p.immutables.find(x=>x.name==='_HASHED_NAME').value,ethers.utils.id(name));
        assert.equal(p.immutables.find(x=>x.name==='_HASHED_VERSION').value,ethers.utils.id('1'));
      }
      const actual=await callAt(address,'decimals',[],tx.execution_header,setupGetters);assert.equal(Number(actual[0]),decimals);
      return {address,decimals,artifact:a.name,deployment_hash:tx.transaction.hash};
    }
    function remapper(mapping) {
      return function visit(value){if(typeof value==='string')return mapping.get(lower(value))||value;
        if(Array.isArray(value))return value.map(visit);
        if(value&&typeof value==='object')return Object.fromEntries(Object.entries(value).map(([k,v])=>[k,visit(v)]));return value;};
    }
    function scenario(spec,poolAddress,tokens,pid,configuration,creation,initialization) {
      const orig=bank.scenarios.find(x=>x.name===spec.name), originalSnapshot=R(orig.source_h,'snapshot');
      const originalThin=thin.cases.find(c=>c.name.startsWith(spec.name+':')).source_pool;
      const mapping=new Map([[orig.pool,poolAddress],[originalThin.multi_asset.pool_id,pid],...orig.underlying.map((a,i)=>[a,tokens[i].address])]);
      const remap=remapper(mapping), template=remap(clone(originalThin));
      assert.equal(template.pool_id,poolAddress);same(template.multi_asset.assets.map(x=>x.token),tokens.map(x=>x.address));
      return {name:spec.name,underlying_count:spec.n,pool:poolAddress,pool_id:pid,tokens,configuration,creation,initialization,
        originalSnapshot,remap,template,original_matrix_swap:spec.ops.find(x=>x[1]==='swap')};
    }
    const orig2=bank.scenarios[0], spec2=specs[0];
    scenarios.push(scenario(spec2,orig2.pool,orig2.underlying.map(address=>({address,decimals:18,artifact:'TestBalancerToken'})),
      template.multi_asset.pool_id,clone(orig2.configuration),R(orig2.create,'transaction'),R(orig2.initialize,'transaction')));
    for(const spec of specs.slice(1)) {
      process.stderr.write('normal native deployment '+spec.name+' ('+spec.n+' underlying)\n');
      const nonce=number(await rpc('eth_getTransactionCount',[lower(admin.address),'latest']));
      const future=Array.from({length:spec.n},(_,i)=>lower(ethers.utils.getContractAddress({from:admin.address,nonce:nonce+i}))), sorted=[...future].sort(), tokens=[];
      for(let i=0;i<spec.n;i++)tokens.push(await deployToken(spec,sorted.indexOf(future[i]),spec.decimals[sorted.indexOf(future[i])]));
      tokens.sort((a,b)=>a.address.localeCompare(b.address));same(tokens.map(x=>x.address),sorted,'actual CREATE token order');
      const params={name:'T34 '+spec.name,symbol:'T34BPT',assetManagers:tokens.map(()=>Z)};
      const settings={tokens:tokens.map(t=>t.address),normalizedWeights:spec.weights,swapFeePercentage:'10000000000000000',
        swapEnabledOnStart:true,mustAllowlistLPs:false,managementAumFeePercentage:'10000000000000000',aumFeeId:3};
      const owner=originalState.roles.manager;
      const create=await op(admin,factory,'create',[params,settings,owner,ethers.constants.HashZero],spec.name+':create');
      const event=create.receipt.logs.filter(l=>l.address===factory).map(l=>{try{return iface.get(factory).parseLog(l);}catch{return null;}}).find(e=>e?.name==='PoolCreated');
      assert(event,'actual Factory PoolCreated');const poolAddress=lower(event.args.pool);
      iface.set(poolAddress,new ethers.utils.Interface(poolArtifact.c.abi));contractArtifact.set(poolAddress,poolArtifact);
      const pid=lower((await callAt(poolAddress,'getPoolId',[],create.execution_header,setupGetters))[0]);
      await nativeProof(poolAddress,create.execution_header);
      const initial=tokens.map(t=>(10000n*10n**BigInt(t.decimals)).toString());
      for(let i=0;i<tokens.length;i++){
        await op(admin,tokens[i].address,'mint',[lower(lp.address),initial[i]],spec.name+':mint-lp-'+i);
        await op(lp,tokens[i].address,'approve',[vault,MAX],spec.name+':approve-lp-'+i);
      }
      const indexIn=spec.ops.find(x=>x[1]==='swap')[2];
      await op(admin,tokens[indexIn].address,'mint',[lower(trader.address),(1000n*10n**BigInt(tokens[indexIn].decimals)).toString()],spec.name+':mint-trader');
      await op(trader,tokens[indexIn].address,'approve',[vault,MAX],spec.name+':approve-trader');
      const init=await op(lp,vault,'joinPool',[pid,lower(lp.address),lower(lp.address),{assets:[poolAddress,...tokens.map(t=>t.address)],maxAmountsIn:[MAX,...initial],fromInternalBalance:false,userData:ethers.utils.defaultAbiCoder.encode(['uint256','uint256[]'],[0,initial])}],spec.name+':INIT');
      scenarios.push(scenario(spec,poolAddress,tokens,pid,{params,settings,owner},create,init));
    }

    const keyFor = (to, method, args) => V.canonical([to, method, args]);
    async function callAt(to, method, args, h, records) {
      const abi = iface.get(to); assert(abi, 'original ABI address');
      const data = lower(abi.encodeFunctionData(method, args));
      const result = await rpc('eth_call', [{to, data}, h.number]);
      records.push({to, method, args: clone(args), data, result, block_tag: h.number, block_hash: h.hash});
      return abi.decodeFunctionResult(method, result);
    }
    async function snapshot(s, h) {
      const sourceSnapshot = s.originalSnapshot, template = clone(s.template);
      const m = template.multi_asset.managed_v2;
      const header_anchor_before = await getHeader(h.number);
      same(header_anchor_before, h, 'before getter header anchor');
      const getters = [], values = new Map();
      for (const ref of sourceSnapshot.getters) {
        const originalGetter = R(ref, 'getter');
        assert.equal(lower(iface.get(originalGetter.to).encodeFunctionData(originalGetter.method, originalGetter.args)), originalGetter.data, 'original getter ABI bytes before address rebinding');
        const g = s.remap(originalGetter);
        values.set(keyFor(g.to, g.method, g.args), await callAt(g.to, g.method, g.args, h, getters));
      }
      const g = (to, method, args=[]) => { const v = values.get(keyFor(to, method, args)); assert(v, 'observed getter '+method); return v; };
      const pool = clone(template), v = pool.multi_asset, state = v.managed_v2;
      delete state.execution_context;
      pool.adapter_version = ADAPTER;
      pool.chain_key = 'evm:'+options.chain.chainId;
      pool.protocol = bank.profile;
      pool.token0 = v.assets[0].token; pool.token1 = v.assets[1].token;
      pool.token_behavior = 'standard'; pool.fee_denominator = 10**18;
      v.amplification = '0'; v.admin_fee = 0;
      const poolAddress = pool.pool_id, vault = v.vault;
      state.block_number = number(h.number); state.timestamp = number(h.timestamp); state.block_hash = h.hash;
      assert.equal(lower(g(poolAddress, 'getVault')[0]), vault);
      assert.equal(lower(g(poolAddress, 'getPoolId')[0]), v.pool_id);
      assert.equal(lower(g(m.factory, 'getVault')[0]), vault);
      assert(g(m.factory, 'isPoolFromFactory', [poolAddress])[0]);
      const registered = g(vault, 'getPoolTokens', [v.pool_id]);
      const registration = g(vault, 'getPool', [v.pool_id]);
      assert.equal(lower(registration[0]), poolAddress); assert.equal(Number(registration[1]), 1);
      state.bpt_index = registered[0].map(lower).indexOf(poolAddress); assert.equal(state.bpt_index, 0);
      same(registered[0].map(lower).slice(1), v.assets.map(a => a.token), 'actual registered assets');
      state.manager = lower(g(poolAddress, 'getOwner')[0]);
      state.manager_bpt_balance = g(poolAddress, 'balanceOf', [state.manager])[0].toString();
      state.protocol_collector_bpt_balance = g(poolAddress, 'balanceOf', [state.protocol_collector])[0].toString();
      const fees = g(poolAddress, 'getManagementAumFeeParams');
      state.management_aum_fee = fees[0].toString(); state.last_collection_timestamp = number(fees[1]);
      state.provider_fee_id = g(poolAddress, 'getProviderFeeId', [3])[0].toString();
      state.live_provider_aum_percentage = g(state.fee_provider, 'getFeeTypePercentage', [Number(state.provider_fee_id)])[0].toString();
      state.cached_protocol_aum_percentage = g(poolAddress, 'getProtocolFeePercentageCache', [3])[0].toString();
      for (const [to, prefix] of [[poolAddress, ''], [vault, 'vault_']]) {
        const paused = g(to, 'getPausedState');
        state[prefix+'paused'] = paused[0]; state[prefix+'pause_window_end'] = number(paused[1]); state[prefix+'buffer_period_end'] = number(paused[2]);
      }
      state.swaps_enabled = g(poolAddress, 'getSwapEnabled')[0];
      state.recovery_mode = g(poolAddress, 'inRecoveryMode')[0];
      state.allowlist_enabled = g(poolAddress, 'getMustAllowlistLPs')[0];
      state.gradual_weight_end = number(g(poolAddress, 'getGradualWeightUpdateParams')[1]);
      state.gradual_fee_end = number(g(poolAddress, 'getGradualSwapFeeUpdateParams')[1]);
      v.weights = g(poolAddress, 'getNormalizedWeights')[0].map(x => x.toString());
      const scales = g(poolAddress, 'getScalingFactors')[0];
      const rawFee = g(poolAddress, 'getSwapFeePercentage')[0].toString();
      pool.fee_numerator = Number(rawFee);
      assert.equal(BigInt(pool.fee_numerator).toString(), rawFee, 'exact public fee integer');
      state.token_info = v.assets.map((asset, index) => {
        const info = g(vault, 'getPoolTokenInfo', [v.pool_id, asset.token]);
        asset.balance = info[0].add(info[1]).toString(); asset.scaling = scales[index].toString();
        assert.equal(registered[1][index+1].toString(), asset.balance);
        const breaker = g(poolAddress, 'getCircuitBreakerState', [asset.token]);
        assert(breaker.every(x => x.toString() === '0'), 'supported inactive original breaker');
        return {cash: info[0].toString(), managed: info[1].toString(),
          vault_actual_balance: g(asset.token, 'balanceOf', [vault])[0].toString(),
          asset_manager: lower(info[3]), last_change_block: number(info[2]), circuit_breaker_enabled: false};
      });
      pool.reserve0 = v.assets[0].balance; pool.reserve1 = v.assets[1].balance;
      const bpt = g(vault, 'getPoolTokenInfo', [v.pool_id, poolAddress]);
      assert.equal(registered[1][state.bpt_index].toString(), bpt[0].add(bpt[1]).toString(), 'registered original BPT balance');
      state.total_supply = g(poolAddress, 'totalSupply')[0].toString();
      state.virtual_supply = (BigInt(state.total_supply)-BigInt(bpt[0])-BigInt(bpt[1])).toString();
      state.actual_supply = g(poolAddress, 'getActualSupply')[0].toString();
      state.pending_aum = (BigInt(state.actual_supply)-BigInt(state.virtual_supply)).toString();
      state.vault_bpt_cash = bpt[0].toString(); state.vault_bpt_managed = bpt[1].toString();
      state.vault_bpt_actual_balance = g(poolAddress, 'balanceOf', [vault])[0].toString();
      assert.equal(Object.keys(state).length, 39, 'complete public adapter 3 fields');
      await refreshCodeHashes(pool, h);
      assert.equal((await callAt(poolAddress, 'name', [], h, []) )[0], s.configuration.params.name);
      same(state.dependencies.map(d=>d.address), s.template.multi_asset.managed_v2.dependencies.map(d=>d.address), 'actual dependencies');
      const header_anchor_after = await getHeader(h.number);
      same(await getHeader(h.number), h, 'after getter header anchor');
      return {scenario: s.name, pool, header: h, header_anchor_before, header_anchor_after, getters, getter_count: getters.length};
    }
    async function verifyPoolImmutables(s,sn) {
      const pool=sn.pool,v=pool.multi_asset,m=v.managed_v2,p=proofs.get(pool.pool_id),get=(name)=>p.immutables.find(i=>i.name===name)?.value;
      const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x).toHexString(),32).toLowerCase();
      const expected={_vault:v.vault,_owner:m.manager,_poolId:v.pool_id,_protocolFeesCollector:m.protocol_collector,
        _protocolFeeProvider:m.fee_provider,_actionIdDisambiguator:m.factory,_pauseWindowEndTime:m.pause_window_end,
        _bufferPeriodEndTime:m.buffer_period_end,_HASHED_NAME:ethers.utils.id(s.configuration.params.name),
        _HASHED_VERSION:ethers.utils.id('1'),_TYPE_HASH:ethers.utils.id('EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)'),
        _weightedMath:m.dependencies.find(x=>x.role==='external-weighted-math').address,
        _recoveryModeHelper:m.dependencies.find(x=>x.role==='recovery-mode-helper').address};
      const ids=[0,2,3].map(id=>{const g=sn.getters.find(g=>g.to===s.pool&&g.method==='getProviderFeeId'&&g.args[0]===id);assert(g);return BigInt(iface.get(s.pool).decodeFunctionResult(g.method,g.result)[0]);});
      expected._feeIds=ids[0]|(ids[1]<<8n)|(ids[2]<<16n);
      for(const i of p.immutables){assert(Object.hasOwn(expected,i.name),'complete original pool immutable interpretation '+i.name);assert.equal(i.value,word(expected[i.name]),'original pool constructor immutable '+i.name);}
      assert(get('_HASHED_NAME'),'actual pool name immutable');
    }
    async function captureBank(label) {
      const h=await getHeader();
      const id='H'+number(h.number),pools=[];
      for(const s of scenarios){const sn=await snapshot(s,h);await verifyPoolImmutables(s,sn);pools.push(sn);}
      same(await getHeader(h.number),h,'full bank header anchor');
      banks[id]={id,label,header:h,header_anchor_before:h,header_anchor_after:await getHeader(h.number),pools,
        getter_count:pools.reduce((n,s)=>n+s.getter_count,0),authority:'actual original-bytecode getters at one common fixed blockTag'};
      return id;
    }
    function balance(sn,token,owner) {
      const row=sn.getters.find(g=>g.to===token&&g.method==='balanceOf'&&lower(g.args[0])===lower(owner));assert(row,'recorded wallet getter');
      return BigInt(iface.get(token).decodeFunctionResult('balanceOf',row.result)[0]);
    }
    const factoryStorageChecks=[];
    for(const row of bank.reference.factory_code_storage.contracts){
      const h=await getHeader(),code=await rpc('eth_getCode',[row.address,h.number]);
      assert.equal(code,R(row.code,'code').hex,'unchanged original factory creation-code storage');
      assert((code.length-2)/2<=24576,'normal creation-code storage EIP170');
      factoryStorageChecks.push({address:row.address,block_tag:h.number,block_hash:h.hash,code_hex:code,code_sha256:V.sha(Buffer.from(code.slice(2),'hex'))});
    }
    const initialBank=await captureBank('initial complete four-pool bank after normal deployment');
    let current=initialBank;
    for(const s of scenarios) {
      process.stderr.write('native Vault.swap and common-H bank '+s.name+'\n');
      const [, ,i,j,amount]=s.original_matrix_swap,tokenIn=s.tokens[i].address,tokenOut=s.tokens[j].address;
      const before=banks[current].pools.find(p=>p.scenario===s.name);
      const args=[{kind:0,poolId:s.pool_id,assetIn:tokenIn,assetOut:tokenOut,amount,userData:'0x'},
        {sender:lower(trader.address),recipient:lower(trader.address),fromInternalBalance:false,toInternalBalance:false},0,MAX];
      const tx=await op(trader,vault,'swap',args,s.name+':current-native-swap');
      const afterId=await captureBank(s.name+':after native Vault.swap'),after=banks[afterId].pools.find(p=>p.scenario===s.name);
      const amountOut=balance(after,tokenOut,trader.address)-balance(before,tokenOut,trader.address);
      const amountIn=balance(before,tokenIn,trader.address)-balance(after,tokenIn,trader.address);
      assert.equal(amountIn.toString(),amount);assert(amountOut>0n,'real positive native swap output');
      const events=tx.receipt.logs.filter(l=>l.address===vault).map(l=>{try{return iface.get(vault).parseLog(l);}catch{return null;}}).filter(x=>x?.name==='Swap');
      assert.equal(events.length,1);const e=events[0].args;
      assert.equal(lower(e.poolId),s.pool_id);assert.equal(lower(e.tokenIn),tokenIn);assert.equal(lower(e.tokenOut),tokenOut);
      assert.equal(e.amountIn.toString(),amount);assert.equal(e.amountOut.toString(),amountOut.toString());
      cases.push({name:s.name+':current-native-swap',scenario:s.name,operation:'swap',source_bank4pools:current,expected_after_bank4pools:afterId,
        execution:tx,expected_native:{status:1,amount_in:amount,amount_out:amountOut.toString(),trader_input_delta:amountIn.toString(),trader_output_delta:amountOut.toString(),
          swap_event:{pool_id:s.pool_id,token_in:tokenIn,token_out:tokenOut,amount_in:e.amountIn.toString(),amount_out:e.amountOut.toString()},
          authority:'actual signed Vault.swap receipt + native ERC20 wallet differences + original Vault Swap event'},
        counts:{source_getters:banks[current].getter_count,after_getters:banks[afterId].getter_count}});
      current=afterId;
    }
    let clockBank=null;
    if(collect30d){
      const previous=await getHeader(),timestamp=number(previous.timestamp)+2592000;
      await rpc('evm_setTime',[timestamp*1000]);await rpc('evm_mine');const h=await getHeader();
      assert.equal(number(h.timestamp),timestamp);assert.equal(h.parentHash,previous.hash);headers.push(h);
      clockBank=await captureBank('all four pools actually reread after deterministic +30d');current=clockBank;
      for(const s of scenarios){
        const before=banks[current].pools.find(x=>x.scenario===s.name),b=before.pool.multi_asset.managed_v2;
        assert(BigInt(b.pending_aum)>0n,'positive native pending AUM');
        const tx=await op(trader,s.pool,'collectAumManagementFees',[],s.name+':current-native-collect-30d');
        const afterId=await captureBank(s.name+':after native AUM collection'),after=banks[afterId].pools.find(x=>x.scenario===s.name),a=after.pool.multi_asset.managed_v2;
        const minted=BigInt(a.total_supply)-BigInt(b.total_supply),manager=BigInt(a.manager_bpt_balance)-BigInt(b.manager_bpt_balance),collector=BigInt(a.protocol_collector_bpt_balance)-BigInt(b.protocol_collector_bpt_balance);
        assert(minted>0n);assert.equal(minted,manager+collector);assert.equal(a.pending_aum,'0');
        cases.push({name:s.name+':current-native-collect-30d',scenario:s.name,operation:'collect',source_bank4pools:current,expected_after_bank4pools:afterId,
          execution:tx,expected_native:{status:1,minted_bpt:minted.toString(),manager_bpt_delta:manager.toString(),protocol_collector_bpt_delta:collector.toString(),authority:'actual original totalSupply and owner/collector wallet differences'},
          counts:{source_getters:banks[current].getter_count,after_getters:banks[afterId].getter_count}});current=afterId;
      }
    }
    // Load only the original pinned wrapper required by the original Node closure.
    E.rr('solc07/wrapper');
    const loaded=E.verifyLoaded(),preservationAfter=preservation();same(preservationBefore,preservationAfter,'complete frozen source file preservation');
    const observedIdentities=[...proofs.values()];
    return {schema_version:1,profile:'balancer-v2-managed-aum-current-catalog',public_adapter_version:ADAPTER,
      authority:'independent local fixed-fixture catalog; original published buildinfo and actual original-bytecode execution; distinct bank',
      fixture_date:FIXTURE_DATE,actual_iso:{first:new Date(number(banks[initialBank].header.timestamp)*1000).toISOString(),
        last:new Date(number(banks[current].header.timestamp)*1000).toISOString()},
      clock_authority:'fixed original chain_options genesis + explicit 30-day advance; no wall-clock cutoff',
      original_bank_sha256:BANK_SHA,original_bank_bytes:bytes.length,
      generator:{filename:PRODUCER,sha256:V.sha(fs.readFileSync(__filename)),source_root:'.',requested_node:REQUESTED_NODE,actual_node:process.execPath,
        requested_path_present:fs.existsSync(REQUESTED_NODE),actual_node_sha256:V.sha(fs.readFileSync(process.execPath)),collect_30d:collect30d},
      original_bank:{filename:'managed-v2-aum.json',sha256:BANK_SHA,bytes:bytes.length,reference:bank.reference,
        tools_inputs:toolInputs()},
      reference_outputs:referenceOutputPolicy(),
      original_reference:{input_closure:originalInputs,node_closure:loaded,source_pins_sha256:bank.reference.source_pins_sha256,compiler_jobs:bank.reference.compiler_jobs},
      source_preservation:{before:preservationBefore,after:preservationAfter},chain_options:options,
      prefix_replay:{count:28,complete_transaction_receipt_header_equality:true,genesis_header:originalHeaders.get(0),transactions:prefixRecords,headers:[...originalHeaders].filter(([n])=>n<=28).sort(([a],[b])=>a-b).map(([,h])=>h)},
      factory_creation_code_storage:factoryStorageChecks,
      scenarios:scenarios.map(({originalSnapshot,remap,template,...rest})=>rest),identities:observedIdentities,code_checks:codeChecks,setup_getters:setupGetters,
      transactions:txs,headers,initial_bank4pools:initialBank,clock_bank4pools:clockBank,final_bank4pools:current,banks,cases,
      counts:{pools:4,underlying_tokens:60,prefix_transactions:28,new_transactions:txs.length,swap_cases:4,collection_cases:collect30d?4:0,banks:Object.keys(banks).length,
        raw_getters:Object.values(banks).reduce((n,b)=>n+b.getter_count,0),native_identities:observedIdentities.length},
      limitations:['independent local reference, not a replacement or time rewrite of frozen bank',
        'T34.9 shared-token overlay across physical pools is not implemented; underlying tokens are disjoint',
        'two pool begins at genuine prefix28 state (two original swaps already executed)',
        'original Authorizer/TestToken with original decimals for three/five; original TestBalancerToken (18 decimals) for fifty',
        'all pools use 1% swap fee, 1% annual AUM fee, provider ID3; original weights and swap directions/amounts',
        'no production service integration, live-chain validation, storage proof or execution trace',
        'requested versioned Node pathname absent here; actual explicit /usr/local/bin/node has exact original version and binary digest']};
  } finally {await raw.disconnect();}
}
function argumentsFor(argv) {
  const args = argv.flatMap(a => /^--(?:input|output)=/.test(a) ? [a.slice(0, a.indexOf('=')), a.slice(a.indexOf('=')+1)] : [a]);
  if (!args.length) return {targets: defaults};
  if (args.length === 2 && args[0] === '--output' && args[1]) return {targets: [path.resolve(args[1])]};
  if (args.length === 1 && args[0] === '--check') return {check: true, targets: defaults};
  if (args.length === 3 && args[0] === '--check' && args[1] === '--input' && args[2])
    return {check: true, targets: [path.resolve(args[2])]};
  throw Error('usage: [--output PATH | --check [--input PATH]]');
}
function outputPath(target) {
  const absolute=path.resolve(target), parent=fs.realpathSync(path.dirname(absolute));
  const resolved=path.join(parent,path.basename(absolute));
  if (!installed) {
    const tmp=fs.realpathSync('/tmp');
    assert(parent===tmp || parent.startsWith(tmp+path.sep),'uninstalled candidate outputs must be in /tmp');
    assert(!(resolved===ROOT || resolved.startsWith(ROOT+path.sep)), 'read-only source ROOT');
  }
  assert(!(resolved===REF || resolved.startsWith(REF+path.sep)),'reference artifact overwrite refused');
  assert(!preservation().some(f=>fs.realpathSync(sourcePath(f.path))===resolved),'protected original source/bank overwrite');
  assert.notEqual(resolved,fs.realpathSync(__filename),'producer overwrite');
  if(fs.existsSync(absolute)){assert(!fs.lstatSync(absolute).isSymbolicLink(),'output symlinks refused');assert(fs.statSync(absolute).isFile());}
  return absolute;
}
async function main(){
  const {targets,check}=argumentsFor(process.argv.slice(2));
  if(!check) {
    assert(installed || targets.length===1,'uninstalled candidate requires explicit --output /tmp/PATH');
    targets.forEach(outputPath);
  } else targets.forEach(p=>assert(fs.statSync(p).isFile(),'local input required'));
  const allowed=fs.existsSync(REQUESTED_NODE)?REQUESTED_NODE:FALLBACK_NODE;
  assert.equal(fs.realpathSync(process.execPath),fs.realpathSync(allowed),'explicit pinned Node executable required');
  const catalog=await generate(),text=JSON.stringify(catalog,null,2)+'\n';
  for(const output of targets) {
    if(check) assert.equal(V.sha(fs.readFileSync(output)),V.sha(text),'exact native regeneration '+output);
    else fs.writeFileSync(output,text,{flag:'w'});
  }
  process.stdout.write(JSON.stringify({passed:true,mode:check?'exact-regeneration-check':'generate',targets,sha256:V.sha(text),bytes:Buffer.byteLength(text),counts:catalog.counts,
    statuses:catalog.cases.map(c=>c.expected_native.status),fixture_date:catalog.fixture_date,actual_iso:catalog.actual_iso,reference_outputs_observed:observedReferenceOutputs()})+'\n');
}
module.exports={generate,outputPath,argumentsFor,preservation,sourcePath,toolInputs,referenceOutputPolicy,observedReferenceOutputs};
if(require.main===module)main().catch(e=>{console.error(e.stack||e);process.exitCode=1;});
