.DEFAULT_GOAL := test
.PHONY: compose-prepare test build check fmt-check lint unit contract-test config-contract-test config-invalid-input-test log-schema-test log-redaction-test log-pressure-test fixture-test reducer-replay-test protocol-differential-test optimizer-oracle-test simulation-integration-test reorg-test fencing-test restore-test head-first-startup-test benchmark acceptance-report journal-integration-test kafka-smoke-test durable-journal-test

# Build and test behavior belongs to each service's own Makefile.
build fmt-check lint unit:
	$(MAKE) -C ../chain-parser-go $@
	$(MAKE) -C ../chain-parser-rust $@
	$(MAKE) -C ../dex-router $@
check: progress-document-test
	python3 tools/acceptance/verify.py
contract-test:
	python3 ../common/protoc/check.py
	python3 tools/ci/contracts.py
	python3 tools/ci/boundary.py
	$(MAKE) -C ../chain-parser-go contract-test
	$(MAKE) -C ../chain-parser-rust contract-test
	$(MAKE) -C ../dex-router contract-test
config-contract-test config-invalid-input-test: contract-test
log-redaction-test log-pressure-test:
	cd ../chain-parser-go && go test ./internal/observability
	$(MAKE) -C ../chain-parser-rust unit
	$(MAKE) -C ../dex-router unit
reducer-replay-test reorg-test fencing-test:
	cd ../chain-parser-go && go test ./internal/reducer
	$(MAKE) -C ../dex-router unit
fixture-test: build debug-build
	python3 tools/acceptance/smoke.py
optimizer-oracle-test restore-test head-first-startup-test: fixture-test
# Production-level gates fail explicitly while unavailable.
protocol-differential-test:
	python3 tools/acceptance/gate.py WP10 WP11
simulation-integration-test:
	python3 tools/acceptance/gate.py WP15
benchmark:
	python3 tools/acceptance/gate.py WP17 WP22
log-schema-test: fixture-test
	python3 tools/ci/log-schema.py
acceptance-report: check fixture-test
	python3 tools/acceptance/report.py
test: acceptance-report
journal-integration-test:
	python3 tools/acceptance/journal_integration.py
kafka-smoke-test: build debug-build
	python3 tools/acceptance/kafka_smoke.py
durable-journal-test: build debug-build
	python3 tools/acceptance/journal_integration.py
	python3 tools/acceptance/kafka_smoke.py

compose-prepare:
	python3 tools/ci/prepare-compose.py

.PHONY: full-local-acceptance reorg-journal-test
reorg-journal-test: build debug-build
	python3 tools/acceptance/reorg_smoke.py
	python3 tools/acceptance/reorg_smoke.py --transport http-development

full-local-acceptance: check fixture-test durable-journal-test reorg-journal-test v3-quote-test multidex-quote-test stable-quote-test singleton-quote-test raydium-quote-test orca-quote-test rayclmm-quote-test rayclmm-token-fee-quote-test rayclmm-bitmap-quote-test rayclmm-seed-quote-test rayclmm-dynamic-quote-test receipt-witness-quote-test source-wal-quote-test receipt-root-quote-test orca-exact-out-current-check rayclmm-exact-out-current-check
	python3 tools/acceptance/report.py

.PHONY: v3-differential-test v3-quote-test v3-reference-refresh
v3-differential-test:
	python3 tools/acceptance/v3_differential.py
v3-quote-test: build debug-build v3-differential-test
	python3 tools/acceptance/v3_smoke.py
v3-reference-refresh:
	python3 tools/ci/v3-oracle-setup.py
	python3 tools/ci/v3-wire-golden.py

.PHONY: debug-build
debug-build:
	$(MAKE) -C ../chain-parser-go debug-build
	$(MAKE) -C ../chain-parser-rust debug-build

.PHONY: multidex-differential-test multidex-quote-test multidex-reference-refresh
multidex-differential-test:
	python3 tools/acceptance/multidex_differential.py
multidex-quote-test: build debug-build multidex-differential-test
	python3 tools/acceptance/multidex_smoke.py
multidex-reference-refresh:
	python3 tools/ci/multidex-oracle-setup.py
	python3 tools/ci/multidex-wire-golden.py

.PHONY: stable-differential-test stable-quote-test
stable-differential-test:
	python3 tools/acceptance/stable_differential.py
stable-quote-test: build debug-build stable-differential-test
	python3 tools/acceptance/stable_smoke.py

.PHONY: stable-reference-refresh
stable-reference-refresh:
	python3 tools/ci/stable-oracle-setup.py
	python3 tools/ci/stable-wire-golden.py

.PHONY: singleton-differential-test singleton-quote-test singleton-reference-refresh
singleton-differential-test:
	python3 tools/acceptance/singleton_differential.py
singleton-quote-test: build debug-build singleton-differential-test
	python3 tools/acceptance/singleton_smoke.py
singleton-reference-refresh:
	python3 tools/ci/singleton-oracle-setup.py
	node tools/ci/singleton-process-fixture.cjs
	python3 tools/ci/singleton-wire-golden.py

.PHONY: raydium-differential-test raydium-quote-test raydium-reference-refresh
raydium-differential-test:
	python3 tools/acceptance/raydium_differential.py
raydium-quote-test: build debug-build raydium-differential-test
	python3 tools/acceptance/raydium_smoke.py
raydium-reference-refresh:
	python3 tools/ci/raydium-golden.py
	python3 tools/ci/raydium-wire-golden.py

.PHONY: orca-differential-test orca-quote-test orca-reference-refresh
orca-differential-test:
	python3 tools/acceptance/orca_differential.py
orca-quote-test: build debug-build orca-differential-test
	python3 tools/acceptance/orca_smoke.py
orca-reference-refresh:
	python3 tools/ci/orca-golden.py
	python3 tools/ci/orca-wire-golden.py

.PHONY: rayclmm-differential-test rayclmm-quote-test rayclmm-reference-refresh
rayclmm-differential-test:
	python3 tools/acceptance/rayclmm_differential.py
rayclmm-quote-test: build debug-build rayclmm-differential-test
	python3 tools/acceptance/rayclmm_smoke.py
rayclmm-reference-refresh:
	python3 tools/ci/rayclmm-golden.py
	python3 tools/ci/rayclmm-state-golden.py
	python3 tools/ci/rayclmm-wire-golden.py

.PHONY: rayclmm-evidence-check
rayclmm-evidence-check:
	python3 tools/acceptance/rayclmm_evidence.py
	python3 tools/ci/rayclmm-progress.py

.PHONY: rayclmm-token-fee-differential-test rayclmm-token-fee-quote-test rayclmm-token-fee-reference-refresh
rayclmm-token-fee-differential-test:
	python3 tools/acceptance/rayclmm_token_fee_differential.py
rayclmm-token-fee-quote-test: build debug-build rayclmm-token-fee-differential-test
	python3 tools/acceptance/rayclmm_token_fee_smoke.py
rayclmm-token-fee-reference-refresh:
	python3 tools/ci/rayclmm-token-fee-golden.py
	python3 tools/ci/rayclmm-token-fee-wire.py

.PHONY: rayclmm-token-fee-evidence-check
rayclmm-token-fee-evidence-check:
	python3 tools/acceptance/rayclmm_token_fee_evidence.py
	python3 tools/ci/rayclmm-token-fee-progress.py

