package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/crypto"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

const completeV2Protocol = "uniswap-v2-complete"
const completeV2Version = "uniswap-v2/core-1.0.1-full-swap-1"

type completeV2Account struct {
	Address string `json:"address"`
	Code    string `json:"code"`
	Storage []struct {
		Slot  string `json:"slot"`
		Value string `json:"value"`
	} `json:"storage"`
}
type completeV2Case struct {
	Name        string      `json:"name"`
	Pool        domain.Pool `json:"pool"`
	StateAnchor struct {
		Number     uint64 `json:"block_number"`
		Hash       string `json:"block_hash"`
		ParentHash string `json:"parent_hash"`
		Timestamp  uint64 `json:"timestamp"`
	} `json:"state_anchor"`
	Accounts []completeV2Account   `json:"accounts"`
	Getters  *completeV2GetterBank `json:"-"`
}

func loadCompleteV2OriginalBank(t *testing.T) completeV2Case {
	t.Helper()
	// Only the native worker's service-local original-artifact bank is read.
	// These RPC responses come from captured accounts and slots.
	raw, err := os.ReadFile("../../../test/contracts/v2-complete.json")
	if err != nil {
		t.Fatal(err)
	}
	var bank struct {
		Cases []completeV2Case `json:"cases"`
	}
	if err = json.Unmarshal(raw, &bank); err != nil || len(bank.Cases) == 0 {
		t.Fatalf("original bank: %v", err)
	}
	return bank.Cases[0]
}
func (c completeV2Case) config() config.PoolConfig {
	return config.PoolConfig{ID: c.Pool.PoolID, Protocol: completeV2Protocol, Token0: c.Pool.Token0, Token1: c.Pool.Token1, FeeNumerator: 3, FeeDenominator: 1000, V2: &config.V2PoolConfig{ExpectedFactory: c.Pool.V2.Factory, ExpectedCodeSHA256: c.Pool.V2.CodeSHA256}}
}
func completeV2Word(n uint64) string        { return fmt.Sprintf("0x%064x", n) }
func completeV2AddressWord(a string) string { return "0x" + strings.Repeat("0", 24) + a[2:] }
func completeV2Mapping(address string, slot []byte) []byte {
	key, _ := hex.DecodeString(strings.Repeat("0", 24) + address[2:])
	return crypto.Keccak256(append(key, slot...))
}
func (c completeV2Case) account(t *testing.T, address string) completeV2Account {
	t.Helper()
	for _, a := range c.Accounts {
		if a.Address == address {
			return a
		}
	}
	t.Fatalf("bank account missing: %s", address)
	return completeV2Account{}
}
func (c completeV2Case) slot(t *testing.T, address, slot string) string {
	t.Helper()
	for _, s := range c.account(t, address).Storage {
		if s.Slot == slot {
			return s.Value
		}
	}
	t.Errorf("bank slot missing: %s %s", address, slot)
	return ""
}
func (c completeV2Case) balance(t *testing.T, token string) string {
	raw, _ := hex.DecodeString(completeV2Word(1)[2:])
	return c.slot(t, token, "0x"+hex.EncodeToString(completeV2Mapping(c.Pool.PoolID, raw)))
}
func (c completeV2Case) pairFor(t *testing.T, token0, token1 string) string {
	raw, _ := hex.DecodeString(completeV2Word(2)[2:])
	return c.slot(t, c.Pool.V2.Factory, "0x"+hex.EncodeToString(completeV2Mapping(token1, completeV2Mapping(token0, raw))))
}

type completeV2RPC struct {
	t       *testing.T
	bank    completeV2Case
	mutate  func(method, to, data string, result any) any
	mu      sync.Mutex
	seen    map[string]int
	headers int
}

