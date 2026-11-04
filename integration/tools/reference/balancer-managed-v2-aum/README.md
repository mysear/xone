# Managed V2 original AUM reference

Technical reference for `balancer-v2-managed-aum` / `balancer-v2/20230411-managed-aum-1`. Project scope and progress are maintained only in the root DEVELOPMENT_PROGRESS.md T34 section.

## Offline use

Run `node integration/tools/ci/managed-v2-aum-golden.cjs` to compile the two original Managed jobs, execute a fresh ordinary Shanghai EVM, validate the bank, and atomically write three fixture copies. Use `--output /tmp/bank.json` for a temporary output. `python3 integration/tools/ci/managed-v2-aum-check.py --check` requires exact temporary regeneration and three identical copies. `--input FILE --validate-only` validates only the selected local copy plus its pinned source/tool closure. Run guards with ordinary Python and `python3 -OO`. Every entry invalidates stale verified success before imports; generation failures remove target outputs.

No install, download or runtime dependency modification occurs. The read-only Node environment is integration/.runtime/stable-reference; the original solc 0.7.1 executable is retained under compiler/. runtime-pins.json binds Node and every loaded dependency, package metadata and lockfile. file-manifest.json inventories immutable sources and tools; mutable generation.json/verified.json are excluded from all input closures.

## Published source

All six complete build-info documents are losslessly gzip-compressed, without changing original input source keys or source contents. source-pins.json records the fixed official balancer/balancer-deployments b55dc2b8a46bce8f27aed5a3b03f719f662f860a URLs, original/compressed hashes, every source key and hash, and task files. Compiler jobs 140 and 9999 are compared against original outputs before original bytecode deployment. Other original dependency jobs retain their original settings (Vault 1500, other dependencies 9999). License texts/provenance are under licenses/; complete original source comments and metadata license declarations remain in each build-info.

Only original Authorizer/Vault/Vault-created PFC/Provider/Factory/libraries/math/helper/ERC20 artifacts are executed. Original library constructors install their exact own address at bytes 1–20; validators check that exact value, declared links, repeated immutables, dependency immutable addresses and every remaining runtime byte. A 24577-byte control must fail; unlimited contract/init-code sizes are false. No facade, storage injection, mainnet transaction or custom AMM output formula is used.

## Bank and projection interface

`objects` is a SHA-256 content-addressed table of `{kind,value}` records. IDs hash canonical sorted-key JSON. A reference is exactly `{ref: ID}`. Validators require every reference to exist with its original digest and kind, and every object to be reachable from top-level roots. Repeated code, getters, token/account observations, receipts and states share objects.

Each scenario has an immutable `source_h` snapshot from the actual initialized local node. Cases separately identify `execution.before`, `execution.after`, the actual signed transaction/raw receipt, and `after_authority=derived-from-local-original-transactions`; no execution result is projected as a fresh canonical source anchor. Snapshots bind one complete raw block header to raw ABI calls/results, account observations and an explicit raw state. All getter reads use that block number. `state.underlying` contains only underlying tokens; Vault BPT registration position, cash/managed and actual balance are retained separately in `state.registration`/`state.vault_bpt`. All amounts are decimal strings; native header quantities retain original RPC hex. Virtual supply is explicitly derived from original totalSupply and Vault BPT cash/managed; actualSupply is an original getter. No final service Pool JSON is assumed here.

Swap expected output is an ABI call to the original deployed ExternalWeightedMath, with original upscale-then-mulDown fee ordering; low precision includes a one-raw-unit counterexample. AUM validation follows the original integer formula in ExternalFees/ExternalAUMFees/ManagedPoolSettings and actual mint logs, never xone math. Provider mappings 2 (YIELD) and 3 (AUM) are distinguished and observed: the original live mock's aumFeeId=2 is not assumed to mean global AUM=3. Manager/collector aliases use unique accounts and aggregate both real mint events.

Raw transaction/header/getter/account evidence is retained in the bank. EVM traces and full storage proofs are not captured, and none are claimed. Real managed balances use public Vault managePoolBalance WITHDRAW/DEPOSIT and actual ERC20 movement, not edited storage. Source/dependency deployment authenticity on mainnet is unverified. TestWETH is the original official test artifact, not canonical mainnet WETH; native settlement is outside this reference. Dynamic schedules, active breakers and unknown token behavior remain unsupported.

## Native identity getter closure

Every sourceH and execution before/after snapshot captures these additional original ABI requests at that snapshot's exact block height (never parser test shape responses):

| Original contract | Required getter requests |
|---|---|
| ManagedPoolFactory | getVault(), isPoolFromFactory(pool), getProtocolFeePercentagesProvider(), getWeightedMath(), getRecoveryModeHelper(), getCreationCodeContracts(), getPauseConfiguration() |
| ManagedPool | name(), getProviderFeeId(0), getProviderFeeId(2), in addition to the existing getProviderFeeId(3) |
| ProtocolFeePercentagesProvider | getVault(), getFeeTypeMaximumPercentage(2), getFeeTypeMaximumPercentage(3) |
| Vault | WETH(), getProtocolFeesCollector() |
| ProtocolFeesCollector | vault() |
| RecoveryModeHelper | getVault() |

`getVault()` is the helper's original ABI method; no fabricated `Vault()` or helper `vault()` selector is provided. Provider caps are checked against the original deployed Provider constructor parameters, not the synthetic 100% cap previously used by parser shape tests. Factory pause results are checked against the original factory deployment timestamp/durations and each snapshot timestamp. Membership, dependencies, name, fee IDs, WETH and collector relationships are independently checked against the pinned original runtimes and actual creation parameters. Missing, forged or noncanonical ABI responses fail validation.

Snapshot getter records remain deduplicated by content. Their exact-height request anchor is the enclosing raw snapshot header; generation rechecks that height's block hash before publishing the snapshot. Source headers and raw transactions are retained unchanged. `economic-pins.json` freezes the historical 27-case/292-transaction replay excluding only the growing getter closure and script-input manifest, and requires every original frame's raw getter mapping to remain an ordered subset. Its historical fixture digest is provenance, not the current bank pin. New identity getter omissions/forgeries are tested after graph resealing, so ordinary and optimized guards cannot pass merely because a mutation broke a content hash.
