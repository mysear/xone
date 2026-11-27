# Orca adaptive Token-2022 SwapV2

The execution corpus uses the pool, tick and Oracle layouts from the pinned Orca adaptive dynamic reference. It runs unsigned simulations with the observed deployment code in LiteSVM. The test sets the Clock, transfer-fee schedules, vaults and wallet balances in a controlled bank. The RPC fixture provides that bank to transaction preparation. The worker then executes the resulting transaction and compares user balances, withheld fees and both vault accounts.

The matrix crosses classic SPL plus Token-2022 on either side and dual Token-2022 with both directions and both amount modes. At the epoch boundary it also replaces the empty trailing array with an observed system-owned PDA and creates an ATA that already has lamports. The threshold test checks the original program's errors 6036 and 6037 and verifies the source bank is unchanged.

## Reproduce

Generate the adaptive input corpus and restore the solders 0.29.0 runtime using the existing Solana rebuild runtime lock. [Pinned reference](../reference/orca-adaptive-dynamic-service/README.md)

From the repository root, run the following commands with a deployment capture in `deployment-capture.json`.

```sh
python3.11 integration/tools/ci/orca-token2022-execution-bank.py deployment-capture.json /tmp/orca-token2022-execution-bank.json
XONE_ORCA_TOKEN2022_BANK=/tmp/orca-token2022-execution-bank.json \
  cargo +1.90.0 test --manifest-path dex-router/Cargo.toml \
  -p xone-execution-preparation --test orca_token2022 -- --ignored
```

The capture is a finalized `getMultipleAccounts` response in the order defined by `PROGRAM_IDS` in the bank generator, using `base64` or `base64+zstd`. The successful run used PublicNode slot 474441254 and response SHA-256 `1274e7d41a85044b85dec18e3680b254f096058104ee3299343796e1f4f1169c`. The ProgramData payloads follow the 45-byte loader header.

| Program | Deployment slot | Payload SHA-256 |
|---|---:|---|
| Whirlpool | 457152296 | `c422b22f3eefe86dd55878a4f505b9c68c26e241dc96af1573ca032b24b8ee3f` |
| SPL Token | 419472000 | `8190d3f7ceb6cb7a7a8d8924bff89f9f611e15ce1f806f2b6237f3311a98f697` |
| Token-2022 | 460497271 | `15a2e615a42e20f31714bbaaee43002ae6f1cda2e8d7118243117ab3c1342e2d` |
| Memo | Immutable | `f520eaf096361abbb9639ea4dc3e5388a87b9330e121f476607b87c46ef67954` |

## Signed swap replay

The Quote API test target contains `orca_dual_token2022_wallet_wire_matches_independent_http_simulation`. Supply the same bank and runtime manifest described below to run normal quote, preparation and independent simulation for both directions. The test compares signed and unsigned settlement amounts, fees and account digests. It derives the expected writable and read-only accounts from the preparation instructions and compares them to `messageAccountLocks` on both responses. It checks RPC validity at the last valid block height and rejects an expired height, unknown blockhash, stale RPC slot and corrupted wallet signature in each direction.

## Position reward collection

`POST /v1/chains/{chain}/pools/{pool}/prepare-orca-reward` resolves an initialized reward slot from the installed pool view. The `claim` body supplies `authority`, `position`, `positionTokenAccount`, `recipient`, `rewardAccount`, `rewardIndex` and `rewardTokenProgram`. Set `simulate` to `true` to verify the unsigned transaction with the configured RPC-capture SBF engine.

The API collects rewards from standalone positions and pays up to the captured vault balance into an existing account. It prepares `CollectReward` for classic SPL and `CollectRewardV2` for the supported Token-2022 `TransferFeeConfig` profile. Set `claim.updateBeforeCollect` to `true` to prepend `UpdateFeesAndRewards` in the same transaction. Both position bounds must be in initialized tick array accounts in the installed view. The preparation binds the arrays and Clock to the same finalized slot and reports the selected reward slot's increment in `rewardUpdate.accruedReward`.

The SBF replay verifies the gross debit, user credit, remaining position balance, fee payer and post-state digests for the position, vault and destination. With the update enabled it also verifies the complete Whirlpool poststate, all fee and reward checkpoints and the selected reward increment. The controlled cases use fixed and dynamic position-bound arrays with a separate sparse array in the pool view. They include full, partial and empty vault cases and a frozen recipient failure after the update instruction.

Use the same deployment capture and bank as the SwapV2 test.

```sh
python3.11 integration/tools/ci/solana-sbf-runtime-manifests.py --rpc-capture \
  solana:mainnet-beta 5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d /tmp/orca-sbf-manifest.json
XONE_ORCA_TOKEN2022_BANK=/tmp/orca-token2022-execution-bank.json \
XONE_ORCA_SBF_MANIFEST=/tmp/orca-sbf-manifest.json \
  cargo +1.90.0 test --manifest-path dex-router/Cargo.toml \
  -p xone-quote-api --test orca_reward -- --ignored
```
