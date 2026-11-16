# Execution preparation

Internal Router component for dynamic account resolution, account/program/epoch/funding checks and unsigned transaction preparation. The adapters are pinned Meteora single-pool classic SPL / admitted transfer-fee Token2022 , pinned Raydium CP-Swap creator-fee classic SPL, pinned canonical/boost PumpSwap classic SPL, and static/rewards Orca Whirlpool classic SPL. Meteora supports ATA creation, WSOL funding, temporary-account refund and output unwrap. Raydium CP-Swap and PumpSwap classic SPL share explicit ATA creation, WSOL funding, temporary-source refund and output unwrap. Unwrap requires caller and recipient to match and uses a newly created temporary account; existing user accounts are never closed.

`quote-api` owns authorization, quote-cache selection, plan-hash verification, rate limits, current QuoteView checks and final quote-id commitment. It checks the view before and after calling this crate. `SolanaQuote` is the immutable selected quote input, not a caller-supplied HTTP DTO; `prepare_solana` also rejects inconsistent amounts/token direction/chain, invalid hashes and expired requests before RPC.

RPC reads and optional `simulateTransaction` are bounded; this crate never signs, sends transactions or handles wallet secrets. RPC simulation remains separate from independent production verification. Other protocol adapters must supply their own verified account layouts and instruction templates before capability flags are enabled.

`prepare_solana_captured` additionally returns an immutable `AccountSnapshot` from the same, single `getMultipleAccounts` response used for validation. It retains observed slot, chain/genesis and explicit null accounts, including Rent and all compiled message dependencies. HTTP responses expose only its hash/summary, while the server retains the raw capture for optional independent SBF execution. Capture is discarded on blocked or failed preparation. A same-context RPC capture does not establish historical consensus-bank identity or production verification.

PumpSwap binds caller and recipient to the declared model user and supports original `sell`, `buy`, and `buy_exact_quote_in` instructions with volume tracking disabled as modeled. Sell-target ExactOut uses the original fixed-input/minimum-output instruction; actual credit and its surplus are retained separately from the plan minimum. `quote-api` includes `minBuyAmount` in the preparation commitment for accurate final receipt checks. Full captured ProgramData closures remain bounded at 16 MiB per capture/input; the aggregate cache byte budget remains 32 MiB.


Pump native SOL classic curve additionally treats sourceNativeSol/destinationNativeSol as explicit asset bindings. The native side's legacy sourceTokenAccount/destinationTokenAccount field identifies the declared user System account, while the SPL side identifies an existing classic token account. Caller, recipient and fee payer equal that user. Buy funding covers the maximum native input plus network fee; sell only requires the fee upfront. Actual native trading deltas remove the fee from debit or add it back to credit. sell/buy/buy_exact_sol_in; buy may explicitly initialize its canonical destination ATA, topping up only the missing rent for absent or empty System-owned addresses; no implicit WSOL conversion or migration transaction.


For canonical ATA initialization, destinationMinimumRentLamports records the RPC rent minimum, destinationPrefundedLamports records an existing empty System account's lamports, and rentLamports records only the payer top-up. Native settlement treats the pre-initialization System account as zero tokens; its after account must be the expected initialized SPL mint/user. These fields are committed with the actual create+swap wire. Other preexisting owners, executable accounts and nonempty System data are rejected.


Orca classic static/rewards use the original legacy swap with one signer, exact input/output, an explicit 1,400,000-unit ComputeBudget limit and the captured native ComputeBudget program. Every Pool and fixed/dynamic tick-array pricing byte is checked against the selected QuoteView. Directional array selection matches the original shifted B-to-A search and three-array limit; one/two-array source windows repeat the last observed account to fill ABI slots, relying on original key deduplication. No outside-window array is fabricated. Classic funding supports explicit ATA, WSOL top-up, temporary refunds and unwrap. Captured WSOL vault donations are retained as lamports, without synchronizing or rewriting the vault. Adaptive Oracle profiles and Token2022 swap instructions require separate execution adapters. Progress and qualification remain in the root development ledger.

The existing adaptive/adaptive-rewards/adaptive-dynamic classic SPL profiles use the same legacy swap with a writable initialized Oracle. Preparation compares all 254 Oracle bytes, including the 16/16/128-byte reserved ranges, its Whirlpool binding and the original account owner. Independent SBF admission independently reconstructs the Oracle and requires the compiled write privilege. Bank fixtures align the selected slot and real program/Token code closure explicitly; their controlled clocks, mint/PDA transformations and user funds do not establish historical consensus. Current qualification is recorded only in the root progress ledger.

Classic Raydium CLMM static/rewards/token-fee profiles use the pinned `swap_v2` instruction. Preparation validates the complete 1544-byte Pool, 4483-byte Observation, every declared 10240-byte tick array, configuration fees, seeds, mint/vault identity, Clock and program closure. Original unused owner/padding bytes and inactive order cohorts are retained; active order quantities, unknown fields and undeclared combinations require separate adapters. Declared dynamic fee/bitmap/custom-seed state is described below. The ancillary Token2022 program has its own explicitly admitted code digest and does not permit Token2022 assets. The captured original Memo bytes are explicitly loaded rather than using the local VM's different bundled program. Classic funding options and the original WSOL vault donation remain separately reconciled.

Declared bitmap/custom-seed/dynamic profiles extend that classic-token template with their complete state. The 1832-byte bitmap extension binds the pool, PDA and both 112-word sides. Nonzero custom seeds participate as u16 little endian in the original Pool PDA and bump. Dynamic fees bind all 34 original state bytes plus the remaining zero reserved tail; they never read request-supplied fee parameters. The extension is a writable first remaining account before the original directional arrays. Advanced model qualification is separate in the root progress ledger; orders, Token2022 assets and undeclared combinations remain rejected.

Independent SBF hosts retain at most two verified Python code images per
constructor thread; no account bank, permission semaphore or execution limit is
shared by that retention. Every engine still hashes the configured source files,
and reuse compares the retained files byte-for-byte with those just-validated
bytes and with the compiled worker. Unix materialized files are read-only;
source or retained-image changes reject instead of silently rebuilding. The
constructor-thread cache is released with the thread, while active engines hold
an owning reference. This removes repeated cold code-file creation in multi-API
hosts without changing compute limits, production deadlines or evidence hashes.
Worker failure logs contain bounded I/O counts and child exit state, not account
or transaction content. Such diagnostics are not execution evidence.

The explicit EVM `evm_rpc` resolver consumes the already bound ExactIn or ExactOut
plan plus an operator supplied RPC/executor/token identity configuration. It
collects one block-hash pinned observation set, checks the complete declared V2
state, and rechecks canonical block and live nonce after reading balances and
allowance. Call count, aggregate response bytes and total duration are bounded;
redirects, retries and number-based state-read fallbacks are disabled. Errors do
not contain the provider URL or server error messages. An unpredictable unknown
block control checks all four balance/code/storage/call selector methods.

This produces `HASH_PINNED_RPC_OBSERVATION` evidence. It does not verify consensus,
a current hardfork fee model, full L2 fees, or independent execution. The server
retains no private key, performs no signing and sends no transaction. The API is
`POST /v1/quotes/{id}/prepare-evm`; the JSON request contains decimal-string
`gas_limit`, `max_fee_per_gas`, `max_priority_fee_per_gas`, and `max_network_fee`.
The runtime capability `execution.evmPreparation` is separate from the qualified
Solana preparation and wallet interfaces. Unsupported pools or missing configured
code identities fail closed; funding/approval blockers return no unsigned wire.
