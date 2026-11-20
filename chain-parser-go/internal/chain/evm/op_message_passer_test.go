package evm

import (
	"compress/gzip"
	"context"
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"os"
	"testing"

	"xone/chain-parser-go/internal/config"
)

func TestOPMessagePasserArchiveAccountRead(t *testing.T) {
	read := func(name string, value any) {
		t.Helper()
		f, err := os.Open("../../../../integration/test/source/op-mainnet-withdrawals/" + name)
		if err != nil {
			t.Fatal(err)
		}
		defer f.Close()
		z, err := gzip.NewReader(f)
		if err != nil {
			t.Fatal(err)
		}
		defer z.Close()
		if err = json.NewDecoder(z).Decode(value); err != nil {
			t.Fatal(err)
		}
	}
	var sample struct {
		Block, Pair json.RawMessage
		Receipts    []Receipt
	}
	read("block-159769967.json.gz", &sample)
	var passer json.RawMessage
	read("passer-159769967.json.gz", &passer)
	ctx := context.WithValue(context.Background(), nativeDutyKey{}, "passer")
	body, err := VerifyChainBlockJSON(ctx, "evm:10", sample.Block, 16<<20)
	if err != nil {
		t.Fatal(err)
	}
	genesis, _ := knownBodyGenesis("evm:10")
	chain := config.Chain{Key: "evm:10", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 32 << 20, MaxTransactions: 4096, MaxHeaders: 64, MaxWindowBlocks: 16, MaxLookahead: 32}}
	for _, bad := range []bool{false, true} {
		f := normalNativeFixture{chain: chain, target: body.Block, observed: body.Block.Number, headers: map[string]json.RawMessage{body.Block.Number: body.Header}, full: map[string]json.RawMessage{body.Block.Number: sample.Block}, receipts: map[string][]Receipt{body.Block.Hash: sample.Receipts}}
		a := f.adapter(t)
		proofs := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			var req struct {
				Method string
				Params []json.RawMessage
			}
			if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
				t.Error(err)
				return
			}
			var result any
			switch req.Method {
			case "eth_chainId":
				result = "0xa"
			case "eth_getBlockByNumber":
				result = map[string]string{"hash": genesis}
			case "eth_getProof":
				var addr string
				var selector struct {
					BlockHash        string
					RequireCanonical bool
				}
				if len(req.Params) != 3 || json.Unmarshal(req.Params[0], &addr) != nil || json.Unmarshal(req.Params[2], &selector) != nil || selector.BlockHash != body.Block.Hash || !selector.RequireCanonical {
					t.Error("wrong proof selector")
					return
				}
				switch addr {
				case opMessagePasserAddress:
					result = passer
					if bad {
						result = sample.Pair
					}
				case "0x4c43646304492a925e335f2b6d840c1489f17815":
					result = sample.Pair
				default:
					t.Error(addr)
				}
			default:
				t.Error(req.Method)
			}
			json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": 1, "result": result})
		}))
		t.Cleanup(proofs.Close)
		a.Chain.ProofRPCURL = proofs.URL
		b, err := a.nativeRuntimeBlock(ctx, body.Block.Number)
		if err != nil {
			t.Fatal(err)
		}
		b, _, err = a.ReadQualifiedReceipts(ctx, b, true)
		if err != nil {
			t.Fatal(err)
		}
		account, err := a.ReadNativeContractAccountProof(ctx, b, "0x4c43646304492a925e335f2b6d840c1489f17815", []string{"0x5", "0x6", "0x7", "0x8", "0x9", "0xa", "0xc"})
		if bad {
			if err == nil || !b.nativeSession.rejected {
				t.Fatal("wrong account admitted", err)
			}
			continue
		}
		if err != nil || !account.Account.Exists {
			t.Fatal(err)
		}
		trusted, err := VerifyExecutionAccountProof(ctx, *b.CurrentStateAnchor, passer, 1<<20)
		if err != nil {
			t.Fatal(err)
		}
		if checkOPMessagePasserRoot(trusted, body.HeaderCommitment.opMessagePasserRoot) != nil || checkOPMessagePasserRoot(trusted, body.HeaderCommitment.StateRoot) == nil {
			t.Fatal("incorrect root binding")
		}
	}
}
