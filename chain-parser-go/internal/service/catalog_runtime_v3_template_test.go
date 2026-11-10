package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

type v3RealPool struct {
	Address string                                                                          `json:"address"`
	Runtime string                                                                          `json:"observed_runtime"`
	SHA     string                                                                          `json:"actual_runtime_sha256"`
	Fields  struct{ Factory, Token0, Token1, Fee, TickSpacing, MaxLiquidityPerTick string } `json:"immutable_fields"`
}

func officialV3RealPools(t *testing.T) []v3RealPool {
	t.Helper()
	raw, err := os.ReadFile("../../test/fixtures/source/testdata/deployment-catalog/v3-pool-template/two-real-pools.json")
	if err != nil {
		t.Fatal(err)
	}
	var file struct {
		Pools []struct {
			Address string            `json:"address"`
			Runtime string            `json:"observed_runtime"`
			SHA     string            `json:"actual_runtime_sha256"`
			Fields  map[string]string `json:"immutable_fields"`
		}
	}
	if err = json.Unmarshal(raw, &file); err != nil {
		t.Fatal(err)
	}
	var pools []v3RealPool
	for _, p := range file.Pools {
		v := v3RealPool{Address: p.Address, Runtime: p.Runtime, SHA: p.SHA}
		v.Fields.Factory = p.Fields["factory"]
		v.Fields.Token0 = p.Fields["token0"]
		v.Fields.Token1 = p.Fields["token1"]
		v.Fields.Fee = p.Fields["fee"]
		v.Fields.TickSpacing = p.Fields["tickSpacing"]
		v.Fields.MaxLiquidityPerTick = p.Fields["maxLiquidityPerTick"]
		pools = append(pools, v)
	}
	if len(pools) != 2 {
		t.Fatal("official two-pool fixture missing")
	}
	return pools
}
func TestOfficialV3TemplateAcceptsTwoObservedImmutableInstances(t *testing.T) {
	pools := officialV3RealPools(t)
	if pools[0].SHA == pools[1].SHA {
		t.Fatal("counterexample did not contain different runtime identities")
	}
	for _, pool := range pools {
		bindings, err := v3BindingsFromStrings(pool.Address, pool.Fields.Factory, pool.Fields.Token0, pool.Fields.Token1, pool.Fields.Fee, pool.Fields.TickSpacing, pool.Fields.MaxLiquidityPerTick)
		if err != nil {
			t.Fatal(err)
		}
		actual, err := verifyOfficialV3Runtime(config.V3PoolTemplateConfig{Profile: config.OfficialUniswapV3Pool076}, pool.Runtime, bindings)
		if err != nil || actual != pool.SHA {
			t.Fatal(pool.Address, actual, err)
		}
		if catalogCheckCode(pool.Runtime, pools[1].SHA) == nil && pool.Address == pools[0].Address {
			t.Fatal("old strict hash check was weakened")
		}
	}
}
func TestOfficialV3TemplateRejectsRuntimeAndLayoutTampering(t *testing.T) {
	p := officialV3RealPools(t)[0]
	bindings, err := v3BindingsFromStrings(p.Address, p.Fields.Factory, p.Fields.Token0, p.Fields.Token1, p.Fields.Fee, p.Fields.TickSpacing, p.Fields.MaxLiquidityPerTick)
	if err != nil {
		t.Fatal(err)
	}
	template, layout, err := officialV3Template()
	if err != nil {
		t.Fatal(err)
	}
	for _, mode := range []string{"opcode", "metadata", "length", "self", "repeated-token", "fee", "max-liquidity"} {
		t.Run(mode, func(t *testing.T) {
			code, _ := hex.DecodeString(strings.TrimPrefix(p.Runtime, "0x"))
			switch mode {
			case "opcode":
				code[0] ^= 1
			case "metadata":
				code[len(code)-4] ^= 1
			case "length":
				code = append(code, 0)
			default:
				field := map[string]string{"self": "original", "repeated-token": "token0", "fee": "fee", "max-liquidity": "maxLiquidityPerTick"}[mode]
				for _, d := range layout {
					if d.Name == field {
						code[d.References[0].Start+31] ^= 1
						break
					}
				}
			}
			if _, err := verifyOfficialV3Runtime(config.V3PoolTemplateConfig{Profile: config.OfficialUniswapV3Pool076}, "0x"+hex.EncodeToString(code), bindings); err == nil {
				t.Fatal("mutated runtime accepted")
			}
		})
	}
	var bad []v3ImmutableDeclaration
	if err = json.Unmarshal([]byte(officialV3ImmutableLayout), &bad); err != nil {
		t.Fatal(err)
	}
	bad[0].References[0].Start++
	raw, _ := json.Marshal(bad)
	if _, err := validateOfficialV3Layout(template, string(raw)); err == nil {
		t.Fatal("arbitrary immutable offset accepted")
	}
	wrong := bindings
	wrong.Token0 = p.Fields.Token1
	if _, err := verifyOfficialV3Runtime(config.V3PoolTemplateConfig{Profile: config.OfficialUniswapV3Pool076}, p.Runtime, wrong); err == nil {
		t.Fatal("getter identity inconsistent with constructor accepted")
	}
	if _, err := verifyOfficialV3Runtime(config.V3PoolTemplateConfig{Profile: "user-template"}, p.Runtime, bindings); err == nil {
		t.Fatal("unreviewed property/profile accepted")
	}
	digest := sha256.Sum256(template)
	if hex.EncodeToString(digest[:]) != config.OfficialV3RuntimeTemplateSHA256 {
		t.Fatal("builtin profile drift")
	}
}

