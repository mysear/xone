package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/trie"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/rpc"
)

func TestReadCompleteRequiresReceiptRootBeforePoolReads(t *testing.T) {
	for _, tc := range []struct{ name, root, want string }{
		{"missing", "", "RECEIPT_ROOT_MISSING_OR_INVALID"},
		{"wrong width", "0x12", "RECEIPT_ROOT_MISSING_OR_INVALID"},
		{"different root", "0x" + strings.Repeat("11", 32), "RECEIPT_ROOT_MISMATCH"},
	} {
		t.Run(tc.name, func(t *testing.T) {
			server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
				json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": map[string]any{
					"number": "0x64", "hash": "0x" + strings.Repeat("aa", 32),
					"parentHash": "0x" + strings.Repeat("bb", 32), "timestamp": "0x64",
					"transactions": []string{}, "receiptsRoot": tc.root,
				}})
			}))
			defer server.Close()
			adapter := Adapter{Client: rpc.New([]string{server.URL}, time.Second, 1<<20)}
			_, _, _, err := adapter.ReadComplete(context.Background(), "0x64")
			if err == nil || err.Error() != tc.want {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
			if _, _, err := adapter.ReadState(context.Background(), "0x64", false); err != nil {
				t.Fatal(err)
			}
		})
	}
}

func receiptRootVectors(t *testing.T) []struct {
	Name     string
	Root     string
	Receipts []Receipt
} {
	t.Helper()
	raw, err := os.ReadFile("../../../test/fixtures/receipt-roots.json")
	if err != nil {
		t.Fatal(err)
	}
	var data struct {
		Cases []struct {
			Name     string
			Root     string
			Receipts []Receipt
		}
	}
	if err := json.Unmarshal(raw, &data); err != nil {
		t.Fatal(err)
	}
	if len(data.Cases) != 12 {
		t.Fatal("incomplete independent vectors", len(data.Cases))
	}
	return data.Cases
}
func bindReceiptVector(root string, receipts []Receipt) (RawBlock, []Receipt) {
	block := RawBlock{ReceiptsRoot: root, Hash: "0x" + strings.Repeat("aa", 32), Transactions: []string{}}
	var index int
	for i := range receipts {
		r := &receipts[i]
		r.TxIndex = fmt.Sprintf("0x%x", i)
		r.TxHash = fmt.Sprintf("tx-%d", i)
		r.BlockHash = block.Hash
		block.Transactions = append(block.Transactions, r.TxHash)
		for j := range r.Logs {
			l := &r.Logs[j]
			l.BlockHash = block.Hash
			l.TxHash = r.TxHash
			l.TxIndex = r.TxIndex
			l.Index = fmt.Sprintf("0x%x", index)
			index++
		}
	}
	return block, receipts
}
func TestReceiptRootMatchesIndependentEthereumJSVectors(t *testing.T) {
	for _, vector := range receiptRootVectors(t) {
		t.Run(vector.Name, func(t *testing.T) {
			block, receipts := bindReceiptVector(vector.Root, vector.Receipts)
			before, _ := json.Marshal(receipts)
			if err := VerifyReceiptRoot(context.Background(), block, receipts); err != nil {
				t.Fatal(err)
			}
			// JSON receipt array order is not transaction order.
			for i, j := 0, len(receipts)-1; i < j; i, j = i+1, j-1 {
				receipts[i], receipts[j] = receipts[j], receipts[i]
			}
			if err := VerifyReceiptRoot(context.Background(), block, receipts); err != nil {
				t.Fatal(err)
			}
			for i, j := 0, len(receipts)-1; i < j; i, j = i+1, j-1 {
				receipts[i], receipts[j] = receipts[j], receipts[i]
			}
			after, _ := json.Marshal(receipts)
			if !bytes.Equal(before, after) {
				t.Fatal("mutated caller input")
			}
		})
	}
}
func TestReceiptRootRejectsConsensusFieldTampering(t *testing.T) {
	cases := []struct {
		name, want string
		mutate     func(*RawBlock, *[]Receipt)
	}{
		{"missing status", "RECEIPT_OUTCOME_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].Status = "" }},
		{"invalid status", "RECEIPT_OUTCOME_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].Status = "0x2" }},
		{"status with leading zero", "RECEIPT_OUTCOME_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].Status = "0x01" }},
		{"status and poststate", "RECEIPT_OUTCOME_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].PostState = "0x" + strings.Repeat("ab", 32) }},
		{"typed poststate", "RECEIPT_OUTCOME_INVALID", func(b *RawBlock, r *[]Receipt) {
			(*r)[0].Status = ""
			(*r)[0].PostState = "0x" + strings.Repeat("ab", 32)
		}},
		{"unknown receipt type", "RECEIPT_TYPE_UNSUPPORTED", func(b *RawBlock, r *[]Receipt) { (*r)[0].Type = "0x7e" }},
		{"wide receipt type", "RECEIPT_TYPE_UNSUPPORTED", func(b *RawBlock, r *[]Receipt) { (*r)[0].Type = "0x10000000000000000" }},
		{"noncanonical receipt type", "RECEIPT_TYPE_UNSUPPORTED", func(b *RawBlock, r *[]Receipt) { (*r)[0].Type = "0x01" }},
		{"missing gas", "RECEIPT_CUMULATIVE_GAS_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].CumulativeGasUsed = "" }},
		{"oversized gas", "RECEIPT_CUMULATIVE_GAS_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].CumulativeGasUsed = "0x10000000000000000" }},
		{"zero gas", "RECEIPT_CUMULATIVE_GAS_ORDER", func(b *RawBlock, r *[]Receipt) { (*r)[0].CumulativeGasUsed = "0x0" }},
		{"missing bloom", "RECEIPT_BLOOM_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].LogsBloom = "" }},
		{"short bloom", "RECEIPT_BLOOM_INVALID", func(b *RawBlock, r *[]Receipt) { (*r)[0].LogsBloom = "0x00" }},
		{"wrong bloom", "RECEIPT_BLOOM_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].LogsBloom = "0x" + strings.Repeat("00", 256) }},
		{"outcome flipped", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].Status = "0x0" }},
		{"type changed", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].Type = "0x2" }},
		{"legacy type omitted on typed data", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].Type = "" }},
		{"gas changed", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].CumulativeGasUsed = "0x5209" }},
		{"log data changed", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs[0].Data = "0x123456" }},
		{"trailing log omitted even with new bloom", "RECEIPT_ROOT_MISMATCH", func(b *RawBlock, r *[]Receipt) {
			(*r)[0].Logs = []Log{}
			(*r)[0].LogsBloom = "0x" + strings.Repeat("00", 256)
		}},
		{"trailing log omitted with old bloom", "RECEIPT_BLOOM_MISMATCH", func(b *RawBlock, r *[]Receipt) { (*r)[0].Logs = []Log{} }},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			vector := receiptRootVectors(t)[2] // access-list success, with a log
			block, receipts := bindReceiptVector(vector.Root, vector.Receipts)
			tc.mutate(&block, &receipts)
			err := VerifyReceiptRoot(context.Background(), block, receipts)
			if err == nil || err.Error() != tc.want {
				t.Fatalf("got %v, want %s", err, tc.want)
			}
		})
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := VerifyReceiptRoot(ctx, RawBlock{}, nil); err == nil || err.Error() != "RPC_DEADLINE" {
		t.Fatal(err)
	}
}

