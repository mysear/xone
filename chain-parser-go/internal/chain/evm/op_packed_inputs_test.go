package evm

import (
	"bytes"
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
	"xone/chain-parser-go/internal/config"
)

const opUpgradeCaptureManifestSHA = "994b73a40295ba29885e0bb77741547d7b382912b86568c6f18e78d79bcf9514"

func opUpgradeFixture(t *testing.T, name string) (*normalNativeFixture, []map[string]json.RawMessage) {
	t.Helper()
	directory := os.Getenv("XONE_OP_UPGRADE_PUBLIC_CAPTURE")
	if directory == "" {
		t.Skip("retained original upgrade capture required")
	}
	manifest, err := os.ReadFile(filepath.Join(directory, "report.json"))
	if err != nil || fmt.Sprintf("%x", sha256.Sum256(manifest)) != opUpgradeCaptureManifestSHA {
		t.Fatal("upgrade capture manifest changed", err)
	}
	var m struct {
		Blocks []struct {
			Name  string
			Files map[string]string
		}
	}
	if err = json.Unmarshal(manifest, &m); err != nil {
		t.Fatal(err)
	}
	files := map[string]string{}
	for _, row := range m.Blocks {
		if row.Name == name {
			files = row.Files
		}
	}
	read := func(filename string) []byte {
		raw, err := os.ReadFile(filepath.Join(directory, name, filename))
		if err != nil || files[filename] == "" || fmt.Sprintf("%x", sha256.Sum256(raw)) != files[filename] {
			t.Fatal("upgrade source changed", filename, err)
		}
		if err = rejectDuplicateReceiptKeys(raw); err != nil {
			t.Fatal(err)
		}
		return raw
	}
	var full struct {
		Response struct{ Result json.RawMessage }
	}
	if err = json.Unmarshal(read("full-block.json"), &full); err != nil {
		t.Fatal(err)
	}
	var vector publicReceiptVector
	vectorRaw := read("vector.json")
	if err = json.Unmarshal(vectorRaw, &vector); err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	json.Unmarshal(full.Response.Result, &fields)
	fields["transactions"], _ = json.Marshal(vector.Block.Transactions)
	header, _ := json.Marshal(fields)
	genesis, _ := knownBodyGenesis("evm:10")
	chain := config.Chain{Key: "evm:10", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}}
	f := &normalNativeFixture{chain: chain, target: vector.Block, headers: map[string]json.RawMessage{vector.Block.Number: header}, full: map[string]json.RawMessage{vector.Block.Number: full.Response.Result}, receipts: map[string][]Receipt{vector.Block.Hash: vector.Receipts}, observed: vector.Block.Number}
	var observations struct{ Receipts []map[string]json.RawMessage }
	json.Unmarshal(vectorRaw, &observations)
	return f, observations.Receipts
}