func officialV3ReadFixture(t *testing.T, mode string) (*CatalogRuntime, *rpc.Client, []ingest.FactoryCandidate, ingest.CatalogAnchor) {
	t.Helper()
	pools := officialV3RealPools(t)
	raw, err := os.ReadFile("../../test/fixtures/source/testdata/deployment-catalog/verified-source-inputs/ethereum-uniswap-v3.json")
	if err != nil {
		t.Fatal(err)
	}
	var input struct {
		Runtime struct {
			Code string `json:"onchainBytecode"`
		} `json:"runtimeBytecode"`
	}
	if err = json.Unmarshal(raw, &input); err != nil {
		t.Fatal(err)
	}
	factory := pools[0].Fields.Factory
	factorySHA := sha256.Sum256(mustV3Hex(t, input.Runtime.Code))
	genesis := "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
	source := config.FactoryDeployment{Address: factory, Version: ingest.UniswapV3Factory, ProvenanceID: "human-label-is-not-approval", FactoryCodeSHA256: hex.EncodeToString(factorySHA[:]), PoolTemplate: &config.V3PoolTemplateConfig{Profile: config.OfficialUniswapV3Pool076}, MaxInitializedTicks: 16, MaxSteps: 64}
	if mode == "strict" {
		source.PoolTemplate = nil
		source.PoolCodeSHA256 = pools[0].SHA
	}
	if mode == "profile" {
		source.PoolTemplate.Profile = "attacker"
	}
	if mode == "wrong-genesis" {
		genesis = runtimeHash("a")
	}
	anchor := ingest.CatalogAnchor{Chain: "evm:1", Height: 18000000, Hash: runtimeHash("c"), Generation: 1}
	word := func(n string) string {
		x, ok := new(big.Int).SetString(n, 10)
		if !ok {
			panic(n)
		}
		return "0x" + hex.EncodeToString(x.FillBytes(make([]byte, 32)))
	}
	addr := func(address string) string { return "0x" + strings.Repeat("0", 24) + address[2:] }
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     any               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		_ = json.NewDecoder(r.Body).Decode(&q)
		if q.Method == "eth_call" || q.Method == "eth_getCode" || q.Method == "eth_getBalance" || q.Method == "eth_getStorageAt" {
			index := 1
			if q.Method == "eth_getStorageAt" {
				index = 2
			}
			var selector map[string]any
			if len(q.Params) > index && json.Unmarshal(q.Params[index], &selector) == nil && selector["blockHash"] != anchor.Hash && selector["blockHash"] != runtimeHash("b") {
				_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "error": map[string]any{"code": -32001, "message": "unknown block"}})
				return
			}
		}
		var result any
		switch q.Method {
		case "eth_chainId":
			result = "0x1"
		case "eth_blockNumber":
			result = fmt.Sprintf("0x%x", anchor.Height)
		case "eth_getBalance":
			result = "0x1"
		case "eth_getStorageAt":
			result = word("0")
		case "eth_getBlockByNumber":
			var tag string
			_ = json.Unmarshal(q.Params[0], &tag)
			number, hash := anchor.Height, anchor.Hash
			if tag == "0x0" {
				number, hash = 0, genesis
			}
			if tag == "0xc65d40" {
				number, hash = 13000000, runtimeHash("b")
			}
			result = evm.RawBlock{Number: fmt.Sprintf("0x%x", number), Hash: hash, ParentHash: runtimeHash("d"), Timestamp: "0x1", Transactions: []string{}}
		case "eth_getCode":
			var address string
			_ = json.Unmarshal(q.Params[0], &address)
			if address == factory {
				result = input.Runtime.Code
			} else {
				for _, p := range pools {
					if p.Address == address {
						result = p.Runtime
						if mode == "opcode" {
							b := mustV3Hex(t, p.Runtime)
							b[0] ^= 1
							result = "0x" + hex.EncodeToString(b)
						}
					}
				}
			}
		case "eth_call":
			var call map[string]string
			_ = json.Unmarshal(q.Params[0], &call)
			to, data := call["to"], call["data"]
			if data == "0x" {
				result = "0x"
				break
			}
			if to == factory && strings.HasPrefix(data, "0x22afcccb") {
				result = word("60")
				break
			}
			if to == factory && strings.HasPrefix(data, "0x1698ee82") {
				result = addr("0x0000000000000000000000000000000000000000")
				for _, p := range pools {
					if len(data) == 202 && data[34:74] == p.Fields.Token0[2:] && data[98:138] == p.Fields.Token1[2:] {
						fee, _ := new(big.Int).SetString(data[138:], 16)
						if fee.String() == p.Fields.Fee {
							result = addr(p.Address)
						}
					}
				}
				break
			}
			for _, p := range pools {
				if p.Address != to {
					continue
				}
				switch data[:10] {
				case "0xc45a0155":
					result = addr(factory)
					if mode == "getter-factory" {
						result = addr(catalogAddr(99))
					}
				case "0x0dfe1681":
					result = addr(p.Fields.Token0)
					if mode == "getter-token" {
						result = addr(catalogAddr(99))
					}
				case "0xd21220a7":
					result = addr(p.Fields.Token1)
				case "0xddca3f43":
					result = word(p.Fields.Fee)
					if mode == "getter-fee" {
						result = word("100")
					}
				case "0xd0c93a7c":
					result = word(p.Fields.TickSpacing)
					if mode == "getter-spacing" {
						result = word("99")
					}
				case "0x70cf754a":
					result = word(p.Fields.MaxLiquidityPerTick)
					if mode == "getter-maximum" {
						result = word("1")
					}
				case "0x3850c7bd":
					result = word("79228162514264337593543950336") + strings.TrimPrefix(word("0"), "0x") + strings.TrimPrefix(word("0"), "0x") + strings.TrimPrefix(word("1"), "0x") + strings.TrimPrefix(word("1"), "0x") + strings.TrimPrefix(word("0"), "0x") + strings.TrimPrefix(word("1"), "0x")
				case "0x1a686502":
					result = word("1000000")
				case "0x5339c296":
					result = word("0")
				default:
					result = "0x"
				}
			}
		}
		_ = json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": result})
	}))
	t.Cleanup(server.Close)
	client := rpc.New([]string{server.URL}, time.Second, 1<<20)
	runtime := &CatalogRuntime{options: CatalogRuntimeOptions{Chain: config.Chain{Key: "evm:1", Genesis: genesis}, Config: config.DynamicCatalogConfig{Sources: []config.FactoryDeployment{source}}}}
	var candidates []ingest.FactoryCandidate
	for _, p := range pools {
		fee, _ := strconv.ParseUint(p.Fields.Fee, 10, 32)
		space, _ := strconv.ParseInt(p.Fields.TickSpacing, 10, 32)
		candidates = append(candidates, ingest.FactoryCandidate{Chain: "evm:1", Factory: factory, Version: ingest.UniswapV3Factory, ProvenanceID: source.ProvenanceID, Height: 13000000, BlockHash: runtimeHash("b"), Pool: p.Address, Token0: p.Fields.Token0, Token1: p.Fields.Token1, Fee: uint32(fee), TickSpacing: int32(space)})
	}
	return runtime, client, candidates, anchor
}
func mustV3Hex(t *testing.T, value string) []byte {
	t.Helper()
	raw, err := hex.DecodeString(strings.TrimPrefix(value, "0x"))
	if err != nil {
		t.Fatal(err)
	}
	return raw
}
func TestOfficialV3CurrentAdmissionProducesPerPoolActualCodePins(t *testing.T) {
	r, client, candidates, anchor := officialV3ReadFixture(t, "")
	expected := officialV3RealPools(t)
	for i, c := range candidates {
		entry, err := r.readCurrent(context.Background(), client, c, anchor)
		if err != nil {
			t.Fatal(c.Pool, err)
		}
		if entry.TemplateBinding == nil || entry.TemplateBinding.ActualRuntimeSHA256 != expected[i].SHA || entry.TemplateBinding.FactorySourceSHA256 == "" || entry.TemplateBinding.Profile != config.OfficialUniswapV3Pool076 {
			t.Fatal("official source binding was not persisted", entry.TemplateBinding)
		}
		if entry.Config.V3.ExpectedCodeSHA256 != expected[i].SHA || entry.Pool.V3.CodeSHA256 != expected[i].SHA {
			t.Fatal("actual code identity was not stored", entry.Config.V3)
		}
	}
}
func TestOfficialV3CurrentAdmissionRejectsGetterAndPropertyTampering(t *testing.T) {
	for _, mode := range []string{"opcode", "getter-factory", "getter-token", "getter-fee", "getter-spacing", "getter-maximum", "wrong-genesis", "profile"} {
		t.Run(mode, func(t *testing.T) {
			r, client, candidates, anchor := officialV3ReadFixture(t, mode)
			if _, err := r.readCurrent(context.Background(), client, candidates[0], anchor); err == nil {
				t.Fatal("tampered official deployment admitted")
			}
		})
	}
}
func TestOfficialV3LegacySingleHashRemainsStrict(t *testing.T) {
	r, client, candidates, anchor := officialV3ReadFixture(t, "strict")
	if _, err := r.readCurrent(context.Background(), client, candidates[0], anchor); err != nil {
		t.Fatal(err)
	}
	if _, err := r.readCurrent(context.Background(), client, candidates[1], anchor); err == nil {
		t.Fatal("legacy hash silently expanded to another runtime")
	}
}