// Synthetic RPC fixture construction only; the golden roots above come from EthereumJS.
func sealReceiptTestFixture(block *RawBlock, receipts []Receipt) {
	encoded := make(types.Receipts, len(receipts))
	for i := range receipts {
		r := &receipts[i]
		r.Type = "0x0"
		r.Status = "0x1"
		r.CumulativeGasUsed = fmt.Sprintf("0x%x", (i+1)*21000)
		out := &types.Receipt{Status: 1, CumulativeGasUsed: uint64((i + 1) * 21000), Logs: []*types.Log{}}
		for _, l := range r.Logs {
			v := &types.Log{Address: common.HexToAddress(l.Address), Data: common.FromHex(l.Data), Topics: []common.Hash{}}
			for _, topic := range l.Topics {
				v.Topics = append(v.Topics, common.HexToHash(topic))
			}
			out.Logs = append(out.Logs, v)
		}
		out.Bloom = types.CreateBloom(out)
		r.LogsBloom = "0x" + fmt.Sprintf("%x", out.Bloom[:])
		encoded[i] = out
	}
	block.ReceiptsRoot = types.DeriveSha(encoded, trie.NewStackTrie(nil)).Hex()
}

func TestReadCompleteRejectsMatchingTailOmissionsFromEveryProvider(t *testing.T) {
	for _, rebloom := range []bool{false, true} {
		t.Run(fmt.Sprint(rebloom), func(t *testing.T) {
			primary, witness := newReceiptWitnessRPC(), newReceiptWitnessRPC()
			primary.bad = "drop-final-log"
			witness.bad = "drop-final-log"
			if rebloom {
				primary.bad = "drop-final-log-rebloom"
				witness.bad = "drop-final-log-rebloom"
			}
			a, b := primary.serve(t), witness.serve(t)
			adapter := witnessAdapter(a.URL, b.URL)
			_, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
			want := "RECEIPT_BLOOM_MISMATCH"
			if rebloom {
				want = "RECEIPT_ROOT_MISMATCH"
			}
			if err == nil || err.Error() != want {
				t.Fatalf("got %v, want %s", err, want)
			}
			if len(pools) > 0 || len(facts) > 0 || primary.stateReads.Load() != 0 || witness.stateReads.Load() != 0 {
				t.Fatal("unqualified receipt data escaped")
			}
		})
	}
}
func TestReceiptRootLegacyTypeOmissionAndGasOrder(t *testing.T) {
	vectors := receiptRootVectors(t)
	vector := vectors[1]
	b, r := bindReceiptVector(vector.Root, vector.Receipts)
	r[0].Type = ""
	if err := VerifyReceiptRoot(context.Background(), b, r); err != nil {
		t.Fatal(err)
	}
	vector = vectors[7] // two receipts
	for _, gas := range []string{"0x5208", "0x1"} {
		b, r = bindReceiptVector(vector.Root, vector.Receipts)
		r[1].CumulativeGasUsed = gas
		if err := VerifyReceiptRoot(context.Background(), b, r); err == nil || err.Error() != "RECEIPT_CUMULATIVE_GAS_ORDER" {
			t.Fatal(err)
		}
	}
}
func TestReadCompleteRootAnchorChangesNeverPublish(t *testing.T) {
	primary, witness := newReceiptWitnessRPC(), newReceiptWitnessRPC()
	witness.bad = "root-after-receipts"
	a, b := primary.serve(t), witness.serve(t)
	adapter := witnessAdapter(a.URL, b.URL)
	if _, _, _, err := adapter.ReadComplete(context.Background(), "0x64"); err == nil || err.Error() != "RECEIPT_PROVIDER_DISAGREEMENT" {
		t.Fatal(err)
	}
	primary = newReceiptWitnessRPC()
	primary.bad = "root-after-receipts"
	a = primary.serve(t)
	adapter = witnessAdapter(a.URL)
	if _, _, _, err := adapter.ReadComplete(context.Background(), "0x64"); err == nil || err.Error() != "ANCHOR_REORGED" {
		t.Fatal(err)
	}
}

