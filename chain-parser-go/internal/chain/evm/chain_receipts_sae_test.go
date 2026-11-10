package evm

import (
	"context"
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

func nativeProofTestLimits() ReceiptProofLimits {
	return ReceiptProofLimits{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}
}
func retainedSAEProof(t *testing.T) (RawBlock, SAEExecutionProof) {
	t.Helper()
	v := loadChainReceiptVector(t, "avalanche-current")
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "avalanche-current", "settlement.json"))
	if err != nil {
		t.Fatal(err)
	}
	var data struct {
		Headers []json.RawMessage
		Window  []ExecutionReceiptBlock
	}
	if err = json.Unmarshal(raw, &data); err != nil {
		t.Fatal(err)
	}
	raw, err = os.ReadFile(filepath.Join(chainReceiptVectors, "avalanche-current", "body.json"))
	if err != nil {
		t.Fatal(err)
	}
	var body struct {
		GenesisHash     string `json:"genesis_hash"`
		RawTransactions []struct {
			Request  struct{ Params []string }
			Response struct{ Result string }
		} `json:"raw_transactions"`
	}
	if err = json.Unmarshal(raw, &body); err != nil {
		t.Fatal(err)
	}
	byHash := map[string]string{}
	for _, tx := range body.RawTransactions {
		byHash[tx.Request.Params[0]] = tx.Response.Result
	}
	for i := range data.Window {
		var block RawBlock
		if err = json.Unmarshal(data.Window[i].Header, &block); err != nil {
			t.Fatal(err)
		}
		for _, hash := range block.Transactions {
			data.Window[i].RawTransactions = append(data.Window[i].RawTransactions, byHash[hash])
		}
	}
	return v.Block, SAEExecutionProof{ChainKey: v.ChainKey, GenesisHash: body.GenesisHash, Headers: data.Headers, Window: data.Window}
}
func TestSAEActualSettlementProvesCurrentExecution(t *testing.T) {
	target, proof := retainedSAEProof(t)
	coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if !coverage.CanonicalReceiptsProven || !coverage.WholeBlockReceiptEventsProven || len(coverage.CanonicalReceipts) != 7 {
		t.Fatal(coverage)
	}
	if coverage.StateAnchor == nil || coverage.StateAnchor.Height != 99642059 || coverage.StateAnchor.BlockHash != target.Hash || coverage.StateAnchor.SettlingHeight != 99642066 || coverage.StateAnchor.StateRoot == "" {
		t.Fatal(coverage.StateAnchor)
	}
	var current struct{ StateRoot string }
	json.Unmarshal(proof.Headers[0], &current)
	if coverage.StateAnchor.StateRoot == current.StateRoot {
		t.Fatal("used stale own-header state root instead of settling state root")
	}
	if err := VerifyChainReceiptRoot(context.Background(), proof.ChainKey, target, coverage.CanonicalReceipts); err == nil {
		t.Fatal("old own-header authority must remain unsupported-before")
	}
}
func TestSAERejectsIncompleteOrTamperedSettlement(t *testing.T) {
	for _, fault := range []string{"receipt", "log", "raw", "drop-receipt", "drop-successor", "state-root", "settled-height", "duplicate-window", "identity", "budget", "deadline"} {
		t.Run(fault, func(t *testing.T) {
			target, proof := retainedSAEProof(t)
			limits := nativeProofTestLimits()
			ctx := context.Background()
			switch fault {
			case "receipt":
				proof.Window[0].Receipts[0].Status = "0x0"
			case "log":
				found := false
				for i := range proof.Window[0].Receipts {
					if len(proof.Window[0].Receipts[i].Logs) > 0 {
						proof.Window[0].Receipts[i].Logs[0].Data = "0xdead"
						found = true
						break
					}
				}
				if !found {
					t.Fatal("sample lacks logs")
				}
			case "raw":
				proof.Window[0].RawTransactions[0] = "0x01"
			case "drop-receipt":
				proof.Window[0].Receipts = proof.Window[0].Receipts[:6]
			case "drop-successor":
				proof.Headers = proof.Headers[:len(proof.Headers)-1]
			case "state-root", "settled-height":
				field := "stateRoot"
				value := "0x" + strings.Repeat("11", 32)
				if fault == "settled-height" {
					field = "settledHeight"
					value = "0x5f06aca"
				}
				var h map[string]json.RawMessage
				json.Unmarshal(proof.Headers[len(proof.Headers)-1], &h)
				h[field], _ = json.Marshal(value)
				proof.Headers[len(proof.Headers)-1], _ = json.Marshal(h)
			case "duplicate-window":
				proof.Window = append(proof.Window, proof.Window[0])
			case "identity":
				target.Hash = "0x" + strings.Repeat("11", 32)
			case "budget":
				limits.MaxBytes = 256
			case "deadline":
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			}
			if _, err := VerifySAEExecutionProof(ctx, target, proof, limits); err == nil {
				t.Fatal("accepted broken SAE proof", fault)
			}
		})
	}
}

