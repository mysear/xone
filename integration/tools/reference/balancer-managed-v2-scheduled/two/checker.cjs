'use strict';
const fs=require('node:fs'),crypto=require('node:crypto');
const {ROOT,REF,E,A,V,inputClosure,sourceManifest:originalSources,scriptMetadata}=require('../inputs.cjs');const {ethers}=E.rr('ethers');
function need(ok,msg){if(!ok)throw Error(msg);}
function eq(a,b,msg){need(JSON.stringify(a)===JSON.stringify(b),msg);}
function check(bank){
 const builds=A.load().builds,abi=new ethers.utils.Interface(A.locate(builds,'ManagedPoolFactory','ManagedPool').c.abi);
 need(bank.original_bank.sha256==='1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66','original bank identity');
 eq(bank.self_source,['generate.cjs','checker.cjs'].map(p=>({path:p,sha256:crypto.createHash('sha256').update(fs.readFileSync(__dirname+'/'+p)).digest('hex')})),'exact self source names and identity');
 eq(bank.generator,scriptMetadata(__dirname),'generator closure');
 need(bank.status==='unintegrated-prototype'&&bank.declaredfutureprofilecandidate.schema==='unknown','candidate only');
 need(bank.prefix_replay.transactions.length===28&&bank.prefix_replay.complete_equality,'prefix28');
 need(bank.cases.length===17,'17 executed swap cases');
 const at=new Map(bank.states.map(s=>[s.id,s]));
 for(const s of bank.states){
  need(s.header.hash===s.header_anchor_after.hash,'header anchor');need(BigInt(s.header.timestamp)<1769904000n,'January fixed clock');
  for(const g of s.getters){
   if(['getGradualWeightUpdateParams','getGradualSwapFeeUpdateParams','getNormalizedWeights','getSwapFeePercentage','getOwner'].includes(g.method)){
    need(g.to===bank.pool,'original pool getter identity');need(g.data===abi.encodeFunctionData(g.method,g.args).toLowerCase(),'original getter selector identity');
   }
  }
  const g=m=>{const x=s.getters.find(g=>g.to===bank.pool&&g.method===m);need(x,'required getter '+m);return abi.decodeFunctionResult(m,x.result);};
  const w=g('getGradualWeightUpdateParams'),f=g('getGradualSwapFeeUpdateParams');
  eq(w.map((v,i)=>i<2?v.toString():v.map(x=>x.toString())),s.schedule.weights,'weight schedule getter binding');
  eq(f.map(v=>v.toString()),s.schedule.fee,'fee schedule getter binding');
  eq(g('getNormalizedWeights')[0].map(x=>x.toString()),s.current_weights,'current weights getter');eq(g('getSwapFeePercentage')[0].toString(),s.current_fee,'current fee getter');
  need(g('getOwner')[0].toLowerCase()===bank.roles.manager,'manager immutable owner');
  const raw=s.storage.find(x=>x.address===bank.pool&&x.key==='0x9');need(raw,'poolState slot9');eq(raw.value,s.packed.pool_word,'packed word actual storage');
  const p=BigInt(raw.value),field=(o,n)=>((p>>BigInt(o))&((1n<<BigInt(n))-1n)).toString();
  eq([field(0,32),field(32,32)],s.schedule.weights.slice(0,2),'weight packed schedule times');
  eq([field(64,32),field(96,32),field(128,62),field(190,62)],s.schedule.fee,'fee packed schedule fields');
  for(let i=0;i<bank.tokens.length;i++){
   const token=bank.tokens[i],key=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[token,11]));
   const row=s.storage.find(x=>x.address===bank.pool&&x.key===key);need(row,'token mapping slot');
   eq(row.value,s.packed.tokens[i].raw_word,'token packed word');const v=BigInt(row.value),mask=(1n<<64n)-1n;
   eq([String(v&mask),String((v>>64n)&mask)],[s.schedule.weights[2][i],s.schedule.weights[3][i]],'raw token start/end getter');
  }
  eq(s.epoch_clock,Number(BigInt(s.header.timestamp)),'epoch clock header binding');
  need(s.accounts.find(a=>a.address===bank.pool).runtime_bytes===24561,'normal original EIP170 runtime');
 }
 for(const sc of bank.scenarios){
  for(const m of sc.management_transactions){const tx=m.execution,parsed=ethers.utils.parseTransaction(tx.signed_transaction);
   need(parsed.from.toLowerCase()===bank.roles.manager,'schedule signer manager key2');need(parsed.to.toLowerCase()===bank.pool,'schedule target original pool');
   need(parsed.hash===tx.transaction.hash&&tx.receipt.transactionHash===parsed.hash,'schedule original signed transaction identity');
   const method=tx.method,expected=method==='updateWeightsGradually'?[bank.start,bank.end,bank.tokens,['200000000000000000','800000000000000000']]:[bank.start,bank.end,'10000000000000000','30000000000000000'];
   eq(tx.args,expected,'schedule transaction start/end/arguments');need(parsed.data.toLowerCase()===abi.encodeFunctionData(method,expected).toLowerCase(),'schedule signed calldata');
   need(tx.transaction.input===parsed.data.toLowerCase()&&BigInt(tx.receipt.status)===1n,'successful schedule transaction');
   const eventName=method==='updateWeightsGradually'?'GradualWeightUpdateScheduled':'GradualSwapFeeUpdateScheduled';
   const logs=tx.receipt.logs.filter(l=>l.address===bank.pool).map(l=>{try{return abi.parseLog(l);}catch{return null;}}).filter(l=>l&&l.name===eventName);
   need(logs.length===1,'original schedule event');eq(logs[0].args[0].toString(),String(bank.start),'event scheduled start');eq(logs[0].args[1].toString(),String(bank.end),'event scheduled end');
  }
  eq(sc.checkpoints.map(c=>c.timestamp),[bank.start-1,bank.start,bank.start+1,bank.start+1296000,bank.end-1,bank.end,bank.end+1],'seven clock checkpoints');
  for(const cp of sc.checkpoints){const s=at.get(cp.state);eq(s.epoch_clock,cp.timestamp,'checkpoint clock');
   if(sc.name!=='feeOnly')eq(s.schedule.weights.slice(0,2),[String(bank.start),String(bank.end)],'scheduled weight start/end');
   if(sc.name!=='weightsOnly')eq(s.schedule.fee.slice(0,2),[String(bank.start),String(bank.end)],'scheduled fee start/end');
  }
 }
 const vaultAbi=new ethers.utils.Interface(A.locate(builds,'Vault','Vault').c.abi);
 for(const c of bank.cases){
  const tx=ethers.utils.parseTransaction(c.execution.signed_transaction);need(tx.hash===c.execution.receipt.transactionHash&&tx.hash===c.execution.transaction.hash,'signed real swap hash');need(tx.from.toLowerCase()===bank.roles.trader&&tx.to.toLowerCase()===bank.vault,'signed real trader/Vault');
  const data=vaultAbi.decodeFunctionData('swap',tx.data);need(data[0].kind===0,'real GIVEN_IN');eq(data[0].amount.toString(),c.amount_in,'signed swap amount');
  const before=at.get(c.before),after=at.get(c.after);need(before&&after,'case state');need(BigInt(c.execution.receipt.status)===1n,'real successful swap receipt');
  const bal=(s,t)=>{const x=s.getters.find(g=>g.to===t&&g.method==='balanceOf'&&g.args[0]===bank.roles.trader);need(x,'trader balance raw');return BigInt(x.result);};
  const out=bal(after,c.token_out)-bal(before,c.token_out),input=bal(before,c.token_in)-bal(after,c.token_in);
  eq(String(out),c.expected_output,'real recipient difference');eq(String(input),c.amount_in,'real input difference');
  const logs=c.execution.receipt.logs.filter(l=>l.address===bank.vault).map(l=>{try{return vaultAbi.parseLog(l);}catch{return null;}}).filter(l=>l&&l.name==='Swap');
  need(logs.length===1,'one original Vault Swap');const ev=logs[0].args;
  eq(ev.amountOut.toString(),String(out),'Swap event output');eq(ev.amountIn.toString(),String(input),'Swap event input');
  need(ev.tokenIn.toLowerCase()===c.token_in&&ev.tokenOut.toLowerCase()===c.token_out,'Swap token identity');
  eq(before.header.timestamp,c.execution.execution_header.timestamp,'source epoch equals execution clock');
 }
 return {states:bank.states.length,cases:bank.cases.length,scenarios:bank.scenarios.length};
}
module.exports={check};
