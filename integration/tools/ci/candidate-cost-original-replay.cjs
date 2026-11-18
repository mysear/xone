// SPDX-License-Identifier: GPL-3.0-or-later
// Execute the exact HTTP candidate calldata over retained original bytecode in
// independent Ganache. The zero-value Router never reads BASEFEE; the explicit
// effective gasPrice is aligned, while both observed block headers are retained.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=process.cwd(),modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const {ethers}=require(path.join(modules,'ethers')),ganache=require(path.join(modules,'ganache'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),need=(v,m)=>{if(!v)throw Error(m);};
const [inputFile,outputFile]=process.argv.slice(2),raw=fs.readFileSync(inputFile),input=JSON.parse(raw);
(async()=>{const rows=[];for(const r of input.records){
 const f=r.fixture,e=f.execution_environment,t=r.execution.transaction,tc=r.execution.transactionContext;
 need(t.value==='0'&&f.state_anchor.chain_key===t.chain_key,'REPLAY_BOUND_SCOPE');
 const transport=ganache.provider({logging:{quiet:true},chain:{chainId:e.chain_id,hardfork:'shanghai',time:new Date(e.timestamp*1000)},miner:{coinbase:e.coinbase,timestampIncrement:0,blockGasLimit:e.gas_limit},wallet:{deterministic:true}});
 try {const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=10;
 for(const a of f.accounts){need(a.exists,'REPLAY_DECLARED_EXISTING_ACCOUNTS');
  await provider.send('evm_setAccountCode',[a.address,a.code]);
  await provider.send('evm_setAccountBalance',[a.address,ethers.utils.hexValue(ethers.BigNumber.from(a.balance))]);
  await provider.send('evm_setAccountNonce',[a.address,ethers.utils.hexValue(a.nonce)]);
  for(const slot of a.storage)await provider.send('evm_setAccountStorageAt',[a.address,slot.slot,slot.value]);
 }
 const token=new ethers.utils.Interface(['function balanceOf(address) view returns (uint256)']);
 const balance=async(address,owner)=>BigInt(await provider.call({to:address,data:token.encodeFunctionData('balanceOf',[owner])}));
 const beforeIn=await balance(r.sell_token,t.from),beforeOut=await balance(r.buy_token,r.recipient),beforeNative=await provider.getBalance(t.from);
 const tx=await provider.getSigner(t.from).sendTransaction({to:t.to,data:t.data,value:0,nonce:tc.nonce,gasLimit:tc.gas_limit,gasPrice:r.execution.costs.gasPriceNativePerUnit});
 const receipt=await tx.wait(),afterIn=await balance(r.sell_token,t.from),afterOut=await balance(r.buy_token,r.recipient),afterNative=await provider.getBalance(t.from);
 const expected=r.execution.outcome;
 need(receipt.status===1&&receipt.gasUsed.toString()===expected.execution.gas_used.toString(),'REPLAY_ORIGINAL_GAS');
 need((beforeIn-afterIn).toString()===expected.actual_input_consumed&&(afterOut-beforeOut).toString()===expected.actual_buy_amount,'REPLAY_ORIGINAL_TOKEN_DELTAS');
 // This replay intentionally tests disjoint caller and coinbase; alias fee
 // accounts retain their separate independent suites.
 need(t.from!==e.coinbase&&beforeNative.sub(afterNative).toString()===r.execution.costs.payerNetNativeDebit,'REPLAY_ORIGINAL_PAYER_DEBIT');
 const header=await provider.getBlock(receipt.blockNumber);
 rows.push({id:r.id,plan_hash:r.plan_hash,transaction_hash:tx.hash,gas_used:receipt.gasUsed.toString(),input:(beforeIn-afterIn).toString(),output:(afterOut-beforeOut).toString(),payer_native_debit:beforeNative.sub(afterNative).toString(),observed_replay_header:{number:header.number,hash:header.hash,timestamp:header.timestamp,base_fee:header.baseFeePerGas.toString()},product_execution_environment:e,calldata_sha256:sha(Buffer.from(t.data.slice(2),'hex'))});
 } finally {await transport.disconnect();}
 }
 fs.writeFileSync(outputFile,JSON.stringify({passed:true,input_sha256:sha(raw),wrapper_sha256:sha(fs.readFileSync(__filename)),records:rows,scope:'ORIGINAL_ROUTER_CALLDATA_GAS_TOKEN_AND_DISJOINT_PAYER_DELTAS_EXPLICIT_EFFECTIVE_GAS_PRICE_NOT_BLOCK_CONSENSUS_OR_EQUAL_BASE_FEE',production_ready:false},null,2)+'\n');
})().catch(e=>{console.error(e);process.exitCode=1;});
