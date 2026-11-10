CONFIG ?= config/config-dev.yaml
.PHONY: build fmt fmt-check lint unit contract-test check check-config run

build:
	go build -trimpath -o bin/chain-parser-go ./cmd/chain-parser
fmt:
	gofmt -w cmd internal test/cmd
fmt-check:
	@test -z "$$(gofmt -l cmd internal test/cmd)"
lint:
	go vet ./...
unit:
	python3 -m unittest discover -s test/tools -p test_go_unit_shards.py
	python3 test/tools/go_unit_shards.py --all
contract-test:
	go test ./internal/domain ./internal/config
check: fmt-check lint unit contract-test build
check-config: build
	./bin/chain-parser-go --config $(CONFIG) --check-config
run: build
	./bin/chain-parser-go --config $(CONFIG)

.PHONY: debug-build
debug-build:
	go build -trimpath -o bin/fixture-parser-go ./test/cmd/fixture-parser-go
