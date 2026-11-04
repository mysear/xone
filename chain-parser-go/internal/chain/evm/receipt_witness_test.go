package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync/atomic"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

type receiptWitnessRPC struct {
	block        RawBlock
	receipts     []Receipt
	bad          string
	witnessReads atomic.Int32
	stateReads   atomic.Int32
	blockReads   atomic.Int32
}

func newReceiptWitnessRPC() *receiptWitnessRPC {
	block, receipts := completeReceiptFixture()
	for i := range block.Transactions {
		block.Transactions[i] = "0x" + strings.Repeat([]string{"a1", "b2", "c3"}[i], 32)
	}
	for i := range receipts {
		receipts[i].TxHash = block.Transactions[i]
		for j := range receipts[i].Logs {
			receipts[i].Logs[j].TxHash = block.Transactions[i]
		}
		for j := range receipts[i].Logs {
			receipts[i].Logs[j].Address = "0x" + strings.Repeat("a", 40)
			receipts[i].Logs[j].Topics = []string{"0x" + strings.Repeat("b", 64)}
			receipts[i].Logs[j].Data = "0xabcdef"
		}
	}
	return &receiptWitnessRPC{block: block, receipts: receipts}
}
func (f *receiptWitnessRPC) serve(t *testing.T) *httptest.Server {
	t.Helper()
	sealReceiptTestFixture(&f.block, f.receipts)
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var req struct {
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		if json.NewDecoder(r.Body).Decode(&req) != nil {
			t.Error("invalid request")
			return
		}
		var result any
		switch req.Method {
		case "eth_chainId":
			result = "0x7a69"
			if f.bad == "wrong-chain" {
				result = "0x1"
			}
		case "eth_getBlockByNumber":
			n := f.blockReads.Add(1)
			block := f.block
			switch f.bad {
			case "block-number":
				block.Number = "0x65"
			case "block-parent":
				block.ParentHash = "other"
			case "block-time":
				block.Timestamp = "0x65"
			case "block-transactions":
				block.Transactions = []string{"other"}
			case "root-after-receipts":
				if n > 1 {
					block.ReceiptsRoot = "0x" + strings.Repeat("11", 32)
				}
			case "reorg-after-receipts":
				if n > 1 {
					block.Hash = "other"
				}
			}
			if f.bad == "uppercase-identities" {
				block.Hash = uppercaseWitnessHex(block.Hash)
				block.ParentHash = uppercaseWitnessHex(block.ParentHash)
				block.Transactions = append([]string{}, block.Transactions...)
				for i := range block.Transactions {
					block.Transactions[i] = uppercaseWitnessHex(block.Transactions[i])
				}
			}
			result = block
		case "eth_getTransactionReceipt":
			f.witnessReads.Add(1)
			if f.bad == "unavailable" {
				w.WriteHeader(503)
				return
			}
			var tx string
			json.Unmarshal(req.Params[0], &tx)
			for _, receipt := range f.receipts {
				if strings.EqualFold(receipt.TxHash, tx) {
					// Deep copy fixture state before a fault changes slices.
					raw, _ := json.Marshal(receipt)
					json.Unmarshal(raw, &receipt)
					switch f.bad {
					case "invalid-data-prefix":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Data = "0XAB"
						}
					case "invalid-data-hex":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Data = "0xGG"
						}
					case "invalid-address":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Address = "0x01"
						}
					case "invalid-topic":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Topics = []string{"0x01"}
						}
					case "missing-topics":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Topics = nil
						}
					case "drop-final-log", "drop-final-log-rebloom":
						if f.bad == "drop-final-log-rebloom" && receipt.TxIndex == "0x2" {
							receipt.LogsBloom = "0x" + strings.Repeat("00", 256)
						}
						if strings.EqualFold(tx, f.block.Transactions[2]) {
							receipt.Logs = []Log{}
						}
					case "drop-all-logs":
						receipt.Logs = []Log{}
					case "different-data":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Data = "0x00"
						}
					case "different-topic":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Topics = []string{"0x" + strings.Repeat("c", 64)}
						}
					case "different-address":
						if len(receipt.Logs) > 0 {
							receipt.Logs[0].Address = "0x" + strings.Repeat("c", 40)
						}
					case "missing-logs":
						receipt.Logs = nil
					case "uppercase-identities":
						receipt.BlockHash = uppercaseWitnessHex(receipt.BlockHash)
						receipt.TxHash = uppercaseWitnessHex(receipt.TxHash)
						for i := range receipt.Logs {
							receipt.Logs[i].BlockHash = uppercaseWitnessHex(receipt.Logs[i].BlockHash)
							receipt.Logs[i].TxHash = uppercaseWitnessHex(receipt.Logs[i].TxHash)
						}
					case "uppercase-bytes":
						for i := range receipt.Logs {
							l := &receipt.Logs[i]
							l.Address = "0x" + strings.ToUpper(l.Address[2:])
							l.Data = "0x" + strings.ToUpper(l.Data[2:])
							for j, topic := range l.Topics {
								l.Topics[j] = "0x" + strings.ToUpper(topic[2:])
							}
						}
					}
					result = receipt
					break
				}
			}
		default:
			f.stateReads.Add(1)
			result = "0x0"
		}
		json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
	}))
	t.Cleanup(server.Close)
	return server
}
func witnessAdapter(urls ...string) Adapter {
	return Adapter{Client: rpc.New(urls, time.Second, 1<<20), Chain: config.Chain{Key: "evm:31337"}}
}

