package evm

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/params"
	"github.com/holiman/uint256"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

func retainedMainnetSenderBlock(t *testing.T) (map[string]json.RawMessage, []map[string]json.RawMessage) {
	t.Helper()
	raw, err := os.ReadFile("../../../test/fixtures/source/testdata/ethereum-mainnet/uniswap-v2-26355865-evidence.json")
	if err != nil {
		t.Fatal(err)
	}
	var evidence provenance.Evidence
	var fields map[string]json.RawMessage
	var transactions []map[string]json.RawMessage
	if json.Unmarshal(raw, &evidence) != nil || json.Unmarshal(evidence.Block, &fields) != nil || json.Unmarshal(fields["transactions"], &transactions) != nil || len(transactions) == 0 || transactions[0]["from"] == nil {
		t.Fatal("retained mainnet full transaction rows required")
	}
	return fields, transactions
}

func TestNormalEthereumBodyEntrypointsAuthenticateOptionalRPCSender(t *testing.T) {
	fields, transactions := retainedMainnetSenderBlock(t)
	var number, blockHash string
	json.Unmarshal(fields["number"], &number)
	json.Unmarshal(fields["hash"], &blockHash)
	var original types.Transaction
	rawOriginal, _ := json.Marshal(transactions[0])
	if err := json.Unmarshal(rawOriginal, &original); err != nil {
		t.Fatal(err)
	}
	originalEncoding, err := original.MarshalBinary()
	if err != nil {
		t.Fatal(err)
	}
	var address common.Address
	if err := json.Unmarshal(transactions[0]["from"], &address); err != nil {
		t.Fatal(err)
	}
	wrong := address
	wrong[0] ^= 1
	for _, profile := range []string{"ethereum-header-transactions-v1", ChainNativeCommitmentsV2} {
		for _, change := range []string{"original", "absent", "uppercase", "wrong", "null", "short", "number", "object"} {
			t.Run(profile+"/"+change, func(t *testing.T) {
				encoded, _ := json.Marshal(transactions)
				var rows []map[string]json.RawMessage
				json.Unmarshal(encoded, &rows)
				switch change {
				case "absent":
					for _, row := range rows {
						delete(row, "from")
					}
				case "uppercase":
					rows[0]["from"], _ = json.Marshal("0x" + strings.ToUpper(address.Hex()[2:]))
				case "wrong":
					rows[0]["from"], _ = json.Marshal(wrong.Hex())
				case "null", "number", "object":
					rows[0]["from"] = map[string]json.RawMessage{"null": json.RawMessage(`null`), "number": json.RawMessage(`1`), "object": json.RawMessage(`{}`)}[change]
				case "short":
					rows[0]["from"] = json.RawMessage(`"0x01"`)
				}
				// The counterexample changes no signed bytes, transaction ID, or
				// header commitment. Only the redundant RPC sender is different.
				txJSON, _ := json.Marshal(rows[0])
				var tx types.Transaction
				if err := json.Unmarshal(txJSON, &tx); err != nil {
					t.Fatal("sender-only mutation changed signed transaction decoding", err)
				}
				wire, err := tx.MarshalBinary()
				if err != nil || !bytes.Equal(wire, originalEncoding) || tx.Hash() != original.Hash() {
					t.Fatal("sender-only mutation changed transaction commitment", err)
				}
				blockFields := make(map[string]json.RawMessage, len(fields))
				for key, value := range fields {
					blockFields[key] = value
				}
				blockFields["transactions"], _ = json.Marshal(rows)
				full, _ := json.Marshal(blockFields)
				var fullReads atomic.Uint64
				genesis, _ := knownBodyGenesis("evm:1")
				server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
					var q struct {
						Method string
						Params []json.RawMessage
					}
					if err := json.NewDecoder(r.Body).Decode(&q); err != nil {
						t.Error(err)
						return
					}
					var result any
					switch q.Method {
					case "eth_chainId":
						result = "0x1"
					case "eth_getBlockByNumber":
						var tag string
						var complete bool
						json.Unmarshal(q.Params[0], &tag)
						json.Unmarshal(q.Params[1], &complete)
						if tag == "0x0" {
							result = map[string]string{"hash": genesis}
						} else if tag == number && complete {
							fullReads.Add(1)
							result = json.RawMessage(full)
						} else {
							t.Error("unexpected block selector", tag, complete)
						}
					default:
						t.Error("sender check unexpectedly read another RPC method", q.Method)
					}
					json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
				}))
				defer server.Close()
				adapter := Adapter{Chain: config.Chain{Key: "evm:1", Genesis: genesis, BlockIntegrity: profile,
					NativeProof: &config.NativeProofConfig{MaxBytes: 64 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}},
					Client: rpc.New([]string{server.URL}, 10*time.Second, 16<<20)}
				observed, err := adapter.Block(context.Background(), number)
				if fullReads.Load() == 0 {
					t.Fatal("normal body entrypoint did not fetch the counterexample")
				}
				if change == "original" || change == "absent" || change == "uppercase" {
					if err != nil || observed.Hash != blockHash {
						t.Fatal("valid retained mainnet body rejected", err)
					}
				} else if err == nil || err.Error() != "BLOCK_INTEGRITY_TRANSACTION_SENDER" {
					t.Fatal("redundant sender was not rejected by normal body entrypoint", err)
				}
			})
		}
	}
}

