#!/usr/bin/env node
'use strict';
// Export original V3 Pool template plus compiler-derived immutable references.
// This is a code template identity, never one runtime hash for all pools.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const {compileStandardHeap}=require('./r01_public_catalog_sources.cjs');
const file=path.join(BASE,'verified-source-inputs/ethereum-uniswap-v3.json'),source=JSON.parse(fs.readFileSync(file));
const compilerPath=path.join(ROOT,'integration/artifacts/local/r01-public-compilers/soljson-v0.7.6+commit.7338295f.js');
const compiler=require(compilerPath),wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'))(compiler);
assert.equal(wrapper.version(),'0.7.6+commit.7338295f.Emscripten.clang');
const input=structuredClone(source.stdJsonInput),pool='contracts/UniswapV3Pool.sol',noDelegate='contracts/NoDelegateCall.sol';
assert(input.sources[pool]&&input.sources[noDelegate]);
input.settings.outputSelection={[pool]:{'':['ast'],UniswapV3Pool:['abi','evm.deployedBytecode','metadata']},[noDelegate]:{'':['ast']}};
const compiled=JSON.parse(compileStandardHeap(compiler,JSON.stringify(input)));
assert(!(compiled.errors||[]).some(e=>e.severity==='error'),JSON.stringify(compiled.errors));
const contract=compiled.contracts[pool].UniswapV3Pool;
const npmPath=path.join(ROOT,'integration/.runtime/v3-sdk-oracle/node_modules/@uniswap/v3-core/artifacts/contracts/UniswapV3Pool.sol/UniswapV3Pool.json');
const npm=JSON.parse(fs.readFileSync(npmPath));
assert.equal('0x'+contract.evm.deployedBytecode.object,npm.deployedBytecode,'OFFICIAL_NPM_POOL_TEMPLATE_EXACT');
const ids=contract.evm.deployedBytecode.immutableReferences,declarations={};
function walk(node,file){if(!node||typeof node!=='object')return;if(node.nodeType==='VariableDeclaration'&&ids[String(node.id)])declarations[String(node.id)]={name:node.name,type:node.typeDescriptions.typeString,source:file,src:node.src};for(const value of Object.values(node)){if(Array.isArray(value))value.forEach(v=>walk(v,file));else if(value&&typeof value==='object')walk(value,file);}}
for(const [name,output] of Object.entries(compiled.sources))walk(output.ast,name);
assert.deepEqual(Object.keys(declarations).sort(),Object.keys(ids).sort());
const ref=p=>({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))});
const output={schema_version:1,protocol:'uniswap-v3',artifact:'UniswapV3Pool',compiler:wrapper.version(),compiler_sha256:sha(fs.readFileSync(compilerPath)),settings:input.settings,standard_json_input_sha256:sha(JSON.stringify(input)),
 deployedBytecode:'0x'+contract.evm.deployedBytecode.object,runtime_template_sha256:sha(Buffer.from(contract.evm.deployedBytecode.object,'hex')),immutableReferences:ids,immutableDeclarations:declarations,
 metadata:JSON.parse(contract.metadata),abi:contract.abi,source_input:ref(file),official_npm_artifact:ref(npmPath),source_pins:Object.fromEntries(Object.entries(input.sources).map(([n,s])=>[n,sha(s.content)])),
 matching_policy:'Match every byte including metadata; only these compiler-declared zero immutable slots may bind per pool. Enforce repeated values and typed factory/token/fee/tickSpacing/maxLiquidityPerTick/self-address bindings. Preserve actual per-pool runtime hash.',
 cache_identity:['chain_key','factory','runtime_template_sha256','pool_address','typed_immutable_bindings','actual_runtime_sha256'],single_runtime_hash_for_all_pools:false};
const directory=path.join(BASE,'v3-pool-template');fs.mkdirSync(directory,{recursive:true});
fs.writeFileSync(path.join(directory,'template.json'),JSON.stringify(output,null,2)+'\n',{flag:'wx'});
fs.writeFileSync(path.join(directory,'official-npm-artifact.json'),fs.readFileSync(npmPath),{flag:'wx'});
console.log(JSON.stringify({path:path.join(directory,'template.json'),template_sha256:output.runtime_template_sha256,immutable_fields:Object.values(declarations).map(d=>d.name)},null,2));
