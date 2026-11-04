// Infrastructure fault injection only: no test output authorizes an oracle corpus.
'use strict';
const fs=require('fs'),path=require('path'),cp=require('child_process'),os=require('os'),assert=require('assert');
const generator=path.join(__dirname,'curve-ng-state-golden.cjs');
const root=path.resolve(__dirname,'../../..');
const reference=path.join(root,'integration/tools/reference/curve-ng-state');
if(process.argv[2]==='--child') {
  const fault=process.argv[3],dir=process.argv[4],Module=require('module'),originalLoad=Module._load;
  const map=p=>typeof p==='string'&&p.startsWith(reference+path.sep)?path.join(dir,path.relative(reference,p)):p;
  const proxy=new Proxy(fs,{get(target,key){
    if(['readFileSync','writeFileSync','renameSync','rmSync','unlinkSync'].includes(key)) return (...args)=>{
      if(key==='readFileSync'&&fault==='pins'&&String(args[0]).endsWith('/curve-models/sources.json')) return '{invalid pin JSON';
      args[0]=map(args[0]); if(key==='renameSync')args[1]=map(args[1]);
      return target[key](...args);
    };
    return target[key];
  }});
  const artifact=JSON.parse(fs.readFileSync(path.join(reference,'compiler-artifact.json')));
  Module._load=function(request,parent,isMain){
    if(request==='fs')return proxy;
    if(request==='child_process')return {...cp,execFileSync(command,args){
      if(args[0]==='--version')return '0.3.10';
      if(fault==='compile')throw Error('injected compiler subprocess failure');
      if(args[1]==='bytecode,bytecode_runtime,abi,layout')return [artifact.bytecode,artifact.runtime,JSON.stringify(artifact.abi),JSON.stringify(artifact.layout)].join('\n');
      return '0x00\n[]';
    }};
    if(request.endsWith('/node_modules/ethers')&&fault==='dependency')throw Error('injected missing dependency');
    if(request.endsWith('/node_modules/ganache')&&fault==='execution')return {provider(){return {
      request(){return Promise.reject(Error('injected execution provider failure'));},
      async disconnect(){fs.writeFileSync(path.join(dir,'disconnected'),'yes');}
    };}};
    return originalLoad.call(this,request,parent,isMain);
  };
  require(generator);
} else {
  let count=0;
  for(const fault of ['dependency','pins','compile','execution']) {
    const dir=fs.mkdtempSync(path.join(os.tmpdir(),'curve-ng-state-fault-'));
    try {
      for(const file of ['execution-closure.json','metadata-probe.json'])fs.writeFileSync(path.join(dir,file),JSON.stringify({generation_complete:true,status:'COMPLETE',stale:true}));
      const child=cp.spawnSync(process.execPath,[__filename,'--child',fault,dir],{encoding:'utf8',timeout:30000});
      assert.strictEqual(child.status,1,`${fault}: expected failure, got ${child.status}: ${child.stderr}`);
      for(const file of ['execution-closure.json','metadata-probe.json']) {
        assert(fs.existsSync(path.join(dir,file)),`${fault}: missing atomic failed receipt ${file}`);
        const receipt=JSON.parse(fs.readFileSync(path.join(dir,file)));
        assert.strictEqual(receipt.generation_complete,false,`${fault}: stale successful receipt survived ${file}`);
        assert.strictEqual(receipt.status,'FAILED',`${fault}: no failure receipt ${file}`);
      }
      if(fault==='execution')assert.strictEqual(fs.readFileSync(path.join(dir,'disconnected'),'utf8'),'yes','Owned provider was not closed');
      count++;
    } finally {fs.rmSync(dir,{recursive:true,force:true});}
  }
  console.log(`Curve NG startup/compile/execution failure tests: ${count} passed`);
}
