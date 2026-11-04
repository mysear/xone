'use strict';
const fs=require('node:fs');
const path=require('node:path');
const assert=require('node:assert/strict');
const {CASES,sha,codeSha,ROOT,validatePins,mapping}=require('./generate.cjs');
function validate(x) {
  assert.equal(x.schema_version,1);assert.equal(x.authority,'original-local-node-artifact-replay-not-live');
  assert.deepEqual(x.source_pins,validatePins(),'source closure');
  assert.deepEqual(x.cases.map(c=>c.name),CASES.map(c=>c.name),'exact case inventory');
  assert.deepEqual(x.reference.case_inventory,CASES.map(c=>c.name));
  assert.equal(x.reference.case_count,10);assert.equal(x.reference.success_count,6);assert.equal(x.reference.revert_count,4);
  assert.equal(x.reference.ethers_version,'ethers/5.8.0');assert.equal(x.reference.ganache_version,'7.9.2');
  assert.equal(x.reference.node.version,process.version);assert.equal(x.reference.node.executable,process.execPath);
  assert.equal(x.reference.node.sha256,sha(fs.readFileSync(process.execPath)));
  for(const f of [...x.reference.scripts,...x.reference.loaded_dependencies])assert.equal(f.sha256,sha(fs.readFileSync(path.join(ROOT,f.path))),`runtime closure ${f.path}`);
  assert(x.reference.loaded_dependencies.length>50,'dependency closure omitted');
  assert.equal(x.reference.source_pins_sha256,sha(fs.readFileSync(path.join(__dirname,'source-pins.json'))));
  assert.equal(x.reference.runtime_identities.length,8);assert(x.reference.setup_transactions.length>=20);
  assert.equal(x.reference.chain_options.chain.allowUnlimitedContractSize,false);
  assert.equal(x.reference.chain_options.chain.hardfork,'shanghai');
  for(const s of x.reference.setup_transactions)assert.equal(Number(BigInt(s.receipt.status)),1);
  const decimal=s=>assert(typeof s==='string'&&/^(0|[1-9][0-9]*)$/.test(s),'decimal');
  const hash=s=>assert(/^0x[0-9a-f]{64}$/.test(s),'hash');
  const word=hash;
  for(const [i,c] of x.cases.entries()) {
    const spec=CASES[i],e=c.expected;
    const q=c.quote_context;
    assert(q,'native quote context required');
    assert.deepEqual(Object.keys(q).sort(),['deadline_ms','quoted_buy_amount','slippage_bps']);
    assert.equal(q.slippage_bps,50);decimal(q.quoted_buy_amount);assert(BigInt(q.quoted_buy_amount)>0n);
    assert.equal(c.transaction.data.slice(0,10),'0x38ed1739','official exact-in selector');
    const words=c.transaction.data.slice(10);
    const encodedMinimum=BigInt('0x'+words.slice(64,128));
    const encodedDeadline=BigInt('0x'+words.slice(256,320));
    assert.equal(BigInt(q.deadline_ms),encodedDeadline*1000n,'encoded deadline context');
    const quotedMinimum=BigInt(q.quoted_buy_amount)*9950n/10000n;
    if(spec.fail==='min-out')assert(encodedMinimum>BigInt(q.quoted_buy_amount),'intentional min-out revert');
    else assert.equal(encodedMinimum,quotedMinimum,'native 50 bps minimum');
    const view=c.evidence.quote_view;
    assert.equal(view.block_hash,c.state_anchor.block_hash);
    assert.equal(view.from,c.transaction.from);assert.equal(view.to,c.executor.target);
    assert.equal(view.data.slice(0,10),'0xd06ca61f','official getAmountsOut selector');
    // ABI return is (uint256[] amounts): offset, length, input amount, gross output.
    assert.equal(view.result.length,2+64*4);assert.equal(BigInt('0x'+view.result.slice(2,66)),32n);
    assert.equal(BigInt('0x'+view.result.slice(66,130)),2n);
    assert.equal(BigInt('0x'+view.result.slice(130,194)),BigInt(c.amount_in));
    assert.equal(BigInt('0x'+view.result.slice(194,258)),BigInt(q.quoted_buy_amount));
    if(spec.fee)assert(BigInt(e.amount_out)<quotedMinimum&&quotedMinimum<=BigInt(e.router_amount_out),'fee recipient minimum violation');
    assert.equal(e.status,spec.fail?'revert':'success');assert.equal(e.receiptstatus,spec.fail?0:1);
    assert.equal(c.schema_version,1);assert.equal(c.execution_environment.chain_id,31337);assert.equal(c.execution_environment.fork,'shanghai');assert.equal(c.state_anchor.chain_key,'evm:31337');assert.equal(c.transaction.chain_key,'evm:31337');
    assert.deepEqual(Object.keys(c.transaction_context).sort(),['chain_id','gas_limit','gas_price','nonce'],'strict Rust transaction context');
    assert.equal(c.transaction_context.chain_id,31337);assert.equal(Number(BigInt(c.evidence.transaction.type)),0);
    assert(Number.isSafeInteger(c.transaction_context.gas_limit),'numeric lossless transaction gas limit');
    assert.equal(c.transaction_context.gas_limit,7000000);
    assert(Number.isSafeInteger(c.transaction_context.nonce)&&c.transaction_context.nonce>=0,'unsigned numeric transaction nonce');
    assert.equal(c.transaction_context.gas_price,'2000000000');assert.equal(c.transaction.value,'0');
    assert.equal(c.execution_environment.block_number,c.state_anchor.block_number+1);
    assert.equal(c.execution_environment.parent_hash,c.state_anchor.block_hash);
    assert.equal(c.execution_environment.timestamp,c.state_anchor.timestamp);
    hash(c.state_anchor.block_hash);hash(c.execution_environment.block_hash);hash(c.execution_environment.prevrandao);
    assert.equal(c.evidence.state_key,`evm:31337/${c.state_anchor.block_hash}`);
    assert.equal(c.evidence.preblock_header.hash,c.state_anchor.block_hash);
    assert.equal(c.evidence.execution_block_header.hash,c.execution_environment.block_hash);
    assert.equal(e.tx_hash,c.evidence.receipt.transactionHash);
    assert.equal(BigInt(e.gas_used),BigInt(c.evidence.receipt.gasUsed));
    assert.equal(BigInt(e.receiptstatus),BigInt(c.evidence.receipt.status));
    assert(BigInt(e.gas_used)>21000n&&BigInt(e.gas_used)<BigInt(c.transaction_context.gas_limit));
    ['caller_sell_before','caller_sell_after','recipient_buy_before','recipient_buy_after','amount_out','sender_input_consumed','gas_used'].forEach(k=>decimal(e[k]));
    assert.equal(BigInt(e.amount_out),BigInt(e.recipient_buy_after)-BigInt(e.recipient_buy_before));
    assert.equal(BigInt(e.sender_input_consumed),BigInt(e.caller_sell_before)-BigInt(e.caller_sell_after));
    if(spec.fail) {assert.equal(e.amount_out,'0');assert.equal(e.sender_input_consumed,'0');assert.equal(e.router_amount_out,null);}
    else {assert.equal(e.sender_input_consumed,c.amount_in);assert(BigInt(e.amount_out)>0n);
      if(spec.fee)assert(BigInt(e.amount_out)<BigInt(e.router_amount_out));else assert.equal(e.amount_out,e.router_amount_out);}
    assert.equal(c.pool.adapter_version,'uniswap-v2/1');assert.equal(c.pool.fee_numerator,3);assert.equal(c.pool.fee_denominator,1000);
    assert.equal(c.pool.v2.factory,c.executor.factory);
    assert.equal(c.executor.target,c.transaction.to);
    const accounts=new Map();
    for(const a of c.accounts) {
      assert(!accounts.has(a.address),'duplicate account');accounts.set(a.address,a);assert.equal(a.exists,true);decimal(a.balance);
      assert(Number.isInteger(a.nonce));assert(/^0x([0-9a-f]{2})*$/.test(a.code));
      const slots=new Set();for(const s of a.storage) {word(s.slot);word(s.value);assert(!slots.has(s.slot));slots.add(s.slot);}
    }
    for(const a of [c.transaction.from,c.recipient,c.pool.pool_id,c.token_in,c.token_out,c.executor.target,c.executor.factory,c.execution_environment.coinbase])assert(accounts.has(a),'required actor omitted');
    assert(Array.isArray(e.post_accounts),'actual post image required for every case');
    assert.deepEqual(e.post_accounts.map(a=>a.address),c.accounts.map(a=>a.address),'post image boundary must equal pre image');
    const postAccounts=new Map();
    for(const [index,a] of e.post_accounts.entries()) {
      const before=c.accounts[index];
      assert.deepEqual(Object.keys(a).sort(),['address','balance','code','exists','nonce','storage']);
      assert.equal(a.exists,true);decimal(a.balance);assert(Number.isSafeInteger(a.nonce)&&a.nonce>=0);
      assert(/^0x([0-9a-f]{2})*$/.test(a.code));assert.equal(a.code,before.code,'swap cannot mutate existing code');
      assert.deepEqual(a.storage.map(s=>s.slot),before.storage.map(s=>s.slot),'all declared post slots required');
      for(const v of a.storage){word(v.slot);word(v.value);}
      postAccounts.set(a.address,a);
    }
    assert.equal(postAccounts.get(c.transaction.from).nonce,e.caller_nonce_after);
    const postSlot=(address,slot)=>{const value=postAccounts.get(address).storage.find(s=>s.slot===slot);assert(value,'required post slot');return BigInt(value.value);};
    assert.equal(postSlot(c.token_in,mapping(c.transaction.from,1)),BigInt(e.caller_sell_after));
    assert.equal(postSlot(c.token_out,mapping(c.recipient,1)),BigInt(e.recipient_buy_after));
    assert(e.pool_after,'actual post Pair getters required');
    assert.deepEqual(Object.keys(e.pool_after).sort(),['block_timestamp_last','reserve0','reserve1']);
    decimal(e.pool_after.reserve0);decimal(e.pool_after.reserve1);
    assert(Number.isSafeInteger(e.pool_after.block_timestamp_last)&&e.pool_after.block_timestamp_last>=0);
    const packedPost=postSlot(c.pool.pool_id,'0x'+BigInt(8).toString(16).padStart(64,'0'));
    assert.equal(packedPost&((1n<<112n)-1n),BigInt(e.pool_after.reserve0));
    assert.equal((packedPost>>112n)&((1n<<112n)-1n),BigInt(e.pool_after.reserve1));
    assert.equal(packedPost>>224n,BigInt(e.pool_after.block_timestamp_last));
    assert.equal(codeSha(accounts.get(c.executor.target).code),c.executor.code_sha256);
    assert.equal(codeSha(accounts.get(c.executor.factory).code),c.executor.factory_code_sha256);
    assert.equal(codeSha(accounts.get(c.pool.pool_id).code),c.pool.v2.code_sha256);
    for(const s of c.evidence.storage_accesses)assert(accounts.get(s.address)?.storage.some(t=>t.slot===s.slot),'accessed storage missing');
    if(spec.fail!=='deadline')assert(c.evidence.storage_accesses.length>0);
    assert(c.block_hashes.some(b=>b.number===c.state_anchor.block_number&&b.hash===c.state_anchor.block_hash));
    if(!spec.fail)assert(c.evidence.storage_accesses.some(s=>s.address===c.pool.pool_id&&BigInt(s.slot)===12n&&s.operations.includes('SSTORE')),'pair lock trace');
    if(spec.donation) {
      const d=c.evidence.input_donation;assert(d,'actual donation proof required');
      assert.deepEqual(d.reserves_after_swap,e.pool_after,'donation post getter observations agree');
      assert.equal(d.amount,'50000000000000000000');assert.equal(c.token_in,c.pool.token0);
      assert.equal(d.balance_slot,mapping(c.pool.pool_id,1));
      const raw=accounts.get(c.token_in).storage.find(s=>s.slot===d.balance_slot);assert(raw,'pair input balance slot required');
      assert.equal(BigInt(raw.value),BigInt(d.pair_input_balance_at_anchor));
      assert.equal(BigInt(raw.value),BigInt(c.pool.reserve0)+BigInt(d.amount),'unsynced raw input balance surplus');
      assert.equal(d.pair_input_balance_before_transfer,c.pool.reserve0);
      assert.equal(d.reserves_before_transfer.reserve0,c.pool.reserve0);assert.equal(d.reserves_before_transfer.reserve1,c.pool.reserve1);
      assert.equal(d.reserves_before_transfer.block_timestamp_last,c.evidence.pool_getters.block_timestamp_last);
      const packed=accounts.get(c.pool.pool_id).storage.find(s=>BigInt(s.slot)===8n);assert(packed,'pair stored reserve slot');
      assert.equal(BigInt(packed.value)&((1n<<112n)-1n),BigInt(c.pool.reserve0));
      assert.equal((BigInt(packed.value)>>112n)&((1n<<112n)-1n),BigInt(c.pool.reserve1));
      const transfer=x.reference.setup_transactions.find(t=>t.receipt.transactionHash===d.transfer_tx_hash);assert(transfer,'actual donation transfer receipt');
      assert.equal(transfer.purpose,'input-donation-before-swap:transfer');
      assert.equal(transfer.receipt.blockHash,c.state_anchor.block_hash);
      assert.equal(transfer.transaction.to,c.token_in);assert.equal(transfer.transaction.from,c.transaction.from);
      assert.equal(transfer.transaction.input.slice(0,10),'0xa9059cbb');
      assert.equal('0x'+transfer.transaction.input.slice(34,74),c.pool.pool_id);
      assert.equal(BigInt('0x'+transfer.transaction.input.slice(74,138)),BigInt(d.amount));
      assert.equal(d.swap_event.amount0_in,(BigInt(c.amount_in)+BigInt(d.amount)).toString());
      assert.equal(d.swap_event.amount1_in,'0');assert.equal(d.swap_event.amount0_out,'0');assert.equal(d.swap_event.amount1_out,e.amount_out);
      assert.equal(d.swap_event.sender,c.executor.target);assert.equal(d.swap_event.recipient,c.recipient);
      const event=c.evidence.receipt.logs.find(l=>Number(BigInt(l.logIndex))===d.swap_event.log_index);assert(event,'original Swap log');
      assert.equal(event.address,c.pool.pool_id);assert.equal(event.data,'0x'+[d.swap_event.amount0_in,'0','0',e.amount_out].map(n=>BigInt(n).toString(16).padStart(64,'0')).join(''));
      assert.equal(d.reserves_after_swap.reserve0,(BigInt(d.pair_input_balance_at_anchor)+BigInt(c.amount_in)).toString());
      assert.equal(d.reserves_after_swap.reserve1,(BigInt(c.pool.reserve1)-BigInt(e.amount_out)).toString());
    }
  }
  return x;
}
module.exports={validate};
