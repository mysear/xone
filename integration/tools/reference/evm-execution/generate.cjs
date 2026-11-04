'use strict';
// Execute only shipped official bytecode. No compiler, service math, or storage injection.
const fs = require('node:fs');
const path = require('node:path');
const crypto = require('node:crypto');
const assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../../../..');
const RUNTIME = path.join(ROOT, 'integration/.runtime/stable-reference');
const {createRequire} = require('node:module');
const runtimeRequire = createRequire(path.join(RUNTIME, 'package.json'));
const {ethers} = runtimeRequire('ethers');
const ganache = runtimeRequire('ganache');
const sha = b => crypto.createHash('sha256').update(b).digest('hex');
const codeSha = code => sha(Buffer.from(code.replace(/^0x/, ''), 'hex'));
const json = x => JSON.stringify(x, null, 2) + '\n';
const addr = x => x.toLowerCase();
const word = x => ethers.utils.hexZeroPad(ethers.BigNumber.from(x === '0x' ? 0 : x).toHexString(), 32).toLowerCase();
const dec = x => BigInt(x).toString();
const number = x => {const n=BigInt(x);assert(n>=0n&&n<=BigInt(Number.MAX_SAFE_INTEGER),'lossless unsigned JSON integer');return Number(n);};
const mapping = (key, slot) => ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'], [key, slot]));
const nested = (a,b,slot) => mapping(b, mapping(a,slot));
const CASES = [
  {name:'token0-to-token1-distinct-recipient'},
  {name:'token1-to-token0-distinct-recipient', reverse:true},
  {name:'token0-to-token1-same-recipient', same:true},
  {name:'token1-to-token0-same-recipient', reverse:true, same:true},
  {name:'deadline-revert', fail:'deadline'},
  {name:'min-out-revert', fail:'min-out'},
  {name:'allowance-revert', fail:'allowance'},
  {name:'balance-revert', fail:'balance'},
  {name:'fee-token-output-counterexample', fee:true},
  {name:'input-donation-before-swap', donation:true},
];
function validatePins() {
  const pins = JSON.parse(fs.readFileSync(path.join(__dirname,'source-pins.json')));
  assert.equal(pins.schema_version,1);
  assert.deepEqual(pins.packages.map(p=>[p.name,p.version]).sort(), [
    ['@uniswap/lib','1.1.1'],['@uniswap/v2-core','1.0.1'],['@uniswap/v2-periphery','1.1.0-beta.0']].sort());
  const seen = new Set();
  for (const p of pins.packages) {
    for (const f of p.files) {
      assert(!seen.has(f.path),`duplicate source pin ${f.path}`);seen.add(f.path);
      assert.equal(sha(fs.readFileSync(path.join(__dirname,f.path))), f.sha256,`source pin ${f.path}`);
    }
    const folder = p.name === '@uniswap/v2-core'?'core':p.name === '@uniswap/v2-periphery'?'periphery':'lib';
    const meta = JSON.parse(fs.readFileSync(path.join(__dirname,`${folder}-registry.json`)));
    assert.equal(sha(fs.readFileSync(path.join(__dirname,`${folder}-registry.json`))),p.metadata_sha256);
    const version = meta.versions ? meta.versions[p.version] : meta;
    assert.equal(version.dist.integrity, p.integrity);
  }
  function walk(dir) { for (const e of fs.readdirSync(dir,{withFileTypes:true})) {
    const p=path.join(dir,e.name);assert(!e.isSymbolicLink(),'symlink source');
    if(e.isDirectory())walk(p);else assert(seen.has(path.relative(__dirname,p)),`unpinned source ${p}`);
  }}
  walk(path.join(__dirname,'packages'));
  return pins;
}
function artifact(pkg,name) {
  const file=`packages/${pkg}/build/${name}.json`;
  const a=JSON.parse(fs.readFileSync(path.join(__dirname,file)));
  assert.equal(a.bytecode.replace(/^0x/,''),a.evm.bytecode.object.replace(/^0x/,''));
  return {file,name,abi:a.abi,creation:'0x'+a.evm.bytecode.object.replace(/^0x/,''),
    template:'0x'+a.evm.deployedBytecode.object.replace(/^0x/,''),immutables:a.evm.deployedBytecode.immutableReferences||{}};
}
function traceFootprint(trace, target) {
  assert(Array.isArray(trace.structLogs)&&trace.structLogs.length,'empty opcode trace');
  const slots = new Map(), addresses = new Set([target]);
  const contexts = new Map([[trace.structLogs[0].depth,target]]);
  const accesses = new Map();
  let previous;
  for(const log of trace.structLogs) {
    const depth=log.depth;
    if(previous && depth > previous.depth) {
      assert.equal(depth,previous.depth+1,'trace depth jump');
      assert(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(previous.op),'unsupported trace entry');
      const to=addr('0x'+previous.stack.at(-2).slice(-40));
      contexts.set(depth,['DELEGATECALL','CALLCODE'].includes(previous.op)?contexts.get(previous.depth):to);
    }
    const context=contexts.get(depth); assert(context,'missing call storage context');
    addresses.add(context);
    if(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(log.op)) addresses.add(addr('0x'+log.stack.at(-2).slice(-40)));
    if(['BALANCE','EXTCODESIZE','EXTCODEHASH','EXTCODECOPY','SELFDESTRUCT'].includes(log.op)) addresses.add(addr('0x'+log.stack.at(-1).slice(-40)));
    assert(!['CREATE','CREATE2','SELFDESTRUCT'].includes(log.op),'unexpected mutating account lifecycle');
    if(['SLOAD','SSTORE'].includes(log.op)) {
      const slot=word('0x'+log.stack.at(-1));
      if(!slots.has(context))slots.set(context,new Set()); slots.get(context).add(slot);
      const k=context+'/'+slot;
      if(!accesses.has(k)) accesses.set(k,{address:context,slot,operations:[]});
      if(!accesses.get(k).operations.includes(log.op))accesses.get(k).operations.push(log.op);
    }
    previous=log;
  }
  return {slots,addresses,accesses:[...accesses.values()].sort((a,b)=>(a.address+a.slot).localeCompare(b.address+b.slot))};
}
async function generate() {
  const source_pins=validatePins();
  const A={router:artifact('periphery','UniswapV2Router02'),token:artifact('periphery','ERC20'),
    fee:artifact('periphery','DeflatingERC20'),weth:artifact('periphery','WETH9'),
    factory:artifact('core','UniswapV2Factory'),pair:artifact('core','UniswapV2Pair')};
  assert.equal(ethers.utils.keccak256(A.pair.creation),'0x96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f');
  const library=fs.readFileSync(path.join(__dirname,'packages/periphery/contracts/libraries/UniswapV2Library.sol'),'utf8');
  assert(library.includes(ethers.utils.keccak256(A.pair.creation).slice(2)),'router library initcode mismatch');
  const keys=[1,2,3,4].map(x=>'0x'+x.toString(16).padStart(64,'0'));
  const owners=keys.map(k=>addr(new ethers.Wallet(k).address));
  const chainOptions={chain:{chainId:31337,networkId:31337,hardfork:'shanghai',time:new Date('2026-10-01T00:00:00Z'),
    allowUnlimitedContractSize:false,allowUnlimitedInitCodeSize:false,vmErrorsOnRPCResponse:false},
    wallet:{accounts:keys.map(secretKey=>({secretKey,balance:ethers.utils.parseEther('10000').toHexString()})),seed:'t32.3-official-evm-reference'},
    miner:{timestampIncrement:0,coinbase:owners[3],blockGasLimit:30000000,defaultGasPrice:2000000000},logging:{quiet:true}};
  const raw=ganache.provider(chainOptions);
  const rpc=(method,params=[])=>raw.request({method,params});
  const provider=new ethers.providers.Web3Provider(raw);
  const wallets=keys.map(k=>new ethers.Wallet(k,provider));
  const setup=[],identities=[];
  const txOptions={gasLimit:7000000,gasPrice:2000000000,type:0,chainId:31337};
  async function transact(wallet,to,data,value='0') {
    const signed=await wallet.signTransaction({...txOptions,to,data,value:ethers.BigNumber.from(value),nonce:await provider.getTransactionCount(wallet.address)});
    const hash=await rpc('eth_sendRawTransaction',[signed]);
    const receipt=await rpc('eth_getTransactionReceipt',[hash]);
    assert(receipt,'missing mined receipt');
    return {signed_transaction:signed,transaction:await rpc('eth_getTransactionByHash',[hash]),receipt};
  }
  async function setupTx(contract,method,args=[],value='0') {
    const result=await transact(wallets[0],contract.address,contract.interface.encodeFunctionData(method,args),value);
    assert.equal(number(result.receipt.status),1,`setup ${method}`);
    setup.push({purpose:method,...result});return result;
  }
  async function identify(a,address,receiptHash,immutableValues=[]) {
    address=addr(address);const code=await rpc('eth_getCode',[address,'latest']);
    const template=Buffer.from(a.template.slice(2),'hex'),normalized=Buffer.from(code.slice(2),'hex');
    assert(normalized.length<=24576,'EIP170');assert.equal(normalized.length,template.length);
    const refs=Object.entries(a.immutables).map(([id,rs],i)=>({id,value:immutableValues[i],references:rs}));
    for(const ref of refs) {
      assert(ref.value,'missing immutable value');
      for(const r of ref.references) {
        assert.equal(r.length,32);assert.equal(normalized.subarray(r.start,r.start+32).toString('hex'),word(ref.value).slice(2));
        template.copy(normalized,r.start,r.start,r.start+32);
      }
    }
    assert.equal('0x'+normalized.toString('hex'),a.template,'official runtime template mismatch');
    identities.push({artifact:a.file,address,creation_transaction_hash:receiptHash,creation_code_sha256:codeSha(a.creation),
      runtime_code_sha256:codeSha(code),runtime_template_sha256:codeSha(a.template),immutable_references:refs});
  }
  async function deploy(a,args=[]) {
    const factory=new ethers.ContractFactory(a.abi,a.creation,wallets[0]);
    const result=await transact(wallets[0],undefined,factory.getDeployTransaction(...args).data);
    assert.equal(number(result.receipt.status),1,`deployment ${a.name}`);
    setup.push({purpose:`deploy:${a.name}`,...result});
    const c=new ethers.Contract(result.receipt.contractAddress,a.abi,wallets[0]);
    await identify(a,c.address,result.receipt.transactionHash,a===A.router?args:[]);
    return c;
  }
  try {
    const genesis=await rpc('eth_getBlockByNumber',['0x0',false]);
    const factory=await deploy(A.factory,[owners[0]]),weth=await deploy(A.weth);
    const router=await deploy(A.router,[factory.address,weth.address]);
    const supply=ethers.utils.parseEther('1000000000');
    const tokenA=await deploy(A.token,[supply]),tokenB=await deploy(A.token,[supply]),fee=await deploy(A.fee,[supply]);
    await setupTx(weth,'deposit',[],ethers.utils.parseEther('1').toString());
    const tokens=[tokenA,tokenB].sort((a,b)=>addr(a.address).localeCompare(addr(b.address)));
    async function createPool(a,b) {
      const creation=await setupTx(factory,'createPair',[a.address,b.address]);
      const address=await factory.getPair(a.address,b.address);
      const calculated=ethers.utils.getCreate2Address(factory.address,ethers.utils.keccak256(ethers.utils.solidityPack(['address','address'],[a.address,b.address].sort((x,y)=>addr(x).localeCompare(addr(y))))),ethers.utils.keccak256(A.pair.creation));
      assert.equal(addr(calculated),addr(address));
      await identify(A.pair,address,creation.receipt.transactionHash);
      const pair=new ethers.Contract(address,A.pair.abi,wallets[0]);
      await setupTx(a,'transfer',[address,ethers.utils.parseEther('1000000')]);
      await setupTx(b,'transfer',[address,ethers.utils.parseEther('2000000')]);
      await setupTx(pair,'mint',[owners[0]]);return pair;
    }
    const pair=await createPool(tokens[0],tokens[1]);
    const feePair=await createPool(tokens[0],fee);
    for(const t of [tokenA,tokenB,fee])await setupTx(t,'approve',[router.address,ethers.constants.MaxUint256]);
    const genesisHash=genesis.hash;
    const cases=[];
    for(const spec of CASES) {
      process.stderr.write(`capture ${spec.name}\n`);
      // Revert cases use actual approve/funding setup transactions, all included in provenance.
      if(spec.fail==='allowance')await setupTx(tokens[0],'approve',[router.address,0]);
      const caller=spec.fail==='balance'?wallets[2]:wallets[0];
      if(spec.fail==='balance') {
        const c=tokens[0].connect(caller);
        const result=await transact(caller,c.address,c.interface.encodeFunctionData('approve',[router.address,ethers.constants.MaxUint256]));
        assert.equal(number(result.receipt.status),1);setup.push({purpose:'empty-caller:approve',...result});
      }
      let donationProof=null;
      if(spec.donation) {
        const before=await rpc('eth_getBlockByNumber',['latest',false]);
        const reserves=await pair.callStatic.getReserves({blockTag:number(before.number)});
        const balance=await tokens[0].balanceOf(pair.address,{blockTag:number(before.number)});
        const donationAmount=ethers.utils.parseEther('50');
        assert.equal(balance.toString(),reserves[0].toString(),'nominal pair starts synchronized before donation');
        const transfer=await setupTx(tokens[0],'transfer',[pair.address,donationAmount]);
        setup.at(-1).purpose='input-donation-before-swap:transfer';
        donationProof={amount:donationAmount.toString(),transfer_tx_hash:transfer.receipt.transactionHash,
          before_transfer_block_hash:before.hash,
          reserves_before_transfer:{reserve0:reserves[0].toString(),reserve1:reserves[1].toString(),block_timestamp_last:reserves[2]},
          pair_input_balance_before_transfer:balance.toString(),balance_slot:mapping(pair.address,1)};
      }
      const activePair=spec.fee?feePair:pair;
      const input=spec.reverse?tokens[1]:tokens[0],output=spec.fee?fee:spec.reverse?tokens[0]:tokens[1];
      const recipient=spec.same?addr(caller.address):owners[1];
      const amount=ethers.utils.parseEther('1000').toString();
      const anchor=await rpc('eth_getBlockByNumber',['latest',false]);const tag=anchor.number;
      const at=(c,m,args=[])=>c.callStatic[m](...args,{blockTag:number(tag)});
      const sellBefore=await at(input,'balanceOf',[caller.address]);
      const buyBefore=await at(output,'balanceOf',[recipient]);
      const reserve=await at(activePair,'getReserves');
      const token0=addr(await at(activePair,'token0')),token1=addr(await at(activePair,'token1'));
      assert.equal(addr(await at(activePair,'factory')),addr(factory.address));
      assert.equal(addr(await at(factory,'getPair',[token0,token1])),addr(activePair.address));
      assert.equal(addr(await at(factory,'getPair',[token1,token0])),addr(activePair.address));
      assert.equal(addr(await at(router,'factory')),addr(factory.address));assert.equal(addr(await at(router,'WETH')),addr(weth.address));
      if(donationProof) {
        assert.equal(addr(input.address),token0,'donation case direction');
        assert.equal(reserve[0].toString(),donationProof.reserves_before_transfer.reserve0,'donation must not sync reserve0');
        assert.equal(reserve[1].toString(),donationProof.reserves_before_transfer.reserve1,'donation must not sync reserve1');
        assert.equal(reserve[2],donationProof.reserves_before_transfer.block_timestamp_last);
        const rawBalance=await at(input,'balanceOf',[activePair.address]);
        assert.equal(rawBalance.sub(reserve[0]).toString(),donationProof.amount,'actual unsynced donation surplus');
        donationProof.pair_input_balance_at_anchor=rawBalance.toString();
      }
      const deadline=number(anchor.timestamp)+(spec.fail==='deadline'?-1:3600);
      const pathTokens=[input.address,output.address];
      // The minimum constrains the actual signed transaction. Its gross quote comes
      // from the unchanged original Router view at the exact pre-transaction bank.
      const quoteData=router.interface.encodeFunctionData('getAmountsOut',[amount,pathTokens]);
      const quoteReturn=await rpc('eth_call',[{from:addr(caller.address),to:addr(router.address),data:quoteData},tag]);
      const quotedAmounts=router.interface.decodeFunctionResult('getAmountsOut',quoteReturn)[0];
      assert.equal(quotedAmounts.length,2);assert.equal(quotedAmounts[0].toString(),amount);
      const grossQuote=quotedAmounts[1];
      const minOut=spec.fail==='min-out'?supply.toString():grossQuote.mul(9950).div(10000).toString();
      const quoteContext={slippage_bps:50,deadline_ms:deadline*1000,quoted_buy_amount:grossQuote.toString()};
      const data=router.interface.encodeFunctionData('swapExactTokensForTokens',[amount,minOut,pathTokens,recipient,deadline]);
      // eth_call is only a supplementary actual-EVM return observation; expected output is measured below.
      let routerAmount=null,callReturn=null;
      try {
        callReturn=await rpc('eth_call',[{from:addr(caller.address),to:addr(router.address),data,gas:ethers.utils.hexValue(txOptions.gasLimit),gasPrice:ethers.utils.hexValue(txOptions.gasPrice)},tag]);
        routerAmount=router.interface.decodeFunctionResult('swapExactTokensForTokens',callReturn)[0][1].toString();
      } catch(e) {assert(spec.fail,`unexpected call failure: ${e.message}`);}
      const result=await transact(caller,router.address,data);
      const receipt=result.receipt;
      const tx=result.transaction;
      assert.equal(number(tx.type),0,'actual legacy transaction type');
      const signedTx=ethers.utils.parseTransaction(result.signed_transaction);assert.equal(signedTx.chainId,31337);assert.equal(signedTx.type,null);
      const block=await rpc('eth_getBlockByNumber',[receipt.blockNumber,false]);
      assert.equal(number(block.number),number(anchor.number)+1);assert.equal(block.parentHash,anchor.hash);
      assert.equal(block.transactions.length,1);assert.equal(block.transactions[0],receipt.transactionHash);
      const trace=await rpc('debug_traceTransaction',[receipt.transactionHash,{disableMemory:true,disableStorage:true,disableStack:false}]);
      const footprint=traceFootprint(trace,addr(router.address));
      const required=[...owners,router.address,factory.address,activePair.address,weth.address,tokenA.address,tokenB.address,fee.address].map(addr);
      required.forEach(a=>footprint.addresses.add(a));
      function add(a,s) {a=addr(a);if(!footprint.slots.has(a))footprint.slots.set(a,new Set());footprint.slots.get(a).add(word(s));}
      for(const t of [tokenA,tokenB,fee]) {
        for(let s=0;s<=4;s++)add(t.address,s);
        for(const a of new Set([addr(caller.address),recipient,addr(activePair.address),addr(router.address)]))add(t.address,mapping(a,1));
        add(t.address,nested(addr(caller.address),addr(router.address),2));
      }
      for(let s=0;s<=12;s++)add(activePair.address,s);
      for(let s=0;s<=3;s++)add(factory.address,s);
      add(factory.address,nested(token0,token1,2));add(factory.address,nested(token1,token0,2));
      // WETH getter helpers have a different layout: totalSupply uses native balance; balances is slot 3.
      add(weth.address,mapping(owners[0],3));
      const accounts=[];
      for(const address of [...footprint.addresses].sort()) {
        const [balance,nonce,code]=await Promise.all([
          rpc('eth_getBalance',[address,tag]),rpc('eth_getTransactionCount',[address,tag]),rpc('eth_getCode',[address,tag])]);
        // Every retained account is independently controlled/funded or an observed successful deployment.
        const evidence=owners.includes(address)?'explicit-genesis-funded-account':identities.some(i=>i.address===address)?'successful-official-creation-receipt':null;
        assert(evidence,`unproven account existence ${address}`);assert(BigInt(balance)>0n||code!=='0x','account unexpectedly empty');
        const storage=[];
        for(const slot of [...(footprint.slots.get(address)||[])].sort())storage.push({slot,value:word(await rpc('eth_getStorageAt',[address,slot,tag]))});
        accounts.push({address,exists:true,balance:dec(balance),nonce:number(nonce),code,storage});
      }
      // Capture the entire declared image boundary at the actual mined H+1 bank.
      // Values come only from native RPC reads; zero slots are retained explicitly.
      const postAccounts=[];
      for(const before of accounts) {
        const [balance,nonce,code]=await Promise.all([
          rpc('eth_getBalance',[before.address,receipt.blockNumber]),
          rpc('eth_getTransactionCount',[before.address,receipt.blockNumber]),
          rpc('eth_getCode',[before.address,receipt.blockNumber])]);
        const storage=[];
        for(const {slot} of before.storage)storage.push({slot,value:word(await rpc('eth_getStorageAt',[before.address,slot,receipt.blockNumber]))});
        // All addresses already have genesis-funding or official-creation authority;
        // the swap trace excludes CREATE, CREATE2 and SELFDESTRUCT.
        postAccounts.push({address:before.address,exists:true,balance:dec(balance),nonce:number(nonce),code,storage});
      }
      const postReserves=await activePair.callStatic.getReserves({blockTag:number(receipt.blockNumber)});
      const poolAfter={reserve0:postReserves[0].toString(),reserve1:postReserves[1].toString(),block_timestamp_last:postReserves[2]};
      assert.equal((await rpc('eth_getBlockByNumber',[receipt.blockNumber,false])).hash,block.hash,'mined block changed after post-state reads');
      const sellAfter=await input.balanceOf(caller.address,{blockTag:number(block.number)});
      const buyAfter=await output.balanceOf(recipient,{blockTag:number(block.number)});
      const status=number(receipt.status)?'success':'revert';
      assert.equal(status,spec.fail?'revert':'success');
      const amountOut=buyAfter.sub(buyBefore).toString();
      if(donationProof) {
        const events=receipt.logs.filter(l=>addr(l.address)===addr(activePair.address)).map(l=>({log:l,event:activePair.interface.parseLog(l)})).filter(v=>v.event.name==='Swap');
        assert.equal(events.length,1,'actual Pair Swap event');
        const {log,event}=events[0];
        donationProof.swap_event={log_index:number(log.logIndex),sender:addr(event.args.sender),recipient:addr(event.args.to),
          amount0_in:event.args.amount0In.toString(),amount1_in:event.args.amount1In.toString(),
          amount0_out:event.args.amount0Out.toString(),amount1_out:event.args.amount1Out.toString()};
        assert.equal(event.args.amount0In.toString(),ethers.BigNumber.from(amount).add(donationProof.amount).toString(),'Pair input event includes donation');
        assert.equal(event.args.amount1In.toString(),'0');
        assert.equal(event.args.amount1Out.toString(),amountOut);
        donationProof.reserves_after_swap=poolAfter;
      }
      if(spec.fail) {assert.equal(amountOut,'0');assert.equal(sellBefore.toString(),sellAfter.toString());}
      else {assert.equal(sellBefore.sub(sellAfter).toString(),amount);assert(BigInt(amountOut)>0n);}
      if(spec.fee) {assert(BigInt(amountOut)<BigInt(minOut),'fee counterexample must violate recipient minimum');assert(BigInt(minOut)<=BigInt(routerAmount),'router gross minimum must pass');}
      else if(!spec.fail)assert.equal(amountOut,routerAmount);
      const block_hashes=[];
      for(let n=Math.max(0,number(anchor.number)-255);n<=number(anchor.number);n++) {
        const b=await rpc('eth_getBlockByNumber',[ethers.utils.hexValue(n),false]);block_hashes.push({number:n,hash:b.hash});
      }
      assert.equal((await rpc('eth_getBlockByNumber',[tag,false])).hash,anchor.hash,'anchor changed after historical reads');
      const routerCode=accounts.find(a=>a.address===addr(router.address)).code;
      const factoryCode=accounts.find(a=>a.address===addr(factory.address)).code;
      const pairCode=accounts.find(a=>a.address===addr(activePair.address)).code;
      const v2={factory:addr(factory.address),code_sha256:codeSha(pairCode)};
      cases.push({schema_version:1,name:spec.name,pool:{chain_key:'evm:31337',pool_id:addr(activePair.address),protocol:'uniswap-v2',
          token0,token1,reserve0:reserve[0].toString(),reserve1:reserve[1].toString(),fee_numerator:3,fee_denominator:1000,
          adapter_version:'uniswap-v2/1',token_behavior:spec.fee?'fee-on-transfer':'standard',v2},
        state_anchor:{chain_key:'evm:31337',block_number:number(anchor.number),block_hash:anchor.hash,parent_hash:anchor.parentHash,timestamp:number(anchor.timestamp),genesis_hash:genesisHash},
        execution_environment:{chain_id:31337,fork:'shanghai',block_number:number(block.number),block_hash:block.hash,parent_hash:block.parentHash,timestamp:number(block.timestamp),coinbase:addr(block.miner),gas_limit:number(block.gasLimit),base_fee:dec(block.baseFeePerGas),prevrandao:block.mixHash},
        accounts,block_hashes,executor:{target:addr(router.address),code_sha256:codeSha(routerCode),factory:addr(factory.address),factory_code_sha256:codeSha(factoryCode),adapter_version:'uniswap-v2-router02/1.1.0-beta.0'},
        transaction:{chain_key:'evm:31337',from:addr(tx.from),to:addr(tx.to),data:tx.input,value:dec(tx.value)},
        transaction_context:{nonce:number(tx.nonce),gas_limit:number(tx.gas),gas_price:dec(tx.gasPrice),chain_id:31337},
        token_in:addr(input.address),token_out:addr(output.address),recipient,amount_in:amount,quote_context:quoteContext,
        expected:{post_accounts:postAccounts,pool_after:poolAfter,status,caller_sell_before:sellBefore.toString(),caller_sell_after:sellAfter.toString(),recipient_buy_before:buyBefore.toString(),recipient_buy_after:buyAfter.toString(),sender_input_consumed:sellBefore.sub(sellAfter).toString(),amount_out:amountOut,router_amount_out:routerAmount,gas_used:dec(receipt.gasUsed),receiptstatus:number(receipt.status),tx_hash:receipt.transactionHash,caller_nonce_before:number(tx.nonce),caller_nonce_after:number(await rpc('eth_getTransactionCount',[addr(caller.address),block.number]))},
        evidence:{receipt,transaction:tx,signed_transaction:result.signed_transaction,preblock_header:anchor,execution_block_header:block,
          trace_sha256:sha(json(trace)),trace_steps:trace.structLogs.length,storage_accesses:footprint.accesses,
          account_existence_authority:'explicit-genesis-funding-or-successful-official-creation-receipt',eth_call_return:callReturn,
          quote_view:{block_hash:anchor.hash,from:addr(caller.address),to:addr(router.address),data:quoteData,result:quoteReturn},
          pool_getters:{block_hash:anchor.hash,token0,token1,factory:v2.factory,reserve0:reserve[0].toString(),reserve1:reserve[1].toString(),block_timestamp_last:reserve[2]},
          ...(donationProof?{input_donation:donationProof}:{}),
          state_key:`evm:31337/${anchor.hash}`,execution_scope:'observed-local-next-block-environment-not-live-canonical'}});
      if(spec.fail==='allowance')await setupTx(tokens[0],'approve',[router.address,ethers.constants.MaxUint256]);
    }
    const ganacheVersion=runtimeRequire('ganache/package.json').version;
    const loaded_dependencies=Object.keys(require.cache).filter(p=>p.startsWith(path.join(RUNTIME,'node_modules')+path.sep)).sort().map(p=>({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))}));
    const sourceFiles=['generate.cjs','validate.cjs','source-pins.json','core-registry.json','periphery-registry.json','lib-registry.json'];
    const scripts=sourceFiles.map(p=>({path:path.relative(ROOT,path.join(__dirname,p)),sha256:sha(fs.readFileSync(path.join(__dirname,p)))}));
    for(const p of ['integration/tools/ci/evm-execution-golden.cjs','integration/tools/ci/evm-execution-check.py','integration/tools/ci/evm-execution-guards.py','integration/.runtime/stable-reference/package.json','integration/.runtime/stable-reference/package-lock.json'])scripts.push({path:p,sha256:sha(fs.readFileSync(path.join(ROOT,p)))});
    return {schema_version:1,authority:'original-local-node-artifact-replay-not-live',source_pins,
      reference:{case_inventory:CASES.map(c=>c.name),case_count:cases.length,success_count:cases.filter(c=>c.expected.status==='success').length,revert_count:cases.filter(c=>c.expected.status==='revert').length,
        chain_options:JSON.parse(json(chainOptions)),genesis_header:genesis,setup_transactions:setup,runtime_identities:identities,
        loaded_dependencies,scripts,node:{version:process.version,executable:process.execPath,sha256:sha(fs.readFileSync(process.execPath))},
        ethers_version:ethers.version,ganache_version:ganacheVersion,
        pair_init_code_keccak256:ethers.utils.keccak256(A.pair.creation),source_pins_sha256:sha(fs.readFileSync(path.join(__dirname,'source-pins.json')))},cases};
  } finally {await raw.disconnect();}
}
module.exports={generate,CASES,json,sha,codeSha,validatePins,mapping,ROOT};