.PHONY: rayclmm-bitmap-differential-test rayclmm-bitmap-quote-test rayclmm-bitmap-reference-refresh
rayclmm-bitmap-differential-test:
	python3 tools/acceptance/rayclmm_bitmap_differential.py
rayclmm-bitmap-quote-test: build debug-build rayclmm-bitmap-differential-test
	python3 tools/acceptance/rayclmm_bitmap_smoke.py
rayclmm-bitmap-reference-refresh:
	python3 tools/ci/rayclmm-bitmap-golden.py
	python3 tools/ci/rayclmm-bitmap-wire.py

.PHONY: rayclmm-bitmap-evidence-check
rayclmm-bitmap-evidence-check:
	python3 tools/acceptance/rayclmm_bitmap_evidence.py
	python3 tools/ci/rayclmm-bitmap-progress.py

.PHONY: rayclmm-seed-differential-test rayclmm-seed-quote-test rayclmm-seed-reference-refresh
rayclmm-seed-differential-test:
	python3 tools/acceptance/rayclmm_seed_differential.py
rayclmm-seed-quote-test: build debug-build rayclmm-seed-audit-test rayclmm-seed-differential-test
	python3 tools/acceptance/rayclmm_seed_smoke.py
rayclmm-seed-reference-refresh:
	python3 tools/ci/rayclmm-seed-golden.py
	python3 tools/ci/rayclmm-seed-wire.py

.PHONY: rayclmm-seed-evidence-check
rayclmm-seed-evidence-check:
	python3 tools/acceptance/rayclmm_seed_evidence.py
	python3 tools/ci/rayclmm-seed-progress.py

.PHONY: rayclmm-seed-audit-test
rayclmm-seed-audit-test:
	python3 -m unittest discover -s tools/acceptance/tests -p test_rayclmm_seed_evidence.py
	python3 -m unittest discover -s tools/ci/tests -p test_rayclmm_seed_traceability.py

.PHONY: rayclmm-dynamic-differential-test rayclmm-dynamic-quote-test rayclmm-dynamic-reference-refresh
rayclmm-dynamic-differential-test:
	python3 tools/acceptance/rayclmm_dynamic_differential.py
rayclmm-dynamic-quote-test: build debug-build rayclmm-dynamic-audit-test rayclmm-dynamic-differential-test
	python3 tools/acceptance/rayclmm_dynamic_smoke.py
rayclmm-dynamic-reference-refresh:
	python3 tools/ci/rayclmm-dynamic-golden.py
	python3 tools/ci/rayclmm-dynamic-wire.py

.PHONY: rayclmm-dynamic-evidence-check rayclmm-dynamic-audit-test
rayclmm-dynamic-evidence-check:
	python3 tools/acceptance/rayclmm_dynamic_evidence.py
	python3 tools/ci/rayclmm-dynamic-progress.py
rayclmm-dynamic-audit-test:
	python3 -m unittest discover -s tools/acceptance/tests -p test_rayclmm_dynamic_resume.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_rayclmm_dynamic_clock.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_rayclmm_dynamic_evidence.py
	python3 -m unittest discover -s tools/ci/tests -p test_rayclmm_dynamic_traceability.py

.PHONY: progress-document-test
progress-document-test:
	python3 -m unittest discover -s tools/ci/tests -p test_progress_document.py

.PHONY: receipt-integrity-test
receipt-integrity-test:
	cd ../chain-parser-go && go test -race ./internal/chain/evm -run 'TestReceiptIntegrity|TestReadCompleteRejectsReceiptGap|TestRPCBlockRequiresExplicit' -count=1

.PHONY: receipt-witness-test receipt-witness-quote-test
receipt-witness-test:
	cd ../chain-parser-go && go test -race ./internal/rpc ./internal/chain/evm -run 'TestProviderClients|TestForkConfirmationUsesByteEquivalent|TestReadComplete.*Witness|TestReadCompleteSingleEndpoint|TestReceiptWitnessDigest' -count=1
receipt-witness-quote-test: build debug-build receipt-witness-test
	python3 tools/acceptance/receipt_witness_smoke.py

.PHONY: source-wal-test source-wal-quote-test
source-wal-test:
	python3 -m unittest discover -s tools/acceptance/tests -p test_source_wal_evidence.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_source_wal_startup.py
	cd ../chain-parser-rust && $(HOME)/.cargo/bin/cargo +1.90.0 test --locked -p xone-solana-ingest --lib --test source_wal --test source_wal_rpc
source-wal-quote-test: build debug-build source-wal-test
	python3 tools/acceptance/source_wal_smoke.py

.PHONY: source-wal-evidence-check
source-wal-evidence-check:
	python3 tools/acceptance/source_wal_evidence.py

.PHONY: receipt-root-test receipt-root-reference-refresh receipt-root-quote-test
receipt-root-test:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_receipt_root*.py'
	python3 -m unittest discover -s tools/acceptance/tests -p test_receipt_reference.py
	cd ../chain-parser-go && go test -race ./internal/chain/evm -run 'TestReceiptRoot|TestReadComplete.*Root|TestReadComplete.*MatchingTail' -count=1
receipt-root-reference-refresh:
	python3 tools/ci/receipt-root-reference.py
receipt-root-quote-test: build debug-build receipt-root-test
	python3 tools/acceptance/receipt_root_smoke.py

.PHONY: receipt-root-evidence-check
receipt-root-evidence-check:
	python3 -m unittest discover -s tools/acceptance/tests -p test_receipt_root_evidence.py
	python3 tools/acceptance/receipt_root_evidence.py

.PHONY: account-shards-test account-shards-quote-test account-shards-regression account-shards-evidence-check
account-shards-test:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_account_shards*.py'
	cd ../chain-parser-rust && $(HOME)/.cargo/bin/cargo +1.90.0 test --locked -p xone-solana-ingest --test rpc_shards
account-shards-quote-test: build debug-build account-shards-test
	python3 tools/acceptance/account_shards_smoke.py
# This batch verifies all normal Solana RPC paths plus the ordinary three-service flow.
# It does not relabel old EVM/protocol differential reports as freshly executed.
account-shards-regression: check build debug-build account-shards-test
	python3 tools/acceptance/account_shards_evidence.py --bind-runtime
	python3 tools/acceptance/smoke.py
	python3 tools/acceptance/raydium_smoke.py
	python3 tools/acceptance/orca_smoke.py
	python3 tools/acceptance/rayclmm_smoke.py
	python3 tools/acceptance/rayclmm_token_fee_smoke.py
	python3 tools/acceptance/rayclmm_bitmap_smoke.py
	python3 tools/acceptance/rayclmm_seed_smoke.py
	python3 tools/acceptance/rayclmm_dynamic_smoke.py
	python3 tools/acceptance/source_wal_smoke.py
	python3 tools/acceptance/account_shards_smoke.py
	python3 tools/acceptance/account_shards_evidence.py --seal-scopes
account-shards-evidence-check:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_account_shards*.py'
	python3 tools/acceptance/account_shards_evidence.py