func TestReceiptRootRejectsExplicitEmptyOrNullTypeFromJSON(t *testing.T) {
	for _, value := range []any{"", nil, 0, true, []string{"0x0"}} {
		vector := receiptRootVectors(t)[1] // valid legacy receipt
		block, receipts := bindReceiptVector(vector.Root, vector.Receipts)
		raw, _ := json.Marshal(receipts[0])
		var object map[string]any
		if err := json.Unmarshal(raw, &object); err != nil {
			t.Fatal(err)
		}
		object["type"] = value
		raw, _ = json.Marshal(object)
		if err := json.Unmarshal(raw, &receipts[0]); err != nil {
			t.Fatal(err)
		}
		if err := VerifyReceiptRoot(context.Background(), block, receipts); err == nil || err.Error() != "RECEIPT_TYPE_UNSUPPORTED" {
			t.Errorf("type=%v got %v", value, err)
		}
	}
}

func TestReceiptRootRejectsExplicitAmbiguousOutcomeFromJSON(t *testing.T) {
	for _, tc := range []struct {
		vector int
		key    string
		value  any
	}{
		{1, "root", nil}, {1, "root", ""},
		{6, "status", nil}, {6, "status", ""},
	} {
		vector := receiptRootVectors(t)[tc.vector]
		block, receipts := bindReceiptVector(vector.Root, vector.Receipts)
		raw, _ := json.Marshal(receipts[0])
		var object map[string]any
		if err := json.Unmarshal(raw, &object); err != nil {
			t.Fatal(err)
		}
		object[tc.key] = tc.value
		raw, _ = json.Marshal(object)
		if err := json.Unmarshal(raw, &receipts[0]); err != nil {
			t.Fatal(err)
		}
		if err := VerifyReceiptRoot(context.Background(), block, receipts); err == nil || err.Error() != "RECEIPT_OUTCOME_INVALID" {
			t.Errorf("%s=%v got %v", tc.key, tc.value, err)
		}
	}
}
