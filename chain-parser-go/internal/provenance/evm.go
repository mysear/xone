// Package provenance verifies retained source commitments independently of the
// quote kernel. A matching provider header is not consensus verification.
package provenance

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethdb/memorydb"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
)

const MaxProofBytes = 1024 * 1024
const MaxEvidenceBytes = 32 * 1024 * 1024

type StorageProof struct {
	Key   string   `json:"key"`
	Value string   `json:"value"`
	Proof []string `json:"proof"`
}
type AccountProof struct {
	Address      string         `json:"address"`
	Balance      string         `json:"balance"`
	Nonce        string         `json:"nonce"`
	CodeHash     string         `json:"codeHash"`
	StorageHash  string         `json:"storageHash"`
	AccountProof []string       `json:"accountProof"`
	StorageProof []StorageProof `json:"storageProof"`
}
type Deployment struct {
	Address string       `json:"address"`
	Code    string       `json:"code"`
	Proof   AccountProof `json:"proof"`
}
type Evidence struct {
	SchemaVersion     int             `json:"schema_version"`
	ChainID           string          `json:"chain_id"`
	ExpectedBlockHash string          `json:"expected_block_hash"`
	Block             json.RawMessage `json:"block"`
	RawTransactions   []string        `json:"raw_transactions"`
	Deployments       []Deployment    `json:"deployments"`
}
type DeploymentReport struct {
	Address          string `json:"address"`
	RuntimeSHA256    string `json:"runtime_sha256"`
	RuntimeKeccak256 string `json:"runtime_keccak256"`
	StorageSlots     int    `json:"storage_slots"`
}
type Report struct {
	Passed                bool               `json:"passed"`
	ChainID               string             `json:"chain_id"`
	BlockHash             string             `json:"block_hash"`
	BlockNumber           uint64             `json:"block_number"`
	Transactions          int                `json:"transactions"`
	Deployments           []DeploymentReport `json:"deployments"`
	ConsensusVerified     bool               `json:"consensus_verified"`
	SourceRebuildVerified bool               `json:"source_rebuild_verified"`
	Authority             string             `json:"authority"`
}

