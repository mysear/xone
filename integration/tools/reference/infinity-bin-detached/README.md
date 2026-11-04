# Detached active bin: independent original burn/swap reference

Schema `infinity-bin-detached/1`, canonical `integration/test/contracts/infinity-bin-detached.json` and byte-identical Go/router copies. All Solidity dependencies are pinned infinity-core `3c5ef1be1f71442d8bc60cc6fd11f9c677b714df` sources. This reference uses a new harness and artifact. Frozen service artifacts, fixtures and input closure are read for unchanged verification and are never rewritten.

A fixture seeds original BinPool reserves/shares, original TreeMath.add membership, and a real original BinPosition position keyed by `calculatePositionKey(owner,id,salt)`. The owner's position owns total supply minus locked MINIMUM_SHARE 1000. Original BinPool.burn burns that whole position. The resulting detached active ID is 8358608, unchanged by burn. The tree removes it once supply is 1000, even though reserves can retain dust. The 1e6 reserves / 1e12 supply seed burns 999999999000 shares, withdraws 999999 of each currency, leaves (1,1) reserves and 1000 shares, zeros the owner's position and leaves an empty root when no neighbors exist.

State API: `active_id`, `bin_step`, `lp_fee`, `protocol_fee` (packed), `protocol_fee0`, `protocol_fee1`, `bins`, `detached_active`, `raw_bins`, `raw_storage`, `tree`. `bins` contains only actual tree members. `detached_active` is null or a separate bin object with ID, reserves, liquidity, shares, price and original position evidence, only when the current active ID is outside the tree. `raw_bins` preserves all seeded raw entries, including a historical removed active after a swap has crossed to real tree liquidity. Once crossed, `detached_active` is null; inverse traversal must use only original TreeMath successors and never reinsert the removed bin.

Each case has `seed`, `burn: {id,shares,pre,expected:{delta0,delta1,amounts,post}}`, and three sequential `trades`. Each trade has `label`, `amount_in`, `zero_for_one`, `state`, `expected:{native_reverted,revert_data,delta0,delta1,amount_out,swap_fee,protocol_amount,post}`. Numbers for quantities are decimal strings; IDs and fees are JSON numbers. Every poststate comes from a committed original-library transaction. Native reverts are caught externally and require exact full-state rollback. `raw_storage` uses the new harness's original State layout at slots 0..6: slot0=0, reserve mapping=1, share mapping=2, positions mapping=3, root=4, L1 mapping=5, L2 mapping=6. This is harness evidence, never Manager admission.

Sixteen bounded cases cover dust1, dust1000, asymmetric dust and zero fees, each with both directions and either empty root or sparse real neighbors. Real-tree cases cross from detached to a real bin, reverse past the removed ID into another real bin, then execute a local loop. Empty-root cases execute a native small trade, exhaust and revert, then reverse directly on the still-active detached reserves.

`manager_source_proof` binds the original Manager source and every maxBinStep reference: the current maximum is checked in initialize; setMaxBinStep updates only the global maximum. Existing pool parameters are not rewritten or validated against a lowered current maximum on swap. Full Manager/Vault settlement is not asserted.

Commands, run from xone:

```
node integration/tools/ci/infinity-bin-detached-golden.cjs
node integration/tools/ci/infinity-bin-detached-manager-proof.cjs
node integration/tools/ci/infinity-bin-detached-receipt-test.cjs
python3 integration/tools/ci/infinity-bin-detached-check.py
python3 -OO integration/tools/ci/infinity-bin-detached-check.py
python3 integration/tools/ci/infinity-bin-detached-guards.py
python3 -OO integration/tools/ci/infinity-bin-detached-guards.py
```

Generation and checks invalidate their receipts before fallible work. Checkers use explicit require functions and retain their checks under Python -OO. The metadata binds source, compiler settings/binary, loaded dependencies, Node executable, new harness/artifact and generator inputs.

The separate `manager-limit-receipt.json` runs a bounded real original Manager/Vault deployment and initialize/setter/getter test using the frozen original service artifacts after full source/compiler/binary/dependency validation. A step50 pool initialized at the central active ID 8388608 retains its slot0 and key after maxBinStep is lowered from 100 to 10. A new step50 pool fails initialization with original BinStepTooLarge(50). No full swap settlement is attempted.