.PHONY: weighted-reference-refresh weighted-quote-test weighted-evidence-check
weighted-reference-refresh:
	python3 tools/ci/weighted-reference-fetch.py
	node tools/ci/weighted-golden.cjs
	python3 tools/ci/weighted-wire-golden.py
weighted-quote-test:
	python3 tools/acceptance/weighted_smoke.py
weighted-evidence-check:
	python3 tools/acceptance/weighted_evidence.py

.PHONY: weighted-regression
weighted-regression:
	python3 tools/acceptance/weighted_evidence.py --bind-runtime
	python3 tools/acceptance/weighted_regression.py
	python3 tools/acceptance/weighted_evidence.py --seal-scopes

.PHONY: weighted-static-reference-refresh weighted-static-regression weighted-static-evidence-check
weighted-static-reference-refresh:
	python3 tools/ci/weighted-reference-fetch.py
	node tools/ci/weighted-static-golden.cjs
	python3 tools/ci/weighted-static-wire-golden.py
weighted-static-regression:
	python3 tools/acceptance/weighted_static_evidence.py --bind-runtime
	python3 tools/acceptance/weighted_static_regression.py
	python3 tools/acceptance/weighted_static_evidence.py --seal-scopes
weighted-static-evidence-check:
	python3 tools/acceptance/weighted_static_evidence.py

.PHONY: five-family-regression five-family-evidence-check
five-family-regression:
	python3 tools/acceptance/five_family_evidence.py --bind-runtime
	python3 tools/acceptance/five_family_regression.py
	python3 tools/acceptance/five_family_evidence.py --seal-scopes
five-family-evidence-check:
	python3 tools/acceptance/five_family_evidence.py

.PHONY: curve-ng-state-test curve-ng-regression curve-ng-evidence-check
curve-ng-state-test:
	python3 tools/ci/curve-ng-state-verify.py --recompile
	python3 tools/ci/curve-ng-state-test.py
	python3 tools/ci/curve-ng-state-verify-test.py
	node tools/ci/curve-ng-state-startup-test.cjs
	python3 tools/acceptance/curve_ng_evidence_test.py
curve-ng-regression:
	python3 tools/acceptance/curve_ng_evidence.py --bind-runtime
	python3 tools/acceptance/curve_ng_regression.py
	python3 tools/acceptance/curve_ng_evidence.py --seal-scopes
curve-ng-evidence-check:
	python3 tools/acceptance/curve_ng_evidence.py

.PHONY: managed-reference-test managed-reference-regression managed-reference-evidence-check
managed-reference-test:
	python3 tools/ci/balancer-managed-fees-verify.py
	python3 tools/ci/balancer-managed-fees-test.py
	python3 -O tools/ci/balancer-managed-fees-test.py
	PYTHONPATH=tools/acceptance python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed*.py'
managed-reference-regression:
	python3 tools/acceptance/managed_reference_evidence.py --bind-runtime
	python3 tools/acceptance/managed_reference_regression.py
	python3 tools/acceptance/managed_reference_evidence.py --seal-scopes
managed-reference-evidence-check:
	python3 tools/acceptance/managed_reference_evidence.py

.PHONY: curve-meta-reference-test curve-meta-regression curve-meta-evidence-check
curve-meta-reference-test:
	python3 tools/ci/curve-meta-test.py
	PYTHONPATH=tools/acceptance python3 -m unittest discover -s tools/acceptance/tests -p 'test_curve_meta*.py'
	PYTHONOPTIMIZE=2 PYTHONPATH=tools/acceptance python3 -m unittest discover -s tools/acceptance/tests -p 'test_curve_meta*.py'
curve-meta-regression:
	python3 tools/acceptance/curve_meta_evidence.py --bind-runtime
	python3 tools/acceptance/curve_meta_regression.py
	python3 tools/acceptance/curve_meta_evidence.py --seal-scopes
curve-meta-evidence-check:
	python3 tools/acceptance/curve_meta_evidence.py

.PHONY: curve-crypto-ramp-reference curve-crypto-ramp-regression curve-crypto-ramp-smoke curve-crypto-ramp-acceptance
curve-crypto-ramp-reference:
	node tools/ci/curve-crypto-ramp-golden.cjs
	python3 tools/ci/curve-crypto-ramp-service-golden.py
	python3 tools/ci/curve-crypto-ramp-verify.py
curve-crypto-ramp-regression:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_curve_crypto*.py'
	PYTHONOPTIMIZE=2 python3 -m unittest discover -s tools/acceptance/tests -p 'test_curve_crypto*.py'
	python3 tools/ci/curve-crypto-ramp-verify.py
	PYTHONOPTIMIZE=2 python3 tools/ci/curve-crypto-ramp-verify.py
curve-crypto-ramp-smoke:
	python3 tools/acceptance/curve_crypto_ramp_smoke.py
curve-crypto-ramp-acceptance:
	python3 tools/acceptance/curve_crypto_ramp_evidence.py

.PHONY: orca-adaptive-rewards-reference orca-adaptive-rewards-regression orca-adaptive-rewards-smoke orca-adaptive-rewards-acceptance
orca-adaptive-rewards-reference:
	python3 tools/ci/orca-adaptive-rewards-service-golden.py
	python3 tools/acceptance/orca_adaptive_rewards_fixture.py
	python3 tools/ci/orca-adaptive-rewards-wire-golden.py
orca-adaptive-rewards-regression:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_orca_adaptive_rewards*.py'
	PYTHONOPTIMIZE=2 python3 -m unittest discover -s tools/acceptance/tests -p 'test_orca_adaptive_rewards*.py'
	python3 tools/ci/orca-adaptive-rewards-service-guards.py
orca-adaptive-rewards-smoke:
	python3 tools/acceptance/orca_adaptive_rewards_smoke.py
orca-adaptive-rewards-acceptance:
	python3 tools/acceptance/orca_adaptive_rewards_evidence.py

.PHONY: orca-adaptive-dynamic-reference orca-adaptive-dynamic-regression orca-adaptive-dynamic-smoke orca-adaptive-dynamic-acceptance
orca-adaptive-dynamic-reference:
	python3 tools/ci/orca-adaptive-dynamic-service-golden.py
	python3 tools/acceptance/orca_adaptive_dynamic_fixture.py
	python3 tools/ci/orca-adaptive-dynamic-wire-golden.py
orca-adaptive-dynamic-regression:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_orca_adaptive_dynamic*.py'
	PYTHONOPTIMIZE=2 python3 -m unittest discover -s tools/acceptance/tests -p 'test_orca_adaptive_dynamic*.py'
	python3 tools/ci/orca-adaptive-dynamic-service-guards.py
orca-adaptive-dynamic-smoke:
	python3 tools/acceptance/orca_adaptive_dynamic_smoke.py
orca-adaptive-dynamic-acceptance:
	python3 tools/acceptance/orca_adaptive_dynamic_evidence.py

.PHONY: evm-execution-reference-check evm-execution-regression evm-execution-smoke evm-execution-acceptance
evm-execution-reference-check:
	python3 tools/ci/evm-execution-check.py
	python3 tools/ci/evm-execution-guards.py
evm-execution-regression:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_evm_execution*.py'
	PYTHONOPTIMIZE=2 python3 -m unittest discover -s tools/acceptance/tests -p 'test_evm_execution*.py'