func TestReadCompleteCrossProviderWitnessRejectsMissingTailAndDivergence(t *testing.T) {
	cases := []struct{ fault, want string }{
		{"drop-final-log", "RECEIPT_WITNESS_INVALID"}, {"drop-all-logs", "RECEIPT_WITNESS_INVALID"},
		{"different-data", "RECEIPT_WITNESS_INVALID"}, {"different-topic", "RECEIPT_WITNESS_INVALID"}, {"different-address", "RECEIPT_WITNESS_INVALID"},
		{"missing-logs", "RECEIPT_WITNESS_INVALID"}, {"invalid-data-prefix", "RECEIPT_WITNESS_INVALID"}, {"invalid-data-hex", "RECEIPT_WITNESS_INVALID"}, {"invalid-address", "RECEIPT_WITNESS_INVALID"}, {"invalid-topic", "RECEIPT_WITNESS_INVALID"}, {"missing-topics", "RECEIPT_WITNESS_INVALID"}, {"unavailable", "RECEIPT_WITNESS_UNAVAILABLE"},
		{"block-number", "RECEIPT_PROVIDER_DISAGREEMENT"}, {"block-parent", "RECEIPT_PROVIDER_DISAGREEMENT"}, {"block-time", "RECEIPT_PROVIDER_DISAGREEMENT"}, {"block-transactions", "RECEIPT_PROVIDER_DISAGREEMENT"},
		{"reorg-after-receipts", "RECEIPT_PROVIDER_DISAGREEMENT"},
	}
	for _, tc := range cases {
		t.Run(tc.fault, func(t *testing.T) {
			primary, witness := newReceiptWitnessRPC(), newReceiptWitnessRPC()
			witness.bad = tc.fault
			a, b := primary.serve(t), witness.serve(t)
			adapter := witnessAdapter(a.URL, b.URL)
			_, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
			if err == nil || err.Error() != tc.want {
				t.Fatalf("got %v want %s", err, tc.want)
			}
			if len(pools) != 0 || len(facts) != 0 || primary.stateReads.Load() != 0 || witness.stateReads.Load() != 0 {
				t.Fatal("unqualified evidence reached pool reading")
			}
		})
	}
}
func TestReadCompleteWitnessCatchesPrimaryTailOmissionAndNeverVotes(t *testing.T) {
	for _, primaryBad := range []bool{false, true} {
		t.Run(map[bool]string{false: "two agree one diverges", true: "primary missing tail"}[primaryBad], func(t *testing.T) {
			a, b, c := newReceiptWitnessRPC(), newReceiptWitnessRPC(), newReceiptWitnessRPC()
			if primaryBad {
				a.bad = "drop-final-log"
			} else {
				c.bad = "drop-final-log"
			}
			sa, sb, sc := a.serve(t), b.serve(t), c.serve(t)
			adapter := witnessAdapter(sa.URL, sb.URL, sc.URL)
			_, _, _, err := adapter.ReadComplete(context.Background(), "0x64")
			want := "RECEIPT_WITNESS_INVALID"
			if primaryBad {
				want = "RECEIPT_BLOOM_MISMATCH"
			}
			if err == nil || err.Error() != want {
				t.Fatal(err)
			}
		})
	}
}
func TestReadCompleteWitnessAcceptsExactBytesAcrossHexCaseAndEmptyBlocks(t *testing.T) {
	for _, empty := range []bool{false, true} {
		a, b := newReceiptWitnessRPC(), newReceiptWitnessRPC()
		b.bad = "uppercase-bytes"
		if empty {
			a.block.Transactions = []string{}
			b.block.Transactions = []string{}
			a.receipts = []Receipt{}
			b.receipts = []Receipt{}
		}
		sa, sb := a.serve(t), b.serve(t)
		adapter := witnessAdapter(sa.URL, sb.URL)
		ref, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
		if err != nil || ref.Number != 100 || len(pools) != 0 || len(facts) != 0 {
			t.Fatal(ref, pools, facts, err)
		}
		if b.blockReads.Load() != 2 {
			t.Fatal("witness anchor not rechecked", b.blockReads.Load())
		}
		if !empty && b.witnessReads.Load() != 3 {
			t.Fatal("did not read complete witness receipts", b.witnessReads.Load())
		}
	}
}
func TestReadCompleteSingleEndpointDoesNotPretendToBeTwoSources(t *testing.T) {
	a := newReceiptWitnessRPC()
	sa := a.serve(t)
	adapter := witnessAdapter(sa.URL, sa.URL)
	if _, _, _, err := adapter.ReadComplete(context.Background(), "0x64"); err != nil {
		t.Fatal(err)
	}
	if a.witnessReads.Load() != 3 {
		t.Fatal("duplicate endpoint counted as independent witness", a.witnessReads.Load())
	}
}

