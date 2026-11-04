'use strict';
const fs=require('node:fs'),path=require('node:path');
const ref=path.resolve(__dirname,'../reference/infinity-bin-detached'),nodeReceipt=path.join(ref,'check-node-receipt.json');
if(require.main===module)fs.writeFileSync(nodeReceipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {root,base,sha,requireThat,frozenService}=require('./infinity-bin-detached-lib.cjs');
const {ethers}=require(path.join(base,'node_modules/ethers'));
const json=p=>JSON.parse(fs.readFileSync(p)),eq=(a,b,m)=>requireThat(JSON.stringify(a)===JSON.stringify(b),m),W=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32).toLowerCase(),map=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['bytes32','uint256'],[W(key),slot]));
const dec=(x,m)=>{requireThat(typeof x==='string'&&/^(0|[1-9][0-9]*)$/.test(x),m||'unsigned decimal');return BigInt(x);};
function validateState(s,seed){
 requireThat(s.active_id>0&&s.active_id<0xffffff&&s.bin_step===seed.bin_step&&s.lp_fee===seed.lp_fee&&s.protocol_fee===seed.protocol_fee,'state identity/profile');
 requireThat(s.protocol_fee0===(seed.protocol_fee&4095)&&s.protocol_fee1===(seed.protocol_fee>>12),'protocol unpack');
 const ids=Object.keys(seed.bins).map(Number),members=ids.filter(id=>s.raw_bins[id]?.in_tree===true);eq(Object.keys(s.raw_bins),ids.map(String),'all raw bins retained');eq(Object.keys(s.bins),members.map(String),'real-tree bins only');
 const ghost=s.raw_bins[s.active_id]?.in_tree===false?s.raw_bins[s.active_id]:null;
 eq(s.detached_active,ghost,'detached active is exactly the outside-tree starting bin');if(ghost)requireThat(ghost.id===s.active_id&&ghost.total_shares==='1000','detached minimum shares');
 const storage=s.raw_storage,read=k=>{const v=storage[k];requireThat(typeof v==='string'&&/^0x[0-9a-f]{64}$/.test(v),'raw storage word');return BigInt(v);};
 requireThat(read(W(0))===BigInt(s.active_id)+(BigInt(s.protocol_fee)<<24n)+(BigInt(s.lp_fee)<<48n),'packed slot0 raw');
 let rootWord=0n;const l1={},l2={};
 for(const id of ids){const b=s.raw_bins[id];requireThat(b.id===id&&typeof b.in_tree==='boolean','raw bin identity');const x=dec(b.reserve0),y=dec(b.reserve1),shares=dec(b.total_shares);requireThat(x<1n<<128n&&y<1n<<128n&&shares>=1000n,'reserve/share domain');
  requireThat(dec(b.liquidity)===x*dec(b.price)+(y<<128n),'native liquidity/price');requireThat(read(map(id,1))===x+(y<<128n),'raw reserves');requireThat(read(map(id,2))===shares,'raw total shares');
  const key=ethers.utils.solidityKeccak256(['uint24','address','bytes32'],[id,seed.owner,seed.salt]);requireThat(b.position_key===key&&read(map(key,3))===dec(b.position_share)&&BigInt(b.position_share)===shares-1000n,'original position key/share');
  const key1=W(id>>16),key2=W(id>>8);l1[key1]??=0n;l2[key2]??=0n;
  if(b.in_tree){rootWord|=1n<<BigInt(id>>16);l1[key1]|=1n<<BigInt((id>>8)&255);l2[key2]|=1n<<BigInt(id&255);eq(s.bins[id],b,'real member snapshot');}
  for(const d of [true,false]){const choices=members.filter(n=>d?n<id:n>id),wanted=choices.length?(d?Math.max(...choices):Math.min(...choices)):(d?0xffffff:0);requireThat(b.next[String(d)]===wanted,'original next excludes detached ID');}
 }
 requireThat(s.tree.level0===W(rootWord)&&read(W(4))===rootWord,'full tree root');
 for(const [name,values,offset]of [['level1',l1,5],['level2',l2,6]]){eq(Object.keys(s.tree[name]).sort(),Object.keys(values).sort(),'all observed tree branches');for(const [key,val]of Object.entries(values))requireThat(s.tree[name][key]===W(val)&&read(map(key,offset))===val,'original tree '+name);}
}
function validateDocument(f){
 requireThat(f.schema==='infinity-bin-detached/1','schema');requireThat(f.reference.minimum_share==='1000'&&f.reference.scope.full_manager_swap_settlement_verified===false,'scope/minimum');requireThat(f.cases.length===16,'bounded sixteen cases');const names=new Set();let nativeReverts=0,dropped=0,emptySuccess=0;
 for(const c of f.cases){requireThat(!names.has(c.name),'unique cases');names.add(c.name);const seed=c.seed,burn=c.burn,post=burn.expected.post,A=8358608;
  requireThat(seed.active_id===A&&seed.bin_step===1&&seed.lp_fee>=0&&seed.lp_fee<=100000&&(seed.protocol_fee&4095)<=4000&&(seed.protocol_fee>>12)<=4000,'static profile');
  validateState(burn.pre,seed);validateState(post,seed);requireThat(burn.id===A&&burn.pre.active_id===A&&post.active_id===A,'burn active unchanged');requireThat(burn.pre.raw_bins[A].in_tree===true&&post.raw_bins[A].in_tree===false&&!post.bins[A]&&post.detached_active!==null,'native burn removes tree active');
  requireThat(dec(seed.bins[A].total_shares)-dec(burn.shares)===1000n&&post.raw_bins[A].total_shares==='1000'&&post.raw_bins[A].position_share==='0','minimum supply burn');
  for(const [field,delta]of [['reserve0','delta0'],['reserve1','delta1']]){const before=dec(seed.bins[A][field]),out=before*dec(burn.shares)/dec(seed.bins[A].total_shares);requireThat(BigInt(burn.expected[delta])===out&&dec(post.raw_bins[A][field])===before-out,'original burn rounded dust');}
  requireThat(burn.expected.amounts===W(BigInt(burn.expected.delta0)+(BigInt(burn.expected.delta1)<<128n)),'packed burn output');
  for(const [id,b]of Object.entries(burn.pre.raw_bins)){eq([b.reserve0,b.reserve1,b.total_shares],[seed.bins[id].reserve0,seed.bins[id].reserve1,seed.bins[id].total_shares],'actual seeded state');if(Number(id)!==A){const {next:oldNext,...oldBin}=b,{next:newNext,...newBin}=post.raw_bins[id];eq(oldBin,newBin,'burn untouched neighbor reserves/positions');}}
  requireThat(c.trades.length===3,'three bounded sequential trades');let before=post;
  for(const t of c.trades){eq(t.state,before,'sequential local state');const e=t.expected,after=e.post;validateState(after,seed);requireThat(typeof t.zero_for_one==='boolean'&&dec(t.amount_in)>0n,'exact input');eq(before.tree,after.tree,'swap never reinserts removed node');
   for(const id of Object.keys(before.raw_bins))requireThat(before.raw_bins[id].in_tree===after.raw_bins[id].in_tree&&before.raw_bins[id].total_shares===after.raw_bins[id].total_shares,'membership/share stable on swap');
   if(e.native_reverted){nativeReverts++;requireThat(typeof e.revert_data==='string'&&/^0x[0-9a-f]+$/.test(e.revert_data),'native revert data');eq(before,after,'native atomic rollback');requireThat(e.amount_out==='0'&&e.delta0==='0'&&e.delta1==='0'&&e.protocol_amount==='0','caught revert zero result');}
   else{
    const zero=t.zero_for_one,amount=dec(t.amount_in),output=dec(e.amount_out),protocol=dec(e.protocol_amount);requireThat(output>0n&&protocol<=amount&&BigInt(zero?e.delta0:e.delta1)===-amount&&BigInt(zero?e.delta1:e.delta0)===output,'native swap deltas');
    const pf=BigInt(zero?before.protocol_fee0:before.protocol_fee1),lp=BigInt(before.lp_fee);requireThat(e.swap_fee===Number(pf+lp-pf*lp/1000000n),'native combined fee');
    for(const field of ['reserve0','reserve1']){const preSum=Object.values(before.raw_bins).reduce((n,b)=>n+BigInt(b[field]),0n),postSum=Object.values(after.raw_bins).reduce((n,b)=>n+BigInt(b[field]),0n),inputField=zero?'reserve0':'reserve1';requireThat(postSum-preSum===(field===inputField?amount-protocol:-output),'all raw reserve conservation including ghost');}
    if(before.detached_active&&after.active_id!==A){requireThat(after.detached_active===null,'drop detached on crossing');dropped++;}
    if(!before.detached_active){requireThat(after.detached_active===null&&after.active_id!==A,'cannot revisit detached after crossing');eq(before.raw_bins[A],after.raw_bins[A],'historical ghost is not revisited');}
    if(Object.keys(before.bins).length===0){emptySuccess++;requireThat(after.active_id===A&&after.detached_active!==null,'empty root active still trades');}
   }
   before=after;
  }
  if(c.name.includes('real-tree')){requireThat(c.trades[0].label==='cross-from-detached'&&c.trades[1].label==='reverse-skips-detached'&&c.trades[2].label==='local-loop-after','real-tree local loop');requireThat(c.trades.every(t=>!t.expected.native_reverted&&t.expected.post.detached_active===null),'real-tree success');}
  else{requireThat(Object.keys(post.bins).length===0&&post.tree.level0===W(0),'burn empties root');requireThat(c.trades[1].expected.native_reverted===true,'empty root exhaustion');}
 }
 requireThat(nativeReverts===9&&dropped===8&&emptySuccess>=8,'native coverage');return {cases:f.cases.length,trades:48,native_reverts:nativeReverts,crossed_detached:dropped};
}
function validateBindings(r){
 requireThat(r.revision==='3c5ef1be1f71442d8bc60cc6fd11f9c677b714df'&&r.compiler==='0.8.26+commit.8a97fa7a.Emscripten.clang','source/compiler identity');requireThat(r.unlimited_contract_size===false&&r.runtime_bytes<=24576,'normal EIP170');eq(r.engine,{name:'ganache',version:'7.9.2',hardfork:'shanghai',chain_id:31337},'engine');
 const frozen=frozenService();eq(r.frozen_service,frozen,'frozen service exact closure');
 const manifestPath=path.join(root,'integration/tools/reference/singleton-cl/source-metadata.json'),manifest=json(manifestPath),pins=new Map(manifest.files.map(s=>[s.local,s]));requireThat(sha(fs.readFileSync(manifestPath))===r.manifest_sha256,'upstream manifest');requireThat(r.files.length>=20,'source closure');for(const s of r.files){eq(s,pins.get(s.local),'source pin '+s.local);requireThat(sha(fs.readFileSync(path.join(base,'sources',s.local)))===s.sha256,'source file '+s.local);}
 for(const n of ['BinPool.sol','BinPosition.sol','TreeMath.sol','BinPoolManager.sol'])requireThat(r.files.some(s=>s.local.endsWith('/'+n)),'required original dependency '+n);
 for(const [p,h]of Object.entries(r.inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'generator input '+p);
 requireThat(sha(fs.readFileSync(path.join(base,'package-lock.json')))===r.dependency_lock_sha256,'dependency lock');for(const [n,v]of [['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']]){const d=r.dependencies[n],b=fs.readFileSync(path.join(root,d.path));requireThat(d.version===v&&JSON.parse(b).version===v&&sha(b)===d.sha256,'package '+n);}
 requireThat(r.loaded_dependencies.length>50&&r.binary_dependencies.length>=2,'loaded dependency/binary closure');for(const d of [...r.loaded_dependencies,...r.binary_dependencies])requireThat(sha(fs.readFileSync(path.isAbsolute(d.path)?d.path:path.join(root,d.path)))===d.sha256,'loaded dependency/binary '+d.path);
 const bytes=fs.readFileSync(path.join(ref,'Detached.artifact.json')),a=JSON.parse(bytes);requireThat(sha(bytes)===r.artifact_sha256,'artifact closure');requireThat((a.runtime_template.length-2)/2===r.runtime_bytes&&sha(Buffer.from(a.runtime_template.slice(2),'hex'))===r.runtime_sha256&&ethers.utils.keccak256(a.runtime_template)===r.runtime_keccak256,'runtime template');
 const source=fs.readFileSync(path.join(ref,'InfinityBinDetached.sol'),'utf8'),input={language:'Solidity',sources:{'InfinityBinDetached.sol':{content:source}},settings:r.settings};requireThat(sha(JSON.stringify(input))===r.compile_input_sha256,'compile input binding');
 const proof=r.manager_source_proof,text=fs.readFileSync(path.join(base,'sources',proof.file),'utf8'),refs=text.split('\n').map((line,i)=>({line:i+1,text:line})).filter(x=>/maxBinStep|setMaxBinStep/.test(x.text));requireThat(sha(text)===proof.sha256,'manager source hash');eq(proof.references,refs,'all maxBinStep references');requireThat(refs.filter(x=>/binStep > maxBinStep/.test(x.text)).length===1&&text.includes('maxBinStep = newMaxBinStep;'),'only initialize max guard');
 return true;
}
function verify(options={}){const receipt=options.receipt??json(path.join(ref,'execution-receipt.json'));requireThat(receipt.passed===true,'successful execution receipt');const bytes=fs.readFileSync(path.join(root,'integration/test/contracts/infinity-bin-detached.json'));for(const d of ['chain-parser-go','dex-router'])requireThat(fs.readFileSync(path.join(root,d,'test/contracts/infinity-bin-detached.json')).equals(bytes),'service copy');requireThat(sha(bytes)===receipt.fixture_sha256,'fixture receipt');const f=JSON.parse(bytes),r=f.reference;requireThat(sha(fs.readFileSync(path.join(ref,'source-metadata.json')))===receipt.source_metadata_sha256,'source metadata receipt');eq(r,json(path.join(ref,'source-metadata.json')),'fixture source metadata');eq(r.inputs,receipt.inputs,'receipt input binding');requireThat(r.artifact_sha256===receipt.artifact_sha256,'artifact receipt');validateBindings(r);const result=validateDocument(f);requireThat(result.cases===receipt.cases&&result.trades===receipt.trades&&result.native_reverts===receipt.native_reverts,'execution counts');return result;}
module.exports={validateState,validateDocument,validateBindings,verify,requireThat};
if(require.main===module)try{const result=process.argv[2]==='--semantic-document'?validateDocument(json(process.argv[3])):verify();fs.writeFileSync(nodeReceipt,JSON.stringify({passed:true,...result})+'\n');console.log(JSON.stringify(result));}catch(e){fs.writeFileSync(nodeReceipt,JSON.stringify({passed:false,error:String(e)})+'\n');console.error('detached check failed: '+e.stack);process.exitCode=1;}
