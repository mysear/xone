'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-detached'),base=path.join(root,'integration/.runtime/singleton-reference');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function requireThat(ok,message){if(!ok)throw Error(message);}
function frozenService(){const file=path.join(root,'integration/tools/reference/infinity-bin-service/delivery-manifest.json'),m=JSON.parse(fs.readFileSync(file));for(const [p,h]of Object.entries(m.frozen_files))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'frozen service changed '+p);return {file:path.relative(root,file),sha256:sha(fs.readFileSync(file)),files:m.frozen_files};}
function compile(){
 const manifestPath=path.join(root,'integration/tools/reference/singleton-cl/source-metadata.json'),manifest=JSON.parse(fs.readFileSync(manifestPath)),pins=new Map(manifest.files.map(s=>[s.local,s])),loaded=new Map();
 const solc=require(path.join(base,'node_modules/solc'));requireThat(solc.version()==='0.8.26+commit.8a97fa7a.Emscripten.clang','compiler');
 function load(name){const pin=pins.get(name);requireThat(pin,'unlisted source '+name);const b=fs.readFileSync(path.join(base,'sources',name));requireThat(sha(b)===pin.sha256,'original source changed '+name);loaded.set(name,pin);return b.toString();}
 const source=fs.readFileSync(path.join(ref,'InfinityBinDetached.sol'),'utf8'),input={language:'Solidity',sources:{'InfinityBinDetached.sol':{content:source}},settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',metadata:{bytecodeHash:'none',appendCBOR:false},outputSelection:{'*':{'*':['abi','storageLayout','evm.bytecode.object','evm.deployedBytecode.object','evm.methodIdentifiers']}}}};
 const output=JSON.parse(solc.compile(JSON.stringify(input),{import:n=>({contents:load(n)})}));requireThat(!(output.errors||[]).some(e=>e.severity==='error'),JSON.stringify(output.errors));
 const artifact=output.contracts['InfinityBinDetached.sol'].InfinityBinDetached;requireThat(artifact.evm.deployedBytecode.object.length/2<=24576,'EIP170');
 return {artifact,input,solc,loaded,load,manifest_sha256:sha(fs.readFileSync(manifestPath))};
}
module.exports={root,ref,base,sha,requireThat,compile,frozenService};
