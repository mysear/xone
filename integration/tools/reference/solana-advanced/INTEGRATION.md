# Solana advanced reward slice integration contract

Main owns central protocol/config/registry/Pool validation and proto generation.
The chat tool refused messaging the native ancestor; this file is the coordination record.

Profiles:
- `orca-whirlpools-rewards`: `orca-whirlpools/cc683ec9-spl-rewards-1`; same config and required account order as Orca static. Classic SPL, static fees, no initialized oracle or control extensions.
- `raydium-clmm-rewards`: `raydium-clmm/6d7a0f82-spl-rewards-1`; same config/account order as CLMM static, base bitmap, seed_index=0, collect_fee_on=0, no dynamic fees or limit orders.

Both use token_behavior=standard. Domain state carries reward_infos as a default empty, omitted-on-empty vector; ticks carry reward_growths_outside likewise. Advanced profiles require exactly three entries (including uninitialized slots); existing profiles require empty entries. Existing JSON hashes remain unchanged.

Proposed wire additions (do not consume reserved tags):
- OrcaWhirlpoolState: repeated OrcaRewardInfo reward_infos = 50
- OrcaTick: repeated Amount reward_growths_outside = 20
- RaydiumCLMMState: repeated RaydiumCLMMRewardInfo reward_infos = 64
- RaydiumCLMMTick: repeated Amount reward_growths_outside = 20
- OrcaRewardInfo: string mint=1; string vault=2; bytes extension=3 (exact 32 bytes); Amount emissions_per_second_x64=4; Amount growth_global_x64=5
- RaydiumCLMMRewardInfo: uint32 reward_state=1; uint64 open_time=2; uint64 end_time=3; uint64 last_update_time=4; Amount emissions_per_second_x64=5; uint64 reward_total_emitted=6; uint64 reward_claimed=7; string token_mint=8; string token_vault=9; string authority=10; Amount reward_growth_global_x64=11

Until wire types are generated, advanced to_proto must fail closed, never silently discard reward state.

Implementation plan: establish native upstream reward/crossing vectors; add failing targeted production tests; implement bounded packed decoding, typed validation, accrual and crossing poststate; compare full reward state and repeated swaps; run only new targeted tests and existing relevant profile regressions. Reference harness is independent from production crates and imports unchanged pinned upstream methods. No SBF, Token CPI or deployed-bytecode execution claims.