evm-execution-smoke:
	python3 tools/acceptance/evm_execution_smoke.py
evm-execution-acceptance:
	python3 tools/acceptance/evm_execution_evidence.py

.PHONY: v2-complete-reference-check v2-complete-wire v2-complete-regression v2-complete-smoke v2-complete-acceptance
v2-complete-reference-check:
	python3 tools/ci/v2-complete-check.py
	python3 tools/ci/v2-complete-guards.py
v2-complete-wire:
	python3 tools/ci/v2-complete-wire-golden.py
v2-complete-regression:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_v2_complete*.py'
	PYTHONOPTIMIZE=2 python3 -m unittest discover -s tools/acceptance/tests -p 'test_v2_complete*.py'
v2-complete-smoke:
	python3 tools/acceptance/v2_complete_smoke.py
v2-complete-acceptance:
	python3 tools/acceptance/v2_complete_evidence.py

.PHONY: managed-v2-aum-reference-check managed-v2-aum-reference-guards managed-v2-aum-wire-check managed-v2-aum-helper-test managed-v2-aum-differential-test managed-v2-aum-quote-test
managed-v2-aum-reference-check:
	python3 tools/ci/managed-v2-aum-check.py --check
managed-v2-aum-reference-guards:
	python3 tools/ci/managed-v2-aum-guards.py
	python3 -OO tools/ci/managed-v2-aum-guards.py
managed-v2-aum-wire-check:
	python3 tools/ci/managed-v2-aum-wire-golden.py --check
managed-v2-aum-helper-test:
	python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2*.py'
	python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2*.py'
managed-v2-aum-differential-test:
	python3 tools/ci/managed-v2-aum-service-golden.py --check
	cd ../chain-parser-go && go test ./internal/chain/evm ./internal/domain ./internal/config -run ManagedV2 -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-protocol-math --test managed_v2_aum --test managed_v2_swap --test managed_v2_pool --test managed_v2_native
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-optimizer --test managed_v2_shared
managed-v2-aum-quote-test: build debug-build managed-v2-aum-wire-check managed-v2-aum-differential-test
	python3 tools/acceptance/managed_v2_smoke.py

.PHONY: managed-v2-aum-acceptance
managed-v2-aum-acceptance:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$(HOME)/.cargo/bin:$$PATH" python3 tools/acceptance/managed_v2_acceptance.py

.PHONY: managed-v2-aum-paused-vault-reference-check
managed-v2-aum-paused-vault-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-aum-paused-vault-golden.cjs --check

.PHONY: managed-v2-aum-recovery-reference-check
managed-v2-aum-recovery-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-aum-recovery-golden.cjs --check

.PHONY: managed-v2-catalog-reference-check managed-v2-catalog-projection-check managed-v2-shared-reference-check managed-v2-multi-differential-test managed-v2-multi-helper-test managed-v2-multi-quote-test
managed-v2-catalog-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-aum-catalog-golden.cjs --check
managed-v2-catalog-projection-check:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_catalog.py --input test/contracts/managed-v2-aum-catalog.json --check test/contracts/managed-v2-aum-catalog-pools.json
managed-v2-shared-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-shared-vault-golden.cjs --check
managed-v2-multi-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run ManagedV2 -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-routing --test managed_v2_shared_vault
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-protocol-math --test managed_v2_catalog --test managed_v2_native --test managed_v2_pool
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-optimizer --test managed_v2_shared_vault
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-state-replica --test managed_v2_scope
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-quote-api --test managed_v2_scope
managed-v2-multi-helper-test:
	PATH="/usr/local/bin:$$PATH" python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2*.py'
	PATH="/usr/local/bin:$$PATH" python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2*.py'
managed-v2-multi-quote-test: build debug-build
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_multi_smoke.py --scope both

.PHONY: managed-v2-multi-acceptance
managed-v2-multi-acceptance:
	PATH="$(CURDIR)/.runtime/protoc-bin:$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_multi_acceptance.py

.PHONY: managed-v2-scheduled-reference-check managed-v2-scheduled-projection-refresh managed-v2-scheduled-projection-check managed-v2-scheduled-wire-check managed-v2-scheduled-helper-test managed-v2-scheduled-differential-test
managed-v2-scheduled-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-scheduled-golden.cjs --check
managed-v2-scheduled-projection-refresh:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input test/contracts/managed-v2-scheduled.json --output test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../chain-parser-go/test/contracts/managed-v2-scheduled.json --output ../chain-parser-go/test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-scheduled.json --original-input test/contracts/managed-v2-aum.json --output ../chain-parser-rust/test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../dex-router/test/contracts/managed-v2-scheduled.json --output ../dex-router/test/contracts/managed-v2-scheduled-pools.json
managed-v2-scheduled-projection-check:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input test/contracts/managed-v2-scheduled.json --check test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../chain-parser-go/test/contracts/managed-v2-scheduled.json --check ../chain-parser-go/test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-scheduled.json --original-input test/contracts/managed-v2-aum.json --check ../chain-parser-rust/test/contracts/managed-v2-scheduled-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_projection.py --input ../dex-router/test/contracts/managed-v2-scheduled.json --check ../dex-router/test/contracts/managed-v2-scheduled-pools.json
managed-v2-scheduled-wire-check:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 tools/ci/managed-v2-scheduled-wire-golden.py --check
managed-v2-scheduled-helper-test:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_scheduled*.py'
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_scheduled*.py'
managed-v2-scheduled-differential-test: managed-v2-scheduled-projection-check
	cd ../chain-parser-go && go test ./internal/chain/evm -run 'TestManagedScheduled|TestManagedV2RejectsForgedFixedFeeIDContext' -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-protocol-math --test managed_v2_scheduled --test managed_v2_scheduled_native -p xone-routing --test managed_v2_scheduled_shared

.PHONY: managed-v2-scheduled-smoke
managed-v2-scheduled-smoke: build debug-build
	PATH="$(CURDIR)/.runtime/protoc-bin:$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_smoke.py

.PHONY: managed-v2-scheduled-shared-reference-refresh managed-v2-scheduled-shared-reference-check managed-v2-scheduled-shared-projection-refresh managed-v2-scheduled-shared-projection-check managed-v2-scheduled-shared-helper-test managed-v2-scheduled-shared-differential-test managed-v2-scheduled-shared-smoke
managed-v2-scheduled-shared-reference-refresh:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-scheduled-shared-golden.cjs
managed-v2-scheduled-shared-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-scheduled-shared-golden.cjs --check
managed-v2-scheduled-shared-projection-refresh:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input test/contracts/managed-v2-scheduled-shared.json --output test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../chain-parser-go/test/contracts/managed-v2-scheduled-shared.json --output ../chain-parser-go/test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-scheduled-shared.json --original-input test/contracts/managed-v2-aum.json --output ../chain-parser-rust/test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../dex-router/test/contracts/managed-v2-scheduled-shared.json --output ../dex-router/test/contracts/managed-v2-scheduled-shared-pools.json
managed-v2-scheduled-shared-projection-check:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input test/contracts/managed-v2-scheduled-shared.json --check test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../chain-parser-go/test/contracts/managed-v2-scheduled-shared.json --check ../chain-parser-go/test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-scheduled-shared.json --original-input test/contracts/managed-v2-aum.json --check ../chain-parser-rust/test/contracts/managed-v2-scheduled-shared-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_projection.py --input ../dex-router/test/contracts/managed-v2-scheduled-shared.json --check ../dex-router/test/contracts/managed-v2-scheduled-shared-pools.json
managed-v2-scheduled-shared-helper-test:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_scheduled_shared*.py'
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_scheduled_shared*.py'
managed-v2-scheduled-shared-differential-test: managed-v2-scheduled-shared-projection-check
	cd ../chain-parser-go && go test ./internal/chain/evm -run 'TestManagedScheduledShared' -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-domain --test managed_v2_scheduled_shared_original
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-domain --test managed_v2_scheduled_shared_original -p xone-routing --test managed_v2_scheduled_shared_native
managed-v2-scheduled-shared-smoke: build debug-build
	PATH="$(CURDIR)/.runtime/protoc-bin:$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_scheduled_shared_smoke.py

