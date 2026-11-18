package evm

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/rlp"
	"xone/chain-parser-go/internal/config"
)

func retainedOPRegolithEvidence(t *testing.T) (RawBlock, NativeReceiptEvidence) {
	t.Helper()
	v := loadChainReceiptVector(t, "optimism-regolith")
	var body struct {
		Transactions []struct{ Response struct{ Result string } } `json:"raw_transactions"`
	}
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "optimism-regolith", "body.json"))
	if err != nil || json.Unmarshal(raw, &body) != nil {
		t.Fatal("retained OP body", err)
	}
	header, err := VerifyChainHeaderJSON(context.Background(), v.ChainKey, retainedChainHeader(t, "optimism-regolith"))
	if err != nil {
		t.Fatal(err)
	}
	sem, err := ResolveNativeSemantics(v.ChainKey, header)
	if err != nil {
		t.Fatal(err)
	}
	genesis, _ := knownBodyGenesis(v.ChainKey)
	block := ExecutionReceiptBlock{Header: retainedChainHeader(t, "optimism-regolith"), Receipts: v.Receipts}
	for _, tx := range body.Transactions {
		block.RawTransactions = append(block.RawTransactions, tx.Response.Result)
	}
	return v.Block, NativeReceiptEvidence{SchemaVersion: 1, ChainKey: v.ChainKey, GenesisHash: genesis, NativeSemanticsID: sem.ID, Kind: "own-header-receipts", Block: &block}
}