func TestOfficialV3TemplateEvidencePersistsButRestartRequiresCurrentAdmission(t *testing.T) {
	observed, client, candidates, anchor := officialV3ReadFixture(t, "")
	f := catalogFixture(t)
	options := f.options
	options.Chain = observed.options.Chain
	options.Client = client
	options.Config.Sources = observed.options.Config.Sources
	options.Path = filepath.Join(t.TempDir(), "official-catalog")
	cfg := probeConfig()
	cfg.HistoricalHeight = 13000000
	cfg.HistoricalHash = runtimeHash("b")
	cfg.Account = candidates[0].Factory
	cfg.TimeoutMS, cfg.MaxAgeMS = 3000, 60000
	provider, err := NewProviderRuntime(client, options.Chain.Key, options.Chain.Genesis, cfg)
	if err != nil {
		t.Fatal(err)
	}
	if err = provider.Refresh(context.Background()); err != nil {
		t.Fatal(err)
	}
	options.Providers = provider
	runtime, err := OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = runtime.Close() })
	fence := CatalogFence{1, 1, 1, 1}
	if err = runtime.Begin(context.Background(), anchor, fence); err != nil {
		t.Fatal(err)
	}
	block := ingest.FactoryBlock{Chain: options.Chain.Key, Anchor: ingest.Cursor{Height: 13000000, Hash: runtimeHash("b")}}
	for i, candidate := range candidates {
		log := ingest.FactoryLog{Address: candidate.Factory, BlockNumber: block.Anchor.Height, BlockHash: block.Anchor.Hash, TransactionHash: runtimeHash(fmt.Sprint(i + 1)), TransactionIndex: uint64(i), LogIndex: uint64(i), Topics: []string{ingest.V3PoolCreatedTopic, "0x" + strings.Repeat("0", 24) + candidate.Token0[2:], "0x" + strings.Repeat("0", 24) + candidate.Token1[2:], fmt.Sprintf("0x%064x", candidate.Fee)}, Data: fmt.Sprintf("0x%064x", candidate.TickSpacing) + strings.Repeat("0", 24) + candidate.Pool[2:]}
		block.Receipts = append(block.Receipts, ingest.FactoryReceipt{BlockHash: log.BlockHash, BlockNumber: log.BlockNumber, TransactionHash: log.TransactionHash, TransactionIndex: log.TransactionIndex, Logs: []ingest.FactoryLog{log}})
	}
	raw, _ := json.Marshal(block)
	block.Raw, err = options.Archive.Put(ingest.Owner{Chain: block.Chain, Height: block.Anchor.Height, Hash: block.Anchor.Hash, ProviderID: "test-public-proof"}, raw)
	if err != nil {
		t.Fatal(err)
	}
	discovered, err := runtime.Discover(context.Background(), ingest.CatalogOnline, block)
	if err != nil || len(discovered) != 2 {
		t.Fatal(discovered, err)
	}
	if err = runtime.Admit(context.Background(), ingest.CatalogOnline, discovered); err != nil {
		t.Fatal(err)
	}
	snapshot := runtime.CatalogSnapshot(ingest.CatalogOnline)
	if !snapshot.Ready || len(snapshot.Entries) != 2 {
		t.Fatal(snapshot)
	}
	if verified, total := runtime.officialTemplatePoolCounts(); verified != 2 || total != 2 {
		t.Fatal(verified, total)
	}
	snapshot.Entries[0].TemplateBinding.ActualRuntimeSHA256 = "tampered"
	if runtime.CatalogSnapshot(ingest.CatalogOnline).Entries[0].TemplateBinding.ActualRuntimeSHA256 == "tampered" {
		t.Fatal("mutable template binding escaped snapshot")
	}
	ids := []string{candidates[0].Pool, candidates[1].Pool}
	frozen, err := runtime.Freeze(context.Background(), fence, ids)
	if err != nil {
		t.Fatal(err)
	}
	frozen.Entries[0].TemplateBinding.CompilerSHA256 = "tampered"
	if runtime.ValidateFrozen(context.Background(), frozen) == nil {
		t.Fatal("tampered template proof authorized final scope")
	}
	if err = runtime.Close(); err != nil {
		t.Fatal(err)
	}
	runtime, err = OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	if len(runtime.disk.Online) != 2 || runtime.disk.Online[0].TemplateBinding == nil || runtime.disk.Online[0].TemplateBinding.CompilerSHA256 != config.OfficialV3CompilerSHA256 {
		t.Fatal("template evidence not durable")
	}
	if runtime.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("saved numeric state trusted on restart")
	}
	anchor.Generation, fence.WriterEpoch, fence.Generation = 2, 2, 2
	if err = runtime.Revalidate(context.Background(), anchor, fence); err != nil {
		t.Fatal(err)
	}
	if verified, total := runtime.officialTemplatePoolCounts(); verified != 2 || total != 2 {
		t.Fatal("restart did not requalify official entries", verified, total)
	}
	options.CheckFence = func(context.Context, CatalogFence) error { return errors.New("owner fenced") }
	if err = runtime.Close(); err != nil {
		t.Fatal(err)
	}
	runtime, err = OpenCatalogRuntime(options)
	if err != nil {
		t.Fatal(err)
	}
	if runtime.Revalidate(context.Background(), anchor, fence) == nil || runtime.CatalogSnapshot(ingest.CatalogOnline).Ready {
		t.Fatal("restart bypassed current owner fence")
	}
}
