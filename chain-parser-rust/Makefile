CONFIG ?= config/config-dev.yaml
.PHONY: build fmt fmt-check lint unit contract-test check check-config run
CARGO := $(HOME)/.cargo/bin/cargo +1.90.0
export PATH := $(HOME)/.cargo/bin:$(PATH)

build:
	$(CARGO) build --locked --workspace
fmt:
	$(CARGO) fmt --all
fmt-check:
	$(CARGO) fmt --all -- --check
lint:
	$(CARGO) clippy --locked --workspace --all-targets -- -D warnings
unit:
	$(CARGO) test --locked --workspace
contract-test:
	$(CARGO) test --locked -p xone-domain -p xone-config
check: fmt-check lint unit contract-test build
check-config: build
	./target/debug/chain-parser-rust --config $(CONFIG) --check-config
run: build
	./target/debug/chain-parser-rust --config $(CONFIG)

.PHONY: debug-build
debug-build:
	$(CARGO) build --locked --features debug-fixtures --bin fixture-parser-rust
