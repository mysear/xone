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

full-local-acceptance: check fixture-test durable-journal-test reorg-journal-test v3-quote-test multidex-quote-test stable-quote-test singleton-quote-test raydium-quote-test orca-quote-test rayclmm-quote-test rayclmm-token-fee-quote-test rayclmm-bitmap-quote-test rayclmm-seed-quote-test rayclmm-dynamic-quote-test receipt-witness-quote-test source-wal-quote-test receipt-root-quote-test
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
