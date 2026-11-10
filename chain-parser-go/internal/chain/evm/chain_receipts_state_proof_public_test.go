package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"math/big"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethdb/memorydb"
	"github.com/ethereum/go-ethereum/trie"
)

func TestPublicAvalancheAbsenceCompatibilityKeepsStorageContradictionsClosed(t *testing.T) {
	root := "../../../test/fixtures/native-state-proof/testdata/avalanche-04"
	raw, err := os.ReadFile(filepath.Join(root, "prestate-evidence.json"))
	if err != nil {
		t.Fatal(err)
	}
	provenanceRaw, err := os.ReadFile(filepath.Join(root, "provenance.json"))
	if err != nil {
		t.Fatal(err)
	}
	var provenance struct {
		Bytes          int      `json:"bytes"`
		SHA256         string   `json:"sha256"`
		Absence        []string `json:"absence_addresses"`
		Contradictions []string `json:"must_reject_storage_value_mismatch"`
	}
	if err = json.Unmarshal(provenanceRaw, &provenance); err != nil {
		t.Fatal(err)
	}
	digest := sha256.Sum256(raw)
	if provenance.Bytes != len(raw) || provenance.SHA256 != hex.EncodeToString(digest[:]) {
		t.Fatal("independently tracked public fixture bytes changed")
	}
	var input struct {
		Expected    string            `json:"expected_block_hash"`
		Headers     []json.RawMessage `json:"native_headers"`
		Deployments []struct {
			Address string          `json:"address"`
			Proof   json.RawMessage `json:"proof"`
		} `json:"deployments"`
	}
	if err = json.Unmarshal(raw, &input); err != nil {
		t.Fatal(err)
	}
	var previous *ChainHeaderCommitment
	var anchor *ExecutionStateAnchor
	for _, h := range input.Headers {
		var fields struct{ Transactions []json.RawMessage }
		if err = json.Unmarshal(h, &fields); err != nil {
			t.Fatal(err)
		}
		headerJSON := h
		if len(fields.Transactions) > 0 && len(fields.Transactions[0]) > 0 && fields.Transactions[0][0] == '{' {
			headerJSON, _, err = projectFullRPCBlock(h)
			if err != nil {
				t.Fatal(err)
			}
		}
		header, err := VerifyChainHeaderJSON(context.Background(), "evm:43114", headerJSON)
		if err != nil {
			t.Fatal("native public header invalid", err)
		}
		height, _ := canonicalIndex(header.Block.Number)
		if previous != nil {
			prior, _ := canonicalIndex(previous.Block.Number)
			if height != prior+1 || header.Block.ParentHash != previous.Block.Hash {
				t.Fatal("public successor chain broken")
			}
		}
		if height == 99656725 && header.Block.Hash != input.Expected {
			t.Fatal("public target identity changed")
		}
		if header.SettledHeight != nil && *header.SettledHeight == 99656725 {
			anchor = &ExecutionStateAnchor{AccountEncodingID: "coreth-account-held-xchain-rlp/v1", Height: 99656725, BlockHash: input.Expected, StateRoot: header.StateRoot, SettlingHeight: height, SettlingHash: header.Block.Hash, Authority: "hashed-native-successor-chain/public-fixture"}
		}
		copy := header
		previous = &copy
	}
	if anchor == nil || anchor.SettlingHeight != 99656733 {
		t.Fatal("actual target state not bound to settling successor", anchor)
	}
	absent, contradiction := map[string]bool{}, map[string]bool{}
	for _, a := range provenance.Absence {
		absent[a] = true
	}
	for _, a := range provenance.Contradictions {
		contradiction[a] = true
	}
	accepted, rejected, absentCount := 0, 0, 0
	for _, dep := range input.Deployments {
		t.Run(dep.Address, func(t *testing.T) {
			result, err := VerifyExecutionAccountProof(context.Background(), *anchor, dep.Proof, 64<<20)
			if contradiction[dep.Address] {
				if err == nil || err.Error() != "STATE_STORAGE_VALUE_MISMATCH" {
					t.Fatal("real nonzero RPC claim versus absence-path contradiction was released", err)
				}
				var proof emptyStateProof
				if err = json.Unmarshal(dep.Proof, &proof); err != nil {
					t.Fatal(err)
				}
				accountOnly := proof
				accountOnly.StorageProof = []emptyStateSlot{}
				actual, err := verifySyntheticState(t, *anchor, accountOnly)
				if err != nil || !actual.Exists || actual.StorageRoot == types.EmptyRootHash.Hex() {
					t.Fatal("contradiction must retain a valid nonempty account/storage root", err, actual)
				}
				contradictions := 0
				for _, slot := range proof.StorageProof {
					n, ok := new(big.Int).SetString(strings.TrimPrefix(slot.Value, "0x"), 16)
					if !ok {
						t.Fatal("bad public quantity")
					}
					if n.Sign() == 0 {
						continue
					}
					db := memorydb.New()
					for _, node := range slot.Proof {
						b, err := hex.DecodeString(strings.TrimPrefix(node, "0x"))
						if err != nil {
							t.Fatal(err)
						}
						if err = db.Put(crypto.Keccak256(b), b); err != nil {
							t.Fatal(err)
						}
					}
					key, err := storageProofKey(slot.Key)
					if err != nil {
						t.Fatal(err)
					}
					leaf, why := trie.VerifyProof(common.HexToHash(actual.StorageRoot), crypto.Keccak256(key), db)
					db.Close()
					if why == nil && leaf == nil {
						contradictions++
					}
				}
				if contradictions == 0 {
					t.Fatal("public nonzero-claim/absence-path control disappeared")
				}
				rejected++
				return
			}
			if err != nil {
				t.Fatal("public account proof rejected", err)
			}
			if absent[dep.Address] {
				absentCount++
				if result.Exists || result.Nonce != 0 || result.Balance != "0x0" || result.CodeHash != types.EmptyCodeHash.Hex() || result.StorageRoot != types.EmptyRootHash.Hex() {
					t.Fatal("public absence not normalized", result)
				}
				if dep.Address == "0xfdea03dbcbeaa743f7f0cb1ae0d1c6677b88852d" && len(result.StorageValues) != 2 {
					t.Fatal("public absent account's two zero slots lost")
				}
			} else if !result.Exists {
				t.Fatal("public present account changed to absent")
			}
			accepted++
		})
	}
	if accepted != 9 || rejected != 4 || absentCount != 3 {
		t.Fatalf("unexpected public proof coverage: accepted=%d rejected=%d absent=%d", accepted, rejected, absentCount)
	}
}