.PHONY: managed-v2-bpt-reference-refresh managed-v2-bpt-reference-check managed-v2-bpt-projection-refresh managed-v2-bpt-projection-check managed-v2-bpt-wire-refresh managed-v2-bpt-wire-check managed-v2-bpt-helper-test managed-v2-bpt-differential-test
managed-v2-bpt-reference-refresh:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-bpt-golden.cjs
managed-v2-bpt-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-bpt-golden.cjs --check
managed-v2-bpt-projection-refresh:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input test/contracts/managed-v2-bpt.json --output test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../chain-parser-go/test/contracts/managed-v2-bpt.json --output ../chain-parser-go/test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-bpt.json --original-input test/contracts/managed-v2-aum.json --output ../chain-parser-rust/test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../dex-router/test/contracts/managed-v2-bpt.json --output ../dex-router/test/contracts/managed-v2-bpt-pools.json
managed-v2-bpt-projection-check:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input test/contracts/managed-v2-bpt.json --check test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../chain-parser-go/test/contracts/managed-v2-bpt.json --check ../chain-parser-go/test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-bpt.json --original-input test/contracts/managed-v2-aum.json --check ../chain-parser-rust/test/contracts/managed-v2-bpt-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_projection.py --input ../dex-router/test/contracts/managed-v2-bpt.json --check ../dex-router/test/contracts/managed-v2-bpt-pools.json
managed-v2-bpt-wire-refresh:
	python3 tools/ci/managed-v2-bpt-wire-golden.py
managed-v2-bpt-wire-check:
	python3 tools/ci/managed-v2-bpt-wire-golden.py --check
managed-v2-bpt-helper-test:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_bpt*.py'
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_bpt*.py'
managed-v2-bpt-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run 'TestManagedV2Bpt' -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-config --test managed_v2
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-config --test managed_v2
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-domain --test managed_v2_bpt_state --test managed_v2_bpt_wire_golden
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-domain --test managed_v2_bpt_state --test managed_v2_bpt_wire_golden -p xone-protocol-math --test managed_v2_bpt_math --test managed_v2_bpt_pool

	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-routing --test managed_v2_bpt -p xone-optimizer --test managed_v2_bpt -p xone-quote-api --test managed_v2_bpt

.PHONY: managed-v2-bpt-smoke
managed-v2-bpt-smoke: build debug-build
	PATH="$(CURDIR)/.runtime/protoc-bin:$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_bpt_smoke.py

.PHONY: managed-v2-breaker-reference-refresh managed-v2-breaker-reference-check managed-v2-breaker-projection-refresh managed-v2-breaker-projection-check managed-v2-breaker-wire-refresh managed-v2-breaker-wire-check managed-v2-breaker-helper-test managed-v2-breaker-differential-test managed-v2-breaker-smoke
managed-v2-breaker-reference-refresh:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-breaker-golden.cjs
managed-v2-breaker-reference-check:
	PATH="/usr/local/bin:$$PATH" node tools/ci/managed-v2-breaker-golden.cjs --check
managed-v2-breaker-projection-refresh:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input test/contracts/managed-v2-breaker.json --output test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../chain-parser-go/test/contracts/managed-v2-breaker.json --output ../chain-parser-go/test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-breaker.json --original-input test/contracts/managed-v2-aum.json --output ../chain-parser-rust/test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../dex-router/test/contracts/managed-v2-breaker.json --output ../dex-router/test/contracts/managed-v2-breaker-pools.json
managed-v2-breaker-projection-check:
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input test/contracts/managed-v2-breaker.json --check test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../chain-parser-go/test/contracts/managed-v2-breaker.json --check ../chain-parser-go/test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../chain-parser-rust/test/contracts/managed-v2-breaker.json --original-input test/contracts/managed-v2-aum.json --check ../chain-parser-rust/test/contracts/managed-v2-breaker-pools.json
	PATH="/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_projection.py --input ../dex-router/test/contracts/managed-v2-breaker.json --check ../dex-router/test/contracts/managed-v2-breaker-pools.json
managed-v2-breaker-wire-refresh:
	python3 tools/ci/managed-v2-breaker-wire-golden.py
managed-v2-breaker-wire-check:
	python3 tools/ci/managed-v2-breaker-wire-golden.py --check
managed-v2-breaker-helper-test:
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_breaker*.py'
	PATH="$(CURDIR)/.runtime/protoc-bin:/usr/local/bin:$$PATH" python3 -OO -m unittest discover -s tools/acceptance/tests -p 'test_managed_v2_breaker*.py'
	python3 -m unittest discover -s tools/ci/tests -p test_progress_document.py
	python3 -OO -m unittest discover -s tools/ci/tests -p test_progress_document.py
managed-v2-breaker-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run 'TestManagedV2(Circuit|Breaker)' -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-config --test managed_v2 -p xone-domain --test managed_v2_circuit_state --test managed_v2_circuit_wire_golden
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-config --test managed_v2 -p xone-domain --test managed_v2_circuit_state --test managed_v2_circuit_wire_golden -p xone-protocol-math --test managed_v2_breaker --test managed_v2_breaker_pool
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-routing --test managed_v2_breaker -p xone-quote-api --test managed_v2_breaker
managed-v2-breaker-smoke: build debug-build
	PATH="$(CURDIR)/.runtime/protoc-bin:$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_smoke.py

.PHONY: managed-v2-breaker-final-check managed-v2-breaker-acceptance
managed-v2-breaker-final-check:
	PATH="$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_final.py
managed-v2-breaker-acceptance:
	PATH="$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/managed_v2_breaker_acceptance.py

