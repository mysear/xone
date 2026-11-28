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

	"xone/chain-parser-go/internal/testfixture"
)

const chainReceiptVectors = "../../../test/fixtures/source/testdata/chain-receipt-vectors"

type publicReceiptVector struct {
	ChainKey string    `json:"chain_key"`
	Block    RawBlock  `json:"block"`
	Receipts []Receipt `json:"receipts"`
}

func readPolygonBorVector(t *testing.T, dest any) {
	t.Helper()
	testfixture.ReadGzipJSON(t, "../../../test/fixtures/native-catalog-replay/polygon-50000144.json.gz", dest)
}

func loadChainReceiptVector(t *testing.T, name string) publicReceiptVector {
	t.Helper()
	var v publicReceiptVector
	if name == "polygon-synthetic" {
		readPolygonBorVector(t, &v)
		return v
	}
	if readHistoricalReceiptVector(t, name, &v) {
		return v
	}
	raw, err := os.ReadFile(filepath.Join(chainReceiptVectors, name, "vector.json"))
	if err != nil {
		t.Fatal(err)
	}
	if err = json.Unmarshal(raw, &v); err != nil {
		t.Fatal(err)
	}
	return v
}

func readHistoricalReceiptVector(t *testing.T, name string, dest any) bool {
	t.Helper()
	path := filepath.Join("../../../test/fixtures/historical-receipts", name+".json.gz")
	if _, err := os.Stat(path); os.IsNotExist(err) {
		return false
	}
	testfixture.ReadGzipJSON(t, path, dest)
	return true
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

func TestPublicCatalogFixedReceiptRoot(t *testing.T) {
	inputs := filepath.SplitList(os.Getenv("XONE_R01_RECEIPT_INPUTS"))
	if len(inputs) == 0 {
		t.Skip("XONE_R01_RECEIPT_INPUTS is unset")
	}
	type reference struct {
		Path   string `json:"path"`
		SHA256 string `json:"sha256"`
	}
	type blockInput struct {
		FullBlock reference `json:"full_block"`
		Receipts  reference `json:"receipts"`
	}
	readRPC := func(t *testing.T, ref reference, method string, result any) []json.RawMessage {
		t.Helper()
		if !filepath.IsLocal(ref.Path) {
			t.Fatalf("invalid evidence path %q", ref.Path)
		}
		raw, err := os.ReadFile(filepath.Join("../../../..", ref.Path))
		if err != nil {
			t.Fatal(err)
		}
		if fmt.Sprintf("%x", sha256.Sum256(raw)) != ref.SHA256 {
			t.Fatalf("evidence digest mismatch for %s", ref.Path)
		}
		var wire struct {
			Method     string            `json:"method"`
			Params     []json.RawMessage `json:"params"`
			HTTPStatus int               `json:"http_status"`
			Response   struct {
				Result json.RawMessage `json:"result"`
			} `json:"response"`
		}
		if err := json.Unmarshal(raw, &wire); err != nil {
			t.Fatal(err)
		}
		if wire.Method != method || wire.HTTPStatus != 200 || len(wire.Response.Result) == 0 || string(wire.Response.Result) == "null" {
			t.Fatalf("invalid %s response in %s", method, ref.Path)
		}
		if err := json.Unmarshal(wire.Response.Result, result); err != nil {
			t.Fatal(err)
		}
		return wire.Params
	}
	readBlock := func(t *testing.T, input blockInput) (json.RawMessage, []json.RawMessage) {
		t.Helper()
		var full json.RawMessage
		readRPC(t, input.FullBlock, "eth_getBlockByNumber", &full)
		header, transactions, err := projectFullRPCBlock(full)
		if err != nil {
			t.Fatal(err)
		}
		return header, transactions
	}
	readReceipts := func(t *testing.T, ref reference, block RawBlock) []Receipt {
		t.Helper()
		var receipts []Receipt
		params := readRPC(t, ref, "eth_getBlockReceipts", &receipts)
		var selector string
		if len(params) != 1 || json.Unmarshal(params[0], &selector) != nil ||
			(selector != block.Number && selector != block.Hash) {
			t.Fatal("receipt request does not point to the verified block")
		}
		return receipts
	}
	for _, input := range inputs {
		raw, err := os.ReadFile(input)
		if err != nil {
			t.Fatal(err)
		}
		var fixture struct {
			blockInput
			ChainKey string    `json:"chain_key"`
			Header   reference `json:"header"`
			SAE      *struct {
				GenesisHash     string       `json:"genesis_hash"`
				SettlingHeaders []reference  `json:"settling_headers"`
				Window          []blockInput `json:"window"`
			} `json:"sae"`
		}
		if err := json.Unmarshal(raw, &fixture); err != nil {
			t.Fatal(err)
		}
		t.Run(fixture.ChainKey, func(t *testing.T) {
			var header json.RawMessage
			readRPC(t, fixture.Header, "eth_getBlockByNumber", &header)
			commitment, err := VerifyChainHeaderJSON(t.Context(), fixture.ChainKey, header)
			if err != nil {
				t.Fatal(err)
			}
			if commitment.SettledHeight != nil {
				if fixture.SAE == nil || len(fixture.SAE.Window) == 0 || len(fixture.SAE.Window) > 32 ||
					len(fixture.SAE.SettlingHeaders) == 0 || len(fixture.SAE.SettlingHeaders) > 32 {
					t.Fatal("invalid native receipt evidence window")
				}
				proof := SAEExecutionProof{ChainKey: fixture.ChainKey, GenesisHash: fixture.SAE.GenesisHash}
				for _, input := range fixture.SAE.Window {
					compact, txs := readBlock(t, input)
					block, err := VerifyChainHeaderJSON(t.Context(), fixture.ChainKey, compact)
					if err != nil {
						t.Fatal(err)
					}
					proof.Headers = append(proof.Headers, compact)
					proof.Window = append(proof.Window, ExecutionReceiptBlock{
						Header: compact, TransactionsJSON: txs, Receipts: readReceipts(t, input.Receipts, block.Block),
					})
				}
				for _, ref := range fixture.SAE.SettlingHeaders {
					var successor json.RawMessage
					readRPC(t, ref, "eth_getBlockByNumber", &successor)
					proof.Headers = append(proof.Headers, successor)
				}
				limits := ReceiptProofLimits{
					MaxBytes: 32 * 1024 * 1024, MaxHeaders: 64, MaxWindowBlocks: 32,
					MaxTransactions: 100000, MaxLookahead: 32,
				}
				coverage, err := VerifySAEExecutionProof(t.Context(), commitment.Block, proof, limits)
				if err != nil {
					t.Fatal(err)
				}
				if coverage.StateAnchor == nil || coverage.StateAnchor.BlockHash != commitment.Block.Hash {
					t.Fatal("receipt settlement does not establish the target state root")
				}
				t.Logf("verified Coreth window %d–%d in settling block %d", coverage.WindowStart, coverage.WindowEnd, coverage.StateAnchor.SettlingHeight)
				return
			}
			compact, txs := readBlock(t, fixture.blockInput)
			full, err := VerifyChainHeaderJSON(t.Context(), fixture.ChainKey, compact)
			if err != nil {
				t.Fatal(err)
			}
			if !sameReceiptAnchor(full.Block, commitment.Block) {
				t.Fatal("full transaction body points to a different block")
			}
			body := ExecutionReceiptBlock{TransactionsJSON: txs}
			budget := receiptProofBudget{remaining: 32 * 1024 * 1024}
			if err := verifyExecutionBody(t.Context(), fixture.ChainKey, commitment, body, &budget); err != nil {
				t.Fatal(err)
			}
			receipts := readReceipts(t, fixture.Receipts, commitment.Block)
			if err := VerifyChainReceiptRoot(t.Context(), fixture.ChainKey, commitment.Block, receipts); err != nil {
				t.Fatal(err)
			}
			t.Logf("verified %d receipts at %s (%s)", len(receipts), commitment.Block.Number, commitment.Block.Hash)
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
	var data struct {
		Cases []struct {
			Name     string
			ChainKey string `json:"chain_key"`
			Root     string
			Receipts []Receipt
		}
	}
	testfixture.ReadGzipJSON(t, "../../../test/fixtures/historical-receipts/encoding-vectors.json.gz", &data)
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
