# Pinned Orca adaptive rewards native reference

Unchanged Orca cc683ec92acb5e7606caa3d8e514d9383e9c7c11 account pack, swap manager, reward manager, tick crossing and postupdate. The wrapper builds synthetic classic SPL mint/vault/Clock bytes and models nominal vault deltas. Two pools use fixed arrays, complete initialized Oracle and initialized rewards; covers overflowing emission product, wrapping reward global/outside and repeated/decay clocks. No deployment, SBF, CPI, reward collection or validator rollback proof. Original business logic is imported directly from the pinned original program. Consumer code is never imported.


The native `trade` adapter also accepts `amount_specified_is_input:false`.
`amount` then names the exact output target passed directly to the unchanged
original manager. The new output corpus uses `sqrt_price_limit:"0"`, requires
full target delivery, and compares complete pool, tick, reward and Oracle post
accounts. `amount_remaining` is the remaining specified-side amount (zero on
accepted full trades). Omitted mode defaults to the existing ExactIn behavior.
This native manager evidence does not execute SPL transfers or qualify an
unsigned transaction, SBF execution, wallet settlement or a deployed program.
