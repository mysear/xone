# Infinity Bin service reference

Frozen schema: `infinity-bin-service/1`. Canonical fixture lives in `integration/test/contracts/infinity-bin-service.json`, with identical copies in `chain-parser-go` and `dex-router`.

`manager`, `vault`, `tokens`, `pools` describe identity and pool keys. `RPCbanks` is an ordered list of initial bank plus four post-trade banks. Every bank has `id`, `block: {num, hash, timestamp}`, `storage[address][slot32hex]`, `callmap[address][calldatahex]`, `code[address]`, `native_balances[address]`, `pools`, `app_reserves[token]`, `protocol_fees[token]`. Addresses and hex are lowercase. Numeric quantities are decimal strings; block numbers/timestamps, active IDs, fees and steps are JSON numbers. Code, calls, native balance and storage in each bank are read with the same explicit block tag.

`trades` has `id`, `pool_id`, `zero_for_one`, decimal `amount_in`, `pre_bank`, `post_bank`, and `expected: {delta0, delta1, amount_out, swap_fee, protocol_amount, fee_amount, bin_fills, post}`. Each fill has ID, original Q128 price, gross input, output, total fee, protocol fee, and before/after reserves. `post` is the complete pool snapshot including the manager-global bank/protocol amounts. Four steps run pool0/true, pool1/false, pool1/true, pool0/false; each input is the previous native output. Both pools contain sparse initialized bins crossing L1/L2 boundaries and zero-reserve initialized entries.

Original infinity-core revision: `3c5ef1be1f71442d8bc60cc6fd11f9c677b714df`. Solc 0.8.26, viaIR, original optimizer runs 25666, Cancun, metadata bytecodeHash none and appendCBOR false. Original Manager runtime is 23852 bytes and Vault is 9247 bytes. Ganache 7.9.2 runs Shanghai with `allowUnlimitedContractSize:false`; deployment and the original Manager/Vault getters are executed. Original Cancun Vault lock/transient storage and full Manager settlement are **not verified**. No unsupported-opcode simulation is presented as settlement.

`InfinityBinServiceMath.sol` is a separate library execution harness; it is never used for Manager identity admission. Fixture reserves/shares/trees are created with original TreeMath.add and then projected into the deployed original Manager using isolated `evm_setAccountStorageAt`. Bank/protocol accounting is projected separately, then checked via the original getters at the same block as all other RPC data. Token fixtures implement metadata getters only; no ERC20 transfer/settlement evidence is claimed.

Manager storage: paused slot 1; protocolFeesAccrued mapping slot 2; protocolFeeController at byte 0 and maxBinStep at byte 20 of slot 3; pools mapping slot 5; poolIdToPoolKey mapping slot 6. Pool base is keccak256(abi.encode(poolId,5)); offsets are slot0=0, reserves map=1, shares map=2, positions=3, root=4, L1 map=5, L2 map=6. L1 key=id>>16; L2 key=id>>8. Initialized depleted entries retain shares and tree membership. Vault isAppRegistered mapping slot 5; reservesOfApp nested mapping slot 6.

Manager Vault immutables patch 32 bytes at offsets 1921,2767,3756,6803,7198. Exact binary artifacts and compiler storageLayout are supplied; admission normalizes only those compiler-declared immutable offsets and requires the pinned original Vault code. `reference.scope` states the verified boundary explicitly.

Frozen commands (run from xone):

```
node integration/tools/ci/infinity-bin-service-compile.cjs
node integration/tools/ci/infinity-bin-service-deploy.cjs
node integration/tools/ci/infinity-bin-service-golden.cjs
python3 integration/tools/ci/infinity-bin-service-check.py
python3 integration/tools/ci/infinity-bin-service-guards.py
python3 -OO integration/tools/ci/infinity-bin-service-check.py
python3 -OO integration/tools/ci/infinity-bin-service-guards.py
```

No Git operation, old corpus write, schema/domain/parser/service implementation, or PG/Kafka/RPC service integration is included in this task.

## JSON Lines replay CLI

```
node integration/tools/ci/infinity-bin-service-replay.cjs
node integration/tools/ci/infinity-bin-service-replay-test.cjs
```

Input lines:

```
{"action":"identity"}
{"action":"quote","state":{"activeid":8388608,"step":1,"lpfee":3000,"protofee":73632,"bins":{"8388608":{"reserve0":"1000000","reserve1":"1000000","shares":"1000000000000"}}},"amount":"1000","zero":true}
```

`active_id/bin_step/lp_fee/protocol_fee`, `amount_in/zero_for_one`, and bin `total_shares` are accepted aliases. `protofee` is the original packed 24-bit protocol fee, with direction 0 in low 12 bits. Bins may also be a sorted or unsorted array of `{id,reserve0,reserve1,shares}`; IDs are normalized and must be unique. Amounts/reserves/shares must be decimal strings. Initialized depleted entries must still have positive shares. No implicit shares are invented.

One owned Ganache provider is used per CLI process, with normal EIP-170. A single frozen Math artifact is deployed. Every quote restores the post-deployment empty snapshot and seeds only the supplied state, so pools/route allocations can be replayed independently in arbitrary API-selected order. Quote result uses the original `BinPool.swap` static return and a real swap transaction for the complete poststate, including original prices, liquidity, shares, full tree and native successors. No quote formula is implemented in JS.

Success reply: `{ok:true, amount_in, amount_out, delta0, delta1, swap_fee, protocol_amount, active_id, nextstate, post}`. `nextstate` has both compact and snake_case field aliases, packed `slot0`, all bins, prices, liquidity, shares, membership, successors, and root/L1/L2 tree. Original native revert reply: `{ok:false,native_reverted:true,revert_data,nextstate,post}`; poststate is the seeded prestate, demonstrating rollback. Profile/JSON errors return `{ok:false,native_reverted:false,error}` and leave subsequent requests usable. Fatal source/compiler/dependency/artifact validation errors terminate with nonzero exit. Stdout is JSON Lines only; engine warnings use stderr. Provider cleanup occurs on stdin EOF or fatal exit.

Identity reports frozen source metadata/execution receipt/artifact/CLI hashes, original Manager and Vault template hashes, deployed Math code hashes, engine/compiler and the explicit absence of full Manager settlement verification. Frozen fixture/trade bytes are unchanged by replay. App reserves and manager protocol accrual remain the calling integration's responsibility; this CLI returns the original protocol amount instead of applying application accounting.