.PHONY: r01-r04-foundation-test r01-r04-traceability-check
r01-r04-foundation-test:
	cd ../chain-parser-go && go build -trimpath -o bin/source-evidence ./cmd/source-evidence
	cd ../chain-parser-go && go test -race ./internal/provenance ./internal/bootstrap ./internal/rpc ./internal/ingest ./internal/service ./internal/reducer ./internal/config
	cd ../chain-parser-go && go test -race ./internal/chain/evm ./internal/config -run 'TestOnlineEthereumBlock|TestEthereumBlockIntegrityConfig|TestChainReceipts|TestChainBodyCommitments|TestReceipt|TestReadComplete|TestCurrentScopeStateShares|TestHistoryFacts|TestNative|TestNormalNative|TestBor|TestCoreth|TestNitro|TestKnownEVM' -count=1
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-solana-ingest --lib --test assembly --test stream --test stream_inheritance --test stream_projection --test stream_wal --test scope_recovery --test yellowstone_transport --test async_frame_wal --test normal_yellowstone --test owner_scope --test manifest_tail --test history_queue --test history_runtime
	python3 -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog.py
	$(MAKE) r01-getter-proof-test
	python3 -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog_history.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog_history.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog_solana_rebuild.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_r01_public_catalog_solana_rebuild.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_evm_deployment_rebuild.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_evm_deployment_rebuild.py
	python3 -m unittest discover -s tools/ci/tests -p test_chain_target_config.py
	python3 -OO -m unittest discover -s tools/ci/tests -p test_chain_target_config.py
	python3 -m unittest discover -s tools/ci/tests -p test_r01_r04_traceability.py
	python3 -OO -m unittest discover -s tools/ci/tests -p test_r01_r04_traceability.py
	python3 -m unittest discover -s tools/ci/tests -p test_progress_document.py
	python3 -OO -m unittest discover -s tools/ci/tests -p test_progress_document.py
	python3 -m unittest discover -s tools/acceptance/tests -p test_r03_authority_process.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_r03_authority_process.py
r01-r04-traceability-check:
	python3 tools/ci/r01_r04_traceability.py

.PHONY: r01-r04-process-test
r01-r04-process-test: build debug-build
	PATH="$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/r01_r04_process_smoke.py

.PHONY: r01-r04-recovery-process-test
r01-r04-recovery-process-test: build
	python3 -m unittest discover -s tools/acceptance/tests -p test_recovery_process_fixture.py
	PATH="$(HOME)/.cargo/bin:/usr/local/bin:$$PATH" python3 tools/acceptance/r01_r04_recovery_process.py

.PHONY: r01-r04-rust-owner-test
r01-r04-rust-owner-test:
	PATH="$(HOME)/.cargo/bin:$$PATH" $(HOME)/.cargo/bin/cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-bank-source -p xone-solana-ingest --lib --tests

.PHONY: r03-authority-process-test
r03-authority-process-test:
	python3 tools/acceptance/r03_authority_process.py

.PHONY: r01-getter-proof-test
r01-getter-proof-test:
	node --check tools/acceptance/r01_getter_slots.cjs
	python3 -m unittest discover -s tools/acceptance/tests -p test_r01_getter_proofs.py
	python3 -OO -m unittest discover -s tools/acceptance/tests -p test_r01_getter_proofs.py

.PHONY: core-routing-test velodrome-v2-reference-refresh velodrome-v2-differential-test velodrome-v2-quote-test
core-routing-test:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-optimizer -p xone-routing
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-quote-api --test api -p xone-config --test config
velodrome-v2-reference-refresh:
	node tools/ci/velodrome-v2-golden.cjs
velodrome-v2-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run 'TestVelodrome|TestStableHashPinnedModelsAndDependencyRejections|TestStableProtocolsRequireTheirOwnStateAndParameters' -count=1
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math --test velodrome_differential -p xone-config --test velodrome -p xone-optimizer --test velodrome_shared
velodrome-v2-quote-test: build debug-build velodrome-v2-differential-test
	python3 tools/acceptance/velodrome_smoke.py

.PHONY: lfj-v21-original-reference
lfj-v21-original-reference:
	node tools/ci/lfj-v21-reference.cjs

.PHONY: lfj-v21-reference-refresh lfj-v21-differential-test lfj-v21-quote-test lfj-v21-model-projector
lfj-v21-reference-refresh: lfj-v21-original-reference
	python3 tools/ci/lfj-v21-state.py
	python3 tools/ci/lfj-v21-wire.py
lfj-v21-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run '^TestLfj' -count=1
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test lfj -p xone-config --test lfj -p xone-protocol-math --test lfj_differential -p xone-optimizer --test lfj_shared
	cd ../chain-parser-rust && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test lfj -p xone-config --test lfj
lfj-v21-model-projector:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example lfj-plan-projector
lfj-v21-quote-test: build debug-build lfj-v21-model-projector lfj-v21-differential-test
	python3 tools/acceptance/lfj_smoke.py

.PHONY: slipstream-context-reference-refresh slipstream-context-differential-test slipstream-context-model-projector slipstream-context-quote-test
slipstream-context-reference-refresh:
	node tools/ci/slipstream-extended-reference.cjs
	python3 tools/ci/slipstream-context-state.py
	python3 tools/ci/slipstream-context-wire.py
slipstream-context-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run '^TestSlipstream' -count=1
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test slipstream_context -p xone-config --test slipstream_context -p xone-protocol-math --test slipstream_context_differential -p xone-optimizer --test slipstream_context_shared
	cd ../chain-parser-rust && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test slipstream_context -p xone-config --test slipstream_context
slipstream-context-model-projector:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example slipstream-plan-projector
slipstream-context-quote-test: build debug-build slipstream-context-model-projector slipstream-context-differential-test
	python3 tools/acceptance/slipstream_context_smoke.py

.PHONY: pancake-v3-lm-reference-refresh pancake-v3-lm-draft-build pancake-v3-lm-draft-differential-test
pancake-v3-lm-reference-refresh:
	node tools/ci/pancake-v3-lm-reference.cjs
	node tools/ci/pancake-v3-lm-state.cjs
	python3 tools/ci/pancake-v3-lm-wire.py
pancake-v3-lm-draft-build:
	PATH="$(HOME)/.cargo/bin:$$PATH" CARGO_TARGET_DIR="$(CURDIR)/.runtime/pancake-lm-draft" cargo +1.90.0 build --locked --offline --manifest-path tools/reference/pancake-v3-lm/draft/Cargo.toml
pancake-v3-lm-draft-differential-test: pancake-v3-lm-draft-build
	python3 tools/ci/pancake-v3-lm-draft-check.py --binary "$(CURDIR)/.runtime/pancake-lm-draft/debug/xone-pancake-lm-draft"

.PHONY: pancake-v3-lm-differential-test pancake-v3-lm-model-projector pancake-v3-lm-quote-test
pancake-v3-lm-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run '^TestPancake' -count=1
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test pancake_lm -p xone-config --test pancake_lm -p xone-protocol-math --test pancake_lm_differential -p xone-optimizer --test pancake_lm_shared
	cd ../chain-parser-rust && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test pancake_lm -p xone-config --test pancake_lm
pancake-v3-lm-model-projector:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example pancake-lm-plan-projector
pancake-v3-lm-quote-test: build debug-build pancake-v3-lm-model-projector pancake-v3-lm-differential-test
	python3 tools/acceptance/pancake_lm_smoke.py

.PHONY: pancake-bsc4-reference-refresh pancake-bsc4-differential-test pancake-bsc4-model-projector pancake-bsc4-quote-test
pancake-bsc4-reference-refresh:
	node tools/ci/pancake-v3-bsc-artifacts.cjs
	node tools/ci/pancake-bsc-predecessor-artifacts.cjs
	node tools/ci/pancake-bsc4-reference.cjs
	node tools/ci/pancake-bsc4-state.cjs
	python3 tools/ci/pancake-bsc4-wire.py
