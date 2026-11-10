package provenance

import (
	"context"
	"encoding/json"
	"errors"
	"github.com/ethereum/go-ethereum/core/types"
	"testing"
)

func TestCaptureBindsBothHeadersAndPinsEveryStateRead(t *testing.T) {
	e := fixture(t)
	var h types.Header
	_ = json.Unmarshal(e.Block, &h)
	var tx types.Transaction
	raw, _ := data(e.RawTransactions[0], -1)
	_ = tx.UnmarshalBinary(raw)
	txJSON, _ := json.Marshal(&tx)
	var body map[string]any
	_ = json.Unmarshal(e.Block, &body)
	body["transactions"] = []json.RawMessage{txJSON}
	block, _ := json.Marshal(body)
	var calls []string
	call := func(ctx context.Context, source, method string, params any, out any) error {
		calls = append(calls, source+":"+method)
		var value any
		switch method {
		case "eth_chainId":
			value = "0x1"
		case "eth_getBlockByNumber":
			value = json.RawMessage(block)
		case "eth_getCode":
			anchor := params.([]any)[1].(map[string]any)
			if anchor["blockHash"] != e.ExpectedBlockHash || anchor["requireCanonical"] != true {
				t.Fatal("unpinned code")
			}
			value = e.Deployments[0].Code
		case "eth_getProof":
			anchor := params.([]any)[2].(map[string]any)
			if anchor["blockHash"] != e.ExpectedBlockHash || anchor["requireCanonical"] != true {
				t.Fatal("unpinned proof")
			}
			value = e.Deployments[0].Proof
		default:
			return errors.New("unexpected method")
		}
		b, _ := json.Marshal(value)
		return json.Unmarshal(b, out)
	}
	got, r, err := Capture(context.Background(), CaptureRequest{Primary: "primary", Witness: "witness", ChainID: "1", BlockTag: "0x2a", Addresses: []string{e.Deployments[0].Address}}, call)
	if err != nil || !r.Passed || got.ExpectedBlockHash != e.ExpectedBlockHash || len(calls) != 7 {
		t.Fatalf("%+v %v %v", r, err, calls)
	}
	for name, hook := range map[string]func(string, string, any, any) error{
		"wrong-chain": func(source, method string, params, out any) error {
			if source == "witness" && method == "eth_chainId" {
				return json.Unmarshal([]byte(`"0x2"`), out)
			}
			return call(context.Background(), source, method, params, out)
		},
		"different-head": func(source, method string, params, out any) error {
			if source == "witness" && method == "eth_getBlockByNumber" {
				h.Time++
				b, _ := json.Marshal(h)
				return json.Unmarshal(b, out)
			}
			return call(context.Background(), source, method, params, out)
		},
		"missing-proof": func(source, method string, params, out any) error {
			if method == "eth_getProof" {
				return errors.New("pruned")
			}
			return call(context.Background(), source, method, params, out)
		},
	} {
		t.Run(name, func(t *testing.T) {
			_, _, err := Capture(context.Background(), CaptureRequest{Primary: "primary", Witness: "witness", ChainID: "1", BlockTag: "0x2a", Addresses: []string{e.Deployments[0].Address}}, func(_ context.Context, s, m string, p, o any) error { return hook(s, m, p, o) })
			if err == nil {
				t.Fatal("source contradiction accepted")
			}
		})
	}
}
func TestCaptureRequiresExplicitDistinctSourcesAndBoundedScope(t *testing.T) {
	calls := 0
	call := func(context.Context, string, string, any, any) error { calls++; return errors.New("unexpected") }
	for _, req := range []CaptureRequest{{Primary: "p", Witness: "p", ChainID: "1", BlockTag: "finalized", Addresses: []string{"x"}}, {Primary: "p", Witness: "w", ChainID: "1", BlockTag: "latest", Addresses: []string{"x"}}, {Primary: "p", Witness: "w", ChainID: "1", BlockTag: "0x01", Addresses: []string{"x"}}, {Primary: "p", Witness: "w", ChainID: "1", BlockTag: "finalized"}} {
		if _, _, err := Capture(context.Background(), req, call); err == nil {
			t.Fatal("invalid request accepted")
		}
	}
	if calls != 0 {
		t.Fatal("invalid scope contacted providers")
	}
}
