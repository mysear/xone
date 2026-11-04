# Curve Crypto full exchange reference

This directory is a local reference for the bounded nominal two-token profile. It
contains the original unmodified `CurveCryptoSwap2.vy`, original
`contracts/tricrypto/CurveTokenV4.vy`, and the same-revision `CurveTokenV5.vy` for
primitive comparison. All three come from
`curvefi/curve-crypto-contract@d7d04cd9ae038970e40be850df99de8c1ff7241b`.
`sources.json` pins their complete bytes. The pool is also checked against the
pre-existing original source in `../curve-models/crypto`.

The generator deploys the original pool with Vyper 0.3.3 and the original V4 LP
with Vyper 0.3.1. `Tokens.sol` supplies only exact ERC20 bookkeeping: real sender
and pool balances, checked transfers, allowances, minting and Transfer events.
There is no LP mock. LP ownership starts with the deployer, then the original
`set_minter(pool)` assigns mint/burn authority to the pool. Every deployment checks
`minter() == pool` and exact deployed LP bytecode. Ganache enforces the ordinary
24576-byte EIP-170 limit; no unlimited-size mode is used.

## API

`quote_full_exact_in(balances, precisions, params, pricing_state, ledger,
timestamp, input, index_in, index_out)` returns `CryptoFullQuote`.
`quote_full_claim_admin(precisions, params, pricing_state, ledger, timestamp)`
executes the original external claim path independently, returning amount zero.

`CryptoFullLedger` fields are `total_supply`, `receiver_balance`, `raw_balances`,
`admin_receiver_enabled`, and `ramp: Option<CryptoRamp>`. Supply and receiver
balance refer to the LP token. Raw balances are actual pool ERC20 balances
**before** transfers. A disabled receiver means the original zero-address
branch: gulp and D/virtual-price recomputation still run; mint arithmetic and
profit subtraction are skipped.

`CryptoRamp` holds `initial_amp`, `initial_gamma`, `initial_time`. The existing
`CryptoParams.amp/gamma` are the future endpoints and
`CryptoState.future_a_gamma_time` is the ramp end. An active ramp requires the
origin. The implementation uses the original weighted interpolation, including
down ramps, checked time subtraction, end sentinel 1, and the internal claim's
second parameter read. This math-level coverage does not widen the main
service's settled-only admission rule.

`CryptoFullQuote` returns `amount_out`, `next_balances`, `fee_rate`,
`next_pricing_state`, `next_total_supply`, `next_receiver_balance`,
`next_raw_balances`, `minted_admin`. Pricing state includes D, scale, oracle,
last price/timestamp, both profit counters, virtual price, not_adjusted, and ramp
end time. Calls return fresh values, so errors leave the caller's inputs intact.
The old `quote_exact_in` keeps its two boundary errors and old corpus behavior.

## LP primitive

Original V4 lines 175–188 implement:

```
assert caller == minter
minted = totalSupply * fraction / 10**18
if minted > 0:
    totalSupply += minted
    balanceOf[receiver] += minted
    emit Transfer(zero, receiver, minted)
return minted
```

The multiplication and both additions are checked uint256 operations. In the
pool, `fraction = virtual_price * 1e18 / (virtual_price - fees) - 1e18`.
The LP performs no second fraction conversion. V5 has the identical primitive.
The pool emits ClaimAdminFee even if a positive fee produces zero minted LP;
the original LP emits no Transfer in that case.

## Runtime and parser coordinates

`runtime-metadata.json` records compiler/dependency versions, original storage
layouts, initcode hash, template hashes, constructor arguments, actual deployed
hashes, LP minters, and observed immutable bytes. Hex files contain the exact
runtime templates.

