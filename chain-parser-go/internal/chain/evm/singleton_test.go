package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"reflect"
	"strings"
	"sync"
	"testing"
	"time"

	"golang.org/x/crypto/sha3"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

// These fixtures model the pinned official layouts directly, without calling
// parser helpers: v4 StateLibrary at bea7f6d and Infinity CLPoolManager at 3c5ef1b.
func singletonTestHash(data []byte) string {
	h := sha3.NewLegacyKeccak256()
	_, _ = h.Write(data)
	return hex.EncodeToString(h.Sum(nil))
}
func singletonTestSelector(signature string) string {
	return "0x" + singletonTestHash([]byte(signature))[:8]
}
func singletonTestWord(n *big.Int) string {
	v := new(big.Int).Set(n)
	if v.Sign() < 0 {
		v.Add(v, new(big.Int).Lsh(big.NewInt(1), 256))
	}
	return fmt.Sprintf("%064x", v)
}
func singletonTestInt(n int64) string { return singletonTestWord(big.NewInt(n)) }
func singletonTestMapping(key, slot string) string {
	raw, err := hex.DecodeString(key + slot)
	if err != nil {
		panic(err)
	}
	return singletonTestHash(raw)
}
func singletonTestOffset(slot string, offset int64) string {
	n, ok := new(big.Int).SetString(slot, 16)
	if !ok {
		panic(slot)
	}
	n.Add(n, big.NewInt(offset))
	n.Mod(n, new(big.Int).Lsh(big.NewInt(1), 256))
	return singletonTestWord(n)
}
func singletonTestConfig(protocol string) config.PoolConfig {
	code := []byte{0x60, 0x01, 0x55}
	digest := sha256.Sum256(code)
	raw := "0xb5ea8ddef4e3db41e212cb7f2b274c27071725aa3d3e6a2b48f02503bca5c023"
	parameters := ""
	if protocol == "pancake-infinity-cl" {
		raw = "0x8b825ed0a737d114ab647f4510299eaa7f3c680bfedbfb5d00a05094f5a9aa0e"
		parameters = "0x" + singletonTestInt(60<<16)
	}
	manager := "0x" + strings.Repeat("44", 20)
	return config.PoolConfig{ID: manager + "/" + raw, Protocol: protocol, Token0: "0x" + strings.Repeat("01", 20), Token1: "0x" + strings.Repeat("02", 20), FeeNumerator: 3000, FeeDenominator: 1000000,
		SingletonCL: &config.SingletonCLPoolConfig{Manager: manager, ExpectedManagerCodeSHA256: hex.EncodeToString(digest[:]), PoolID: raw, LPFee: 3000, TickSpacing: 60, Parameters: parameters, TickWordRadius: 1, MaxInitializedTicks: 8, MaxSteps: 512}}
}

type singletonRPCFixture struct {
	config   config.PoolConfig
	anchor   map[string]any
	code     string
	results  map[string]string
	rpcError string
	mu       sync.Mutex
	calls    []string
}

