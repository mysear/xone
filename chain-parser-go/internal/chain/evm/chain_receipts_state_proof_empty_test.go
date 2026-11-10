package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethdb/memorydb"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
)

type emptyStateSlot struct {
	Key   string   `json:"key"`
	Value string   `json:"value"`
	Proof []string `json:"proof"`
}
type emptyStateProof struct {
	Address      string           `json:"address"`
	AccountProof []string         `json:"accountProof"`
	Balance      string           `json:"balance"`
	Nonce        string           `json:"nonce"`
	CodeHash     string           `json:"codeHash"`
	StorageHash  string           `json:"storageHash"`
	StorageProof []emptyStateSlot `json:"storageProof"`
}

func syntheticStateProof(t *testing.T, encoding string, exists, emptyStorage bool) (ExecutionStateAnchor, emptyStateProof) {
	t.Helper()
	// Fresh in-memory tries do not need a persisted node database.
	storage := trie.NewEmpty(nil)
	if !emptyStorage {
		for _, n := range []byte{1, 2} {
			key := common.BigToHash(new(big.Int).SetUint64(uint64(n)))
			encoded, _ := rlp.EncodeToBytes([]byte{n + 6})
			if err := storage.Update(crypto.Keccak256(key.Bytes()), encoded); err != nil {
				t.Fatal(err)
			}
		}
	}
	root := storage.Hash()
	accountItems := []any{uint64(0), new(big.Int), root, types.EmptyCodeHash.Bytes()}
	if encoding == "coreth-account-held-xchain-rlp/v1" {
		accountItems = append(accountItems, false)
	}
	account, _ := rlp.EncodeToBytes(accountItems)
	state := trie.NewEmpty(nil)
	other := common.HexToAddress("0x10")
	address := common.HexToAddress("0x20")
	if err := state.Update(crypto.Keccak256(other.Bytes()), account); err != nil {
		t.Fatal(err)
	}
	if exists {
		if err := state.Update(crypto.Keccak256(address.Bytes()), account); err != nil {
			t.Fatal(err)
		}
	}
	nodes := func(tree *trie.Trie, key []byte) []string {
		t.Helper()
		proof := memorydb.New()
		defer proof.Close()
		if err := tree.Prove(key, proof); err != nil {
			t.Fatal(err)
		}
		result := []string{}
		it := proof.NewIterator(nil, nil)
		defer it.Release()
		for it.Next() {
			result = append(result, "0x"+hex.EncodeToString(it.Value()))
		}
		if err := it.Error(); err != nil {
			t.Fatal(err)
		}
		return result
	}
	p := emptyStateProof{Address: address.Hex(), AccountProof: nodes(state, crypto.Keccak256(address.Bytes())), Balance: "0x0", Nonce: "0x0", CodeHash: types.EmptyCodeHash.Hex(), StorageHash: root.Hex(), StorageProof: []emptyStateSlot{}}
	if !exists {
		p.StorageHash = types.EmptyRootHash.Hex()
	}
	for _, n := range []int64{1, 3} {
		key := common.BigToHash(big.NewInt(n))
		value := "0x0"
		proof := []string{}
		if exists && !emptyStorage {
			proof = nodes(storage, crypto.Keccak256(key.Bytes()))
			if n == 1 {
				value = "0x7"
			}
		}
		p.StorageProof = append(p.StorageProof, emptyStateSlot{key.Hex(), value, proof})
	}
	hash := common.HexToHash("0x1234").Hex()
	return ExecutionStateAnchor{AccountEncodingID: encoding, Height: 1, BlockHash: hash, StateRoot: state.Hash().Hex(), SettlingHeight: 1, SettlingHash: hash, Authority: "synthetic-real-mpt-test"}, p
}
func verifySyntheticState(t *testing.T, a ExecutionStateAnchor, p emptyStateProof) (VerifiedAccountCommitment, error) {
	t.Helper()
	raw, err := json.Marshal(p)
	if err != nil {
		t.Fatal(err)
	}
	return VerifyExecutionAccountProof(context.Background(), a, raw, 1<<20)
}

