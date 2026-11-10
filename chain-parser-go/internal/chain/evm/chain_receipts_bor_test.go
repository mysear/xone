package evm

import (
	"context"
	"encoding/json"
	"strings"
	"testing"
)

func retainedBorProof(t *testing.T) (RawBlock, []Receipt, BorReceiptProof) {
	t.Helper()
	v := loadChainReceiptVector(t, "polygon-synthetic")
	var full struct{ Transactions []json.RawMessage }
	json.Unmarshal(retainedFullChainBlock(t, "polygon-synthetic"), &full)
	genesis, _ := knownBodyGenesis(v.ChainKey)
	return v.Block, v.Receipts, BorReceiptProof{ChainKey: v.ChainKey, GenesisHash: genesis, Header: retainedChainHeader(t, "polygon-synthetic"), TransactionsJSON: full.Transactions}
}
func TestBorActualCanonicalCoverageSeparatesSystemObservations(t *testing.T) {
	block, receipts, proof := retainedBorProof(t)
	coverage, err := VerifyBorReceiptCoverage(context.Background(), block, receipts, proof, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	if !coverage.CanonicalBodyProven || !coverage.CanonicalReceiptsProven || len(coverage.CanonicalReceipts) != 389 || len(coverage.SystemObservations) != 1 {
		t.Fatal(coverage)
	}
	if len(coverage.SystemObservations[0].Receipt.Logs) != 41 || coverage.SystemObservations[0].LogsContentCommitted || coverage.WholeBlockReceiptEventsProven {
		t.Fatal("uncommitted system logs were qualified", coverage)
	}
	if coverage.RequireWholeBlockEvents() == nil {
		t.Fatal("whole-block business events must remain unqualified")
	}
	selection, err := coverage.SelectBusinessLogs([]string{"0x28c96a719edc7c1ba3fb1bafcadf470ace9dfd75"}, CanonicalTransactionEvents)
	if err != nil || len(selection.Logs) == 0 || !selection.Qualified {
		t.Fatal("unrelated system observations disabled proven canonical scope", selection, err)
	}
	if _, err := coverage.SelectBusinessLogs([]string{"0x1fdeaf938267ca43388ed1fdb879eaf91e920c7a"}, CanonicalTransactionEvents); err == nil {
		t.Fatal("business-relevant unproved system events must stay unqualified even when selecting canonical events")
	}
	if _, err := coverage.SelectBusinessLogs([]string{"0x1fdeaf938267ca43388ed1fdb879eaf91e920c7a"}, WholeBlockEvents); err == nil {
		t.Fatal("system pool/factory event cannot qualify without independent proof")
	}
	if err := VerifyChainReceiptRoot(context.Background(), proof.ChainKey, block, receipts); err == nil {
		t.Fatal("old strict whole-set rejection must remain unsupported-before")
	}
}
func TestBorCoverageCannotSilentlyQualifyUncommittedChanges(t *testing.T) {
	for _, fault := range []string{"system-data", "drop-system-observation", "canonical-data", "canonical-tail", "arbitrary-tail"} {
		t.Run(fault, func(t *testing.T) {
			block, rs, proof := retainedBorProof(t)
			switch fault {
			case "system-data":
				rs[len(rs)-1].Logs[0].Data = "0xdead"
			case "drop-system-observation":
				rs = rs[:len(rs)-1]
				block.Transactions = block.Transactions[:len(block.Transactions)-1]
				proof.TransactionsJSON = proof.TransactionsJSON[:len(proof.TransactionsJSON)-1]
				var h map[string]json.RawMessage
				json.Unmarshal(proof.Header, &h)
				h["transactions"], _ = json.Marshal(block.Transactions)
				proof.Header, _ = json.Marshal(h)
			case "canonical-data":
				if rs[0].Status == "0x0" {
					rs[0].Status = "0x1"
				} else {
					rs[0].Status = "0x0"
				}
			case "canonical-tail":
				rs = rs[:len(rs)-2]
			case "arbitrary-tail":
				fake := "0x" + strings.Repeat("ab", 32)
				block.Transactions[len(block.Transactions)-1] = fake
				rs[len(rs)-1].TxHash = fake
				for i := range rs[len(rs)-1].Logs {
					rs[len(rs)-1].Logs[i].TxHash = fake
				}
				var h map[string]json.RawMessage
				json.Unmarshal(proof.Header, &h)
				h["transactions"], _ = json.Marshal(block.Transactions)
				proof.Header, _ = json.Marshal(h)
			}
			coverage, err := VerifyBorReceiptCoverage(context.Background(), block, rs, proof, nativeProofTestLimits())
			if fault == "system-data" || fault == "drop-system-observation" {
				if err != nil {
					t.Fatal(err)
				}
				if coverage.WholeBlockReceiptEventsProven || coverage.RequireWholeBlockEvents() == nil {
					t.Fatal("RPC-only change/omission upgraded authority")
				}
			} else if err == nil {
				t.Fatal("accepted canonical membership mutation", fault)
			}
		})
	}
}