func TestOPPackedUpgradeNormalSourcesAndHistoryRecreateVersionedInputs(t *testing.T) {
	rows := []map[string]any{}
	for _, test := range []struct {
		name, format, dataRule, operatorRule string
		users                                int
		upgrade                              bool
	}{
		{"ecotone-0", "Bedrock", "RegolithActivationDepositsOnly", "NotActive", 0, true},
		{"ecotone-2", "Ecotone", "Ecotone", "NotActive", 11, false},
		{"fjord-0", "Ecotone", "Fjord", "NotActive", 0, true},
		{"fjord-2", "Ecotone", "Fjord", "NotActive", 19, false},
		{"isthmus-0", "Ecotone", "Fjord", "IsthmusActivationDepositsOnly", 0, true},
		{"isthmus-2", "Isthmus", "Fjord", "Isthmus", 34, false},
		{"jovian-0", "Isthmus", "Fjord", "Jovian", 0, true},
		{"jovian-2", "Jovian", "Fjord", "Jovian", 64, false},
	} {
		t.Run(test.name, func(t *testing.T) {
			f, observations := opUpgradeFixture(t, test.name)
			a := f.adapter(t)
			var projection *OPPackedFeeInputs
			a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error {
				projection = b.NativeCoverage.OPPackedFeeInputs
				return nil
			}
			if _, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "op-upgrade"), "latest"); err != nil {
				t.Fatal(err)
			}
			if projection == nil || projection.AttributesFormat != test.format || projection.DataFeeRule != test.dataRule || projection.OperatorFeeRule != test.operatorRule || projection.UpgradeBlock != test.upgrade || len(projection.UserTransactions) != test.users {
				t.Fatal("wrong versioned normal source", projection)
			}
			if projection.Authority != "RPC_HEADER_BODY_RECEIPTS_COMMITMENT_NOT_CONSENSUS" || projection.AttributesSHA256 == "" || projection.ReceiptsRoot != f.target.ReceiptsRoot {
				t.Fatal("unbound fee inputs")
			}
			var archive NativeHistoryEvidence
			a.OnHistoryEvidence = func(ctx context.Context, e NativeHistoryEvidence) error {
				archive = e
				_, _, err := VerifyNativeHistoryEvidence(ctx, f.chain, e)
				return err
			}
			if _, _, err := a.ReadFactsAndCatalog(context.WithValue(context.Background(), nativeDutyKey{}, "op-upgrade-history"), f.target.Number); err != nil {
				t.Fatal(err)
			}
			before, _ := json.Marshal(projection)
			after, _ := json.Marshal(archive.OPPackedFeeInputs)
			if !bytes.Equal(before, after) {
				t.Fatal("history projection differs")
			}
			if test.users > 0 {
				for _, fault := range []string{"data-rule", "operator-rule", "format", "attributes", "gas", "sender", "index", "wire", "root", "deposit", "authority", "activation", "upgrade", "header", "time"} {
					t.Run("projection-"+fault, func(t *testing.T) {
						raw, _ := json.Marshal(archive)
						var e NativeHistoryEvidence
						json.Unmarshal(raw, &e)
						p := e.OPPackedFeeInputs
						switch fault {
						case "data-rule":
							p.DataFeeRule = "Regolith"
						case "operator-rule":
							p.OperatorFeeRule = "foreign/operator"
						case "format":
							p.AttributesFormat = "Bedrock"
						case "attributes":
							p.AttributesCalldata = "0x00"
						case "gas":
							p.UserTransactions[0].GasUsed++
						case "sender":
							p.UserTransactions[0].Sender = "0x" + strings.Repeat("11", 20)
						case "index":
							p.UserTransactions[0].Index = 0
						case "wire":
							p.UserTransactions[0].EncodedTransaction = "0x02"
						case "root":
							p.ReceiptsRoot = "0x" + strings.Repeat("11", 32)
						case "deposit":
							p.DepositSourceHash = "0x" + strings.Repeat("11", 32)
						case "authority":
							p.Authority = "CONSENSUS"
						case "activation":
							p.ActivationID = "foreign/fork"
						case "upgrade":
							p.UpgradeBlock = true
						case "header":
							p.HeaderSHA256 = strings.Repeat("11", 32)
						case "time":
							p.ExecutionTimestamp++
						}
						if _, _, err := VerifyNativeHistoryEvidence(context.Background(), f.chain, e); err == nil || !strings.Contains(err.Error(), "HISTORY_OP_PACKED_FEE_INPUT_PROJECTION_MISMATCH") {
							t.Fatal("accepted changed projection", err)
						}
					})
				}
			}
			archive.OPPackedFeeInputs = nil
			if _, _, err := VerifyNativeHistoryEvidence(context.Background(), f.chain, archive); err != nil {
				t.Fatal("old archive", err)
			}
			rows = append(rows, map[string]any{"name": test.name, "inputs": projection, "observed_rpc_receipts": observations})
		})
	}
	if output := os.Getenv("XONE_OP_UPGRADE_INPUT_OUTPUT"); output != "" {
		data, err := json.MarshalIndent(map[string]any{"passed": true, "blocks": rows, "production_ready": false, "parser_fee_math_added": false, "projection_mutations_rejected": 60, "old_archive_compatible": true}, "", "  ")
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

func TestOPPackedProjectionSemanticAndBudgetFailuresRemainClosed(t *testing.T) {
	f, _ := opUpgradeFixture(t, "jovian-2")
	a := f.adapter(t)
	var coverage *VerifiedReceiptCoverage
	a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error { coverage = b.NativeCoverage; return nil }
	if _, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "op-controls"), "latest"); err != nil {
		t.Fatal(err)
	}
	for _, fault := range []string{"sender", "target", "mint", "value", "system", "selector", "size", "source-hash", "l1-hash", "sequence", "future-l1-time", "native-semantics", "gas-regression", "budget", "canceled"} {
		t.Run(fault, func(t *testing.T) {
			raw, _ := json.Marshal(coverage.Evidence.Block)
			var b ExecutionReceiptBlock
			json.Unmarshal(raw, &b)
			header, err := VerifyChainHeaderJSON(context.Background(), f.chain.Key, b.Header)
			if err != nil {
				t.Fatal(err)
			}
			c := *coverage
			ctx := context.Background()
			budget := receiptProofBudget{remaining: 16 << 20}
			d, err := decodeOPDeposit(common.FromHex(b.RawTransactions[0]))
			if err != nil {
				t.Fatal(err)
			}
			switch fault {
			case "sender":
				d.From = common.Address{}
			case "target":
				d.To = make([]byte, 20)
			case "mint":
				d.Mint.SetUint64(1)
			case "value":
				d.Value.SetUint64(1)
			case "system":
				d.System = true
			case "selector":
				d.Data[0] ^= 1
			case "size":
				d.Data = append(d.Data, 0)
			case "source-hash":
				d.SourceHash[31] ^= 1
			case "l1-hash":
				d.Data[100] ^= 1
			case "sequence":
				d.Data[19] ^= 1
			case "future-l1-time":
				d.Data[20] = 255
			case "native-semantics":
				c.NativeSemanticsID = "foreign"
			case "gas-regression":
				b.Receipts[1].CumulativeGasUsed = "0x0"
			case "budget":
				budget.remaining = 1
			case "canceled":
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			}
			encoded, _ := rlp.EncodeToBytes(d)
			b.RawTransactions[0] = "0x7e" + hex.EncodeToString(encoded)
			if _, err = projectOPPackedFeeInputs(ctx, c, header, b, &budget); err == nil {
				t.Fatal("accepted semantic/source fault")
			}
		})
	}
}

