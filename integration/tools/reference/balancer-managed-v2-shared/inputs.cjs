'use strict';
// Exact input authentication, CLI modes and output boundaries. No chain is loaded here.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../../..'),TEMP_ROOT=os.tmpdir();
const REF=path.join(ROOT,'integration/tools/reference/balancer-managed-v2-aum');
const V=require(path.join(REF,'validate.cjs')),E=require(path.join(REF,'environment.cjs'));
const CUTOFF=Date.parse('2026-02-01T00:00:00Z')/1000;
const defaults=['integration','chain-parser-go','chain-parser-rust','dex-router'].map(p=>path.join(ROOT,p,'test/contracts/managed-v2-shared-vault.json'));
const scriptPaths=['generate.cjs','validate.cjs','shared_checks.cjs','inputs.cjs'].map(p=>path.relative(ROOT,path.join(__dirname,p))).concat('integration/tools/ci/managed-v2-shared-vault-golden.cjs').sort();
function scripts(){return scriptPaths.map(p=>({path:p,sha256:V.sha(fs.readFileSync(path.join(ROOT,p)))}));}
function preservation(){
 const input=E.verifyInputs(); // The original manifest is the authority; unknown inputs fail here.
 const paths=[...input.files.map(f=>f.path),'integration/tools/reference/balancer-managed-v2-aum/file-manifest.json',
  'integration/test/contracts/managed-v2-aum.json','integration/test/contracts/managed-v2-aum-pools.json'].sort();
 assert.equal(new Set(paths).size,paths.length);
 return paths.map(p=>{const f=path.join(ROOT,p);assert(fs.lstatSync(f).isFile(),'immutable regular file');return {path:p,bytes:fs.statSync(f).size,sha256:V.sha(fs.readFileSync(f))};});
}
function verifyRuntime(){
 const requested='/usr/local/bin/node25.3.0',allowed=fs.existsSync(requested)?requested:'/usr/local/bin/node';
 assert.equal(fs.realpathSync(process.execPath),fs.realpathSync(allowed),'explicit pinned Node executable required');
 E.verifyInputs();return E.nodeClosure();
}
// The immutable original is parsed and fully validated once per process. Its
// exact bytes and immutable input inventory are authenticated on every use.
let authenticatedOriginal;
function authenticateOriginal(){
 E.verifyInputs();
 const bytes=fs.readFileSync(path.join(ROOT,'integration/test/contracts/managed-v2-aum.json'));
 assert.equal(bytes.length,7905624);assert.equal(V.sha(bytes),'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66');
 if(!authenticatedOriginal)authenticatedOriginal=V.validate(JSON.parse(bytes));
 return authenticatedOriginal;
}
function regular(p){const s=fs.lstatSync(p);assert(s.isFile()&&!s.isSymbolicLink(),'regular file required; symlinks refused');assert.equal(s.nlink,1,'hardlinks refused');return s;}
function components(base,parent){
 const relative=path.relative(base,parent);assert(!relative.startsWith('..')&&!path.isAbsolute(relative),'path outside allowed parent');
 let current=base;for(const part of relative.split(path.sep).filter(Boolean)){current=path.join(current,part);const s=fs.lstatSync(current);assert(s.isDirectory()&&!s.isSymbolicLink(),'symlink output parent refused');}
}
function outputPath(target,{defaultTarget=false}={}){
 assert(path.isAbsolute(target),'absolute --output PATH required');const p=path.normalize(target);
 assert(p.endsWith('.json'),'JSON output only');
 if(defaultTarget){assert(defaults.includes(p),'unknown default output');components(fs.realpathSync(ROOT),path.dirname(p));}
 else {
  assert(!p.startsWith(ROOT+path.sep),'custom output cannot overwrite repository files');
  // macOS /tmp is an OS alias for /private/tmp; every component below it is checked.
  const aliases=[TEMP_ROOT,'/tmp',fs.realpathSync('/tmp')].map(x=>path.resolve(x));
  const alias=aliases.find(x=>p.startsWith(x+path.sep));assert(alias,'custom output must be an absolute temporary JSON path');
  const base=fs.realpathSync(alias);
  const parent=path.join(base,path.relative(alias,path.dirname(p)));components(base,parent);
  assert(fs.realpathSync(path.dirname(p))===parent,'symlink output parent refused');
 }
 try{fs.lstatSync(p);regular(p);}catch(e){if(e.code!=='ENOENT')throw e;}
 return p;
}
function inputPath(target){assert(path.isAbsolute(target),'absolute --input PATH required');const p=path.normalize(target);regular(p);return p;}
function argumentsFor(argv){
 const args=argv.flatMap(a=>/^--(?:input|output)=/.test(a)?[a.slice(0,a.indexOf('=')),a.slice(a.indexOf('=')+1)]:/^--(?:input|output)\//.test(a)?[a.match(/^--(?:input|output)/)[0],a.replace(/^--(?:input|output)/,'')]:[a]);
 if(!args.length)return {mode:'generate',targets:defaults,defaultTarget:true};
 if(args.length===1&&args[0]==='--check')return {mode:'check',targets:defaults,defaultTarget:true};
 if(args.length===2&&args[0]==='--output')return {mode:'generate',targets:[outputPath(args[1])]};
 if(args.length===3&&args[0]==='--check'&&args[1]==='--input')return {mode:'check',targets:[inputPath(args[2])]};
 if(args.length===2&&args[0]==='--validate')return {mode:'validate',targets:[inputPath(args[1])]};
 throw Error('usage: [--output /tmp/FILE.json | --check [--input /absolute/FILE.json] | --validate /absolute/FILE.json]');
}
function write(p,text,options){
 outputPath(p,options);
 const fd=fs.openSync(p,fs.constants.O_WRONLY|fs.constants.O_CREAT|fs.constants.O_NOFOLLOW,0o644);
 try{const s=fs.fstatSync(fd);assert(s.isFile()&&s.nlink===1,'output must be an unlinked regular file');fs.ftruncateSync(fd,0);fs.writeFileSync(fd,text);}finally{fs.closeSync(fd);}
}
async function main(argv=process.argv.slice(2)){
 const options=argumentsFor(argv);verifyRuntime();
 const validator=require('./validate.cjs');
 if(options.mode==='validate'){
  const p=options.targets[0],summary=validator.validate(JSON.parse(fs.readFileSync(p)));
  process.stdout.write(JSON.stringify({...summary,mode:'validate',input:p,sha256:V.sha(fs.readFileSync(p))})+'\n');return;
 }
 for(const p of options.targets)options.mode==='check'?inputPath(p):outputPath(p,options);
 const before=options.mode==='check'?options.targets.map(p=>fs.readFileSync(p)):null;
 if(before){for(const b of before)validator.validate(JSON.parse(b));for(const b of before)assert(b.equals(before[0]),'independent local copies differ');}
 // Regeneration runs the pinned native chain into an owned scratch directory only.
 const tmp=fs.mkdtempSync(path.join(os.tmpdir(),'managed-v2-shared-regen-')),prior=process.env.TMPDIR;
 let value,text;
 try{
  process.env.TMPDIR=tmp;
  value=await require('./generate.cjs').generate();
  validator.validate(value);
  text=JSON.stringify(value,null,2)+'\n';const regenerated=path.join(tmp,'regenerated.json');fs.writeFileSync(regenerated,text,{flag:'wx'});
  if(before)for(let i=0;i<before.length;i++){
   assert(fs.readFileSync(options.targets[i]).equals(before[i]),'input changed during exact check');
   assert(before[i].equals(fs.readFileSync(regenerated)),'exact byte-identical native regeneration');
  }
  else for(const p of options.targets)write(p,text,options);
 }finally{if(prior===undefined)delete process.env.TMPDIR;else process.env.TMPDIR=prior;fs.rmSync(tmp,{recursive:true,force:true});}
 process.stdout.write(JSON.stringify({passed:true,mode:options.mode,exact_regeneration:options.mode==='check',outputs:options.targets,sha256:V.sha(text),bytes:Buffer.byteLength(text),counts:value.counts,statuses:value.cases.map(c=>c.expected_native.status)})+'\n');
}
function fail(e){process.stderr.write((e.stack||String(e))+'\n');process.stdout.write(JSON.stringify({passed:false,error:e.message})+'\n');process.exitCode=1;}
module.exports={ROOT,REF,CUTOFF,defaults,scripts,preservation,verifyRuntime,outputPath,inputPath,argumentsFor,authenticateOriginal,main,fail};