func TestMainnetRPCSenderUsesHistoricalSignatureRules(t *testing.T) {
	key, err := crypto.HexToECDSA("0000000000000000000000000000000000000000000000000000000000000001")
	if err != nil {
		t.Fatal(err)
	}
	to := common.HexToAddress("0x1111111111111111111111111111111111111111")
	tx, err := types.SignTx(types.NewTx(&types.LegacyTx{To: &to, Value: big.NewInt(1), Gas: 21000, GasPrice: big.NewInt(1)}), types.FrontierSigner{}, key)
	if err != nil {
		t.Fatal(err)
	}
	v, r, s := tx.RawSignatureValues()
	highS := new(big.Int).Sub(crypto.S256().Params().N, s)
	parity := new(big.Int).SetInt64(27)
	if v.Uint64() == 27 {
		parity.SetInt64(28)
	}
	historical := types.NewTx(&types.LegacyTx{To: &to, Value: big.NewInt(1), Gas: 21000, GasPrice: big.NewInt(1), V: parity, R: r, S: highS})
	declared, _ := json.Marshal(crypto.PubkeyToAddress(key.PublicKey).Hex())
	activation := params.MainnetChainConfig.HomesteadBlock.Uint64()
	before := RawBlock{Number: fmt.Sprintf("0x%x", activation-1), Timestamp: "0x1"}
	after := RawBlock{Number: fmt.Sprintf("0x%x", activation), Timestamp: "0x1"}
	if err := verifyMainnetRPCSender(before, historical, declared); err != nil {
		t.Fatal("Frontier historical high-S signature rejected", err)
	}
	if err := verifyMainnetRPCSender(after, historical, declared); err == nil || err.Error() != "BLOCK_INTEGRITY_TRANSACTION_SENDER" {
		t.Fatal("Homestead accepted historical high-S signature", err)
	}
}

func TestMainnetRPCSenderUsesMainnetTimestampActivation(t *testing.T) {
	key, err := crypto.HexToECDSA("0000000000000000000000000000000000000000000000000000000000000001")
	if err != nil {
		t.Fatal(err)
	}
	unsigned := types.NewTx(&types.BlobTx{
		ChainID: uint256.NewInt(1), GasTipCap: uint256.NewInt(1), GasFeeCap: uint256.NewInt(2),
		Gas: 21000, To: common.HexToAddress("0x1111111111111111111111111111111111111111"),
		Value: uint256.NewInt(0), BlobFeeCap: uint256.NewInt(2),
		BlobHashes: []common.Hash{common.HexToHash("0x01" + strings.Repeat("00", 31))},
		V:          uint256.NewInt(0), R: uint256.NewInt(0), S: uint256.NewInt(0),
	})
	tx, err := types.SignTx(unsigned, types.NewCancunSigner(big.NewInt(1)), key)
	if err != nil {
		t.Fatal(err)
	}
	declared, _ := json.Marshal(crypto.PubkeyToAddress(key.PublicKey).Hex())
	activation := *params.MainnetChainConfig.CancunTime
	// Isolate timestamp-dependent signer selection. These are signer vectors,
	// not assertions that either synthetic height/time pair was mined.
	block := RawBlock{Number: fmt.Sprintf("0x%x", params.MainnetChainConfig.LondonBlock.Uint64()+1), Timestamp: fmt.Sprintf("0x%x", activation-1)}
	if err := verifyMainnetRPCSender(block, tx, declared); err == nil || err.Error() != "BLOCK_INTEGRITY_TRANSACTION_SENDER" {
		t.Fatal("pre-Cancun signer accepted blob transaction", err)
	}
	block.Timestamp = fmt.Sprintf("0x%x", activation)
	if err := verifyMainnetRPCSender(block, tx, declared); err != nil {
		t.Fatal("Cancun signer rejected authenticated blob transaction", err)
	}
}