func singletonFixture(protocol string) *singletonRPCFixture {
	p := singletonTestConfig(protocol)
	f := &singletonRPCFixture{config: p, anchor: map[string]any{"blockHash": "0x" + strings.Repeat("ab", 32), "requireCanonical": true}, code: "0x600155", results: map[string]string{}}
	v := p.SingletonCL
	price := new(big.Int).Lsh(big.NewInt(1), 96)
	protocolFee := int64(100 | 500<<12)
	if protocol == "uniswap-v4" {
		base := singletonTestMapping(v.PoolID[2:], singletonTestInt(6))
		packed := new(big.Int).Set(price)
		packed.Or(packed, new(big.Int).Lsh(big.NewInt(0xffffff), 160)) // tick -1
		packed.Or(packed, new(big.Int).Lsh(big.NewInt(protocolFee), 184))
		packed.Or(packed, new(big.Int).Lsh(big.NewInt(3000), 208))
		f.results[singletonTestSelector("extsload(bytes32)")+base] = "0x" + singletonTestWord(packed)
		f.results[singletonTestSelector("extsload(bytes32)")+singletonTestOffset(base, 3)] = "0x" + singletonTestInt(1000)
		for _, pos := range []int64{-2, -1, 0} {
			bitmap := new(big.Int)
			if pos == -1 {
				bitmap.SetBit(bitmap, 255, 1)
			}
			if pos == 0 {
				bitmap.SetBit(bitmap, 1, 1)
			}
			slot := singletonTestMapping(singletonTestInt(pos), singletonTestOffset(base, 5))
			f.results[singletonTestSelector("extsload(bytes32)")+slot] = "0x" + singletonTestWord(bitmap)
		}
		for _, tick := range []int64{-60, 60} {
			net := int64(1000)
			if tick > 0 {
				net = -1000
			}
			upper := big.NewInt(net)
			if net < 0 {
				upper.Add(upper, new(big.Int).Lsh(big.NewInt(1), 128))
			}
			packed := new(big.Int).Or(big.NewInt(1000), new(big.Int).Lsh(upper, 128))
			slot := singletonTestMapping(singletonTestInt(tick), singletonTestOffset(base, 4))
			f.results[singletonTestSelector("extsload(bytes32)")+slot] = "0x" + singletonTestWord(packed)
		}
	} else {
		raw := v.PoolID[2:]
		f.results[singletonTestSelector("paused()")] = "0x" + singletonTestInt(0)
		f.results[singletonTestSelector("poolIdToPoolKey(bytes32)")+raw] = "0x" + strings.Repeat("0", 24) + p.Token0[2:] + strings.Repeat("0", 24) + p.Token1[2:] + singletonTestInt(0) + strings.Repeat("0", 24) + v.Manager[2:] + singletonTestInt(3000) + v.Parameters[2:]
		f.results[singletonTestSelector("getSlot0(bytes32)")+raw] = "0x" + singletonTestWord(price) + singletonTestInt(-1) + singletonTestInt(protocolFee) + singletonTestInt(3000)
		f.results[singletonTestSelector("getLiquidity(bytes32)")+raw] = "0x" + singletonTestInt(1000)
		for _, pos := range []int64{-2, -1, 0} {
			bitmap := new(big.Int)
			if pos == -1 {
				bitmap.SetBit(bitmap, 255, 1)
			}
			if pos == 0 {
				bitmap.SetBit(bitmap, 1, 1)
			}
			f.results[singletonTestSelector("getPoolBitmapInfo(bytes32,int16)")+raw+singletonTestInt(pos)] = "0x" + singletonTestWord(bitmap)
		}
		for _, tick := range []int64{-60, 60} {
			net := int64(1000)
			if tick > 0 {
				net = -1000
			}
			f.results[singletonTestSelector("getPoolTickInfo(bytes32,int24)")+raw+singletonTestInt(tick)] = "0x" + singletonTestInt(1000) + singletonTestInt(net) + singletonTestInt(0) + singletonTestInt(0)
		}
	}
	return f
}
func (f *singletonRPCFixture) slotKey() string {
	if f.config.Protocol == "uniswap-v4" {
		return singletonTestSelector("extsload(bytes32)") + singletonTestMapping(f.config.SingletonCL.PoolID[2:], singletonTestInt(6))
	}
	return singletonTestSelector("getSlot0(bytes32)") + f.config.SingletonCL.PoolID[2:]
}
func (f *singletonRPCFixture) liquidityKey() string {
	if f.config.Protocol == "uniswap-v4" {
		base := singletonTestMapping(f.config.SingletonCL.PoolID[2:], singletonTestInt(6))
		return singletonTestSelector("extsload(bytes32)") + singletonTestOffset(base, 3)
	}
	return singletonTestSelector("getLiquidity(bytes32)") + f.config.SingletonCL.PoolID[2:]
}
func (f *singletonRPCFixture) tickKey(tick int64) string {
	if f.config.Protocol == "uniswap-v4" {
		base := singletonTestMapping(f.config.SingletonCL.PoolID[2:], singletonTestInt(6))
		return singletonTestSelector("extsload(bytes32)") + singletonTestMapping(singletonTestInt(tick), singletonTestOffset(base, 4))
	}
	return singletonTestSelector("getPoolTickInfo(bytes32,int24)") + f.config.SingletonCL.PoolID[2:] + singletonTestInt(tick)
}
func (f *singletonRPCFixture) bitmapKey(pos int64) string {
	if f.config.Protocol == "uniswap-v4" {
		base := singletonTestMapping(f.config.SingletonCL.PoolID[2:], singletonTestInt(6))
		return singletonTestSelector("extsload(bytes32)") + singletonTestMapping(singletonTestInt(pos), singletonTestOffset(base, 5))
	}
	return singletonTestSelector("getPoolBitmapInfo(bytes32,int16)") + f.config.SingletonCL.PoolID[2:] + singletonTestInt(pos)
}
func (f *singletonRPCFixture) setSlotField(index int, value *big.Int) {
	key := f.slotKey()
	raw := f.results[key][2:]
	if f.config.Protocol == "uniswap-v4" {
		shifts := []uint{0, 160, 184, 208}
		widths := []uint{160, 24, 24, 24}
		n, _ := new(big.Int).SetString(raw, 16)
		mask := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), widths[index]), big.NewInt(1))
		mask.Lsh(mask, shifts[index])
		n.AndNot(n, mask)
		val := new(big.Int).Set(value)
		if val.Sign() < 0 {
			val.Add(val, new(big.Int).Lsh(big.NewInt(1), widths[index]))
		}
		n.Or(n, new(big.Int).Lsh(val, shifts[index]))
		f.results[key] = "0x" + singletonTestWord(n)
	} else {
		f.results[key] = "0x" + raw[:index*64] + singletonTestWord(value) + raw[(index+1)*64:]
	}
}
func (f *singletonRPCFixture) adapter(t *testing.T) *Adapter {
	t.Helper()
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		f.mu.Lock()
		defer f.mu.Unlock()
		var q struct {
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
			t.Error(err)
			return
		}
		if len(q.Params) != 2 {
			t.Errorf("unexpected RPC params %s", r.URL)
			return
		}
		var anchor map[string]any
		_ = json.Unmarshal(q.Params[1], &anchor)
		if !reflect.DeepEqual(anchor, f.anchor) {
			t.Errorf("anchor changed: got %v want %v", anchor, f.anchor)
		}
		key := q.Method
		out := f.code
		if q.Method == "eth_getCode" {
			var to string
			_ = json.Unmarshal(q.Params[0], &to)
			if to != f.config.SingletonCL.Manager {
				t.Errorf("wrong manager %s", to)
			}
		} else if q.Method == "eth_call" {
			var call map[string]string
			_ = json.Unmarshal(q.Params[0], &call)
			if call["to"] != f.config.SingletonCL.Manager {
				t.Errorf("call sent to %s", call["to"])
			}
			key = call["data"]
			var ok bool
			out, ok = f.results[key]
			if !ok {
				t.Errorf("unexpected singleton call %s", key)
				out = "0x"
			}
		} else {
			t.Errorf("unexpected RPC method %s", q.Method)
		}
		f.calls = append(f.calls, key)
		response := map[string]any{"jsonrpc": "2.0", "id": 1, "result": out}
		if key == f.rpcError {
			delete(response, "result")
			response["error"] = map[string]any{"code": -32000}
		}
		_ = json.NewEncoder(w).Encode(response)
	}))
	t.Cleanup(server.Close)
	return &Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337"}}
}
func TestSingletonCLPinnedOfficialReads(t *testing.T) {
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		t.Run(protocol, func(t *testing.T) {
			f := singletonFixture(protocol)
			a := f.adapter(t)
			got, err := a.readSingletonCL(context.Background(), f.config, f.anchor)
			if err != nil {
				t.Fatal(err)
			}
			s := got.SingletonCL
			if s == nil {
				t.Fatal("missing typed singleton state")
			}
			if got.PoolID != f.config.ID || got.Protocol != protocol || got.Token0 != f.config.Token0 || got.Token1 != f.config.Token1 || got.ChainKey != "evm:31337" || got.Reserve0 != "0" || got.Reserve1 != "0" || got.FeeNumerator != 3000 || got.FeeDenominator != 1000000 || got.TokenBehavior != "standard" {
				t.Fatalf("wrong pool: %+v", got)
			}
			wantAdapter := "uniswap-v4/core-1"
			if protocol == "pancake-infinity-cl" {
				wantAdapter = "pancake-infinity-cl/cl-1"
			}
			if got.AdapterVersion != wantAdapter || s.Manager != f.config.SingletonCL.Manager || s.ManagerCodeSHA256 != f.config.SingletonCL.ExpectedManagerCodeSHA256 || s.PoolID != f.config.SingletonCL.PoolID || s.Parameters != f.config.SingletonCL.Parameters || s.Hooks != "0x"+strings.Repeat("0", 40) || s.LPFee != 3000 || s.ProtocolFee0 != 100 || s.ProtocolFee1 != 500 {
				t.Fatalf("wrong identity/fees: %+v", s)
			}
			if s.Core.Tick != -1 || s.Core.TickSpacing != 60 || s.Core.SqrtPriceX96 != "79228162514264337593543950336" || s.Core.Liquidity != "1000" || s.Core.MaxSteps != 512 {
				t.Fatalf("wrong core: %+v", s.Core)
			}
			words := s.Core.TickWords
			if len(words) != 3 || words[0].WordPos != -2 || words[0].Bitmap != "0" || words[1].WordPos != -1 || words[2].WordPos != 0 || words[2].Bitmap != "2" {
				t.Fatalf("wrong bitmap window: %+v", words)
			}
			ticks := s.Core.Ticks
			if len(ticks) != 2 || ticks[0].Tick != -60 || ticks[0].LiquidityGross != "1000" || ticks[0].LiquidityNet != "1000" || ticks[1].Tick != 60 || ticks[1].LiquidityNet != "-1000" {
				t.Fatalf("wrong ticks: %+v", ticks)
			}
			wantReads := 8
			if protocol == "pancake-infinity-cl" {
				wantReads = 10
			}
			f.mu.Lock()
			defer f.mu.Unlock()
			if len(f.calls) != wantReads {
				t.Fatalf("reads %d want %d: %v", len(f.calls), wantReads, f.calls)
			}
		})
	}
}
func TestSingletonCLRejectsMalformedOrUnsafeRPCState(t *testing.T) {
	common := []string{"code-hash", "code-empty", "code-hex", "code-prefix", "clone", "slot-short", "slot-long", "slot-hex", "slot-prefix", "price-zero", "price-tick", "tick-range", "fee-mismatch", "fee-dynamic", "protocol-fee0", "protocol-fee1", "liquidity-width", "missing-tick", "net-exceeds-gross", "gross-overflow", "tick-short", "bitmap-short", "bitmap-range", "tick-budget", "rpc-code", "rpc-slot", "rpc-tick"}
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		modes := append([]string{}, common...)
		if protocol == "uniswap-v4" {
			modes = append(modes, "slot-reserved")
		} else {
			modes = append(modes, "price-width", "tick-width", "protocol-width", "lp-width", "tick-gross-width", "tick-net-width", "paused", "paused-width", "key-token0", "key-token1", "key-hooks", "key-manager", "key-fee", "key-parameters", "key-address-width", "key-fee-width", "key-short")
		}
		for _, mode := range modes {
			t.Run(protocol+"/"+mode, func(t *testing.T) {
				f := singletonFixture(protocol)
				slot := f.slotKey()
				tick := f.tickKey(-60)
				switch mode {
				case "code-hash":
					f.code = "0x600255"
				case "code-empty":
					f.code = "0x"
				case "code-hex":
					f.code = "0xzz"
				case "code-prefix":
					f.code = "600155"
				case "clone":
					f.code = "0x363d3d373d3d3d363d73" + strings.Repeat("55", 20) + "5af43d82803e903d91602b57fd5bf3"
					b, _ := hex.DecodeString(f.code[2:])
					h := sha256.Sum256(b)
					f.config.SingletonCL.ExpectedManagerCodeSHA256 = hex.EncodeToString(h[:])
				case "slot-short":
					f.results[slot] = f.results[slot][:len(f.results[slot])-2]
				case "slot-long":
					f.results[slot] += singletonTestInt(0)
				case "slot-hex":
					f.results[slot] = "0x" + strings.Repeat("zz", len(f.results[slot])/2-1)
				case "slot-prefix":
					f.results[slot] = f.results[slot][2:]
				case "slot-reserved":
					n, _ := new(big.Int).SetString(f.results[slot][2:], 16)
					n.SetBit(n, 232, 1)
					f.results[slot] = "0x" + singletonTestWord(n)
				case "price-width":
					f.setSlotField(0, new(big.Int).Lsh(big.NewInt(1), 160))
				case "price-zero":
					f.setSlotField(0, big.NewInt(0))
				case "price-tick":
					f.setSlotField(0, big.NewInt(1))
				case "tick-width":
					f.setSlotField(1, new(big.Int).Lsh(big.NewInt(1), 24))
				case "tick-range":
					f.setSlotField(1, big.NewInt(-887273))
				case "protocol-width":
					f.setSlotField(2, new(big.Int).Lsh(big.NewInt(1), 24))
				case "lp-width":
					f.setSlotField(3, new(big.Int).Lsh(big.NewInt(1), 24))
				case "fee-mismatch":
					f.setSlotField(3, big.NewInt(3001))
				case "fee-dynamic":
					f.setSlotField(3, big.NewInt(0x800000))
				case "protocol-fee0", "protocol-fee1":
					fee := int64(1001)
					if protocol == "pancake-infinity-cl" {
						fee = 4001
					}
					if mode == "protocol-fee1" {
						fee <<= 12
					}
					f.setSlotField(2, big.NewInt(fee))
				case "liquidity-width":
					f.results[f.liquidityKey()] = "0x" + singletonTestWord(new(big.Int).Lsh(big.NewInt(1), 128))
				case "missing-tick", "net-exceeds-gross", "gross-overflow":
					gross := big.NewInt(0)
					net := big.NewInt(0)
					if mode == "net-exceeds-gross" {
						gross.SetInt64(1)
						net.SetInt64(2)
					}
					if mode == "gross-overflow" {
						gross.Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))
					}
					if protocol == "uniswap-v4" {
						f.results[tick] = "0x" + singletonTestWord(new(big.Int).Or(gross, new(big.Int).Lsh(net, 128)))
					} else {
						f.results[tick] = "0x" + singletonTestWord(gross) + singletonTestWord(net) + singletonTestInt(0) + singletonTestInt(0)
					}
				case "tick-short":
					f.results[tick] = f.results[tick][:len(f.results[tick])-2]
				case "tick-gross-width":
					f.results[tick] = "0x" + singletonTestWord(new(big.Int).Lsh(big.NewInt(1), 128)) + f.results[tick][66:]
				case "tick-net-width":
					f.results[tick] = f.results[tick][:66] + singletonTestWord(new(big.Int).Lsh(big.NewInt(1), 127)) + f.results[tick][130:]
				case "bitmap-short":
					f.results[f.bitmapKey(-1)] = "0x"
				case "bitmap-range":
					price, e := domain.SqrtRatioAtTick(domain.MinV3Tick)
					if e != nil {
						t.Fatal(e)
					}
					f.setSlotField(0, price)
					f.setSlotField(1, big.NewInt(int64(domain.MinV3Tick)))
					f.results[f.bitmapKey(-58)] = "0x" + singletonTestInt(1)
					f.results[f.bitmapKey(-57)] = "0x" + singletonTestInt(0)
				case "tick-budget":
					f.config.SingletonCL.MaxInitializedTicks = 1
				case "rpc-code":
					f.rpcError = "eth_getCode"
				case "rpc-slot":
					f.rpcError = slot
				case "rpc-tick":
					f.rpcError = tick
				case "paused":
					f.results[singletonTestSelector("paused()")] = "0x" + singletonTestInt(1)
				case "paused-width":
					f.results[singletonTestSelector("paused()")] = "0x" + singletonTestInt(2)
				default:
					key := singletonTestSelector("poolIdToPoolKey(bytes32)") + f.config.SingletonCL.PoolID[2:]
					raw := f.results[key][2:]
					if mode == "key-short" {
						f.results[key] = "0x" + raw[:len(raw)-2]
						break
					}
					i := map[string]int{"key-token0": 0, "key-token1": 1, "key-hooks": 2, "key-manager": 3, "key-fee": 4, "key-parameters": 5, "key-address-width": 0, "key-fee-width": 4}[mode]
					replacement := singletonTestInt(1)
					if mode == "key-address-width" {
						replacement = singletonTestWord(new(big.Int).Lsh(big.NewInt(1), 160))
					}
					if mode == "key-fee-width" {
						replacement = singletonTestWord(new(big.Int).Lsh(big.NewInt(1), 24))
					}
					f.results[key] = "0x" + raw[:i*64] + replacement + raw[(i+1)*64:]
				}
				a := f.adapter(t)
				got, err := a.readSingletonCL(context.Background(), f.config, f.anchor)
				if err == nil {
					t.Fatalf("unsafe %s accepted: %+v", mode, got)
				}
				if !reflect.DeepEqual(got, domain.Pool{}) {
					t.Fatalf("partial pool escaped: %+v", got)
				}
				if mode == "tick-budget" {
					f.mu.Lock()
					defer f.mu.Unlock()
					for _, call := range f.calls {
						if call == f.tickKey(-60) || call == f.tickKey(60) {
							t.Fatal("tick budget enforced after tick reads")
						}
					}
				}
			})
		}
	}
}
func TestSingletonCLRejectsInvalidConfigurationBeforeRPC(t *testing.T) {
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		for _, mode := range []string{"nil", "profile", "manager", "zero-manager", "digest", "poolid", "physical-id", "token0", "native", "order", "fee", "fee-config", "denominator", "spacing-zero", "spacing-high", "parameters", "radius", "ticks-zero", "ticks-high", "steps-zero", "steps-high", "anchor-missing", "anchor-number", "anchor-noncanonical"} {
			t.Run(protocol+"/"+mode, func(t *testing.T) {
				f := singletonFixture(protocol)
				p := f.config
				switch mode {
				case "nil":
					p.SingletonCL = nil
				case "profile":
					p.Protocol = "uniswap-v3"
				case "manager":
					p.SingletonCL.Manager = "invalid"
				case "zero-manager":
					p.SingletonCL.Manager = "0x" + strings.Repeat("0", 40)
				case "digest":
					p.SingletonCL.ExpectedManagerCodeSHA256 = "bad"
				case "poolid":
					p.SingletonCL.PoolID = "0x" + strings.Repeat("f", 64)
				case "physical-id":
					p.ID = p.SingletonCL.PoolID
				case "token0":
					p.Token0 = "not-an-address"
				case "native":
					p.Token0 = "0x" + strings.Repeat("0", 40)
				case "order":
					p.Token0, p.Token1 = p.Token1, p.Token0
				case "fee":
					p.SingletonCL.LPFee = 1000000
				case "fee-config":
					p.FeeNumerator = 3001
				case "denominator":
					p.FeeDenominator = 1000
				case "spacing-zero":
					p.SingletonCL.TickSpacing = 0
				case "spacing-high":
					p.SingletonCL.TickSpacing = 32768
				case "parameters":
					p.SingletonCL.Parameters = "0x" + strings.Repeat("f", 64)
				case "radius":
					p.SingletonCL.TickWordRadius = 17
				case "ticks-zero":
					p.SingletonCL.MaxInitializedTicks = 0
				case "ticks-high":
					p.SingletonCL.MaxInitializedTicks = 4097
				case "steps-zero":
					p.SingletonCL.MaxSteps = 0
				case "steps-high":
					p.SingletonCL.MaxSteps = 8193
				case "anchor-missing":
					f.anchor = nil
				case "anchor-number":
					f.anchor = map[string]any{"blockNumber": "0x64", "requireCanonical": true}
				case "anchor-noncanonical":
					f.anchor["requireCanonical"] = false
				}
				a := f.adapter(t)
				_, err := a.readSingletonCL(context.Background(), p, f.anchor)
				if err == nil {
					t.Fatalf("invalid config accepted: %s", mode)
				}
				f.mu.Lock()
				defer f.mu.Unlock()
				if len(f.calls) != 0 {
					t.Fatalf("invalid config made RPC reads: %v", f.calls)
				}
			})
		}
	}
}