func TestReadCompleteWitnessQuarantinedSecondSourceCannotSilentlyDegrade(t *testing.T) {
	primary, wrong := newReceiptWitnessRPC(), newReceiptWitnessRPC()
	wrong.bad = "wrong-chain"
	a, b := primary.serve(t), wrong.serve(t)
	adapter := witnessAdapter(a.URL, b.URL)
	if err := adapter.Client.VerifyEVM(context.Background(), "evm:31337", primary.block.Hash); err != nil {
		t.Fatal(err)
	}
	_, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
	if err == nil || err.Error() != "RECEIPT_WITNESS_SOURCE_REQUIRED" {
		t.Fatal(err)
	}
	if len(pools) != 0 || len(facts) != 0 || wrong.witnessReads.Load() != 0 {
		t.Fatal("quarantined source used for witness")
	}
}

func TestReceiptWitnessDigestNormalizesReceiptOrderWithoutMutation(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	before, _ := json.Marshal(fixture.receipts)
	expected, err := receiptDigest(context.Background(), fixture.block, fixture.receipts)
	if err != nil {
		t.Fatal(err)
	}
	reversed := []Receipt{fixture.receipts[2], fixture.receipts[0], fixture.receipts[1]}
	actual, err := receiptDigest(context.Background(), fixture.block, reversed)
	if err != nil || actual != expected {
		t.Fatal(actual, expected, err)
	}
	after, _ := json.Marshal(fixture.receipts)
	if string(before) != string(after) {
		t.Fatal("digest mutated receipt/log/topic slices")
	}
}