// Construct only a JSON view of the retained signed wires. The normal reader
// reconstructs exactly those bytes and checks their existing txIDs/body root.
func normalOPFixture(t *testing.T) *normalNativeFixture {
	t.Helper()
	block, evidence := retainedOPRegolithEvidence(t)
	var header map[string]json.RawMessage
	json.Unmarshal(evidence.Block.Header, &header)
	transactions := []json.RawMessage{}
	for i, wire := range evidence.Block.RawTransactions {
		encoded := common.FromHex(wire)
		fields := map[string]any{}
		if encoded[0] == 0x7e {
			d, err := decodeOPDeposit(encoded)
			if err != nil {
				t.Fatal(err)
			}
			fields = map[string]any{"type": "0x7e", "sourceHash": d.SourceHash.Hex(), "from": d.From.Hex(), "to": common.BytesToAddress(d.To).Hex(), "mint": fmt.Sprintf("0x%x", d.Mint), "value": fmt.Sprintf("0x%x", d.Value), "gas": fmt.Sprintf("0x%x", d.Gas), "isSystemTx": d.System, "input": "0x" + hex.EncodeToString(d.Data)}
		} else {
			var tx types.Transaction
			if err := tx.UnmarshalBinary(encoded); err != nil {
				t.Fatal(err)
			}
			raw, _ := tx.MarshalJSON()
			json.Unmarshal(raw, &fields)
		}
		fields["hash"], fields["blockHash"], fields["blockNumber"], fields["transactionIndex"] = block.Transactions[i], block.Hash, block.Number, fmt.Sprintf("0x%x", i)
		raw, _ := json.Marshal(fields)
		transactions = append(transactions, raw)
	}
	header["transactions"], _ = json.Marshal(transactions)
	full, _ := json.Marshal(header)
	chain := config.Chain{Key: evidence.ChainKey, Genesis: evidence.GenesisHash, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	return &normalNativeFixture{chain: chain, target: block, headers: map[string]json.RawMessage{block.Number: evidence.Block.Header}, full: map[string]json.RawMessage{block.Number: full}, receipts: map[string][]Receipt{block.Hash: evidence.Block.Receipts}, observed: block.Number}
}

func TestOPRegolithNormalSourceAndHistoryReplayBindPublicSignedBody(t *testing.T) {
	f := normalOPFixture(t)
	a := f.adapter(t)
	var observed *OPRegolithFeeInputs
	a.OnComplete = func(ctx context.Context, b RawBlock, _ []Receipt) error {
		if b.NativeCoverage == nil || b.NativeCoverage.OPRegolithFeeInputs == nil {
			t.Fatal("normal callback lost fee inputs")
		}
		observed = b.NativeCoverage.OPRegolithFeeInputs
		if observed.DepositHash != f.target.Transactions[0] || observed.BlockHash != f.target.Hash || observed.Authority != "RPC_HEADER_BODY_COMMITMENT_NOT_CONSENSUS" || len(observed.UserTransactions) != 3 {
			t.Fatal(observed)
		}
		for i, tx := range observed.UserTransactions {
			if tx.Index != uint64(i+1) || tx.Hash != f.target.Transactions[i+1] || tx.Sender == "" {
				t.Fatal(tx)
			}
		}
		return nil
	}
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "op-baseline")
	if _, _, _, err := a.ReadComplete(ctx, "latest"); err != nil {
		t.Fatal(err)
	}
	if observed == nil || f.fullReads.Load() != 2 || f.stateReads.Load() != 0 {
		t.Fatal("not normal committed pipeline", f.fullReads.Load())
	}
	var archive NativeHistoryEvidence
	a.OnHistoryEvidence = func(ctx context.Context, e NativeHistoryEvidence) error {
		archive = e
		_, _, err := VerifyNativeHistoryEvidence(ctx, a.Chain, e)
		return err
	}
	if _, _, err := a.ReadFactsAndCatalog(context.WithValue(context.Background(), nativeDutyKey{}, "op-history"), f.target.Number); err != nil {
		t.Fatal(err)
	}
	if archive.OPRegolithFeeInputs == nil {
		t.Fatal("history lost fee inputs")
	}
	expected, _ := json.Marshal(observed)
	actual, _ := json.Marshal(archive.OPRegolithFeeInputs)
	if string(expected) != string(actual) {
		t.Fatal("history differs from normal source")
	}
	for _, fault := range []string{"attributes", "chain", "block", "hash", "wire", "sender", "index", "authority", "deposit", "deposit-source", "root", "time"} {
		t.Run("projection-"+fault, func(t *testing.T) {
			encoded, _ := json.Marshal(archive)
			var changed NativeHistoryEvidence
			json.Unmarshal(encoded, &changed)
			p := changed.OPRegolithFeeInputs
			switch fault {
			case "attributes":
				p.AttributesCalldata = "0x00"
			case "chain":
				p.ChainKey = "evm:8453"
			case "block":
				p.BlockHash = "0x" + strings.Repeat("11", 32)
			case "hash":
				p.UserTransactions[0].Hash = p.DepositHash
			case "wire":
				p.UserTransactions[0].EncodedTransaction = "0x02"
			case "sender":
				p.UserTransactions[0].Sender = "0x" + strings.Repeat("11", 20)
			case "index":
				p.UserTransactions[0].Index = 0
			case "authority":
				p.Authority = "CONSENSUS"
			case "deposit":
				p.DepositHash = p.UserTransactions[0].Hash
			case "deposit-source":
				p.DepositSourceHash = "0x" + strings.Repeat("11", 32)
			case "root":
				p.TransactionsRoot = "0x" + strings.Repeat("11", 32)
			case "time":
				p.ExecutionTimestamp++
			}
			if _, _, err := VerifyNativeHistoryEvidence(context.Background(), a.Chain, changed); err == nil || !strings.Contains(err.Error(), "HISTORY_OP_FEE_INPUT_PROJECTION_MISMATCH") {
				t.Fatal("accepted mutated projection", err)
			}
		})
	}
	archive.OPRegolithFeeInputs = nil
	if _, _, err := VerifyNativeHistoryEvidence(context.Background(), a.Chain, archive); err != nil {
		t.Fatal("old archive compatibility", err)
	}
	if path := os.Getenv("XONE_OP_L1_INPUT_OUTPUT"); path != "" {
		if err := os.MkdirAll(path, 0755); err != nil {
			t.Fatal(err)
		}
		raw, err := json.MarshalIndent(map[string]any{"passed": true, "inputs": observed, "production_ready": false, "fee_arithmetic_performed_by_parser": false, "old_archive_compatible": true, "projection_mutations_rejected": 12}, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err = os.WriteFile(filepath.Join(path, "normal-op-fee-inputs.json"), append(raw, '\n'), 0644); err != nil {
			t.Fatal(err)
		}
	}
}

func TestOPRegolithReplayRejectsRawBodyCorruptionCancellationAndBudget(t *testing.T) {
	for _, fault := range []string{"attributes-raw", "user-raw", "missing-deposit", "wrong-genesis", "canceled", "budget"} {
		t.Run(fault, func(t *testing.T) {
			block, e := retainedOPRegolithEvidence(t)
			limits := nativeProofTestLimits()
			ctx := context.Background()
			switch fault {
			case "attributes-raw":
				e.Block.RawTransactions[0] += "00"
			case "user-raw":
				e.Block.RawTransactions[1] += "00"
			case "missing-deposit":
				e.Block.RawTransactions = e.Block.RawTransactions[1:]
			case "wrong-genesis":
				e.GenesisHash = "0x" + strings.Repeat("11", 32)
			case "canceled":
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			case "budget":
				limits.MaxBytes = 2048
			}
			if _, err := VerifyNativeReceiptEvidence(ctx, "evm:10", block, e, limits); err == nil {
				t.Fatal("accepted source fault")
			}
		})
	}
}

func TestOPRegolithArchiveWhitespaceCannotChangeHeaderInputCommitment(t *testing.T) {
	block, evidence := retainedOPRegolithEvidence(t)
	var pretty bytes.Buffer
	if err := json.Indent(&pretty, evidence.Block.Header, "", "  "); err != nil {
		t.Fatal(err)
	}
	evidence.Block.Header = pretty.Bytes()
	before, err := VerifyNativeReceiptEvidence(context.Background(), "evm:10", block, evidence, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	encoded, _ := json.Marshal(evidence)
	var restored NativeReceiptEvidence
	if err = json.Unmarshal(encoded, &restored); err != nil {
		t.Fatal(err)
	}
	after, err := VerifyNativeReceiptEvidence(context.Background(), "evm:10", block, restored, nativeProofTestLimits())
	if err != nil {
		t.Fatal(err)
	}
	left, _ := json.Marshal(before.OPRegolithFeeInputs)
	right, _ := json.Marshal(after.OPRegolithFeeInputs)
	if string(left) != string(right) {
		t.Fatal("archive JSON whitespace changed derived fee inputs")
	}
}

func TestOPRegolithNormalSourceRejectsChangedBodyAndFinalAnchorBeforePublication(t *testing.T) {
	for _, fault := range []string{"body", "final-anchor"} {
		t.Run(fault, func(t *testing.T) {
			f := normalOPFixture(t)
			if fault == "body" {
				var fields map[string]json.RawMessage
				json.Unmarshal(f.full[f.target.Number], &fields)
				var transactions []map[string]json.RawMessage
				json.Unmarshal(fields["transactions"], &transactions)
				transactions[0]["input"] = json.RawMessage(`"0x00"`)
				fields["transactions"], _ = json.Marshal(transactions)
				f.full[f.target.Number], _ = json.Marshal(fields)
			} else {
				f.corruptSettling.Store(true)
			}
			a := f.adapter(t)
			published := 0
			a.OnComplete = func(context.Context, RawBlock, []Receipt) error { published++; return nil }
			if _, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "op-bad-source"), "latest"); err == nil || published != 0 {
				t.Fatal("changed source reached publication", err, published)
			}
		})
	}
}