func invalid(reason string) error { return errors.New("PROVENANCE_" + reason) }
func data(value string, width int) ([]byte, error) {
	if !strings.HasPrefix(value, "0x") || len(value)%2 != 0 || width >= 0 && len(value) != 2+2*width {
		return nil, invalid("HEX")
	}
	b, e := hex.DecodeString(value[2:])
	if e != nil {
		return nil, invalid("HEX")
	}
	return b, nil
}
func quantity(value string) (*big.Int, error) {
	if !strings.HasPrefix(value, "0x") || len(value) < 3 || len(value) > 3 && value[2] == '0' {
		return nil, invalid("QUANTITY")
	}
	n, ok := new(big.Int).SetString(value[2:], 16)
	if !ok || n.Sign() < 0 || n.BitLen() > 256 || "0x"+n.Text(16) != value {
		return nil, invalid("QUANTITY")
	}
	return n, nil
}
func proof(root common.Hash, key []byte, nodes []string) ([]byte, error) {
	db := memorydb.New()
	defer db.Close()
	size := 0
	if len(nodes) > 1024 {
		return nil, invalid("PROOF_LIMIT")
	}
	for _, node := range nodes {
		if len(node) > 2+MaxProofBytes*2 {
			return nil, invalid("PROOF_LIMIT")
		}
		b, e := data(node, -1)
		if e != nil {
			return nil, e
		}
		size += len(b)
		if size > MaxProofBytes {
			return nil, invalid("PROOF_LIMIT")
		}
		if e = db.Put(crypto.Keccak256(b), b); e != nil {
			return nil, invalid("PROOF_STORAGE")
		}
	}
	value, e := trie.VerifyProof(root, key, db)
	if e != nil {
		return nil, invalid("PROOF")
	}
	return value, nil
}
func Verify(e Evidence) (Report, error) {
	var zero Report
	if e.SchemaVersion != 1 {
		return zero, invalid("SCHEMA")
	}
	chain, ok := new(big.Int).SetString(e.ChainID, 10)
	if !ok || chain.Sign() <= 0 || chain.Text(10) != e.ChainID {
		return zero, invalid("CHAIN")
	}
	if _, err := data(e.ExpectedBlockHash, 32); err != nil {
		return zero, invalid("HEADER_ANCHOR_REQUIRED")
	}
	raw, err := json.Marshal(e)
	if err != nil || len(raw) > MaxEvidenceBytes {
		return zero, invalid("EVIDENCE_LIMIT")
	}
	var h types.Header
	if json.Unmarshal(e.Block, &h) != nil || h.Number == nil || !h.Number.IsUint64() {
		return zero, invalid("HEADER")
	}
	if !strings.EqualFold(h.Hash().Hex(), e.ExpectedBlockHash) {
		return zero, invalid("HEADER_HASH")
	}
	if e.RawTransactions == nil || len(e.RawTransactions) > 100000 {
		return zero, invalid("TRANSACTIONS_MISSING_OR_LIMIT")
	}
	txs := make(types.Transactions, len(e.RawTransactions))
	seen := map[common.Hash]bool{}
	for i, encoded := range e.RawTransactions {
		b, err := data(encoded, -1)
		if err != nil {
			return zero, err
		}
		tx := new(types.Transaction)
		if tx.UnmarshalBinary(b) != nil {
			return zero, invalid("TRANSACTION_ENCODING")
		}
		if tx.Protected() && tx.ChainId().Cmp(chain) != 0 {
			return zero, invalid("TRANSACTION_CHAIN")
		}
		if seen[tx.Hash()] {
			return zero, invalid("DUPLICATE_TRANSACTION")
		}
		seen[tx.Hash()] = true
		txs[i] = tx
	}
	if types.DeriveSha(txs, trie.NewStackTrie(nil)) != h.TxHash {
		return zero, invalid("TRANSACTIONS_ROOT")
	}
	report := Report{Passed: true, ChainID: e.ChainID, BlockHash: h.Hash().Hex(), BlockNumber: h.Number.Uint64(), Transactions: len(txs), Deployments: []DeploymentReport{}, Authority: "commitments-checked-against-explicit-header-hash; anchor trust and source rebuild remain external"}
	addresses := map[common.Address]bool{}
	if len(e.Deployments) == 0 || len(e.Deployments) > 1024 {
		return zero, invalid("DEPLOYMENT_LIMIT")
	}
	for _, deployment := range e.Deployments {
		a, err := data(deployment.Address, 20)
		if err != nil {
			return zero, err
		}
		address := common.BytesToAddress(a)
		if addresses[address] {
			return zero, invalid("DUPLICATE_DEPLOYMENT")
		}
		addresses[address] = true
		if !strings.EqualFold(deployment.Proof.Address, deployment.Address) {
			return zero, invalid("PROOF_ADDRESS")
		}
		accountRaw, err := proof(h.Root, crypto.Keccak256(a), deployment.Proof.AccountProof)
		if err != nil {
			return zero, err
		}
		if len(accountRaw) == 0 {
			return zero, invalid("DEPLOYMENT_ABSENT")
		}
		var account types.StateAccount
		if rlp.DecodeBytes(accountRaw, &account) != nil || account.Balance == nil {
			return zero, invalid("ACCOUNT_ENCODING")
		}
		balance, err := quantity(deployment.Proof.Balance)
		if err != nil {
			return zero, err
		}
		nonce, err := quantity(deployment.Proof.Nonce)
		if err != nil || !nonce.IsUint64() {
			return zero, invalid("NONCE")
		}
		codeHash, err := data(deployment.Proof.CodeHash, 32)
		if err != nil {
			return zero, err
		}
		storageRoot, err := data(deployment.Proof.StorageHash, 32)
		if err != nil {
			return zero, err
		}
		if nonce.Uint64() != account.Nonce || balance.Cmp(account.Balance.ToBig()) != 0 || !bytes.Equal(codeHash, account.CodeHash) || !bytes.Equal(storageRoot, account.Root.Bytes()) {
			return zero, invalid("ACCOUNT_FIELDS")
		}
		code, err := data(deployment.Code, -1)
		if err != nil {
			return zero, err
		}
		if len(code) == 0 || !bytes.Equal(crypto.Keccak256(code), account.CodeHash) {
			return zero, invalid("DEPLOYMENT_CODE_HASH")
		}
		keys := map[common.Hash]bool{}
		for _, slot := range deployment.Proof.StorageProof {
			key, err := data(slot.Key, 32)
			if err != nil {
				return zero, err
			}
			k := common.BytesToHash(key)
			if keys[k] {
				return zero, invalid("DUPLICATE_STORAGE")
			}
			keys[k] = true
			expected, err := quantity(slot.Value)
			if err != nil {
				return zero, err
			}
			stored, err := proof(account.Root, crypto.Keccak256(key), slot.Proof)
			if err != nil {
				return zero, err
			}
			var value []byte
			if len(stored) > 0 {
				if rlp.DecodeBytes(stored, &value) != nil || len(value) > 32 || len(value) > 0 && value[0] == 0 {
					return zero, invalid("STORAGE_ENCODING")
				}
			}
			if new(big.Int).SetBytes(value).Cmp(expected) != 0 {
				return zero, invalid("STORAGE_VALUE")
			}
		}
		sum := sha256.Sum256(code)
		report.Deployments = append(report.Deployments, DeploymentReport{Address: address.Hex(), RuntimeSHA256: fmt.Sprintf("%x", sum), RuntimeKeccak256: common.BytesToHash(account.CodeHash).Hex(), StorageSlots: len(keys)})
	}
	return report, nil
}