func retainedFreshSAEProof(t *testing.T) (RawBlock, SAEExecutionProof, json.RawMessage) {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "avalanche-current", "settlement-fresh.json"))
	if err != nil {
		t.Fatal(err)
	}
	var data struct {
		ChainKey     string `json:"chain_key"`
		GenesisHash  string `json:"genesis_hash"`
		TargetHeight uint64 `json:"target_height"`
		Headers      []json.RawMessage
		Window       []ExecutionReceiptBlock
		StateProof   json.RawMessage `json:"state_proof"`
	}
	if err = json.Unmarshal(raw, &data); err != nil {
		t.Fatal(err)
	}
	var target RawBlock
	for _, block := range data.Window {
		var b RawBlock
		json.Unmarshal(block.Header, &b)
		n, _ := canonicalIndex(b.Number)
		if n == data.TargetHeight {
			target = b
		}
	}
	return target, SAEExecutionProof{ChainKey: data.ChainKey, GenesisHash: data.GenesisHash, Headers: data.Headers, Window: data.Window}, data.StateProof
}
func TestSAEFreshPublicSettlementAndAccountProof(t *testing.T) {
	target, proof, state := retainedFreshSAEProof(t)
	coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	anchor, err := coverage.TargetStateAnchor()
	if err != nil {
		t.Fatal(err)
	}
	account, err := VerifyExecutionAccountProof(context.Background(), *anchor, state, 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	if !account.Exists || !strings.EqualFold(account.Address, "0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7") || account.StateAnchor.BlockHash != target.Hash {
		t.Fatal(account)
	}
}
func TestSAEAccountProofRejectsRPCOrAnchorTampering(t *testing.T) {
	for _, fault := range []string{"balance", "address", "state-root", "node", "budget"} {
		t.Run(fault, func(t *testing.T) {
			target, proof, raw := retainedFreshSAEProof(t)
			coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
			if err != nil {
				t.Fatal(err)
			}
			anchor, _ := coverage.TargetStateAnchor()
			var fields map[string]json.RawMessage
			json.Unmarshal(raw, &fields)
			maxBytes := 1 << 20
			switch fault {
			case "balance":
				fields["balance"] = json.RawMessage(`"0x12345"`)
			case "address":
				fields["address"] = json.RawMessage(`"0x0000000000000000000000000000000000000000"`)
			case "state-root":
				anchor.StateRoot = "0x" + strings.Repeat("11", 32)
			case "node":
				var nodes []string
				json.Unmarshal(fields["accountProof"], &nodes)
				nodes[0] = "0x01"
				fields["accountProof"], _ = json.Marshal(nodes)
			case "budget":
				maxBytes = 64
			}
			raw, _ = json.Marshal(fields)
			if _, err := VerifyExecutionAccountProof(context.Background(), *anchor, raw, maxBytes); err == nil {
				t.Fatal("accepted tampered state proof", fault)
			}
		})
	}
}

func TestSAEActualMultiBlockWindowDoesNotMislabelStateHeight(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "avalanche-current", "settlement-multi.json"))
	if err != nil {
		t.Fatal(err)
	}
	var proof SAEExecutionProof
	if err = json.Unmarshal(raw, &proof); err != nil {
		t.Fatal(err)
	}
	var target RawBlock
	json.Unmarshal(proof.Window[0].Header, &target)
	coverage, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if coverage.WindowStart != 99642060 || coverage.WindowEnd != 99642061 || len(coverage.CanonicalReceipts) != 2 || coverage.StateAnchor.Height != 99642061 || coverage.StateAnchor.SettlingHeight != 99642068 {
		t.Fatal(coverage)
	}
	if _, err := coverage.TargetStateAnchor(); err == nil {
		t.Fatal("state after window end was incorrectly assigned to earlier target")
	}
	if err := coverage.RequireWholeBlockEvents(); err != nil {
		t.Fatal("receipt events are proved for earlier target even when intermediate state root is not", err)
	}
	proof.Window = proof.Window[:1]
	if _, err := VerifySAEExecutionProof(context.Background(), target, proof, nativeProofTestLimits()); err == nil {
		t.Fatal("accepted incomplete flattened receipt window")
	}
}
