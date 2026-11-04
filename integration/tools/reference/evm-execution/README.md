# Official Uniswap V2 execution reference

This reference executes unchanged published contract bytecode on a deterministic local Ganache 7.9.2 chain. Its authority is `original-local-node-artifact-replay-not-live`. It makes no live-chain, production, SBF, or independent REVM equivalence claim.

## Commands and module API

Run from the workspace root with the existing Node executable and the existing `integration/.runtime/stable-reference/node_modules` tree:

```sh
node integration/tools/ci/evm-execution-golden.cjs
python3 integration/tools/ci/evm-execution-check.py
python3 integration/tools/ci/evm-execution-guards.py
```

The generator writes identical copies to `integration/test/contracts/evm-execution.json` and `dex-router/test/contracts/evm-execution.json`. `--output PATH` writes one selected copy; `--check PATH` captures all ten transactions again and compares the entire parsed result and canonical JSON bytes exactly. The Python checker also requires the two default copies to be byte identical. Every generation removes its previous selected success files before loading the generator, dependencies, or source pins. Capture failures publish no replacement success.

`generate.cjs` exports `generate(): Promise<Fixture>`, `validatePins()`, the fixed `CASES` inventory, and the SHA/JSON helpers. `validate.cjs` exports `validate(fixture)` for source, runtime, account, receipt, schema, and observation consistency checks. Full acceptance additionally requires exact EVM regeneration through the CLI.

## Input authority and runtime

The pre-existing `source-pins.json` commits every published package file and the registry metadata. The generator checks those hashes, the exact package versions, registry integrity declarations, complete extracted-file membership, and absence of source symlinks. Tarball integrity verification was performed when these pinned packages were acquired; this runner uses the resulting committed extracted files.

Artifacts are periphery `1.1.0-beta.0` Router02, ERC20, DeflatingERC20 and WETH9, and core `1.0.1` Factory and Pair. The Pair creation code must have Keccak-256 `96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f`, matching the published Router library. Every deployed runtime must equal the published template, after checking and normalizing only the recorded Router immutable ranges. Factory-created Pair addresses are also checked against CREATE2.

The fixture commits the Node executable/version/SHA-256, every loaded file dependency, package lock, generator/checker/guard code, artifact runtime identities, signed setup transactions, and their actual receipts. The four private keys in chain options are fixed public test keys. Chain ID and network ID are 31337, fork is Shanghai, EIP-170 and initcode size limits are enabled, genesis time is 2026-10-01T00:00:00Z, and `timestampIncrement` is zero. Every transaction is signed legacy type 0 with explicit chain ID, nonce, 7,000,000 gas limit, and 2,000,000,000 wei gas price. The installed Ganache uses its existing JavaScript transport fallback if the optional platform uWS binary is unavailable.

No compiler, custom AMM, service math, state override, or storage projection is used. Setup deploys actual constructors, deposits 1 ETH into WETH9, creates both pairs through Factory, transfers token reserves, calls Pair.mint, and approves real allowances. Fee-token behavior is the official test token's 1% burn.

## Fixture schema 1

The root has `schema_version: 1`, `authority`, `source_pins`, `reference`, and `cases`. `reference` records the fixed case inventory, counts, runtime and source closure, genesis, runtime identities, and setup receipts.

Each case includes these fields for direct selection into the independent Rust `FixtureState`:

| Field | Shape |
| --- | --- |
| `schema_version` | integer `1` |
| `state_anchor` | `{chain_key, block_number, block_hash, parent_hash, timestamp, genesis_hash}` |
| `execution_environment` | `{chain_id: 31337, fork: "shanghai", block_number, block_hash, parent_hash, timestamp, coinbase, gas_limit, base_fee, prevrandao}` |
| `accounts` | array of `{address, exists, balance, nonce, code, storage}` |
| `accounts[].storage` | array of `{slot, value}`, both exactly `0x` plus 64 lowercase hex digits |
| `block_hashes` | array of `{number, hash}`, covering genesis through the anchor for this small local chain |

All block numbers, timestamps, nonces and environment gas limits are integers. Native balances and base fee are decimal strings; hashes are `0x` plus 64 lowercase hex digits. Code is full `0x` hex. `state_anchor` is the pre-transaction block H. `execution_environment` is the actual observed mined transaction block H+1, whose parent hash equals the anchor hash. Equal timestamps are intentional. The state key in evidence is `evm:31337/<preblock hash>`; the transaction block is never substituted as the state anchor.

Additional per-case fields are `name`, domain-shaped `pool`, `executor`, `transaction`, `transaction_context`, `token_in`, `token_out`, `recipient`, `amount_in`, `quote_context`, `expected`, and `evidence`. Pool uses protocol `uniswap-v2`, adapter `uniswap-v2/1`, fee 3/1000, historical reserve getters, and `v2: {factory, code_sha256}`. Code SHA-256 values are bare lowercase hex over runtime bytes. The fee counterexample marks `token_behavior: "fee-on-transfer"`; the other cases use `"standard"`.

