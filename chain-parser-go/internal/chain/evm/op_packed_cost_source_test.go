package evm

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"math/big"
	"os"
	"path/filepath"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"xone/chain-parser-go/internal/config"
)

// The captured response is not trusted just because its digest is fixed. This
// test sends it through the ordinary RPC adapter and history verification.
func TestOPPackedActualPublicNormalBodyReceiptsAndSignedInputs(t *testing.T) {
	directory := os.Getenv("XONE_OP_PACKED_PUBLIC_CAPTURE")
	if directory == "" {
		t.Skip("retained current public capture required")
	}
	read := func(name, expected string) []byte {
		raw, err := os.ReadFile(filepath.Join(directory, name))
		if err != nil || fmt.Sprintf("%x", sha256.Sum256(raw)) != expected {
			t.Fatal("captured input changed", name, err)
		}
		if err = rejectDuplicateReceiptKeys(raw); err != nil {
			t.Fatal(err)
		}
		return raw
	}
	raw := read("full-block.json", "7dea5cbffa43bea10eecf8218708238c078af5fc131b82e3cdd04e9f45ee94ef")
	var envelope struct {
		Response struct{ Result json.RawMessage }
	}
	if err := json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	vectorRaw := read("vector.json", "68cd47b21cdaa542e92d53b01c6cdeaaa3a43860244e5cbfa9dd064e703ae00c")
	var vector publicReceiptVector
	if err := json.Unmarshal(vectorRaw, &vector); err != nil {
		t.Fatal(err)
	}
	if vector.ChainKey != "evm:10" || len(vector.Block.Transactions) != 50 || len(vector.Receipts) != 50 {
		t.Fatal("unexpected captured scope")
	}
	var full map[string]json.RawMessage
	json.Unmarshal(envelope.Response.Result, &full)
	full["transactions"], _ = json.Marshal(vector.Block.Transactions)
	header, _ := json.Marshal(full)
	genesis, _ := knownBodyGenesis(vector.ChainKey)
	chain := config.Chain{Key: vector.ChainKey, Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	f := &normalNativeFixture{chain: chain, target: vector.Block, headers: map[string]json.RawMessage{vector.Block.Number: header}, full: map[string]json.RawMessage{vector.Block.Number: envelope.Response.Result}, receipts: map[string][]Receipt{vector.Block.Hash: vector.Receipts}, observed: vector.Block.Number}
	a := f.adapter(t)
	var captured *VerifiedReceiptCoverage
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error { captured = b.NativeCoverage; return nil }
	if _, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "op-packed"), "latest"); err != nil {
		t.Fatal(err)
	}
	if captured == nil || !captured.CanonicalBodyProven || !captured.CanonicalReceiptsProven || captured.Evidence == nil || captured.OPRegolithFeeInputs != nil || f.fullReads.Load() != 2 {
		t.Fatal("normal source commitment missing or historical fee model leaked")
	}
	if captured.OPPackedFeeInputs == nil || captured.OPPackedFeeInputs.DataFeeRule != "Fjord" || captured.OPPackedFeeInputs.OperatorFeeRule != "Jovian" || len(captured.OPPackedFeeInputs.UserTransactions) != 49 {
		t.Fatal("normal modern fee projection missing")
	}
	archives := 0
	a.OnHistoryEvidence = func(ctx context.Context, e NativeHistoryEvidence) error {
		archives++
		_, _, err := VerifyNativeHistoryEvidence(ctx, chain, e)
		return err
	}
	if _, _, err := a.ReadFactsAndCatalog(context.WithValue(context.Background(), nativeDutyKey{}, "op-packed-history"), vector.Block.Number); err != nil || archives != 1 {
		t.Fatal("normal archive replay", err, archives)
	}
	proof, err := VerifyChainBlockJSON(context.Background(), vector.ChainKey, envelope.Response.Result, 16<<20)
	if err != nil {
		t.Fatal(err)
	}
	deposit, err := decodeOPDeposit(common.FromHex(proof.RawTransactions[0]))
	if err != nil || deposit.From != common.HexToAddress("0xdeaddeaddeaddeaddeaddeaddeaddeaddead0001") || common.BytesToAddress(deposit.To) != common.HexToAddress("0x4200000000000000000000000000000000000015") || deposit.Mint.Sign() != 0 || deposit.Value.Sign() != 0 || deposit.System || len(deposit.Data) != 178 || hex.EncodeToString(deposit.Data[:4]) != "3db6be2b" {
		t.Fatal("captured L1-info identity", err)
	}
	sequence := binary.BigEndian.Uint64(deposit.Data[12:20])
	if deposit.SourceHash != opL1InfoSourceHash(common.BytesToHash(deposit.Data[100:132]), sequence) {
		t.Fatal("captured L1-info sourceHash")
	}
	var observations struct{ Receipts []map[string]json.RawMessage }
	json.Unmarshal(vectorRaw, &observations)
	previous := uint64(0)
	users := []map[string]any{}
	for i, wire := range proof.RawTransactions {
		cumulative, err := canonicalIndex(vector.Receipts[i].CumulativeGasUsed)
		if err != nil || cumulative < previous {
			t.Fatal("committed cumulative gas", err)
		}
		gas := cumulative - previous
		previous = cumulative
		if i == 0 {
			continue
		}
		var tx types.Transaction
		if err := tx.UnmarshalBinary(common.FromHex(wire)); err != nil || tx.Type() != types.DynamicFeeTxType || tx.ChainId().Cmp(big.NewInt(10)) != 0 {
			t.Fatal("captured user encoding", err)
		}
		sender, err := types.Sender(types.NewLondonSigner(big.NewInt(10)), &tx)
		if err != nil || tx.Hash().Hex() != vector.Block.Transactions[i] {
			t.Fatal("captured signed identity", err)
		}
		users = append(users, map[string]any{"index": i, "hash": tx.Hash().Hex(), "sender": sender.Hex(), "encoded_transaction": wire[2:], "gas_used": gas, "observed_rpc_receipt": observations.Receipts[i]})
	}
	if len(users) != 49 {
		t.Fatal("captured users incomplete")
	}
	// Meaningful negative controls use the same public source and normal
	// verifier. Neither a missing tail nor a changed payload can be published.
	for _, fault := range []string{"body", "receipt", "receipt-tail", "header"} {
		t.Run(fault, func(t *testing.T) {
			encoded, _ := json.Marshal(captured.Evidence)
			var changed NativeReceiptEvidence
			json.Unmarshal(encoded, &changed)
			switch fault {
			case "body":
				if len(changed.Block.RawTransactions) != 50 {
					t.Fatal("normal evidence lost body")
				}
				changed.Block.RawTransactions[1] += "00"
			case "receipt":
				changed.Block.Receipts[1].CumulativeGasUsed = "0x0"
			case "receipt-tail":
				changed.Block.Receipts = changed.Block.Receipts[:49]
			case "header":
				var fields map[string]json.RawMessage
				json.Unmarshal(changed.Block.Header, &fields)
				fields["gasUsed"] = json.RawMessage(`"0x0"`)
				changed.Block.Header, _ = json.Marshal(fields)
			}
			if _, err := VerifyNativeReceiptEvidence(context.Background(), vector.ChainKey, vector.Block, changed, nativeProofTestLimits()); err == nil {
				t.Fatal("accepted mutated public commitment")
			}
		})
	}
	if output := os.Getenv("XONE_OP_PACKED_INPUT_OUTPUT"); output != "" {
		data, err := json.MarshalIndent(map[string]any{"passed": true, "authority": "RPC_HEADER_BODY_COMMITMENT_NOT_CONSENSUS", "block_hash": vector.Block.Hash, "timestamp": vector.Block.Timestamp, "native_semantics_id": captured.NativeSemanticsID, "attributes": hex.EncodeToString(deposit.Data), "users": users, "production_projection": captured.OPPackedFeeInputs, "normal_archive_replayed": true, "source_mutations_rejected": 4, "parser_fee_math_added": false, "production_ready": false}, "", "  ")
		if err != nil {
			t.Fatal(err)
		}
		if err = os.MkdirAll(filepath.Dir(output), 0755); err != nil {
			t.Fatal(err)
		}
		if err = os.WriteFile(output, append(data, '\n'), 0644); err != nil {
			t.Fatal(err)
		}
	}
}