pancake-bsc4-differential-test:
	cd ../chain-parser-go && go test ./internal/domain ./internal/config ./internal/chain/evm -run '^TestPancakeBsc4' -count=1
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test pancake_bsc_lm -p xone-config --test pancake_bsc_lm -p xone-protocol-math --test pancake_bsc_lm -p xone-optimizer --test pancake_bsc_lm_shared
	cd ../chain-parser-rust && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-domain --test pancake_bsc_lm -p xone-config --test pancake_bsc_lm
pancake-bsc4-model-projector:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example pancake-bsc4-plan-projector
pancake-bsc4-quote-test: build debug-build pancake-bsc4-model-projector pancake-bsc4-differential-test
	python3 tools/acceptance/pancake_bsc4_smoke.py

.PHONY: exact-out-reference-refresh exact-out-differential-test exact-out-model-projector exact-out-quote-test
exact-out-reference-refresh:
	node tools/ci/v2-exact-out-reference.cjs
	node tools/ci/v2-exact-out-reference.cjs --allocation-grid
	python3 tools/ci/v2-exact-out-state.py
exact-out-differential-test:
	python3 tools/acceptance/exact_out_reference.py
	python3 tools/acceptance/exact_out_reference.py --grid
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math --test exact_out -p xone-routing --test exact_out -p xone-optimizer --test exact_out -p xone-quote-api --test exact_out
exact-out-model-projector:
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example exact-out-plan-projector
exact-out-quote-test: build debug-build exact-out-model-projector exact-out-differential-test
	python3 tools/acceptance/exact_out_smoke.py

.PHONY: exact-out-current-check exact-out-evidence-check
exact-out-current-check: build debug-build
	python3 tools/acceptance/validate_exact_out.py
	python3 tools/acceptance/exact_out_acceptance.py
exact-out-evidence-check:
	python3 tools/acceptance/exact_out_acceptance.py

.PHONY: pancake-bsc-exact-out-reference-refresh pancake-bsc-exact-out-differential-test pancake-bsc-exact-out-quote-test pancake-bsc-exact-out-current-check pancake-bsc-exact-out-evidence-check
pancake-bsc-exact-out-reference-refresh:
	node tools/ci/pancake-bsc4-exact-out-reference.cjs
	node tools/ci/pancake-bsc4-exact-out-state.cjs
pancake-bsc-exact-out-differential-test:
	python3 tools/acceptance/pancake_bsc_exact_out_reference.py
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test pancake_bsc_exact_out
pancake-bsc-exact-out-quote-test: build debug-build
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example pancake-bsc-exact-out-plan-projector
	python3 tools/acceptance/pancake_bsc_exact_out_smoke.py
pancake-bsc-exact-out-current-check: build debug-build
	python3 tools/acceptance/validate_pancake_bsc_exact_out.py
	python3 tools/acceptance/pancake_bsc_exact_out_acceptance.py
pancake-bsc-exact-out-evidence-check:
	python3 tools/acceptance/pancake_bsc_exact_out_acceptance.py

.PHONY: pancake-lm-exact-out-reference-refresh pancake-lm-exact-out-differential-test pancake-lm-exact-out-quote-test pancake-lm-exact-out-current-check pancake-lm-exact-out-evidence-check
pancake-lm-exact-out-reference-refresh:
	node tools/ci/pancake-lm-exact-out-reference.cjs
	node tools/ci/pancake-lm-exact-out-state.cjs
pancake-lm-exact-out-differential-test:
	python3 tools/acceptance/pancake_lm_exact_out_reference.py
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test pancake_lm_exact_out
pancake-lm-exact-out-quote-test: build debug-build
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example pancake-lm-exact-out-plan-projector
	python3 tools/acceptance/pancake_lm_exact_out_smoke.py
pancake-lm-exact-out-current-check: build debug-build
	python3 tools/acceptance/validate_pancake_lm_exact_out.py
	python3 tools/acceptance/pancake_lm_exact_out_acceptance.py
pancake-lm-exact-out-evidence-check:
	python3 tools/acceptance/pancake_lm_exact_out_acceptance.py

.PHONY: slipstream-exact-out-reference-refresh slipstream-exact-out-differential-test slipstream-exact-out-quote-test slipstream-exact-out-current-check slipstream-exact-out-evidence-check
slipstream-exact-out-reference-refresh:
	node tools/ci/slipstream-exact-out-reference.cjs
	python3 tools/ci/slipstream-exact-out-state.py
slipstream-exact-out-differential-test:
	python3 tools/acceptance/slipstream_exact_out_reference.py
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test slipstream_exact_out
slipstream-exact-out-quote-test: build debug-build
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example slipstream-exact-out-plan-projector
	python3 tools/acceptance/slipstream_exact_out_smoke.py
slipstream-exact-out-current-check: build debug-build
	python3 tools/acceptance/validate_slipstream_exact_out.py
slipstream-exact-out-evidence-check:
	python3 tools/acceptance/slipstream_exact_out_acceptance.py

.PHONY: lfj-exact-out-reference-refresh lfj-exact-out-differential-test lfj-exact-out-quote-test lfj-exact-out-current-check lfj-exact-out-evidence-check
lfj-exact-out-reference-refresh:
	node tools/ci/lfj-v21-exact-out-reference.cjs
	python3 tools/ci/lfj-v21-exact-out-state.py
lfj-exact-out-differential-test:
	python3 tools/acceptance/lfj_exact_out_reference.py
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 test --locked -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test lfj_exact_out
lfj-exact-out-quote-test: build debug-build
	cd ../dex-router && PATH="$(HOME)/.cargo/bin:$$PATH" cargo +1.90.0 build --locked --example lfj-exact-out-plan-projector
	python3 tools/acceptance/lfj_exact_out_smoke.py
lfj-exact-out-current-check: build debug-build
	python3 tools/acceptance/validate_lfj_exact_out.py
lfj-exact-out-evidence-check:
	python3 tools/acceptance/lfj_exact_out_acceptance.py

.PHONY: meteora-mm-reference-refresh meteora-mm-differential-test meteora-mm-current-check
meteora-mm-reference-refresh:
	python3 tools/ci/meteora-reference-fetch.py --fetch
	python3 tools/reference/meteora-dlmm/harness/reference.py
	python3 tools/ci/meteora-mm-state.py
	python3 tools/ci/meteora-mm-edge-reference.py
	python3 tools/acceptance/meteora_mm_reference.py
	python3 tools/acceptance/meteora_mm_edge_reference.py
meteora-mm-differential-test:
	PATH="$(HOME)/.cargo/bin:$$PATH" CARGO_BUILD_JOBS=2 cargo +1.90.0 test --locked --manifest-path ../chain-parser-rust/Cargo.toml -p xone-solana-ingest --test meteora
	PATH="$(HOME)/.cargo/bin:$$PATH" CARGO_BUILD_JOBS=2 cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test meteora --test meteora_differential
meteora-mm-current-check:
	python3 tools/acceptance/validate_meteora_mm.py

.PHONY: meteora-exact-out-reference-refresh meteora-exact-out-differential-test meteora-exact-out-current-check
meteora-exact-out-reference-refresh:
	python3 tools/ci/meteora-reference-fetch.py --fetch
	python3 tools/ci/meteora-exact-out-reference.py
	python3 tools/acceptance/meteora_exact_out_reference.py --reexecute