func (b *completeV2RPC) serve(w http.ResponseWriter, r *http.Request) {
	var q struct {
		ID     int               `json:"id"`
		Method string            `json:"method"`
		Params []json.RawMessage `json:"params"`
	}
	if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
		b.t.Error(err)
		http.Error(w, "bad request", 400)
		return
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	c := b.bank
	var out any
	var to, data string
	switch q.Method {
	case "eth_getBlockByNumber":
		b.headers++
		out = map[string]any{"number": fmt.Sprintf("0x%x", c.StateAnchor.Number), "hash": c.StateAnchor.Hash, "parentHash": c.StateAnchor.ParentHash, "timestamp": fmt.Sprintf("0x%x", c.StateAnchor.Timestamp), "transactions": []string{}, "receiptsRoot": "0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421"}
	case "eth_getCode", "eth_call", "eth_getStorageAt":
		pos := 1
		if q.Method == "eth_getStorageAt" {
			pos = 2
		}
		var anchor map[string]any
		if len(q.Params) != pos+1 || json.Unmarshal(q.Params[pos], &anchor) != nil || len(anchor) != 2 || anchor["blockHash"] != c.StateAnchor.Hash || anchor["requireCanonical"] != true {
			b.t.Errorf("unpinned %s: %s", q.Method, q.Params)
			http.Error(w, "unpinned", 400)
			return
		}
		if q.Method == "eth_call" {
			var call map[string]string
			if json.Unmarshal(q.Params[0], &call) != nil || len(call) != 2 {
				b.t.Error("invalid eth_call")
			}
			to, data = call["to"], call["data"]
			switch {
			case to == c.Pool.PoolID:
				switch data {
				case "0xc45a0155":
					out = c.slot(b.t, to, completeV2Word(5))
				case "0x0dfe1681":
					out = c.slot(b.t, to, completeV2Word(6))
				case "0xd21220a7":
					out = c.slot(b.t, to, completeV2Word(7))
				case "0x0902f1ac":
					raw, _ := hex.DecodeString(c.slot(b.t, to, completeV2Word(8))[2:])
					out = "0x" + strings.Repeat("0", 36) + hex.EncodeToString(raw[18:]) + strings.Repeat("0", 36) + hex.EncodeToString(raw[4:18]) + strings.Repeat("0", 56) + hex.EncodeToString(raw[:4])
				case "0x5909c0d5":
					out = c.slot(b.t, to, completeV2Word(9))
				case "0x5a3d5493":
					out = c.slot(b.t, to, completeV2Word(10))
				}
			case to == c.Pool.V2.Factory && strings.HasPrefix(data, "0xe6a43905") && len(data) == 138:
				if data[10:34] != strings.Repeat("0", 24) || data[74:98] != strings.Repeat("0", 24) {
					b.t.Error("getPair padding")
				}
				out = c.pairFor(b.t, "0x"+data[34:74], "0x"+data[98:138])
			case (to == c.Pool.Token0 || to == c.Pool.Token1) && data == "0x70a08231"+completeV2AddressWord(c.Pool.PoolID)[2:]:
				out = c.balance(b.t, to)
			}
		} else {
			json.Unmarshal(q.Params[0], &to)
			if q.Method == "eth_getCode" {
				out = c.account(b.t, to).Code
			} else {
				json.Unmarshal(q.Params[1], &data)
				if to != c.Pool.PoolID || (data != "0xc" && data != completeV2Word(12)) {
					b.t.Errorf("unexpected storage read: %s %s", to, data)
				} else {
					out = c.slot(b.t, to, completeV2Word(12))
				}
			}
		}
		if c.Getters != nil {
			switch q.Method {
			case "eth_call":
				found := false
				for _, call := range c.Getters.Calls {
					if call.To == to && call.Data == data {
						found = true
						if out != call.Result || call.BlockHash != c.StateAnchor.Hash || call.BlockNumber != c.StateAnchor.Number {
							b.t.Errorf("captured getter disagrees with bank: %s %s", to, data)
						}
						out = call.Result
						break
					}
				}
				if !found {
					b.t.Errorf("missing native getter: %s %s", to, data)
				}
			case "eth_getStorageAt":
				lock := c.Getters.Lock
				if out != lock.Value || lock.Address != to || lock.Slot != completeV2Word(12) || lock.BlockHash != c.StateAnchor.Hash || lock.BlockNumber != c.StateAnchor.Number {
					b.t.Error("native lock observation disagrees with bank")
				}
				out = lock.Value
			}
		}
		b.seen[q.Method+"/"+to+"/"+data]++
	default:
		b.t.Errorf("unexpected RPC: %s", q.Method)
	}
	if out == nil {
		b.t.Errorf("unhandled RPC %s %s %s", q.Method, to, data)
	}
	if b.mutate != nil {
		out = b.mutate(q.Method, to, data, out)
	}
	json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": out})
}
func newCompleteV2RPC(t *testing.T, c completeV2Case, mutate func(string, string, string, any) any) (*Adapter, *completeV2RPC) {
	t.Helper()
	b := &completeV2RPC{t: t, bank: c, mutate: mutate, seen: map[string]int{}}
	s := httptest.NewServer(http.HandlerFunc(b.serve))
	t.Cleanup(s.Close)
	return &Adapter{Client: rpc.New([]string{s.URL}, 3*time.Second, 1<<20), Chain: config.Chain{Key: c.Pool.ChainKey, Pools: []config.PoolConfig{c.config()}}}, b
}
func completeV2Amount(t *testing.T, word string) string {
	t.Helper()
	n, ok := new(big.Int).SetString(strings.TrimPrefix(word, "0x"), 16)
	if !ok {
		t.Fatal("invalid bank word")
	}
	return n.String()
}
func assertCompleteV2Projection(t *testing.T, c completeV2Case, p domain.Pool) {
	t.Helper()
	if p.Protocol != completeV2Protocol || p.AdapterVersion != completeV2Version || p.Reserve0 != c.Pool.Reserve0 || p.Reserve1 != c.Pool.Reserve1 || p.V2 == nil {
		t.Fatalf("pool identity/reserves: %+v", p)
	}
	raw, e := json.Marshal(p.V2)
	if e != nil {
		t.Fatal(e)
	}
	var got struct {
		Factory string         `json:"factory"`
		Code    string         `json:"code_sha256"`
		Swap    map[string]any `json:"swap"`
	}
	d := json.NewDecoder(strings.NewReader(string(raw)))
	d.UseNumber()
	if e = d.Decode(&got); e != nil {
		t.Fatal(e)
	}
	packed, _ := hex.DecodeString(c.slot(t, c.Pool.PoolID, completeV2Word(8))[2:])
	expected := map[string]any{
		"raw_balance0": completeV2Amount(t, c.balance(t, c.Pool.Token0)), "raw_balance1": completeV2Amount(t, c.balance(t, c.Pool.Token1)),
		"block_timestamp_last":   json.Number(new(big.Int).SetBytes(packed[:4]).String()),
		"price0_cumulative_last": completeV2Amount(t, c.slot(t, c.Pool.PoolID, completeV2Word(9))), "price1_cumulative_last": completeV2Amount(t, c.slot(t, c.Pool.PoolID, completeV2Word(10))),
		"unlocked": true, "factory_code_sha256": "3abc53f12a9cb8ae37ebfada9efc261c1ab4c2759d161e341a49bf67df3f8321",
		"bank_block_number": json.Number(fmt.Sprint(c.StateAnchor.Number)), "bank_block_hash": c.StateAnchor.Hash, "bank_unix_timestamp": json.Number(fmt.Sprint(c.StateAnchor.Timestamp)),
	}
	want, _ := json.Marshal(expected)
	have, _ := json.Marshal(got.Swap)
	if got.Factory != c.Pool.V2.Factory || got.Code != c.Pool.V2.CodeSHA256 || string(want) != string(have) {
		t.Fatalf("complete projection\nwant %s\ngot  %s", want, raw)
	}
}
func TestV2CompleteReadsOriginalBankAtOneHash(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	a, b := newCompleteV2RPC(t, c, nil)
	ref, pools, err := a.ReadState(context.Background(), "latest", false)
	if err != nil || len(pools) != 1 {
		t.Fatalf("complete bank read: pools=%d err=%v", len(pools), err)
	}
	assertCompleteV2Projection(t, c, pools[0])
	if ref.Number != c.StateAnchor.Number || ref.Hash != c.StateAnchor.Hash || ref.TimestampMS != c.StateAnchor.Timestamp*1000 {
		t.Fatal(ref)
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	count := 0
	for _, n := range b.seen {
		if n != 1 {
			t.Fatalf("repeated dependency: %v", b.seen)
		}
		count += n
	}
	if count != 13 || b.headers != 2 {
		t.Fatalf("missing dependencies or canonical recheck: reads=%d headers=%d", count, b.headers)
	}
}
func TestV2CompleteRejectsTamperedRPCBank(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	tests := []struct {
		name, method, to, data string
		result                 any
	}{
		{"pair-code", "eth_getCode", c.Pool.PoolID, "", "0x00"},
		{"factory-code", "eth_getCode", c.Pool.V2.Factory, "", "0x00"},
		{"pair-factory", "eth_call", c.Pool.PoolID, "0xc45a0155", completeV2Word(0)},
		{"factory-padding", "eth_call", c.Pool.PoolID, "0xc45a0155", "0x01" + completeV2AddressWord(c.Pool.V2.Factory)[4:]},
		{"token0", "eth_call", c.Pool.PoolID, "0x0dfe1681", completeV2AddressWord(c.Pool.Token1)},
		{"token1-padding", "eth_call", c.Pool.PoolID, "0xd21220a7", "0x01" + completeV2AddressWord(c.Pool.Token1)[4:]},
		{"reserve-width", "eth_call", c.Pool.PoolID, "0x0902f1ac", fmt.Sprintf("0x%064x%064x%064x", new(big.Int).Lsh(big.NewInt(1), 112), 1, 1)},
		{"timestamp-width", "eth_call", c.Pool.PoolID, "0x0902f1ac", fmt.Sprintf("0x%064x%064x%064x", 1, 1, uint64(1)<<32)},
		{"reserve-short", "eth_call", c.Pool.PoolID, "0x0902f1ac", completeV2Word(1)},
		{"balance0-missing", "eth_call", c.Pool.Token0, "0x70a08231" + completeV2AddressWord(c.Pool.PoolID)[2:], nil},
		{"balance1-short", "eth_call", c.Pool.Token1, "0x70a08231" + completeV2AddressWord(c.Pool.PoolID)[2:], "0x01"},
		{"cumulative0-unknown", "eth_call", c.Pool.PoolID, "0x5909c0d5", nil},
		{"cumulative1-long", "eth_call", c.Pool.PoolID, "0x5a3d5493", "0x" + strings.Repeat("0", 66)},
		{"lock-zero", "eth_getStorageAt", c.Pool.PoolID, "", completeV2Word(0)},
		{"lock-two", "eth_getStorageAt", c.Pool.PoolID, "", completeV2Word(2)},
		{"lock-high-bits", "eth_getStorageAt", c.Pool.PoolID, "", "0x01" + completeV2Word(1)[4:]},
		{"lock-short", "eth_getStorageAt", c.Pool.PoolID, "", "0x1"},
		{"lock-unknown", "eth_getStorageAt", c.Pool.PoolID, "", nil},
	}
	for _, tokens := range [][2]string{{c.Pool.Token0, c.Pool.Token1}, {c.Pool.Token1, c.Pool.Token0}} {
		tests = append(tests, struct {
			name, method, to, data string
			result                 any
		}{"getPair-" + tokens[0], "eth_call", c.Pool.V2.Factory, "0xe6a43905" + completeV2AddressWord(tokens[0])[2:] + completeV2AddressWord(tokens[1])[2:], completeV2Word(0)})
	}
	for _, tc := range tests {
		t.Run(tc.name, func(t *testing.T) {
			changed := false
			a, _ := newCompleteV2RPC(t, c, func(method, to, data string, out any) any {
				if method == tc.method && to == tc.to && (tc.data == "" || data == tc.data) {
					changed = true
					return tc.result
				}
				return out
			})
			_, pools, err := a.ReadState(context.Background(), "latest", false)
			if !changed || err == nil || pools != nil {
				t.Fatalf("tampered dependency must fail whole read: reached=%v pools=%d err=%v", changed, len(pools), err)
			}
		})
	}
}

func TestV2CompleteRejectsHeaderChangeAndInvalidConfig(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	for _, mode := range []string{"hash-changed", "parent-changed", "timestamp-changed", "number-changed", "hash-short", "hash-uppercase", "parent-short", "timestamp-overflow", "fractional-quantity", "pool-address", "zero-pool", "token-address", "same-token", "zero-token", "factory-address", "zero-factory", "identity-missing", "config-hash", "fee"} {
		t.Run(mode, func(t *testing.T) {
			n := 0
			a, _ := newCompleteV2RPC(t, c, func(method, to, data string, out any) any {
				if method != "eth_getBlockByNumber" {
					return out
				}
				n++
				h := out.(map[string]any)
				switch mode {
				case "hash-changed":
					if n == 2 {
						h["hash"] = "0x" + strings.Repeat("a", 64)
					}
				case "parent-changed":
					if n == 2 {
						h["parentHash"] = "0x" + strings.Repeat("b", 64)
					}
				case "timestamp-changed":
					if n == 2 {
						h["timestamp"] = "0x1"
					}
				case "number-changed":
					if n == 2 {
						h["number"] = "0x1"
					}
				case "hash-short":
					h["hash"] = "0x12"
				case "hash-uppercase":
					h["hash"] = strings.ToUpper(c.StateAnchor.Hash)
				case "parent-short":
					h["parentHash"] = "0x12"
				case "timestamp-overflow":
					h["timestamp"] = "0xffffffffffffffff"
				case "fractional-quantity":
					h["timestamp"] = "0x1.1"
				}
				return h
			})
			p := &a.Chain.Pools[0]
			switch mode {
			case "pool-address":
				p.ID = "0x12"
			case "zero-pool":
				p.ID = "0x" + strings.Repeat("0", 40)
			case "token-address":
				p.Token0 = "0X" + p.Token0[2:]
			case "same-token":
				p.Token1 = p.Token0
			case "zero-token":
				p.Token0 = "0x" + strings.Repeat("0", 40)
			case "factory-address":
				p.V2.ExpectedFactory = "0x12"
			case "zero-factory":
				p.V2.ExpectedFactory = "0x" + strings.Repeat("0", 40)
			case "identity-missing":
				p.V2 = nil
			case "config-hash":
				p.V2.ExpectedCodeSHA256 = strings.Repeat("a", 64)
			case "fee":
				p.FeeNumerator = 4
			}
			_, pools, err := a.ReadState(context.Background(), "latest", false)
			if err == nil || pools != nil {
				t.Fatalf("invalid state accepted: pools=%d err=%v", len(pools), err)
			}
		})
	}
}

func TestV2CompleteCapturesUnsyncedBalancesAndU256(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	for _, token := range []string{c.Pool.Token0, c.Pool.Token1} {
		t.Run(token, func(t *testing.T) {
			// Model a donation at the token balance mapping while preserving reserve
			// and oracle slots. Full U256 balances are valid even above uint112.
			d := loadCompleteV2OriginalBank(t)
			keyRaw, _ := hex.DecodeString(completeV2Word(1)[2:])
			key := "0x" + hex.EncodeToString(completeV2Mapping(d.Pool.PoolID, keyRaw))
			for i := range d.Accounts {
				if d.Accounts[i].Address == token {
					for j := range d.Accounts[i].Storage {
						if d.Accounts[i].Storage[j].Slot == key {
							d.Accounts[i].Storage[j].Value = "0x" + strings.Repeat("f", 64)
						}
					}
				}
			}
			for i := range d.Accounts {
				if d.Accounts[i].Address == d.Pool.PoolID {
					for j := range d.Accounts[i].Storage {
						s := &d.Accounts[i].Storage[j]
						if s.Slot == completeV2Word(9) || s.Slot == completeV2Word(10) {
							s.Value = "0x" + strings.Repeat("f", 64)
						}
					}
				}
			}
			a, _ := newCompleteV2RPC(t, d, nil)
			_, pools, err := a.ReadState(context.Background(), "latest", false)
			if err != nil || len(pools) != 1 {
				t.Fatalf("donated U256 balance rejected: %v", err)
			}
			assertCompleteV2Projection(t, d, pools[0])
		})
	}
}

func TestV2CompleteRejectsNonDATAAndOverlongDependencies(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	targets := []struct{ name, method, to, data string }{
		{"pair-code", "eth_getCode", c.Pool.PoolID, ""},
		{"factory-code", "eth_getCode", c.Pool.V2.Factory, ""},
		{"factory", "eth_call", c.Pool.PoolID, "0xc45a0155"},
		{"token0", "eth_call", c.Pool.PoolID, "0x0dfe1681"},
		{"token1", "eth_call", c.Pool.PoolID, "0xd21220a7"},
		{"reserves", "eth_call", c.Pool.PoolID, "0x0902f1ac"},
		{"getPair", "eth_call", c.Pool.V2.Factory, "0xe6a43905" + completeV2AddressWord(c.Pool.Token0)[2:] + completeV2AddressWord(c.Pool.Token1)[2:]},
		{"balance0", "eth_call", c.Pool.Token0, "0x70a08231" + completeV2AddressWord(c.Pool.PoolID)[2:]},
		{"balance1", "eth_call", c.Pool.Token1, "0x70a08231" + completeV2AddressWord(c.Pool.PoolID)[2:]},
		{"cumulative0", "eth_call", c.Pool.PoolID, "0x5909c0d5"},
		{"cumulative1", "eth_call", c.Pool.PoolID, "0x5a3d5493"},
		{"lock", "eth_getStorageAt", c.Pool.PoolID, ""},
	}
	for _, target := range targets {
		for _, mode := range []string{"no-prefix", "bad-hex", "extra-word", "missing", "empty"} {
			t.Run(target.name+"/"+mode, func(t *testing.T) {
				changed := false
				a, _ := newCompleteV2RPC(t, c, func(method, to, data string, out any) any {
					if method != target.method || to != target.to || (target.data != "" && data != target.data) {
						return out
					}
					changed = true
					value := out.(string)
					switch mode {
					case "no-prefix":
						return value[2:]
					case "bad-hex":
						return value[:len(value)-1] + "z"
					case "extra-word":
						return value + strings.Repeat("0", 64)
					case "missing":
						return nil
					default:
						return "0x"
					}
				})
				_, pools, err := a.ReadState(context.Background(), "latest", false)
				if !changed || err == nil || pools != nil {
					t.Fatalf("malformed dependency accepted: reached=%v pools=%d err=%v", changed, len(pools), err)
				}
			})
		}
	}
}

func TestV2CompleteRejectsUnboundReferenceBeforeRPC(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	for _, mode := range []string{"fractional-ms", "other-chain", "short-hash", "other-hash", "number-anchor", "not-canonical", "extra-anchor"} {
		t.Run(mode, func(t *testing.T) {
			a, b := newCompleteV2RPC(t, c, nil)
			ref := domain.BlockRef{ChainKey: c.Pool.ChainKey, Number: c.StateAnchor.Number, Hash: c.StateAnchor.Hash, ParentHash: c.StateAnchor.ParentHash, TimestampMS: c.StateAnchor.Timestamp * 1000}
			anchor := map[string]any{"blockHash": ref.Hash, "requireCanonical": true}
			switch mode {
			case "fractional-ms":
				ref.TimestampMS++
			case "other-chain":
				ref.ChainKey = "evm:1"
			case "short-hash":
				ref.Hash = "0x12"
			case "other-hash":
				anchor["blockHash"] = "0x" + strings.Repeat("a", 64)
			case "number-anchor":
				anchor = map[string]any{"blockNumber": "0x12", "requireCanonical": true}
			case "not-canonical":
				anchor["requireCanonical"] = false
			case "extra-anchor":
				anchor["blockNumber"] = "0x12"
			}
			if _, err := a.readV2Complete(context.Background(), c.config(), anchor, ref); err == nil {
				t.Fatal("unbound reference accepted")
			}
			b.mu.Lock()
			defer b.mu.Unlock()
			if len(b.seen) != 0 || b.headers != 0 {
				t.Fatal("invalid reference reached RPC")
			}
		})
	}
}

func TestV2CompleteLegacyProfileKeepsBaseRead(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	a, b := newCompleteV2RPC(t, c, nil)
	a.Chain.Pools[0].Protocol = "uniswap-v2"
	_, pools, err := a.ReadState(context.Background(), "latest", false)
	if err != nil || len(pools) != 1 {
		t.Fatalf("legacy base read: %v", err)
	}
	p := pools[0]
	if p.Protocol != "uniswap-v2" || p.AdapterVersion != "uniswap-v2/1" || p.V2 == nil || p.V2.Swap != nil || p.Reserve0 != c.Pool.Reserve0 || p.Reserve1 != c.Pool.Reserve1 {
		t.Fatalf("legacy read changed: %+v", p)
	}
	b.mu.Lock()
	defer b.mu.Unlock()
	if len(b.seen) != 5 || b.headers != 2 {
		t.Fatalf("legacy dependencies changed: %v headers=%d", b.seen, b.headers)
	}
}