`transaction` has `{chain_key, from, to, data, value: "0"}`. `transaction_context` has `{nonce, gas_limit, gas_price, chain_id}`; its nonce, gas limit and chain ID are lossless JSON integers, and its gas price is a decimal string. Legacy type 0 is verified and preserved in the raw RPC transaction and signed envelope rather than adding an unknown field to the strict Rust transaction context. `executor` has `{target, code_sha256, factory, factory_code_sha256, adapter_version: "uniswap-v2-router02/1.1.0-beta.0"}`.

`quote_context` is `{slippage_bps: 50, deadline_ms, quoted_buy_amount}`. The gross quote comes from an actual `getAmountsOut(amount_in, path)` call to the published Router at the pre-transaction bank. Every encoded minimum except the intentional `min-out-revert` is `floor(quoted_buy_amount * 9950 / 10000)`; `deadline_ms` equals the encoded deadline seconds multiplied by 1000. The min-out revert retains its intentionally excessive minimum, and the deadline revert retains its expired deadline. `evidence.quote_view` preserves the actual anchored call data, caller, target, and return bytes. This is an input constraint for exact plan binding; all expected outcomes still come from the mined transaction and measured balances. The fee counterexample succeeds at Router while its taxed recipient delta falls below the encoded 50 bps minimum.

`expected` records actual receipt status, transaction hash and gas used; caller sell and recipient buy balances before and after; actual sender token consumption; measured recipient `amount_out`; supplementary `router_amount_out` when the anchored EVM call is decodable; and actual caller nonce before and after mining. The recipient delta, including fee-token burn effects, is the expected native output. No math or Router return supplies that delta.

`expected.post_accounts` is required for all ten cases. It has exactly the same address and storage-slot boundary as the pre-transaction `accounts`, including explicit zero slots. Every post native balance, nonce, code byte string and listed storage value is queried from the original local node at `receipt.blockNumber` (the actual H+1 block). Existence retains the independently established genesis-funding or official-creation authority; no lifecycle-changing opcode is admitted by the swap trace. `expected.pool_after` is required and contains `{reserve0, reserve1, block_timestamp_last}` from the original Pair's `getReserves` at that same mined block. Its header hash is rechecked after capture. These are observed post-state values for comparing the complete declared REVM overlay boundary, with no claim about unenumerated chain state or an all-chain state root.

`evidence` contains the raw signed transaction, RPC transaction and receipt, both complete headers, trace digest/step count, exact opcode-derived storage footprint, historical pool getter observations, and explicit local replay scope. It is extra provenance, not part of Rust `FixtureState`.

## Strict state capture

For each actual transaction, `debug_traceTransaction` supplies every SLOAD/SSTORE key. CALL/STATICCALL enter the callee storage context; DELEGATECALL/CALLCODE retain the caller's storage context. The trace includes Pair lock slot 12 stores. Empty storage footprints are valid for the deadline revert, which terminates before storage access.

Images also include both tokens' caller, recipient, pair and Router balance mappings at slot 1, caller-to-Router allowances at nested slot 2, token fixed slots, Pair slots 0–12, Factory slots and both nested `getPair` directions at slot 2. Router and Pair factory/token/reserve getter helpers therefore have all required account/code/storage inputs even when the swap does not call Factory. WETH and the funded miner are retained as existing accounts. Every captured storage value, including zero, is fetched from H after the transaction, then H's header hash is rechecked. Account balances, nonces and code are fetched at H as well. Existence is supported by explicit genesis funding or a successful official creation receipt, never inferred from a zero balance or empty code.

## Fixed case inventory

1. `token0-to-token1-distinct-recipient`
2. `token1-to-token0-distinct-recipient`
3. `token0-to-token1-same-recipient`
4. `token1-to-token0-same-recipient`
5. `deadline-revert`
6. `min-out-revert`
7. `allowance-revert`
8. `balance-revert`
9. `fee-token-output-counterexample`
10. `input-donation-before-swap`

There are six successful receipts and four reverted receipts. The fee counterexample is included for native execution comparison and is outside normal-token one-hop math admission. Revert cases remain full execution cases. Independent REVM replay and execution-plan binding are performed by the owning Rust/main workers.

The last case transfers 50 input tokens to the nominal Pair through the official ERC20 before capturing its pre-bank, with no `sync` or `mint`. `evidence.input_donation` records that real transfer receipt, the unchanged reserve getters, the input-token pair balance slot and its actual surplus, the decoded Pair Swap event, and actual post-swap reserve getters. Its pre-bank raw input balance exceeds stored reserve0 by 50 tokens; the Swap event observes 1050 tokens of input for the 1000-token Router transaction. The Router quote and recipient delta still agree. State images retain the actual stored reserves and raw balances separately.