| Item | Value |
|---|---|
| Pool template | 20814 bytes |
| Pool template SHA-256 | `c19b756213feac7b03ee10b12888c3abe427bd10f9ec125004091b56dd995a80` |
| Pool deployed size | 20974 bytes |
| Immutable tail | 160 bytes, five ABI words |
| LP runtime | 2969 bytes, no immutable tail |
| LP runtime SHA-256 | `d3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d` |
| LP balanceOf mapping / totalSupply / minter slots | 5 / 7 / 8 |

Immutable offsets are absolute zero-based byte offsets in deployed code:

| Field | Offset | Length |
|---|---:|---:|
| token | 20814 | 32 |
| coins[0] | 20846 | 32 |
| coins[1] | 20878 | 32 |
| PRECISIONS[0] | 20910 | 32 |
| PRECISIONS[1] | 20942 | 32 |

Address words are left zero-padded. Precision is `10**(18-decimals)`.
The generator compares the exact prefix and all five words with deployed
addresses/decimals. The verifier reconstructs each deployed runtime from template
and tail and checks its recorded hash. This is artifact-specific parser evidence.

## Reproduce and check

Run from the xone root. Existing `integration/.runtime/stable-reference/node_modules`
provides pinned installed ethers/ganache/solc versions, recorded in metadata.
The pool compiler is the existing
`integration/tools/reference/curve-models/.runtime/vyper-0.3.3/bin/vyper`.
Install the separate LP compiler if absent:

```sh
/Library/Developer/CommandLineTools/usr/bin/python3 -m venv integration/tools/reference/curve-crypto-full/.runtime/vyper-0.3.1
integration/tools/reference/curve-crypto-full/.runtime/vyper-0.3.1/bin/pip install vyper==0.3.1
node integration/tools/ci/curve-crypto-full-golden.cjs
python3 integration/tools/ci/curve-crypto-full-verify.py
PYTHONOPTIMIZE=2 python3 integration/tools/ci/curve-crypto-full-verify.py
python3 integration/tools/ci/curve-crypto-full-tests.py
PYTHONOPTIMIZE=2 python3 integration/tools/ci/curve-crypto-full-tests.py
env PATH=/Users/mac/.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin:$PATH cargo test --manifest-path dex-router/Cargo.toml -p xone-protocol-math --test curve_crypto_full --test curve_crypto_standalone
```

The corpus contains 60 isolated cases across 18/18 and 18/6 decimals: 44 successful
calls, 16 transaction reverts, 4 accepted adjustments, 18 positive admin mints,
and 12 successful direct claims. It exercises accepted/rejected automatic price
updates, failed adjustment followed by claim, donation gulp, receiver zero
including fees exceeding virtual price, active up/down ramps and end sentinels,
small derivative pricing, timestamp behavior, loss, checked overflow, LP zero
rounding, zero admin fee and raw transfer bookkeeping. Some cases deliberately
seed storage to isolate arithmetic boundaries; these are not historical on-chain
states or a claim that every seeded state is reachable through governance.

Every successful vector records full pool/LP/raw/trader before and after state,
exchange/claim events and LP mint Transfer events. Every revert verifies complete
rollback of the observed state. The Rust test compares all returned poststate
fields, and the old standalone test still checks the unchanged 288-case corpus.

Generation and verification atomically invalidate their own receipts before
optional dependency loading or input parsing. Explicit Python `require` checks
survive `-OO`. Four private-workspace failure probes exercise dependency failure,
malformed pins, malformed receipts and pin mismatches without modifying the live
receipts. The generator disconnects its own Ganache provider in `finally`.

## Scope

This is reference-only evidence for nominal ERC20s with exact transfers and no
callbacks, transfer taxes, rebasing, hooks or pool/trader/receiver address aliases.
The caller supplies coherent same-block parameters, pricing state and ledgers,
a live pool, and a funded/approved trader. Slippage selection, arbitrary token
behavior, historical chain state, production admission and VERIFIED status are
outside this sidecar. Domain/wire/config/parser/dispatch and service fixtures are
owned by the main thread. Old corpora, generators and progress documents were not
edited here. No Git operations were performed.
