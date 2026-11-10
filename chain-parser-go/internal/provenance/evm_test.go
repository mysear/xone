package provenance

import (
	"encoding/hex"
	"encoding/json"
	"math/big"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
)

func fixture(t *testing.T) Evidence {
	t.Helper()
	tx := types.NewTx(&types.LegacyTx{Nonce: 1, GasPrice: big.NewInt(2), Gas: 21000, To: ptr(common.HexToAddress("0x1234")), Value: big.NewInt(4)})
	raw, e := tx.MarshalBinary()
	if e != nil {
		t.Fatal(e)
	}
	code := []byte{0x60, 0x00}
	address := common.HexToAddress("0x1234")
	account := types.NewEmptyStateAccount()
	account.Nonce = 3
	account.CodeHash = crypto.Keccak256(code)
	accountRaw, e := rlp.EncodeToBytes(account)
	if e != nil {
		t.Fatal(e)
	}
	// A single Ethereum MPT leaf: hex-prefix encoded 64-nibble account key.
	leaf, e := rlp.EncodeToBytes([]any{append([]byte{0x20}, crypto.Keccak256(address.Bytes())...), accountRaw})
	if e != nil {
		t.Fatal(e)
	}
	h := &types.Header{ParentHash: common.HexToHash("0x1"), UncleHash: types.EmptyUncleHash, Coinbase: common.Address{}, Root: crypto.Keccak256Hash(leaf), TxHash: types.DeriveSha(types.Transactions{tx}, trie.NewStackTrie(nil)), ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), Number: big.NewInt(42), GasLimit: 30000000, GasUsed: 21000, Time: 1, Extra: []byte{}}
	header, e := json.Marshal(h)
	if e != nil {
		t.Fatal(e)
	}
	return Evidence{SchemaVersion: 1, ChainID: "1", ExpectedBlockHash: h.Hash().Hex(), Block: header, RawTransactions: []string{"0x" + hex.EncodeToString(raw)}, Deployments: []Deployment{{Address: address.Hex(), Code: "0x" + hex.EncodeToString(code), Proof: AccountProof{Address: address.Hex(), Balance: "0x0", Nonce: "0x3", CodeHash: common.BytesToHash(account.CodeHash).Hex(), StorageHash: account.Root.Hex(), AccountProof: []string{"0x" + hex.EncodeToString(leaf)}}}}}
}
func ptr[T any](v T) *T { return &v }
func TestProofChecksHeaderBodyDeploymentAndAccountCommitments(t *testing.T) {
	e := fixture(t)
	r, err := Verify(e)
	if err != nil {
		t.Fatal(err)
	}
	if r.BlockNumber != 42 || r.Transactions != 1 || len(r.Deployments) != 1 || r.ConsensusVerified || r.SourceRebuildVerified {
		t.Fatalf("unexpected report %+v", r)
	}
	if len(r.Deployments[0].RuntimeSHA256) != 64 {
		t.Fatal("missing runtime identity")
	}
}
func TestProofRejectsChangedHeaderTransactionsCodeAndAccountFields(t *testing.T) {
	changes := map[string]func(*Evidence){
		"no-independent-hash": func(e *Evidence) { e.ExpectedBlockHash = "" },
		"header": func(e *Evidence) {
			var h types.Header
			_ = json.Unmarshal(e.Block, &h)
			h.Time++
			e.Block, _ = json.Marshal(&h)
		},
		"missing-body":    func(e *Evidence) { e.RawTransactions = nil },
		"empty-body":      func(e *Evidence) { e.RawTransactions = []string{} },
		"changed-body":    func(e *Evidence) { e.RawTransactions[0] = "0x00" },
		"code":            func(e *Evidence) { e.Deployments[0].Code = "0x6001" },
		"proof-node":      func(e *Evidence) { e.Deployments[0].Proof.AccountProof[0] = "0xc0" },
		"wrong-address":   func(e *Evidence) { e.Deployments[0].Address = common.HexToAddress("0x1235").Hex() },
		"nonce":           func(e *Evidence) { e.Deployments[0].Proof.Nonce = "0x4" },
		"balance":         func(e *Evidence) { e.Deployments[0].Proof.Balance = "0x1" },
		"storage-root":    func(e *Evidence) { e.Deployments[0].Proof.StorageHash = common.Hash{}.Hex() },
		"unknown-version": func(e *Evidence) { e.SchemaVersion = 2 },
		"chain":           func(e *Evidence) { e.ChainID = "0" },
		"duplicates":      func(e *Evidence) { e.Deployments = append(e.Deployments, e.Deployments[0]) },
	}
	for name, change := range changes {
		t.Run(name, func(t *testing.T) {
			e := fixture(t)
			change(&e)
			if _, err := Verify(e); err == nil {
				t.Fatal("tampered evidence accepted")
			}
		})
	}
}
func TestProofChecksStorageTrieValuesAndMissingLeaves(t *testing.T) {
	e := fixture(t)
	slot := common.HexToHash("0x6")
	valueRaw, _ := rlp.EncodeToBytes([]byte{0x2a})
	leaf, _ := rlp.EncodeToBytes([]any{append([]byte{0x20}, crypto.Keccak256(slot.Bytes())...), valueRaw})
	storageRoot := crypto.Keccak256Hash(leaf)
	account := types.NewEmptyStateAccount()
	account.Nonce = 3
	account.CodeHash = crypto.Keccak256([]byte{0x60, 0})
	account.Root = storageRoot
	raw, _ := rlp.EncodeToBytes(account)
	accountLeaf, _ := rlp.EncodeToBytes([]any{append([]byte{0x20}, crypto.Keccak256(common.HexToAddress(e.Deployments[0].Address).Bytes())...), raw})
	var h types.Header
	_ = json.Unmarshal(e.Block, &h)
	h.Root = crypto.Keccak256Hash(accountLeaf)
	e.Block, _ = json.Marshal(&h)
	e.ExpectedBlockHash = h.Hash().Hex()
	p := &e.Deployments[0].Proof
	p.StorageHash = storageRoot.Hex()
	p.AccountProof = []string{"0x" + hex.EncodeToString(accountLeaf)}
	p.StorageProof = []StorageProof{{Key: slot.Hex(), Value: "0x2a", Proof: []string{"0x" + hex.EncodeToString(leaf)}}}
	if _, err := Verify(e); err != nil {
		t.Fatal(err)
	}
	p.StorageProof[0].Value = "0x2b"
	if _, err := Verify(e); err == nil {
		t.Fatal("storage value mutation accepted")
	}
	p.StorageProof[0].Value = "0x0"
	p.StorageProof[0].Proof = nil
	if _, err := Verify(e); err == nil {
		t.Fatal("missing nonempty storage proof accepted")
	}
}
func TestProofInputBudgetsAndCanonicalHexFailClosed(t *testing.T) {
	e := fixture(t)
	e.Deployments[0].Proof.AccountProof = append(e.Deployments[0].Proof.AccountProof, "0x"+strings.Repeat("00", MaxProofBytes+1))
	if _, err := Verify(e); err == nil {
		t.Fatal("oversized node accepted")
	}
	e = fixture(t)
	e.Deployments[0].Proof.Nonce = "0x03"
	if _, err := Verify(e); err == nil {
		t.Fatal("noncanonical quantity accepted")
	}
}