func TestOPRegolithProjectionChecksDepositIdentityABIAndUserSignature(t *testing.T) {
	for _, fault := range []string{"from", "to", "value", "mint", "system", "selector", "size", "l1-time", "number-width", "sequence-width", "source-hash", "l1-hash", "sequence", "user-signature", "budget"} {
		t.Run(fault, func(t *testing.T) {
			block, e := retainedOPRegolithEvidence(t)
			ctx := context.Background()
			header, err := VerifyChainHeaderJSON(ctx, "evm:10", e.Block.Header)
			if err != nil {
				t.Fatal(err)
			}
			d, err := decodeOPDeposit(common.FromHex(e.Block.RawTransactions[0]))
			if err != nil {
				t.Fatal(err)
			}
			switch fault {
			case "from":
				d.From = common.Address{}
			case "to":
				d.To = make([]byte, 20)
			case "value":
				d.Value.SetUint64(1)
			case "mint":
				d.Mint.SetUint64(1)
			case "system":
				d.System = true
			case "selector":
				d.Data[0] = 0xff
			case "size":
				d.Data = append(d.Data, 0)
			case "l1-time":
				d.Data[36] = 1
			case "number-width":
				d.Data[4] = 1
			case "sequence-width":
				d.Data[132] = 1
			case "source-hash":
				d.SourceHash[31] ^= 1
			case "l1-hash":
				d.Data[100] ^= 1
			case "sequence":
				d.Data[163] ^= 1
			}
			if fault != "user-signature" && fault != "budget" {
				encoded, _ := rlp.EncodeToBytes(d)
				e.Block.RawTransactions[0] = "0x7e" + hex.EncodeToString(encoded)
			}
			if fault == "user-signature" {
				var tx types.Transaction
				tx.UnmarshalBinary(common.FromHex(e.Block.RawTransactions[1]))
				bad := types.NewTx(&types.LegacyTx{Nonce: tx.Nonce(), GasPrice: tx.GasPrice(), Gas: tx.Gas(), To: tx.To(), Value: tx.Value(), Data: tx.Data()})
				encoded, _ := bad.MarshalBinary()
				e.Block.RawTransactions[1] = "0x" + hex.EncodeToString(encoded)
				header.Block.Transactions[1] = bad.Hash().Hex()
			}
			coverage := VerifiedReceiptCoverage{ChainKey: "evm:10", Block: block, CanonicalBodyProven: true, CanonicalReceiptsProven: true, NativeSemanticsID: e.NativeSemanticsID}
			budget := receiptProofBudget{remaining: 16 << 20}
			if fault == "budget" {
				budget.remaining = 1
			}
			if _, err = projectOPRegolithFeeInputs(ctx, coverage, header, *e.Block, &budget); err == nil {
				t.Fatal("accepted invalid fee input")
			}
		})
	}
}

func TestOPRegolithSourceHashMatchesPinnedOriginalOptimism(t *testing.T) {
	path := os.Getenv("XONE_OP_DEPOSIT_SOURCE_VECTORS")
	if path == "" {
		t.Skip("current original upstream source-hash vectors required")
	}
	var doc struct {
		Cases []struct {
			L1Hash     string `json:"l1_hash"`
			Sequence   string `json:"sequence"`
			SourceHash string `json:"source_hash"`
		} `json:"cases"`
	}
	raw, err := os.ReadFile(path)
	if err != nil || json.Unmarshal(raw, &doc) != nil || len(doc.Cases) < 24 {
		t.Fatal("current source-hash vectors", err)
	}
	for _, row := range doc.Cases {
		var sequence uint64
		if _, err = fmt.Sscan(row.Sequence, &sequence); err != nil {
			t.Fatal(err)
		}
		if got := opL1InfoSourceHash(common.HexToHash(row.L1Hash), sequence).Hex(); got != row.SourceHash {
			t.Fatal("original source-hash mismatch", row, got)
		}
	}
}