func TestAccountAbsenceProofNormalizesOnlyPermittedEmptySentinels(t *testing.T) {
	zero := common.Hash{}.Hex()
	for _, encoding := range []string{"ethereum-account-rlp/v1", "coreth-account-held-xchain-rlp/v1"} {
		for _, code := range []string{zero, types.EmptyCodeHash.Hex()} {
			for _, storage := range []string{zero, types.EmptyRootHash.Hex()} {
				a, p := syntheticStateProof(t, encoding, false, true)
				p.CodeHash = code
				p.StorageHash = storage
				result, err := verifySyntheticState(t, a, p)
				if err != nil {
					t.Fatalf("proven absence rejected %s %s %s: %v", encoding, code, storage, err)
				}
				if result.Exists || result.Nonce != 0 || result.Balance != "0x0" || result.CodeHash != types.EmptyCodeHash.Hex() || result.StorageRoot != types.EmptyRootHash.Hex() || result.HeldXChainAsset != nil {
					t.Fatal("absence was not normalized", result)
				}
				for _, value := range result.StorageValues {
					if value != "0x0" {
						t.Fatal("absent account storage not zero", result)
					}
				}
			}
		}
	}
}
func TestProvenEmptyStorageAcceptsZeroSlotsWithoutTrieNodes(t *testing.T) {
	for _, encoding := range []string{"ethereum-account-rlp/v1", "coreth-account-held-xchain-rlp/v1"} {
		a, p := syntheticStateProof(t, encoding, true, true)
		result, err := verifySyntheticState(t, a, p)
		if err != nil {
			t.Fatal("empty storage still required nonexistent trie nodes", err)
		}
		if !result.Exists || result.StorageRoot != types.EmptyRootHash.Hex() || len(result.StorageValues) != 2 {
			t.Fatal(result)
		}
	}
}
func TestEmptyStateProofRejectsFalseAccountAndStorageDeclarations(t *testing.T) {
	for _, fault := range []string{"balance", "nonce", "code", "storage", "slot", "account-path", "no-account-nodes", "existing-code-zero", "existing-storage-zero", "nonempty-root-slot"} {
		t.Run(fault, func(t *testing.T) {
			a, p := syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", false, true)
			switch fault {
			case "balance":
				p.Balance = "0x1"
			case "nonce":
				p.Nonce = "0x1"
			case "code":
				p.CodeHash = common.HexToHash("0x1234").Hex()
			case "storage":
				p.StorageHash = common.HexToHash("0x1234").Hex()
			case "slot":
				p.StorageProof[0].Value = "0x1"
			case "account-path":
				p.Address = common.HexToAddress("0x10").Hex()
				p.CodeHash = common.Hash{}.Hex()
			case "no-account-nodes":
				p.AccountProof = []string{}
			case "existing-code-zero":
				a, p = syntheticStateProof(t, "ethereum-account-rlp/v1", true, true)
				p.CodeHash = common.Hash{}.Hex()
			case "existing-storage-zero":
				a, p = syntheticStateProof(t, "ethereum-account-rlp/v1", true, true)
				p.StorageHash = common.Hash{}.Hex()
			case "nonempty-root-slot":
				a, p = syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", true, false)
				p.StorageProof[1].Value = "0x1"
			}
			if _, err := verifySyntheticState(t, a, p); err == nil {
				t.Fatal("false account/storage claim accepted", fault)
			}
		})
	}
}
func TestEmptyStateProofKeepsEncodingDuplicateNodeAndBudgetGuards(t *testing.T) {
	for _, fault := range []string{"bad-quantity", "bad-key", "bad-slot-value", "duplicate-slot", "bad-node", "too-many-nodes", "duplicate-json", "null-account-proof", "null-storage-proof", "bad-hash-width", "budget", "canceled"} {
		t.Run(fault, func(t *testing.T) {
			a, p := syntheticStateProof(t, "ethereum-account-rlp/v1", true, true)
			switch fault {
			case "bad-quantity":
				p.Nonce = "0x00"
			case "bad-key":
				p.StorageProof[0].Key = "0x01"
			case "bad-slot-value":
				p.StorageProof[0].Value = "0x00"
			case "duplicate-slot":
				p.StorageProof[1].Key = "0x1"
			case "bad-node":
				p.StorageProof[0].Proof = []string{"0xzz"}
			case "too-many-nodes":
				p.StorageProof[0].Proof = make([]string, 1025)
				for i := range p.StorageProof[0].Proof {
					p.StorageProof[0].Proof[i] = "0x80"
				}
			case "bad-hash-width":
				p.StorageHash = "0x0"
			}
			raw, _ := json.Marshal(p)
			if fault == "duplicate-json" {
				raw = append([]byte(`{"nonce":"0x0",`), raw[1:]...)
			}
			if fault == "null-account-proof" {
				raw = []byte(strings.Replace(string(raw), `"accountProof":[`, `"accountProof":null,"ignored":[`, 1))
			}
			if fault == "null-storage-proof" {
				raw = []byte(strings.Replace(string(raw), `"storageProof":[`, `"storageProof":null,"ignored":[`, 1))
			}
			max := 1 << 20
			if fault == "budget" {
				max = 2*len(raw) + 511
			}
			ctx := context.Background()
			if fault == "canceled" {
				var cancel context.CancelFunc
				ctx, cancel = context.WithCancel(ctx)
				cancel()
			}
			if _, err := VerifyExecutionAccountProof(ctx, a, raw, max); err == nil {
				t.Fatal("proof guard weakened", fault)
			}
		})
	}
}
func TestNonemptyStorageProofStillVerifiesEachKey(t *testing.T) {
	a, p := syntheticStateProof(t, "ethereum-account-rlp/v1", true, false)
	result, err := verifySyntheticState(t, a, p)
	if err != nil {
		t.Fatal(err)
	}
	if result.StorageValues[common.BigToHash(big.NewInt(1)).Hex()] != "0x7" || result.StorageValues[common.BigToHash(big.NewInt(3)).Hex()] != "0x0" {
		t.Fatal(result)
	}
	p.StorageProof[0].Proof = []string{}
	if _, err = verifySyntheticState(t, a, p); err == nil {
		t.Fatal("nonempty storage accepted without its key proof")
	}
}

