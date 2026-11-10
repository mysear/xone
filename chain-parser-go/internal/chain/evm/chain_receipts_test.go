package evm

import (
	"context"
	"crypto/sha256"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/crypto"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

const chainReceiptVectors = "../../../test/fixtures/source/testdata/chain-receipt-vectors"

type publicReceiptVector struct {
	ChainKey string    `json:"chain_key"`
	Block    RawBlock  `json:"block"`
	Receipts []Receipt `json:"receipts"`
}

func loadChainReceiptVector(t *testing.T, name string) publicReceiptVector {
	t.Helper()
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "vector.json"))
	if err != nil {
		t.Fatal(err)
	}
	var v publicReceiptVector
	if err = json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return v
}
func TestChainReceiptsPublicVectors(t *testing.T) {
	for _, name := range []string{"ethereum", "base-regolith", "base-canyon", "optimism-regolith", "optimism-canyon", "arbitrum", "bsc", "polygon", "avalanche"} {
		t.Run(name, func(t *testing.T) {
			v := loadChainReceiptVector(t, name)
			if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err != nil {
				t.Fatal(err)
			}
			if len(v.Receipts) == 0 {
				t.Fatal("public vector must exercise receipts")
			}
		})
	}
}
func TestChainReceiptsRejectPublicMutations(t *testing.T) {
	for _, name := range []string{"base-canyon", "base-regolith", "arbitrum", "bsc", "polygon", "avalanche", "ethereum"} {
		for _, mutation := range []string{"root", "type", "gas-regression", "bloom", "log-data", "drop-tail", "drop-both-tails", "duplicate", "wrong-index", "wrong-block", "missing-logs"} {
			t.Run(name+"/"+mutation, func(t *testing.T) {
				v := loadChainReceiptVector(t, name)
				switch mutation {
				case "root":
					v.Block.ReceiptsRoot = "0x" + strings.Repeat("11", 32)
				case "type":
					v.Receipts[0].Type = "0x67"
				case "gas-regression":
					v.Receipts[len(v.Receipts)-1].CumulativeGasUsed = "0x0"
				case "bloom":
					v.Receipts[0].LogsBloom = "0x" + strings.Repeat("ff", 256)
				case "log-data":
					found := false
					for i := range v.Receipts {
						if len(v.Receipts[i].Logs) > 0 {
							v.Receipts[i].Logs[0].Data = "0xdead"
							found = true
							break
						}
					}
					if !found {
						t.Skip("vector has no logs")
					}
				case "drop-tail":
					v.Receipts = v.Receipts[:len(v.Receipts)-1]
				case "drop-both-tails":
					v.Receipts = v.Receipts[:len(v.Receipts)-1]
					v.Block.Transactions = v.Block.Transactions[:len(v.Block.Transactions)-1]
				case "duplicate":
					v.Receipts = append(v.Receipts, v.Receipts[0])
				case "wrong-index":
					v.Receipts[0].TxIndex = "0x01"
				case "wrong-block":
					v.Receipts[0].BlockHash = "0x" + strings.Repeat("ee", 32)
				case "missing-logs":
					v.Receipts[0].Logs = nil
				}
				if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err == nil {
					t.Fatal("accepted mutation")
				}
			})
		}
	}
}
func TestChainReceiptsOPRawFields(t *testing.T) {
	v := loadChainReceiptVector(t, "base-canyon")
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "base-canyon", "vector.json"))
	if err != nil {
		t.Fatal(err)
	}
	var envelope struct {
		Receipts []json.RawMessage `json:"receipts"`
	}
	if err = json.Unmarshal(raw, &envelope); err != nil {
		t.Fatal(err)
	}
	for _, field := range []string{"depositNonce", "depositReceiptVersion", "type", "status", "cumulativeGasUsed", "logsBloom", "logs"} {
		for _, value := range []string{"null", "17", "\"0x0\"", "\"0x2\"", "\"0x01\"", "\"0x\"", "\"0X1\"", "\"0x10000000000000000\"", "omitted", "duplicate"} {
			t.Run(field+"/"+value, func(t *testing.T) {
				var fields map[string]json.RawMessage
				json.Unmarshal(envelope.Receipts[0], &fields)
				if value == "omitted" {
					delete(fields, field)
				} else if value != "duplicate" {
					fields[field] = json.RawMessage(value)
				}
				encoded, _ := json.Marshal(fields)
				if value == "duplicate" {
					encoded = append(encoded[:len(encoded)-1], []byte(fmt.Sprintf(",%q:%s}", field, fields[field]))...)
				}
				var receipt Receipt
				if err := json.Unmarshal(encoded, &receipt); err != nil {
					return
				}
				rs := append([]Receipt(nil), v.Receipts...)
				rs[0] = receipt
				if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, rs); err == nil {
					t.Fatal("accepted malformed/removed field")
				}
			})
		}
	}
}
func TestChainReceiptsEthereumRemainsStrict(t *testing.T) {
	for _, name := range []string{"base-regolith", "base-canyon", "arbitrum"} {
		v := loadChainReceiptVector(t, name)
		if err := VerifyReceiptRoot(context.Background(), v.Block, v.Receipts); err == nil {
			t.Fatal("global Ethereum accepted native receipt", name)
		}
	}
	v := loadChainReceiptVector(t, "ethereum")
	if err := VerifyChainReceiptRoot(context.Background(), "solana:mainnet-beta", v.Block, v.Receipts); err == nil {
		t.Fatal("accepted non EVM selector")
	}
	if err := VerifyChainReceiptRoot(context.Background(), "evm:999999", v.Block, v.Receipts); err != nil {
		t.Fatal("custom EVM must retain strict Ethereum fixtures", err)
	}
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	if err := VerifyChainReceiptRoot(ctx, v.ChainKey, v.Block, v.Receipts); err == nil {
		t.Fatal("accepted cancellation")
	}
}

