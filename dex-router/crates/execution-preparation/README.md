# Execution preparation

Internal Router component for dynamic account resolution, account/program/epoch/funding checks and unsigned transaction preparation. The current adapter is pinned Meteora single-pool classic SPL / admitted transfer-fee Token2022; the request supports ATA creation, WSOL funding, temporary-account refund and output unwrap.

`quote-api` owns authorization, quote-cache selection, plan-hash verification, rate limits, current QuoteView checks and final quote-id commitment. It checks the view before and after calling this crate. `SolanaQuote` is the immutable selected quote input, not a caller-supplied HTTP DTO; `prepare_solana` also rejects inconsistent amounts/token direction/chain, invalid hashes and expired requests before RPC.

RPC reads and optional `simulateTransaction` are bounded; this crate never signs, sends transactions or handles wallet secrets. RPC simulation remains separate from independent production verification. Other protocol adapters must supply their own verified account layouts and instruction templates before capability flags are enabled.