func TestOPUpgradeContractCreationMissingDestinationMatchesNullButCannotEraseCallTarget(t *testing.T) {
	f, _ := opUpgradeFixture(t, "ecotone-0")
	var block struct{ Transactions []json.RawMessage }
	json.Unmarshal(f.full[f.target.Number], &block)
	for _, index := range []int{0, 1} {
		var fields map[string]json.RawMessage
		json.Unmarshal(block.Transactions[index], &fields)
		original, err := encodeChainTransactionJSON(f.chain.Key, f.target, uint64(index), block.Transactions[index])
		if err != nil {
			t.Fatal(err)
		}
		fields["to"] = json.RawMessage("null")
		asNull, _ := json.Marshal(fields)
		withNull, err := encodeChainTransactionJSON(f.chain.Key, f.target, uint64(index), asNull)
		if index == 1 {
			if err != nil || !bytes.Equal(original, withNull) {
				t.Fatal("contract creation missing/null mismatch", err)
			}
			delete(fields, "to")
			missing, _ := json.Marshal(fields)
			withMissing, err := encodeChainTransactionJSON(f.chain.Key, f.target, uint64(index), missing)
			if err != nil || !bytes.Equal(original, withMissing) {
				t.Fatal("contract creation absent mismatch", err)
			}
		} else {
			if err == nil {
				t.Fatal("erased committed call target")
			}
			delete(fields, "to")
			missing, _ := json.Marshal(fields)
			if _, err := encodeChainTransactionJSON(f.chain.Key, f.target, uint64(index), missing); err == nil {
				t.Fatal("omitted committed call target")
			}
		}
	}
}

func TestOPUpgradeBlocksRejectUsersAndPackedSourceRejectsInvalidSignature(t *testing.T) {
	for _, name := range []string{"ecotone-0", "fjord-0", "isthmus-0", "jovian-0", "jovian-2"} {
		t.Run(name, func(t *testing.T) {
			f, _ := opUpgradeFixture(t, name)
			a := f.adapter(t)
			var c *VerifiedReceiptCoverage
			a.OnComplete = func(_ context.Context, b RawBlock, _ []Receipt) error { c = b.NativeCoverage; return nil }
			if _, _, _, err := a.ReadComplete(context.WithValue(context.Background(), nativeDutyKey{}, "upgrade-user-control"), "latest"); err != nil {
				t.Fatal(err)
			}
			raw, _ := json.Marshal(c.Evidence.Block)
			var b ExecutionReceiptBlock
			json.Unmarshal(raw, &b)
			h, err := VerifyChainHeaderJSON(context.Background(), f.chain.Key, b.Header)
			if err != nil {
				t.Fatal(err)
			}
			// A decoded but unsigned transaction must fail even if a caller
			// supplies internally matching identity hashes to this private layer.
			transaction := types.NewTransaction(0, common.Address{}, nil, 21000, nil, nil)
			encoded, _ := transaction.MarshalBinary()
			b.RawTransactions[1] = "0x" + hex.EncodeToString(encoded)
			h.Block.Transactions[1] = crypto.Keccak256Hash(encoded).Hex()
			c.Block = h.Block
			budget := receiptProofBudget{remaining: 16 << 20}
			_, err = projectOPPackedFeeInputs(context.Background(), *c, h, b, &budget)
			if err == nil {
				t.Fatal("accepted unsigned/upgrade user")
			}
			expected := "OP_UPGRADE_BLOCK_USER_TRANSACTION_NOT_ADMITTED"
			if name == "jovian-2" {
				expected = "OP_PACKED_USER_TRANSACTION_SIGNATURE"
			}
			if !strings.Contains(err.Error(), expected) {
				t.Fatal("wrong rejection layer", err)
			}
		})
	}
}
