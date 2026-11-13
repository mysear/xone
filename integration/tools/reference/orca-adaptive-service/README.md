# Orca adaptive service fixture API (v1)

`integration/test/contracts/orca-adaptive-service.json` is canonical. Byte-identical
copies live under chain-parser-rust and dex-router. This fixture has no dependency
on their domain, parser, protocol-math, protobuf, or online implementation.

Stable envelope:

```text
schema: "orca-adaptive-service-native-v1"
metadata: { layouts, source, code_identity, packing, limitations }
identity: { revision, source_inventory_sha256, harness_closure, compiler,
            executed_binary_sha256, execution, sbf:false }
scenarios: [{ name, config, banks: [{ slot, timestamp, accounts, source_slots,
              native_state, next_trade: { params, expected } | null }] }]
```

`accounts` maps base58 public keys to normal Solana RPC accounts:
`{lamports,owner,executable,rentEpoch,space,data:[base64,"base64"]}`.
`source_slots` maps the same keys to the enclosing bank slot. Each bank is a
complete synthetic finalized snapshot and includes program, ProgramData, classic
Token program, Clock, config, adaptive fee tier, pool, Oracle, two mints, two
vaults and three contiguous fixed tick arrays. Pools use the fee tier index seed,
not tick spacing, for their actual PDA; all bump and account addresses are given
in config's `fixture` metadata. Config otherwise follows the existing pool config
shape (`id,protocol,program_id,token0,token1,accounts,orca_whirlpool`).

`native_state` is an independent dump of the original program structs. All u128,
i128 and monetary u64 values are decimal strings. Timestamps/slots and fee rates
are JSON integers. It includes every Whirlpool field, Oracle constants, variables
and reserved bytes, all 88 tick slots in each array, all three uninitialized reward
slots including extensions, mint/vault decoded bytes, raw vault amounts and Clock.
Domain field aliases are deliberately left to the consuming main implementation.

Params: `{amount:string,a_to_b:bool,sqrt_price_limit:string,timestamp:u64,
max_steps:8192,amount_specified_is_input:true}`. Native swap_manager itself has no
step counter: 8192 is the consumer budget, never claimed as native enforcement.
Success expected: `{status:"fulfilled",amount_in,amount_out,amount_remaining:"0",
lp_fee,protocol_fee,fee_amount,initial_total_fee_rate,next_state,post_accounts,
transfer_model}`. Post accounts use the same bank Clock/slot; the next observed
bank advances slot and Clock to the next trade time. Vaults receive nominal input
and lose nominal output exactly, with fees retained in the input vault. This is
explicit fixture arithmetic, not execution of SPL CPI. A native failure has
`status:"native_rejected"`, `error`, `error_origin` and unchanged post accounts.

The executable accepts newline-delimited JSON, with `action:"identity"`,
`action:"generate"`, `action:"state",config,bank`, or `action:"trade",config,bank,params`. Every trade reads the
packed input accounts through original program types before executing unchanged
swap_manager / Whirlpool::update_after_swap / Oracle updates. Golden verification
replays every trade and compares the complete result and state to the fixture.
No expected state is read from the application domain or old math corpus.

Run from the xone root:

```sh
python3 integration/tools/ci/orca-adaptive-service-golden.py
python3 integration/tools/ci/orca-adaptive-service-check.py
```

The wrapper uses Rust 1.90.0, --offline --locked and the already populated
`/tmp/xone-orca-adaptive-swap-native` dependency cache, compiling a new independent
binary. Source/compiler/binary/closure/fixture hashes are written to receipt.json;
the receipt is invalidated before any build/generation/verification starts.

Program/Token code strings are reused byte-for-byte from the previous synthetic
Orca fixture and explicitly do not attest a deployed SBF program. Original account
pack and native manager evidence does not establish a validator transaction, CPI,
RPC finality, deployment or on-chain code equivalence.


The native `trade` adapter also accepts `amount_specified_is_input:false`.
`amount` then names the exact output target passed directly to the unchanged
original manager. The new output corpus uses `sqrt_price_limit:"0"`, requires
full target delivery, and compares complete pool, tick, reward and Oracle post
accounts. `amount_remaining` is the remaining specified-side amount (zero on
accepted full trades). Omitted mode defaults to the existing ExactIn behavior.
This native manager evidence does not execute SPL transfers or qualify an
unsigned transaction, SBF execution, wallet settlement or a deployed program.