meteora-exact-out-differential-test:
	PATH="$(HOME)/.cargo/bin:$$PATH" CARGO_BUILD_JOBS=2 cargo +1.90.0 test --locked --manifest-path ../dex-router/Cargo.toml -p xone-protocol-math -p xone-routing -p xone-optimizer -p xone-quote-api --test meteora_exact_out
meteora-exact-out-current-check:
	python3 tools/acceptance/validate_meteora_exact_out.py
	python3 tools/acceptance/meteora_exact_out_acceptance.py

.PHONY: family-reference-refresh family-expansion-current-check
family-reference-refresh:
	python3 tools/ci/meteora-only-y-reference.py
	python3 tools/ci/pump-reference-fetch.py
	python3 tools/ci/pump-bonding-reference.py
	python3 tools/ci/pump-bonding-decoder.py
	python3 tools/acceptance/family_reference.py --reexecute
family-expansion-current-check:
	python3 tools/acceptance/validate_family_expansion.py
	python3 tools/acceptance/meteora_only_y_smoke.py
	python3 tools/acceptance/family_expansion_acceptance.py

.PHONY: solana-expanded-reference-refresh solana-expanded-current-check
solana-expanded-reference-refresh:
	python3 tools/ci/meteora-reference-fetch.py
	python3 tools/ci/pump-reference-fetch.py
	python3 tools/ci/pump-amm-reference-fetch.py
	python3 tools/ci/meteora-bitmap-reference.py
	python3 tools/ci/meteora-rewards-reference.py
	python3 tools/ci/meteora-orders-reference.py
	python3 tools/ci/meteora-token2022-reference.py
	python3 tools/ci/pump-amm-reference.py
	python3 tools/ci/pump-exact-out-reference.py
	python3 tools/ci/pump-sell-exact-out-reference.py
	python3 tools/ci/pump-migration-reference.py
	python3 tools/ci/pump-amm-decoder.py
	python3 tools/acceptance/solana_expanded_reference.py --reexecute
solana-expanded-current-check:
	python3 tools/acceptance/validate_solana_expanded.py
	python3 tools/acceptance/solana_expanded_acceptance.py

.PHONY: meteora-orders-exact-out-reference-refresh meteora-orders-exact-out-current-check
meteora-orders-exact-out-reference-refresh:
	python3 tools/ci/meteora-reference-fetch.py
	python3 tools/ci/meteora-orders-exact-out-reference.py
	python3 tools/acceptance/meteora_orders_exact_out_reference.py --reexecute
meteora-orders-exact-out-current-check:
	python3 tools/acceptance/validate_meteora_orders_exact_out.py

.PHONY: quote-execution-reference-refresh quote-execution-current-check
quote-execution-reference-refresh:
	python3 tools/ci/pump-amm-sell-target-reference.py
	python3 tools/ci/meteora-execution-bank.py
	python3 tools/acceptance/pump_amm_sell_target_reference.py --reexecute
quote-execution-current-check:
	python3 tools/acceptance/validate_quote_execution.py

.PHONY: account-preparation-current-check
account-preparation-current-check:
	python3 tools/acceptance/validate_account_preparation.py

.PHONY: native-funding-current-check native-funding-reference-refresh
native-funding-reference-refresh:
	python3 tools/ci/meteora-wsol-bank.py
native-funding-current-check:
	python3 tools/acceptance/validate_native_funding.py

.PHONY: temporary-wsol-current-check
temporary-wsol-current-check:
	python3 tools/acceptance/validate_temporary_wsol.py

.PHONY: token2022-preparation-current-check
token2022-preparation-current-check:
	python3 tools/acceptance/validate_token2022_preparation.py

.PHONY: pipeline-foundation-current-check
pipeline-foundation-current-check:
	python3 tools/acceptance/validate_pipeline_foundation.py

.PHONY: raydium-preparation-current-check
raydium-preparation-current-check:
	python3 tools/acceptance/validate_raydium_preparation.py

.PHONY: solana-independent-current-check
solana-independent-current-check:
	python3 tools/acceptance/validate_solana_independent.py

.PHONY: wallet-lifecycle-current-check
wallet-lifecycle-current-check:
	python3 tools/acceptance/validate_wallet_lifecycle.py

.PHONY: rpc-capture-current-check
rpc-capture-current-check:
	python3 tools/acceptance/validate_rpc_capture.py

.PHONY: pump-amm-preparation-current-check
pump-amm-preparation-current-check:
	python3 tools/acceptance/validate_pump_amm_preparation.py

.PHONY: pump-curve-preparation-current-check
pump-curve-preparation-current-check:
	python3 tools/acceptance/validate_pump_curve_preparation.py

.PHONY: pump-native-ata-current-check
pump-native-ata-current-check:
	python3 tools/acceptance/validate_pump_native_ata.py

.PHONY: pump-amm-funding-current-check
pump-amm-funding-current-check:
	python3 tools/acceptance/validate_pump_amm_funding.py


.PHONY: orca-exact-out-reference orca-exact-out-current-check
orca-exact-out-reference:
	CARGO_TARGET_DIR="$(CURDIR)/tools/reference/orca-whirlpools/target" cargo +1.90.0 build --offline --locked --release --manifest-path tools/reference/solana-advanced/orca/Cargo.toml
	python3 tools/ci/solana-advanced-golden.py --family orca
	cp test/contracts/solana-advanced/orca-rewards.json ../chain-parser-rust/test/contracts/solana-advanced/orca-rewards.json
	cp test/contracts/solana-advanced/orca-rewards.json ../dex-router/test/contracts/solana-advanced/orca-rewards.json
	python3 tools/ci/orca-adaptive-service-golden.py
	python3 tools/ci/orca-adaptive-rewards-service-golden.py
	python3 tools/ci/orca-adaptive-dynamic-service-golden.py
	python3 tools/acceptance/orca_adaptive_fixture.py
	python3 tools/acceptance/orca_adaptive_rewards_fixture.py
	python3 tools/acceptance/orca_adaptive_dynamic_fixture.py
	python3 tools/acceptance/orca_exact_out_reference.py
orca-exact-out-current-check:
	python3 tools/acceptance/validate_orca_exact_out.py


.PHONY: rayclmm-exact-out-reference rayclmm-exact-out-current-check
rayclmm-exact-out-reference:
	CARGO_TARGET_DIR=/tmp/xone-solana-advanced-ray cargo +1.90.0 build --offline --locked --manifest-path tools/reference/solana-advanced/raydium-clmm/Cargo.toml --config tools/reference/solana-advanced/raydium-clmm/cargo-source.toml
	python3 tools/ci/solana-advanced-golden.py --family raydium-clmm
	cp test/contracts/solana-advanced/raydium-clmm-rewards.json ../chain-parser-rust/test/contracts/solana-advanced/raydium-clmm-rewards.json
	cp test/contracts/solana-advanced/raydium-clmm-rewards.json ../dex-router/test/contracts/solana-advanced/raydium-clmm-rewards.json
	python3 tools/acceptance/rayclmm_exact_out_reference.py
rayclmm-exact-out-current-check:
	python3 tools/acceptance/validate_rayclmm_exact_out.py
