// Offline compiler-size experiment over exact pinned source bytes.
'use strict';
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/balancer-managed-fees');
const wrapper=require(path.join(root,'integration/.runtime/stable-reference/node_modules/solc07'));
const solc=process.argv.includes('--upstream')?wrapper.setupMethods(require(path.join(ref,'compiler/soljson-v0.7.1.js'))):wrapper;
const rev='e281ff40583c39bf1d46c322334b2b13936e0e33',manifest=JSON.parse(fs.readFileSync(path.join(root,'integration/tools/reference/balancer-multi',rev+'.json'))),sources={};
for(const f of manifest.files){const b=fs.readFileSync(path.join(root,'integration/tools/reference/balancer-multi/sources',rev,f.path));if(crypto.createHash('sha256').update(b).digest('hex')!==f.sha256)throw Error('source hash');sources[f.path]={content:b.toString()};}
const results=[];
for(const runs of [200,1])for(const yul of [true,false]){
 const settings={optimizer:{enabled:true,runs,details:{yul}},outputSelection:{'pkg/pool-weighted/contracts/smart/ManagedPool.sol':{'ManagedPool':['evm.deployedBytecode.object']}}};
 const o=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings}),{import:p=>{const k=p.startsWith('@balancer-labs/v2-')?'pkg/'+p.slice(18):p;return sources[k]?{contents:sources[k].content}:{error:p};}}));
 if((o.errors||[]).some(e=>e.severity==='error'))throw Error(o.errors.map(e=>e.formattedMessage).join('\n'));
 const row={solc:solc.version(),settings,runtime_bytes:o.contracts['pkg/pool-weighted/contracts/smart/ManagedPool.sol'].ManagedPool.evm.deployedBytecode.object.length/2};results.push(row);console.log(JSON.stringify(row));
}

fs.writeFileSync(path.join(ref,process.argv.includes("--upstream")?"compiler-probe-0.7.1.json":"compiler-probe-0.7.6.json"),JSON.stringify({revision:rev,eip170_limit:24576,source_manifest_sha256:crypto.createHash("sha256").update(fs.readFileSync(path.join(root,"integration/tools/reference/balancer-multi",rev+".json"))).digest("hex"),results},null,2)+"\n");