func TestChainReceiptsIndependentNativeEncodings(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "encoding-vectors.json"))
	if err != nil {
		t.Fatal(err)
	}
	var data struct {
		Cases []struct {
			Name     string
			ChainKey string `json:"chain_key"`
			Root     string
			Receipts []Receipt
		}
	}
	if err = json.Unmarshal(raw, &data); err != nil {
		t.Fatal(err)
	}
	if len(data.Cases) != 17 {
		t.Fatalf("incomplete independent native vectors: %d", len(data.Cases))
	}
	for _, v := range data.Cases {
		t.Run(v.Name, func(t *testing.T) {
			b, rs := bindReceiptVector(v.Root, v.Receipts)
			b.Number = fmt.Sprintf("0x%x", 80084800)
			if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, b, rs); err != nil {
				t.Fatal(err)
			}
			if err := VerifyReceiptRoot(context.Background(), b, rs); err == nil {
				t.Fatal("strict Ethereum accepted native envelope or zero gas")
			}
			if len(rs) > 1 {
				rs[0], rs[1] = rs[1], rs[0]
				if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, b, rs); err != nil {
					t.Fatal("index ordering", err)
				}
			}
		})
	}
}
func TestChainReceiptsCorethPinnedTypeList(t *testing.T) {
	for _, v := range receiptRootVectors(t) {
		if v.Name == "type-4" {
			b, rs := bindReceiptVector(v.Root, v.Receipts)
			if err := VerifyChainReceiptRoot(context.Background(), "evm:43114", b, rs); err == nil {
				t.Fatal("pinned Coreth/libevm has no set-code receipt encoding")
			}
			return
		}
	}
	t.Fatal("missing Ethereum type-4 fixture")
}
func TestChainReceiptsRejectUncommittedBorSynthetic(t *testing.T) {
	v := loadChainReceiptVector(t, "polygon")
	number, _ := canonicalIndex(v.Block.Number)
	var height [8]byte
	binary.BigEndian.PutUint64(height[:], number)
	key := append([]byte("matic-bor-receipt-"), height[:]...)
	key = append(key, common.FromHex(v.Block.Hash)...)
	tx := crypto.Keccak256Hash(key).Hex()
	last := v.Receipts[len(v.Receipts)-1]
	synthetic := Receipt{Type: "0x0", TxHash: tx, BlockHash: v.Block.Hash, TxIndex: fmt.Sprintf("0x%x", len(v.Receipts)), Logs: []Log{}, Status: "0x1", CumulativeGasUsed: last.CumulativeGasUsed, GasUsed: "0x0", LogsBloom: "0x" + strings.Repeat("00", 256)}
	v.Block.Transactions = append(v.Block.Transactions, tx)
	v.Receipts = append(v.Receipts, synthetic)
	err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts)
	if err == nil || err.Error() != "RECEIPT_BOR_SYNTHETIC_UNCOMMITTED" {
		t.Fatal("must not silently drop synthetic suffix", err)
	}
	// No list/root-mismatch retry may turn an arbitrary zero gas suffix into an
	// excluded Bor receipt. Only a separate proof lane can authenticate it.
	v.Block.Transactions[len(v.Block.Transactions)-1] = "arbitrary-tail"
	v.Receipts[len(v.Receipts)-1].TxHash = "arbitrary-tail"
	if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err == nil {
		t.Fatal("arbitrary synthetic suffix accepted")
	}
}
func TestChainReceiptsJSONDuplicateLogsAndCaseAliases(t *testing.T) {
	for _, raw := range []string{
		`{"type":"0x7e","Type":"0x7e"}`,
		`{"logs":[{"data":"0x","data":"0x"}]}`,
		`{"depositNonce":"0x0","depositNonce":"0x0"}`,
	} {
		var r Receipt
		if err := json.Unmarshal([]byte(raw), &r); err == nil {
			t.Fatal("accepted duplicate", raw)
		}
	}
}