func TestVerifiedStorageValueDerivesOnlyAuthenticatedEmptyRoot(t *testing.T) {
	for _, exists := range []bool{false, true} {
		a, p := syntheticStateProof(t, "coreth-account-held-xchain-rlp/v1", exists, true)
		p.StorageProof = []emptyStateSlot{}
		result, err := verifySyntheticState(t, a, p)
		if err != nil {
			t.Fatal(err)
		}
		for _, key := range []string{"0x0", "0x1234", common.HexToHash("0xabcdef").Hex()} {
			value, err := result.StorageValue(key)
			if err != nil || value != "0x0" {
				t.Fatal("unlisted empty-root key not proved zero", key, value, err)
			}
		}
		if _, err := result.StorageValue("0x00"); err == nil {
			t.Fatal("storage lookup accepted noncanonical key")
		}
		raw, _ := json.Marshal(result)
		var unverified VerifiedAccountCommitment
		json.Unmarshal(raw, &unverified)
		if _, err := unverified.StorageValue("0x1"); err == nil {
			t.Fatal("serialized trust flags substituted for a proof")
		}
	}
	a, p := syntheticStateProof(t, "ethereum-account-rlp/v1", true, false)
	result, err := verifySyntheticState(t, a, p)
	if err != nil {
		t.Fatal(err)
	}
	if value, err := result.StorageValue("0x1"); err != nil || value != "0x7" {
		t.Fatal(value, err)
	}
	if value, err := result.StorageValue("0x3"); err != nil || value != "0x0" {
		t.Fatal(value, err)
	}
	if _, err := result.StorageValue("0x2"); err == nil {
		t.Fatal("unlisted nonempty-root slot treated as zero")
	}
	// Output projections are caller-editable; they cannot extend proof coverage.
	result.StorageRoot = types.EmptyRootHash.Hex()
	result.StorageValues[common.HexToHash("0x2").Hex()] = "0x0"
	result.StorageValues[common.HexToHash("0x1").Hex()] = "0x9"
	if _, err := result.StorageValue("0x2"); err == nil {
		t.Fatal("edited projection forged empty-root authority")
	}
	if value, err := result.StorageValue("0x1"); err != nil || value != "0x7" {
		t.Fatal("edited projection forged a proved slot value", value, err)
	}
}
