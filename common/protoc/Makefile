CARGO := $(HOME)/.cargo/bin/cargo +1.90.0
export PATH := $(HOME)/.cargo/bin:$(PATH)
.PHONY: generate check
# protoc 3.21.12 and protoc-gen-go 1.30.0 are the checked-in generator versions.
generate:
	protoc -I proto --go_out=golang --go_opt=module=xone/common/protoc/golang proto/dex/v1/state.proto
	$(CARGO) run --locked --manifest-path generator/Cargo.toml
check:
	python3 check.py