func TestReadCompleteWitnessCancellationRetainsOuterDeadline(t *testing.T) {
	primary := newReceiptWitnessRPC()
	a := primary.serve(t)
	var blocked atomic.Bool
	release := make(chan struct{})
	b := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var request any
		if json.NewDecoder(r.Body).Decode(&request) != nil {
			return
		}
		blocked.Store(true)
		select {
		case <-r.Context().Done():
		case <-release:
		}
	}))
	defer b.Close()
	defer close(release)
	adapter := witnessAdapter(a.URL, b.URL)
	ctx, cancel := context.WithTimeout(context.Background(), 100*time.Millisecond)
	defer cancel()
	started := time.Now()
	_, _, _, err := adapter.ReadComplete(ctx, "0x64")
	if err == nil || err.Error() != "RECEIPT_WITNESS_UNAVAILABLE" || !blocked.Load() {
		t.Fatal(err, blocked.Load())
	}
	if time.Since(started) > time.Second {
		t.Fatal("per-endpoint calls extended the total deadline")
	}
}

func uppercaseWitnessHex(value string) string {
	return "0x" + strings.ToUpper(value[2:])
}
func TestReadCompleteWitnessAcceptsByteIdenticalHashAndTransactionCase(t *testing.T) {
	primary, witness := newReceiptWitnessRPC(), newReceiptWitnessRPC()
	witness.bad = "uppercase-identities"
	a, b := primary.serve(t), witness.serve(t)
	adapter := witnessAdapter(a.URL, b.URL)
	if _, _, _, err := adapter.ReadComplete(context.Background(), "0x64"); err != nil {
		t.Fatal(err)
	}
}

func TestReadCompleteReceiptAggregateBudgetStopsIndividuallySmallResponses(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	for i := range fixture.receipts {
		fixture.receipts[i].Logs = []Log{{BlockHash: fixture.block.Hash, TxHash: fixture.block.Transactions[i], TxIndex: fmt.Sprintf("0x%x", i), Index: fmt.Sprintf("0x%x", i), Address: "0x" + strings.Repeat("a", 40), Topics: []string{}, Data: "0x" + strings.Repeat("ab", 700)}}
		wire, _ := json.Marshal(fixture.receipts[i])
		if len(wire) > 4096 {
			t.Fatal("fixture exceeds per-response bound")
		}
	}
	server := fixture.serve(t)
	adapter := witnessAdapter(server.URL)
	adapter.Client.MaxBytes = 4096
	_, pools, facts, err := adapter.ReadComplete(context.Background(), "0x64")
	if err == nil || err.Error() != "RECEIPT_BLOCK_BUDGET" {
		t.Fatal(err)
	}
	if len(pools) != 0 || len(facts) != 0 || fixture.stateReads.Load() != 0 {
		t.Fatal("over-budget block escaped")
	}
}
func TestValidateReceiptLogEncodingBeforeNormalization(t *testing.T) {
	cases := []struct {
		name   string
		change func(*Log)
	}{
		{"data prefix", func(l *Log) { l.Data = "0XAB" }}, {"data hex", func(l *Log) { l.Data = "0xGG" }},
		{"odd data", func(l *Log) { l.Data = "0x1" }}, {"missing data", func(l *Log) { l.Data = "" }},
		{"short address", func(l *Log) { l.Address = "0x01" }}, {"bad address", func(l *Log) { l.Address = "0x" + strings.Repeat("z", 40) }},
		{"missing topics", func(l *Log) { l.Topics = nil }}, {"short topic", func(l *Log) { l.Topics = []string{"0x01"} }},
		{"too many topics", func(l *Log) {
			l.Topics = make([]string, 5)
			for i := range l.Topics {
				l.Topics[i] = "0x" + strings.Repeat("a", 64)
			}
		}},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			f := newReceiptWitnessRPC()
			tc.change(&f.receipts[0].Logs[0])
			if err := ValidateReceipts(f.block, f.receipts); err == nil || err.Error() != "LOG_ENCODING" {
				t.Fatal(err)
			}
		})
	}
}

func TestReceiptWitnessDigestCanceledWorkStopsBeforeHashing(t *testing.T) {
	fixture := newReceiptWitnessRPC()
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if _, err := receiptDigest(ctx, fixture.block, fixture.receipts); err == nil || err.Error() != "RPC_DEADLINE" {
		t.Fatal(err)
	}
}
