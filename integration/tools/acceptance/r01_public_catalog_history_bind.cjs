#!/usr/bin/env node
'use strict';
// Rebuild actual V2 Pair and V3 Pool/Factory, then bind proof-authenticated pool
// instances and the complete original-state execution reference. No quote kernel.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog');
const {ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers'));
const {bindImmutables,compileStandardHeap}=require('./r01_public_catalog_sources.cjs');
const wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'));
const sha=x=>crypto.createHash('sha256').update(x).digest('hex'),load=p=>JSON.parse(fs.readFileSync(p)),bytes=h=>Buffer.from(h.replace(/^0x/,''),'hex');
function pinnedCompiler(version){const manifest=load(path.join(BASE,'public-source-build-inputs.json')),pin=manifest.compilers.find(p=>p.version===version);assert(pin,'COMPILER_PIN_MISSING');const file=path.join(ROOT,pin.blob.path);assert.equal(sha(fs.readFileSync(file)),pin.blob.sha256,'COMPILER_BYTES');assert.equal(sha(fs.readFileSync(path.join(ROOT,pin.official_list.path))),pin.official_list.sha256,'OFFICIAL_COMPILER_LIST_BYTES');const list=load(path.join(ROOT,pin.official_list.path)).builds.find(b=>b.longVersion===version);assert.equal(list.sha256.replace(/^0x/,''),pin.blob.sha256,'OFFICIAL_COMPILER_PIN');return {pin,file,module:require(file),compiler:wrapper(require(file))};}
function bindV3(template,runtime,poolAddress,expected={}){
 const linked=bindImmutables(template.deployedBytecode,runtime,template.immutableReferences),fields={};
 for(const [id,value] of Object.entries(linked.bindings)){
  const declaration=template.immutableDeclarations[id];assert(declaration,'IMMUTABLE_DECLARATION');const n=BigInt(value.value),type=declaration.type;
  if(type==='address'){assert(n<2n**160n,'IMMUTABLE_ADDRESS_RANGE');fields[declaration.name]='0x'+n.toString(16).padStart(40,'0');}
  else if(type.startsWith('uint')){assert(n<2n**BigInt(type.slice(4)),'IMMUTABLE_UINT_RANGE');fields[declaration.name]=n.toString();}
  else if(type==='int24'){const signed=n>=(1n<<255n)?n-(1n<<256n):n;assert(signed>0n&&signed<(1n<<23n),'IMMUTABLE_TICK_SPACING');fields[declaration.name]=signed.toString();}
  else assert.fail('IMMUTABLE_TYPE_UNSUPPORTED '+type);
 }
 assert.equal(fields.original,poolAddress.toLowerCase(),'IMMUTABLE_SELF_ADDRESS');
 assert(BigInt(fields.token0)<BigInt(fields.token1),'IMMUTABLE_TOKEN_ORDER');
 for(const [name,value] of Object.entries(expected))assert.equal(fields[name],String(value).toLowerCase(),'IMMUTABLE_TYPED_BINDING '+name);
 return {address:poolAddress.toLowerCase(),actual_runtime_sha256:sha(bytes(runtime)),runtime_template_sha256:template.runtime_template_sha256,immutable_fields:fields,immutable_bindings:linked.bindings};
}
async function verify(directory){
 const evidence=load(path.join(directory,'prestate-evidence.json')),execution=load(path.join(directory,'fork-execution.json')),accounts=new Map(evidence.deployments.map(d=>[d.address.toLowerCase(),d]));
 assert.equal(execution.anchor.hash,evidence.expected_block_hash,'EXECUTION_H_BINDING');assert.equal(execution.anchor.height,Number(BigInt(evidence.block.number)),'EXECUTION_HEIGHT_BINDING');
 assert.deepEqual(execution.overrides.balance_overrides,[],'UNDECLARED_BALANCE_OVERRIDE');assert.deepEqual(execution.overrides.storage_overrides,[],'UNDECLARED_STORAGE_OVERRIDE');assert.deepEqual(execution.overrides.code_overrides,[],'UNDECLARED_CODE_OVERRIDE');assert.equal(execution.public_transactions_sent,0,'PUBLIC_TRANSACTIONS');
 const sender=execution.cases[0].sender,senderProof=accounts.get(sender).proof;
 assert.equal(BigInt(execution.original.sender_native_balance),BigInt(senderProof.balance),'FORK_SENDER_BALANCE_OVERRIDE');assert.equal(BigInt(execution.original.sender_nonce),BigInt(senderProof.nonce),'FORK_SENDER_NONCE_OVERRIDE');
 const pairFile=path.join(ROOT,'integration/tools/reference/evm-execution/packages/core/build/UniswapV2Pair.json'),pairArtifact=load(pairFile),pairMetadata=JSON.parse(pairArtifact.metadata),v2=pinnedCompiler('0.5.16+commit.9c3226ce');
 const sources={};for(const name of Object.keys(pairMetadata.sources))sources[name]={content:fs.readFileSync(path.join(path.dirname(path.dirname(pairFile)),name),'utf8')};
 const settings={...pairMetadata.settings,outputSelection:{'*':{'*':['metadata','evm.deployedBytecode']}}};delete settings.compilationTarget;
 const compiled=JSON.parse(v2.compiler.compile(JSON.stringify({language:'Solidity',sources,settings})));assert(!(compiled.errors||[]).some(e=>e.severity==='error'));
 const [file,name]=Object.entries(pairMetadata.settings.compilationTarget)[0],pair=compiled.contracts[file][name];
 const template=load(path.join(BASE,'v3-pool-template/template.json')),official=load(path.join(BASE,'v3-pool-template/official-npm-artifact.json'));
 assert.equal(sha(fs.readFileSync(path.join(ROOT,template.source_input.path))),template.source_input.sha256,'EXACT_V3_SOURCE_INPUT_BYTES');assert.equal(sha(fs.readFileSync(path.join(BASE,'v3-pool-template/official-npm-artifact.json'))),template.official_npm_artifact.sha256,'OFFICIAL_POOL_ARTIFACT_BYTES');
 const v3Source=load(path.join(ROOT,template.source_input.path)),input=structuredClone(v3Source.stdJsonInput),v3=pinnedCompiler('0.7.6+commit.7338295f');
 input.settings.outputSelection={'contracts/UniswapV3Pool.sol':{'':['ast'],UniswapV3Pool:['metadata','evm.bytecode','evm.deployedBytecode']},'contracts/UniswapV3Factory.sol':{UniswapV3Factory:['metadata','evm.deployedBytecode']},'contracts/NoDelegateCall.sol':{'':['ast']}};
 const output=JSON.parse(compileStandardHeap(v3.module,JSON.stringify(input)));assert(!(output.errors||[]).some(e=>e.severity==='error'));
 const pool=output.contracts['contracts/UniswapV3Pool.sol'].UniswapV3Pool,factory=output.contracts['contracts/UniswapV3Factory.sol'].UniswapV3Factory;
 assert.equal('0x'+pool.evm.deployedBytecode.object,template.deployedBytecode,'REBUILT_POOL_TEMPLATE');assert.equal('0x'+pool.evm.bytecode.object,official.bytecode,'REBUILT_OFFICIAL_POOL_CREATION');
 assert.deepEqual(pool.evm.deployedBytecode.immutableReferences,template.immutableReferences,'REBUILT_IMMUTABLE_REFERENCES');
 const declarations={};function walk(node,source){if(!node||typeof node!=='object')return;if(node.nodeType==='VariableDeclaration'&&pool.evm.deployedBytecode.immutableReferences[String(node.id)])declarations[String(node.id)]={name:node.name,type:node.typeDescriptions.typeString,source,src:node.src};for(const child of Object.values(node))if(Array.isArray(child))child.forEach(v=>walk(v,source));else if(child&&typeof child==='object')walk(child,source);}
 for(const [source,obj] of Object.entries(output.sources))walk(obj.ast,source);assert.deepEqual(declarations,template.immutableDeclarations,'REBUILT_IMMUTABLE_AST');
 const factoryAddress='0x1f98431c8ad98523631ae4a59f267346ea31f984';const factoryBinding=bindImmutables(factory.evm.deployedBytecode.object,accounts.get(factoryAddress).code,factory.evm.deployedBytecode.immutableReferences);
 const cases=[];
 for(const c of execution.cases){
  const actual=accounts.get(c.pool).code;
  assert.equal(BigInt(c.recipient_after)-BigInt(c.recipient_before),BigInt(c.actual_net_recipient),'ACTUAL_NET_RECIPIENT');assert(BigInt(c.actual_net_recipient)>0n);
  assert.equal(BigInt(c.sender_sell_before)-BigInt(c.sender_sell_after),BigInt(c.amount_in),'ACTUAL_INPUT_CONSUMED');
  assert.equal(c.deposit.transaction.from,c.sender);assert.equal(c.approval.transaction.from,c.sender);assert.equal(c.swap.transaction.from,c.sender);
  for(const tx of [c.deposit,c.approval,c.swap]){assert.equal(BigInt(tx.receipt.status),1n);assert.equal(tx.receipt.transactionHash,tx.transaction.hash);}
  assert.equal(Number(BigInt(c.deposit.before_header.number)),execution.anchor.height+1,'DECLARED_FORK_INITIALIZATION_BLOCK');
  assert.equal(c.original_chain_mined_transaction,false,'SIMULATED_REFERENCE_NOT_ORIGINAL_MINED_TRANSACTION');assert.equal(execution.overrides.original_chain_hardfork_equivalence_claimed,false,'FORK_ENVIRONMENT_EQUIVALENCE_NOT_PROVEN');assert.equal(c.approval.before_header.hash,c.deposit.header.hash,'DEPOSIT_APPROVAL_CONTINUITY');assert.equal(c.swap.before_header.hash,c.approval.header.hash,'APPROVAL_SWAP_CONTINUITY');
  assert.equal(BigInt(c.deposit.transaction.nonce),BigInt(senderProof.nonce),'ORIGINAL_NONCE');assert.equal(BigInt(c.approval.transaction.nonce),BigInt(senderProof.nonce)+1n);assert.equal(BigInt(c.swap.transaction.nonce),BigInt(senderProof.nonce)+2n);
  assert.equal(c.swap.transaction.to,c.router,'ORIGINAL_ROUTER_TARGET');assert.equal(BigInt(c.deposit.transaction.value),BigInt(c.native_value_deposited),'DECLARED_DEPOSIT');
  let identity;
  if(c.protocol==='uniswap-v2'){
   require('./evm_deployment_rebuild.cjs').verifyRebuild(pairMetadata,pair,actual);
   identity={address:c.pool,actual_runtime_sha256:sha(bytes(actual)),source_compiler_rebuild_verified:true,artifact_sha256:sha(fs.readFileSync(pairFile))};
  }else{
   const abi=new ethers.utils.Interface(['function exactInputSingle((address tokenIn,address tokenOut,uint24 fee,address recipient,uint256 deadline,uint256 amountIn,uint256 amountOutMinimum,uint160 sqrtPriceLimitX96)) payable returns(uint256)']);
   const params=abi.decodeFunctionData('exactInputSingle',c.swap.transaction.input)[0];assert.equal(params.amountIn.toString(),c.amount_in);assert.equal(params.recipient.toLowerCase(),c.recipient);
   const [token0,token1]=[c.token_in,c.token_out].sort();identity=bindV3(template,actual,c.pool,{factory:factoryAddress,token0,token1,fee:params.fee});
   const salt=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','address','uint24'],[token0,token1,params.fee]));
   assert.equal(ethers.utils.getCreate2Address(factoryAddress,salt,ethers.utils.keccak256(official.bytecode)).toLowerCase(),c.pool,'FACTORY_CREATE2_POOL_ASSOCIATION');
  }
  cases.push({protocol:c.protocol,pool:c.pool,source_compiler_runtime_verified:true,fixed_H_original_public_prestate_verified:true,actual_net_recipient:c.actual_net_recipient,token_out:c.token_out,identity,original_chain_mined_transaction:false});
 }
 const relatedPools=['0x88e6a0c2ddd26feeb64f039a2c41296fcb3f5640','0xc2e9f25be6257c210d7adf0d4cd6e3e881ba25f8'].map(address=>bindV3(template,accounts.get(address).code,address,{factory:factoryAddress}));
 return {related_v3_pools:relatedPools,schema_version:1,passed:true,anchor:execution.anchor,scope:execution.scope,cases,factory:{address:factoryAddress,source_compiler_runtime_verified:true,runtime_sha256:sha(bytes(accounts.get(factoryAddress).code)),immutable_bindings:factoryBinding.bindings,pool_execution_association_verified:true},source_compiler_rebuild_verified:true,original_chain_mined_transaction:false,consensus_verified:false,public_transactions_sent:0};
}
module.exports={bindV3,verify};
if(require.main===module){verify(path.resolve(process.argv[2])).then(result=>console.log(JSON.stringify(result,null,2))).catch(error=>{console.error(error.stack);process.exit(1)});}
