# Initialized rewards at pinned Solana program revisions

This slice adds `orca-whirlpools-rewards` at Orca
`cc683ec92acb5e7606caa3d8e514d9383e9c7c11` and `raydium-clmm-rewards` at Raydium
`6d7a0f82d2f293c553c69aaf943377367c6c4cc6`. These remain classic SPL Token,
static-fee profiles. Orca supports full exact-input and exact-output targets;
Raydium retains its full exact-input interface in this slice. Existing profiles reject populated reward
state; their omitted JSON fields and empty wire fields retain their previous shape.

See [INTEGRATION.md](INTEGRATION.md) for the exact centralized profile and wire
contract. Only the owned domain source files are mirrored between services.

## Reference authority and reproduction

`orca/` and `raydium-clmm/` are independent native reference crates. Their adapters
were derived from the existing pinned reference harnesses in the sibling folders;
the changes inject and extract complete reward data through the upstream structs.
They depend directly on unchanged upstream program sources. No XONE domain,
parser, router or production arithmetic is linked into either reference. The
Raydium adapter retains the original reference's bitmap/seed/dynamic adapters to
support its shared account serialization and base swap harness, but the new
profile's observations use base bitmap, default seed and static fee mode only.

From the repository root:

```sh
python3 integration/tools/ci/solana-advanced-build.py
python3 integration/tools/ci/solana-advanced-golden.py --check
```

Omit `--check` only when intentionally regenerating the checked-in native evidence.
The generator verifies every file in the pinned source inventories (including
Raydium's pinned parity-common dependency). It records source revision and inventory
digest in each corpus. Cargo uses the already-pinned dependency lockfiles and the
existing local source replacement for parity-common. Rust 1.90.0 is selected
explicitly because this host's default Rust 1.83.0 cannot compile the workspaces.
No upstream source is modified.

The corpora under `integration/test/contracts/solana-advanced/` contain independent
swap outcomes, chained swaps with held/incremented timestamps, scalar reward and
tick-crossing edge cases, and native serialized packed-account RPC fixtures.
Service-local copies are referenced directly by the new Rust tests. The quote
comparison checks the entire next pool, so growth outside, emissions, fee state,
liquidity, observations, epoch and vault bookkeeping cannot be dropped unnoticed.

Native execution calls Orca's `swap_manager::swap`,
`Whirlpool::update_after_swap`, `next_whirlpool_reward_infos` and
`next_tick_cross_update`; Raydium's `swap_internal`,
`PoolState::update_reward_infos`, and `TickState::cross`. Raydium's original
`Clock::get()` is serviced by the existing deterministic native syscall stub.

These observations do **not** execute SBF, Anchor instructions/account constraints,
a validator bank, signatures, SPL Token CPI, reward claims or deployed executable
bytes. Executable bytes in RPC fixtures are explicitly synthetic identities. The
vault changes compared by these tests are the existing reference adapter's
checked bookkeeping after the original native swap result.

## Implemented account and arithmetic boundary

| Program | Packed state retained | Poststate behavior |
| --- | --- | --- |
| Orca | All three 128-byte reward records at pool offset 269; original 32-byte extension; emissions and growth u128; all three tick reward growths at tick offset 65; fixed and dynamic tick arrays | Accrue once using pre-swap liquidity and bank time; checked u128 product overflow suppresses the delta; global growth and crossing subtraction wrap at u128; timestamp advances on successful swaps |
| Raydium CLMM | All three 169-byte records at pool offset 397, including schedule/state, emitted/claimed u64, mint/vault/authority, emissions/global growth u128; tick reward growths at tick offset 68 | Clamp time to reward end; round emitted tokens up and growth down; saturate total at u64::MAX and recompute last growth from remaining supply; preserve already-exhausted slots; update opening/ended state and recent epoch; crossing wraps u128 |

New profiles require exactly three reward slots and exactly three growth entries
for every initialized tick, even when some rewards are uninitialized. Uninitialized
rewards must have zero schedule/emission/growth (Raydium authority and Orca slot-zero
authority bytes remain preserved). Nonzero growth for an uninitialized reward is
rejected. Orca control extension slots one and two remain zero; Raydium limit order
fields, reserved tails and unrelated advanced fields remain rejected.

## Remaining capabilities and concrete dependencies

- **Orca adaptive fees remain unsupported.** The initialized Oracle account must
  retain pool binding, `trade_enable_timestamp`, adaptive constants
  (`filter_period`, `decay_period`, `reduction_factor`,
  `adaptive_fee_control_factor`, `max_volatility_accumulator`, `tick_group_size`,
  `major_swap_threshold_ticks`) and variables
  (`last_reference_update_timestamp`, `last_major_swap_timestamp`,
  `volatility_reference`, `tick_group_index_reference`,
  `volatility_accumulator`). A complete slice must implement reference decay,
  intra-swap tick-group boundaries/fee changes, major-swap timestamp changes,
  and oracle poststate, using the original native adaptive manager as oracle.
  Merely allowing a tier index different from spacing or initialized Oracle would
  admit incorrect swaps; these guards remain in place.
- **Token-2022 transfer-fee profiles remain unsupported for Orca, Raydium CLMM and
  Raydium CPMM.** A complete slice needs separately pinned Token-2022 executable
  identities, token-program ownership per side, exact TLV allowlists and mint
  account type, older/newer fee schedules selected by bank epoch, both vault
  withheld amounts, and gross debit versus net credit/output semantics around
  the original v2 swap entrypoints. Raydium's `collect_fee_on` controls AMM fee
  denomination and does not establish Token-2022 support. Transfer hooks,
  confidential transfers and other unmodeled TLVs must continue failing closed.
  Original token processor execution needs a separate real instruction/CPI or
  SBF harness before making any token execution claim; a pure fee helper is
  insufficient.
- **Cross-products remain unsupported:** rewards combined with Raydium dynamic
  fees, bitmap extension/custom seed/output-denominated fees or Orca adaptive
  fees; nonzero Orca control extensions; reward claims/position settlement and
  limit orders. These should receive their own finite profile and native evidence
  rather than changing the existing profile's semantics.


Orca target quotes also support `action:"quote_exact_out"` with `output` and
`zero_for_one`, using the unchanged fixed cc683ec9 manager's output-specified
branch and original reward/tick post updates. Both ordinary and initialized
reward pools use this adapter. Raydium's interface remains unchanged. This is
native manager/math evidence, not SPL CPI, SBF or wallet transaction evidence.