func TestChainReceiptsCurrentAvalancheFailsClosed(t *testing.T) {
	v := loadChainReceiptVector(t, "avalanche-current")
	if len(v.Block.Transactions) != 7 || len(v.Receipts) != 7 {
		t.Fatal("changed retained current-network vector")
	}
	if err := ValidateReceipts(v.Block, v.Receipts); err != nil {
		t.Fatal(err)
	}
	if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err == nil || err.Error() != "RECEIPT_ROOT_MISMATCH" {
		t.Fatal("Helicon header root cannot certify these receipts", err)
	}
}

func TestChainReceiptsPublicBorSyntheticFailsClosed(t *testing.T) {
	v := loadChainReceiptVector(t, "polygon-synthetic")
	if err := ValidateReceipts(v.Block, v.Receipts); err != nil {
		t.Fatal(err)
	}
	if err := VerifyChainReceiptRoot(context.Background(), v.ChainKey, v.Block, v.Receipts); err == nil || err.Error() != "RECEIPT_BOR_SYNTHETIC_UNCOMMITTED" {
		t.Fatal("uncommitted public Bor receipt must not be certified by root", err)
	}
}

func TestChainReceiptsRetainedArtifactIntegrity(t *testing.T) {
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, "manifest.json"))
	if err != nil {
		t.Fatal(err)
	}
	var manifest struct {
		Files []struct {
			Path   string
			Bytes  int
			SHA256 string
		}
	}
	if err = json.Unmarshal(raw, &manifest); err != nil {
		t.Fatal(err)
	}
	if len(manifest.Files) < 50 {
		t.Fatal("incomplete source/vector manifest", len(manifest.Files))
	}
	for _, file := range manifest.Files {
		if filepath.IsAbs(file.Path) || strings.HasPrefix(filepath.Clean(file.Path), "..") {
			t.Fatal("non-local manifest entry", file.Path)
		}
		raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, filepath.FromSlash(file.Path)))
		if err != nil {
			t.Fatal(err)
		}
		sum := sha256.Sum256(raw)
		if len(raw) != file.Bytes || fmt.Sprintf("%x", sum) != file.SHA256 {
			t.Fatal("artifact bytes changed without refreshing manifest", file.Path)
		}
	}
}