func TestSingletonCLBoundaryWindowsAndBudgets(t *testing.T) {
	for _, protocol := range []string{"uniswap-v4", "pancake-infinity-cl"} {
		for _, mode := range []string{"minimum", "maximum", "radius-zero", "exact-tick-budget", "max-protocol-fees", "gross-at-limit", "gross-above-limit"} {
			t.Run(protocol+"/"+mode, func(t *testing.T) {
				f := singletonFixture(protocol)
				wantPositions := []int32{-2, -1, 0}
				switch mode {
				case "minimum", "maximum":
					index := domain.MinV3Tick
					positions := []int64{-58, -57}
					if mode == "maximum" {
						index = domain.MaxV3Tick - 1
						positions = []int64{56, 57}
					}
					price, e := domain.SqrtRatioAtTick(index)
					if e != nil {
						t.Fatal(e)
					}
					f.setSlotField(0, price)
					f.setSlotField(1, big.NewInt(int64(index)))
					f.results[f.liquidityKey()] = "0x" + singletonTestInt(0)
					wantPositions = nil
					for _, pos := range positions {
						f.results[f.bitmapKey(pos)] = "0x" + singletonTestInt(0)
						wantPositions = append(wantPositions, int32(pos))
					}
				case "radius-zero":
					f.config.SingletonCL.TickWordRadius = 0
					wantPositions = []int32{-1}
				case "exact-tick-budget":
					f.config.SingletonCL.MaxInitializedTicks = 2
				case "max-protocol-fees":
					fee := int64(1000)
					if protocol == "pancake-infinity-cl" {
						fee = 4000
					}
					f.setSlotField(2, big.NewInt(fee|fee<<12))
				case "gross-at-limit", "gross-above-limit":
					// Independent calculation from each pinned core's Solidity sdiv/smod formula.
					spacing := int64(f.config.SingletonCL.TickSpacing)
					minimum := int64(domain.MinV3Tick) / spacing
					if int64(domain.MinV3Tick)%spacing < 0 {
						minimum--
					}
					count := int64(domain.MaxV3Tick)/spacing - minimum + 1
					gross := new(big.Int).Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), big.NewInt(count))
					if mode == "gross-above-limit" {
						gross.Add(gross, big.NewInt(1))
					}
					for _, tick := range []int64{-60, 60} {
						key := f.tickKey(tick)
						if protocol == "uniswap-v4" {
							f.results[key] = "0x" + singletonTestWord(gross)
						} else {
							f.results[key] = "0x" + singletonTestWord(gross) + singletonTestInt(0) + singletonTestInt(0) + singletonTestInt(0)
						}
					}
				}
				a := f.adapter(t)
				got, e := a.readSingletonCL(context.Background(), f.config, f.anchor)
				if mode == "gross-above-limit" {
					if e == nil || !strings.Contains(e.Error(), "SINGLETON_TICK_GROSS") {
						t.Fatalf("floor-derived gross limit not enforced: %v", e)
					}
					return
				}
				if e != nil {
					t.Fatal(e)
				}
				words := got.SingletonCL.Core.TickWords
				var positions []int32
				for _, word := range words {
					positions = append(positions, word.WordPos)
				}
				if !reflect.DeepEqual(positions, wantPositions) {
					t.Fatalf("window %v want %v", positions, wantPositions)
				}
				if mode == "radius-zero" && len(got.SingletonCL.Core.Ticks) != 1 {
					t.Fatal("radius-zero fetched ticks outside window")
				}
			})
		}
	}
}
