'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function requireThat(ok,message){if(!ok)throw Error(message);}
function compile(){
 const base=path.join(root,'integration/.runtime/singleton-reference');
 const solc=require(path.join(base,'node_modules/solc'));
 const manifest=JSON.parse(fs.readFileSync(path.join(root,'integration/tools/reference/singleton-cl/source-metadata.json')));
 const pins=new Map(manifest.files.map(f=>[f.local,f]));const loaded=new Map();
 const input={language:'Solidity',sources:{},settings:{optimizer:{enabled:true,runs:25666},viaIR:true,evmVersion:'cancun',metadata:{bytecodeHash:'none',appendCBOR:false},remappings:['@openzeppelin/=infinity/lib/openzeppelin-contracts/'],outputSelection:{'*':{'*':['abi','storageLayout','evm.bytecode.object','evm.deployedBytecode.object','evm.deployedBytecode.immutableReferences','evm.methodIdentifiers']}}}};
 for(const name of ['infinity/src/pool-bin/BinPoolManager.sol','infinity/src/Vault.sol'])input.sources[name]={content:load(name)};
 function load(name){const pin=pins.get(name);requireThat(pin,'unlisted source '+name);const b=fs.readFileSync(path.join(base,'sources',name));requireThat(sha(b)===pin.sha256,'source digest '+name);loaded.set(name,pin);return b.toString();}
 const out=JSON.parse(solc.compile(JSON.stringify(input),{import:n=>({contents:load(n)})}));
 requireThat(!(out.errors||[]).some(e=>e.severity==='error'),JSON.stringify(out.errors));
 const result={schema:'infinity-bin-service-compile/1',compiler:solc.version(),settings:input.settings,compile_input_sha256:sha(JSON.stringify(input)),sources:[...loaded.values()].sort((a,b)=>a.local.localeCompare(b.local)),contracts:{}};
 for(const [name,file] of [['BinPoolManager','infinity/src/pool-bin/BinPoolManager.sol'],['Vault','infinity/src/Vault.sol']]){const a=out.contracts[file][name];result.contracts[name]={runtime_bytes:a.evm.deployedBytecode.object.length/2,creation_sha256:sha(Buffer.from(a.evm.bytecode.object,'hex')),runtime_template_sha256:sha(Buffer.from(a.evm.deployedBytecode.object,'hex')),immutable_references:a.evm.deployedBytecode.immutableReferences,storage_layout:a.storageLayout,selectors:a.evm.methodIdentifiers};}
 return {result,out,input,base,load,loaded,solc};
}
module.exports={compile,sha,requireThat,root,ref};
if(require.main===module){const receipt=path.join(ref,'compile-receipt.json');fs.writeFileSync(receipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');try{const {result}=compile();fs.writeFileSync(path.join(ref,'compile-evidence.json'),JSON.stringify(result,null,2)+'\n');fs.writeFileSync(receipt,JSON.stringify({passed:true,evidence_sha256:sha(fs.readFileSync(path.join(ref,'compile-evidence.json')))},null,2)+'\n');console.log(JSON.stringify(result,null,2));}catch(e){fs.writeFileSync(receipt,JSON.stringify({passed:false,error:String(e)})+'\n');console.error(e);process.exitCode=1;}}
