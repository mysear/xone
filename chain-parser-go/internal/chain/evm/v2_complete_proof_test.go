package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethdb/memorydb"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

type v2ProofBank struct {
	bank       completeV2Case
	header     map[string]any
	state      *trie.Trie
	storage    map[string]*trie.Trie
	proofReads atomic.Int64
}

func v2TrieNodes(t *testing.T, tree *trie.Trie, key []byte) []string {
	t.Helper()
	db := memorydb.New()
	defer db.Close()
	if e := tree.Prove(key, db); e != nil {
		t.Fatal(e)
	}
	it := db.NewIterator(nil, nil)
	defer it.Release()
	out := []string{}
	for it.Next() {
		out = append(out, "0x"+hex.EncodeToString(it.Value()))
	}
	if e := it.Error(); e != nil {
		t.Fatal(e)
	}
	return out
}
func newV2ProofBank(t *testing.T, fault string) *v2ProofBank {
	t.Helper()
	f := &v2ProofBank{bank: loadCompleteV2OriginalBank(t), state: trie.NewEmpty(nil), storage: map[string]*trie.Trie{}}
	f.bank.Pool.ChainKey = "evm:1"

	if fault == "slot-limit-token" {
		for i := range f.bank.Accounts {
			if f.bank.Accounts[i].Address == f.bank.Pool.Token0 {
				code := "0x"
				for n := 0; n < 9; n++ {
					code += fmt.Sprintf("60%02x5450", n)
				}
				f.bank.Accounts[i].Code = code + "600060005260206000f3"
			}
		}
	}

	if fault == "unsupported-token-view" {
		for i := range f.bank.Accounts {
			if f.bank.Accounts[i].Address == f.bank.Pool.Token0 {
				f.bank.Accounts[i].Code = "0x4360005260206000f3"
			}
		}
	}
	for _, account := range f.bank.Accounts {
		tree := trie.NewEmpty(nil)
		for _, slot := range account.Storage {
			n := new(big.Int).SetBytes(common.FromHex(slot.Value))
			if fault == "committed-token-balance" && account.Address == f.bank.Pool.Token0 && common.HexToHash(slot.Slot) == common.BytesToHash(completeV2Mapping(f.bank.Pool.PoolID, common.BigToHash(big.NewInt(1)).Bytes())) {
				n.Add(n, big.NewInt(1))
			}

			if account.Address == f.bank.Pool.PoolID {
				target := map[string]string{"committed-identity": "0x5", "committed-reserves": "0x8", "committed-cumulative": "0x9", "committed-lock": "0xc"}[fault]
				if target != "" && common.HexToHash(target) == common.HexToHash(slot.Slot) {
					n.Add(n, big.NewInt(1))
				}
			}
			if account.Address == f.bank.Pool.V2.Factory && (fault == "committed-forward-map" || fault == "committed-reverse-map") {
				first, second := f.bank.Pool.Token0, f.bank.Pool.Token1
				if fault == "committed-reverse-map" {
					first, second = second, first
				}
				if common.HexToHash(slot.Slot) == common.HexToHash(v2CompletePairMapping(first, second)) {
					n.Add(n, big.NewInt(1))
				}
			}
			if n.Sign() == 0 {
				continue
			}
			encoded, _ := rlp.EncodeToBytes(n.Bytes())
			if e := tree.Update(crypto.Keccak256(common.HexToHash(slot.Slot).Bytes()), encoded); e != nil {
				t.Fatal(e)
			}
		}
		if fault == "empty-token-storage" && account.Address == f.bank.Pool.Token0 {
			tree = trie.NewEmpty(nil)
		}
		raw := common.FromHex(account.Code)
		if fault == "committed-pair-code" && account.Address == f.bank.Pool.PoolID {
			raw = []byte{0x00}
		}
		root := tree.Hash()
		value, _ := rlp.EncodeToBytes([]any{uint64(0), new(big.Int), root, crypto.Keccak256(raw)})
		if e := f.state.Update(crypto.Keccak256(common.FromHex(account.Address)), value); e != nil {
			t.Fatal(e)
		}
		f.storage[account.Address] = tree
	}
	h := &types.Header{ParentHash: common.HexToHash(f.bank.StateAnchor.ParentHash), UncleHash: types.EmptyUncleHash, Root: f.state.Hash(), TxHash: types.EmptyTxsHash, ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), Number: new(big.Int).SetUint64(f.bank.StateAnchor.Number), GasLimit: 30000000, Time: f.bank.StateAnchor.Timestamp, Extra: []byte{}, BaseFee: big.NewInt(1)}
	raw, e := json.Marshal(h)
	if e != nil {
		t.Fatal(e)
	}
	if e = json.Unmarshal(raw, &f.header); e != nil {
		t.Fatal(e)
	}
	f.header["transactions"] = []any{}
	f.bank.StateAnchor.Hash = h.Hash().Hex()
	f.bank.Pool.V2.Swap.BankBlockHash = h.Hash().Hex()
	return f
}
func (f *v2ProofBank) proof(t *testing.T, address string, keys []string) emptyStateProof {
	t.Helper()
	a := f.bank.account(t, address)
	tree := f.storage[address]
	root := tree.Hash()
	p := emptyStateProof{Address: address, AccountProof: v2TrieNodes(t, f.state, crypto.Keccak256(common.FromHex(address))), Balance: "0x0", Nonce: "0x0", CodeHash: crypto.Keccak256Hash(common.FromHex(a.Code)).Hex(), StorageHash: root.Hex(), StorageProof: []emptyStateSlot{}}
	for _, key := range keys {
		value := "0x0"
		encoded, e := tree.Get(crypto.Keccak256(common.HexToHash(key).Bytes()))
		if e != nil {
			t.Fatal(e)
		}
		if len(encoded) > 0 {
			var decoded []byte
			if e = rlp.DecodeBytes(encoded, &decoded); e != nil {
				t.Fatal(e)
			}
			value = "0x" + new(big.Int).SetBytes(decoded).Text(16)
		}
		p.StorageProof = append(p.StorageProof, emptyStateSlot{Key: key, Value: value, Proof: v2TrieNodes(t, tree, crypto.Keccak256(common.HexToHash(key).Bytes()))})
	}
	return p
}
func (f *v2ProofBank) adapter(t *testing.T, fault string) *Adapter {
	t.Helper()
	base := &completeV2RPC{t: t, bank: f.bank, seen: map[string]int{}}
	if strings.HasPrefix(fault, "getter-") || fault == "empty-token-storage" {
		base.mutate = func(method, to, data string, result any) any {

			if fault == "empty-token-storage" && method == "eth_call" && to == f.bank.Pool.Token0 && strings.HasPrefix(data, "0x70a08231") {
				return completeV2Word(0)
			}
			if fault == "getter-token-balance" && method == "eth_call" && to == f.bank.Pool.Token0 && strings.HasPrefix(data, "0x70a08231") {
				return completeV2Word(1)
			}
			if method == "eth_call" && to == f.bank.Pool.PoolID {
				switch fault {
				case "getter-reserves":
					if data == "0x0902f1ac" {
						return completeV2Word(1) + completeV2Word(2)[2:] + completeV2Word(3)[2:]
					}
				case "getter-cumulative":
					if data == "0x5909c0d5" {
						return completeV2Word(1)
					}
				}
			}
			return result
		}
	}
	server := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		var q struct {
			ID     int               `json:"id"`
			Method string            `json:"method"`
			Params []json.RawMessage `json:"params"`
		}
		body, e := io.ReadAll(r.Body)
		if e != nil {
			t.Error(e)
			return
		}
		raw := string(body)
		if e = json.Unmarshal([]byte(raw), &q); e != nil {
			t.Error(e)
			return
		}
		respond := func(value any) {
			json.NewEncoder(w).Encode(map[string]any{"jsonrpc": "2.0", "id": q.ID, "result": value})
		}
		switch q.Method {
		case "eth_chainId":
			respond("0x1")
		case "eth_getBlockByNumber":
			var tag string
			json.Unmarshal(q.Params[0], &tag)
			if tag == "0x0" {
				g, _ := knownBodyGenesis("evm:1")
				respond(map[string]any{"hash": g})
			} else {
				respond(f.header)
			}
		case "eth_getBlockReceipts":
			respond([]any{})
		case "eth_getProof":
			f.proofReads.Add(1)
			var address string
			var keys []string
			var anchor map[string]any
			json.Unmarshal(q.Params[0], &address)
			json.Unmarshal(q.Params[1], &keys)
			json.Unmarshal(q.Params[2], &anchor)
			if anchor["blockHash"] != f.bank.StateAnchor.Hash || anchor["requireCanonical"] != true || len(anchor) != 2 {
				t.Error("proof not hash-pinned")
			}
			p := f.proof(t, address, keys)
			if fault == "missing-storage" && len(p.StorageProof) > 0 {
				p.StorageProof = p.StorageProof[:len(p.StorageProof)-1]
			}
			if fault == "bad-node" {
				p.AccountProof = []string{"0xc0"}
			}

			if fault == "missing-token-storage" && address == f.bank.Pool.Token0 && len(p.StorageProof) > 0 {
				p.StorageProof = []emptyStateSlot{}
			}
			if fault == "token-code-mismatch" && address == f.bank.Pool.Token0 {
				p.CodeHash = crypto.Keccak256Hash([]byte{0}).Hex()
			}
			if fault == "wrong-account" {
				p.Address = "0x" + strings.Repeat("11", 20)
			}
			if fault == "committed-pair-code" && address == f.bank.Pool.PoolID {
				p.CodeHash = crypto.Keccak256Hash([]byte{0}).Hex()
			}
			respond(p)
		default:
			r.Body = io.NopCloser(strings.NewReader(raw))
			base.serve(w, r)
		}
	}))
	t.Cleanup(server.Close)
	genesis, _ := knownBodyGenesis("evm:1")
	return &Adapter{Client: rpc.New([]string{server.URL}, 3*time.Second, 16<<20), Chain: config.Chain{Key: "evm:1", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto", NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}, Pools: []config.PoolConfig{f.bank.config()}}}
}
func TestV2CompleteNativeStateProofNormalReadBindsOriginalPairAndFactory(t *testing.T) {
	f := newV2ProofBank(t, "")
	a := f.adapter(t, "")
	ref, pools, e := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
	if e != nil || len(pools) != 1 {
		t.Fatal(e, len(pools))
	}
	if ref.Hash != f.bank.StateAnchor.Hash || pools[0].V2.Swap.BankBlockHash != ref.Hash || f.proofReads.Load() != 6 {
		t.Fatal("normal state did not verify both commitments", ref, f.proofReads.Load())
	}
	if out := os.Getenv("XONE_V2_NATIVE_PROOF_OUTPUT"); out != "" {
		if e = os.MkdirAll(out, 0700); e != nil {
			t.Fatal(e)
		}
		raw, _ := json.MarshalIndent(map[string]any{"scope": "SYNTHETIC_HASHED_HEADER_REAL_MPT_ORIGINAL_RUNTIME_AND_STORAGE", "reference": ref, "pool": pools[0], "proof_requests": f.proofReads.Load(), "consensus_verified": false, "token_balances_proven": true}, "", "  ")
		if e = os.WriteFile(filepath.Join(out, "normal-pair-factory-state-proof.json"), raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
}
func TestV2CompleteNativeStateProofRejectsGetterAndProofContradictions(t *testing.T) {
	for _, fault := range []string{"getter-reserves", "getter-cumulative", "missing-storage", "bad-node", "wrong-account", "committed-pair-code", "committed-identity", "committed-reserves", "committed-cumulative", "committed-lock", "committed-forward-map", "committed-reverse-map"} {
		t.Run(fault, func(t *testing.T) {
			f := newV2ProofBank(t, fault)
			a := f.adapter(t, fault)
			_, pools, e := a.ReadState(context.Background(), fmt.Sprintf("0x%x", f.bank.StateAnchor.Number), true)
			if e == nil || len(pools) != 0 {
				t.Fatal("contradiction published", fault, e, pools)
			}
		})
	}
}

func TestV2CompleteNativeStateProofFailureExhaustsSharedSessionAndHonorsCancellation(t *testing.T) {
	for _, fault := range []string{"budget", "canceled", "bank-timestamp", "bank-hash"} {
		t.Run(fault, func(t *testing.T) {
			f := newV2ProofBank(t, "")
			a := f.adapter(t, "")
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			block, e := a.Block(ctx, fmt.Sprintf("0x%x", f.bank.StateAnchor.Number))
			if e != nil {
				t.Fatal(e)
			}
			block, _, e = a.ReadQualifiedReceipts(ctx, block, true)
			if e != nil {
				t.Fatal(e)
			}
			reader, e := a.nativeStateReader(ctx, block)
			if e != nil {
				t.Fatal(e)
			}
			pool, e := reader.readV2Complete(ctx, f.bank.config(), map[string]any{"blockHash": block.Hash, "requireCanonical": true}, domain.BlockRef{ChainKey: a.Chain.Key, Number: f.bank.StateAnchor.Number, Hash: block.Hash, ParentHash: block.ParentHash, TimestampMS: f.bank.StateAnchor.Timestamp * 1000})
			if e != nil {
				t.Fatal(e)
			}
			switch fault {
			case "budget":
				block.nativeSession.remaining = 64
			case "canceled":
				cancel()
			case "bank-timestamp":
				pool.V2.Swap.BankUnixTimestamp++
			case "bank-hash":
				pool.V2.Swap.BankBlockHash = "0x" + strings.Repeat("11", 32)
			}
			if e = a.verifyNativeV2CompleteState(ctx, reader, block, pool); e == nil {
				t.Fatal("invalid proof context accepted")
			}
			if block.nativeSession.remaining != 0 {
				t.Fatal("failed session left reusable proof budget")
			}
			before := f.proofReads.Load()
			if _, e = a.ReadNativeContractAccountProof(ctx, block, pool.PoolID, []string{"0x8"}); e == nil {
				t.Fatal("failed proof session reused")
			}
			if f.proofReads.Load() != before {
				t.Fatal("failed session did more proof IO")
			}
		})
	}
}

func TestV2AccountBatchProvesEveryAccountAtOneVerifiedAnchor(t *testing.T) {
	f := newV2ProofBank(t, "")
	a := f.adapter(t, "")
	ctx := context.Background()
	block, err := a.Block(ctx, fmt.Sprintf("0x%x", f.bank.StateAnchor.Number))
	if err != nil {
		t.Fatal(err)
	}
	block, _, err = a.ReadQualifiedReceipts(ctx, block, true)
	if err != nil {
		t.Fatal(err)
	}
	batch := accountProofBatch{adapter: a, block: block, ctx: ctx}
	address := f.bank.Pool.PoolID
	keys := []string{"0x8"}
	before := f.proofReads.Load()
	first, err := batch.read(address, keys)
	if err != nil {
		t.Fatal(err)
	}
	second, err := batch.read(address, keys)
	if err != nil {
		t.Fatal(err)
	}
	if first.Account.StorageRoot != second.Account.StorageRoot || first.ProofBytesUsed <= second.ProofBytesUsed || f.proofReads.Load() != before+2 {
		t.Fatalf("unexpected proof reuse: %d/%d, reads %d", first.ProofBytesUsed, second.ProofBytesUsed, f.proofReads.Load()-before)
	}
	// A standalone request must verify its own evidence even after a successful batch.
	block.NativeCoverage.Evidence.NativeSemanticsID = "invalid"
	if _, err = a.ReadNativeContractAccountProof(ctx, block, address, keys); err == nil {
		t.Fatal("standalone read reused the batch anchor")
	}
	if _, err = batch.read(address, keys); err == nil || f.proofReads.Load() != before+2 {
		t.Fatal("rejected session admitted a cached read", err)
	}
}
