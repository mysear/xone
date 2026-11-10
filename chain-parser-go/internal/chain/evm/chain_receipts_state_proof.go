package evm

import (
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"math/big"
	"strings"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"github.com/ethereum/go-ethereum/crypto"
	"github.com/ethereum/go-ethereum/ethdb/memorydb"
	"github.com/ethereum/go-ethereum/rlp"
	"github.com/ethereum/go-ethereum/trie"
)

type VerifiedAccountCommitment struct {
	// Runtime proof authority is not restored from editable/serialized output.
	proofBytesUsed      int
	provenStorageRoot   *common.Hash
	provenStorageValues map[string]string
	HeldXChainAsset     *bool                `json:"held_xchain_asset,omitempty"`
	StateAnchor         ExecutionStateAnchor `json:"state_anchor"`
	Address             string               `json:"address"`
	Exists              bool                 `json:"exists"`
	Nonce               uint64               `json:"nonce"`
	Balance             string               `json:"balance"`
	CodeHash            string               `json:"code_hash"`
	StorageRoot         string               `json:"storage_root"`
	StorageValues       map[string]string    `json:"storage_values"`
}

// VerifyExecutionAccountProof verifies EIP-1186 account and storage values
// against a previously verified execution-state anchor. In particular, callers
// must use SAE's TargetStateAnchor, never the target block's stale own-header
// stateRoot. This proves committed values, not arbitrary eth_call outputs.
func VerifyExecutionAccountProof(ctx context.Context, anchor ExecutionStateAnchor, raw json.RawMessage, maxBytes int) (VerifiedAccountCommitment, error) {
	var out VerifiedAccountCommitment
	if err := ctx.Err(); err != nil {
		return out, err
	}
	if maxBytes <= 0 || maxBytes > 64<<20 || len(raw) > maxBytes/2 {
		return out, errors.New("STATE_PROOF_BUDGET")
	}
	if !receiptHex(anchor.StateRoot, 32) || !receiptHex(anchor.BlockHash, 32) || !receiptHex(anchor.SettlingHash, 32) || anchor.Authority == "" {
		return out, errors.New("STATE_PROOF_ANCHOR_INVALID")
	}
	if err := rejectDuplicateReceiptKeys(raw); err != nil {
		return out, err
	}
	var proof struct {
		Address      string          `json:"address"`
		AccountProof []string        `json:"accountProof"`
		Balance      json.RawMessage `json:"balance"`
		Nonce        string          `json:"nonce"`
		CodeHash     string          `json:"codeHash"`
		StorageHash  string          `json:"storageHash"`
		StorageProof []struct {
			Key   string          `json:"key"`
			Value json.RawMessage `json:"value"`
			Proof []string        `json:"proof"`
		} `json:"storageProof"`
	}
	if json.Unmarshal(raw, &proof) != nil || !receiptHex(proof.Address, 20) || !receiptHex(proof.CodeHash, 32) || !receiptHex(proof.StorageHash, 32) || proof.AccountProof == nil || proof.StorageProof == nil || len(proof.StorageProof) > 4096 {
		return out, errors.New("STATE_PROOF_ENCODING_INVALID")
	}
	balance, err := headerQuantity(proof.Balance, true)
	if err != nil {
		return out, err
	}
	nonce, err := canonicalIndex(proof.Nonce)
	if err != nil {
		return out, err
	}
	budget := receiptProofBudget{remaining: maxBytes}
	if err = budget.charge(2*len(raw) + 512); err != nil {
		return out, err
	}
	db := memorydb.New()
	defer db.Close()
	loadNodes := func(nodes []string) error {
		if len(nodes) > 1024 {
			return errors.New("STATE_PROOF_NODE_BUDGET")
		}
		for _, node := range nodes {
			if err := ctx.Err(); err != nil {
				return err
			}
			if !receiptHex(node, -1) || len(node) <= 2 {
				return errors.New("STATE_PROOF_NODE_INVALID")
			}
			if err := budget.charge(len(node) + (len(node)-2)/2 + 128); err != nil {
				return err
			}
			bytes, err := hex.DecodeString(node[2:])
			if err != nil {
				return err
			}
			if err = db.Put(crypto.Keccak256(bytes), bytes); err != nil {
				return err
			}
		}
		return nil
	}
	if err = loadNodes(proof.AccountProof); err != nil {
		return out, err
	}
	value, err := trie.VerifyProof(common.HexToHash(anchor.StateRoot), crypto.Keccak256(common.FromHex(proof.Address)), db)
	if err != nil {
		return out, errors.New("STATE_ACCOUNT_PROOF_INVALID")
	}
	var account struct {
		Nonce    uint64
		Balance  *big.Int
		Root     common.Hash
		CodeHash []byte
	}
	exists := value != nil
	var heldXChainAsset *bool
	if exists {
		switch anchor.AccountEncodingID {
		case "coreth-account-held-xchain-rlp/v1":
			var native struct {
				Nonce           uint64
				Balance         *big.Int
				Root            common.Hash
				CodeHash        []byte
				HeldXChainAsset bool
			}
			if rlp.DecodeBytes(value, &native) != nil {
				return out, errors.New("STATE_ACCOUNT_RLP_INVALID")
			}
			account.Nonce = native.Nonce
			account.Balance = native.Balance
			account.Root = native.Root
			account.CodeHash = native.CodeHash
			heldXChainAsset = &native.HeldXChainAsset
		case "ethereum-account-rlp/v1":
			if rlp.DecodeBytes(value, &account) != nil {
				return out, errors.New("STATE_ACCOUNT_RLP_INVALID")
			}
		default:
			return out, errors.New("STATE_ACCOUNT_ENCODING_UNSUPPORTED")
		}
		if account.Balance == nil || account.Balance.BitLen() > 256 || len(account.CodeHash) != 32 {
			return out, errors.New("STATE_ACCOUNT_RLP_INVALID")
		}
	} else {
		account.Balance = new(big.Int)
		account.Root = types.EmptyRootHash
		account.CodeHash = types.EmptyCodeHash.Bytes()
	}
	codeMatches := common.BytesToHash(account.CodeHash) == common.HexToHash(proof.CodeHash)
	storageMatches := account.Root == common.HexToHash(proof.StorageHash)
	if !exists {
		// A verified absence has no account leaf. RPCs may use all-zero
		// sentinels or canonical empty hashes, but cannot assert other values.
		codeMatches = codeMatches || common.HexToHash(proof.CodeHash) == (common.Hash{})
		storageMatches = storageMatches || common.HexToHash(proof.StorageHash) == (common.Hash{})
	}
	if account.Nonce != nonce || account.Balance.Cmp(balance) != 0 || !storageMatches || !codeMatches {
		return out, errors.New("STATE_ACCOUNT_VALUE_MISMATCH")
	}
	slots := map[string]string{}
	for _, slot := range proof.StorageProof {
		key, err := storageProofKey(slot.Key)
		if err != nil {
			return out, err
		}
		normalized := common.BytesToHash(key).Hex()
		if _, duplicate := slots[normalized]; duplicate {
			return out, errors.New("STATE_STORAGE_PROOF_DUPLICATE")
		}
		expected, err := headerQuantity(slot.Value, true)
		if err != nil {
			return out, err
		}
		if err = loadNodes(slot.Proof); err != nil {
			return out, err
		}
		var encoded []byte
		if account.Root != types.EmptyRootHash {
			// Nonempty storage still requires a valid path for each key. An
			// authenticated empty root alone proves zero for every storage key.
			encoded, err = trie.VerifyProof(account.Root, crypto.Keccak256(key), db)
			if err != nil {
				return out, errors.New("STATE_STORAGE_PROOF_INVALID")
			}
		}
		observed := new(big.Int)
		if encoded != nil {
			var bytes []byte
			if rlp.DecodeBytes(encoded, &bytes) != nil || len(bytes) > 32 || (len(bytes) > 0 && bytes[0] == 0) {
				return out, errors.New("STATE_STORAGE_RLP_INVALID")
			}
			observed.SetBytes(bytes)
		}
		if observed.Cmp(expected) != 0 {
			return out, errors.New("STATE_STORAGE_VALUE_MISMATCH")
		}
		value := "0x" + observed.Text(16)
		if err = budget.charge(len(normalized) + len(value) + 128); err != nil {
			return out, err
		}
		slots[normalized] = value
	}
	if err = ctx.Err(); err != nil {
		return out, err
	}
	provenSlots := make(map[string]string, len(slots))
	for key, value := range slots {
		provenSlots[key] = value
	}
	return VerifiedAccountCommitment{proofBytesUsed: maxBytes - budget.remaining, provenStorageRoot: &account.Root, provenStorageValues: provenSlots, HeldXChainAsset: heldXChainAsset, StateAnchor: anchor, Address: strings.ToLower(proof.Address), Exists: exists, Nonce: account.Nonce, Balance: "0x" + account.Balance.Text(16), CodeHash: common.BytesToHash(account.CodeHash).Hex(), StorageRoot: account.Root.Hex(), StorageValues: slots}, nil
}
func storageProofKey(value string) ([]byte, error) {
	if receiptHex(value, 32) {
		return common.FromHex(value), nil
	}
	raw, _ := json.Marshal(value)
	n, err := headerQuantity(raw, true)
	if err != nil {
		return nil, errors.New("STATE_STORAGE_KEY_INVALID")
	}
	return common.LeftPadBytes(n.Bytes(), 32), nil
}

// StorageValue resolves an explicitly proved key, or any key under a proved
// empty storage root. A nonempty root never supplies an unlisted key's value.
// Serialized commitments must be reconstructed from their original MPT proof.
func (c VerifiedAccountCommitment) StorageValue(key string) (string, error) {
	if len(key) > 66 {
		return "", errors.New("STATE_STORAGE_KEY_INVALID")
	}
	decoded, err := storageProofKey(key)
	if err != nil {
		return "", err
	}
	if c.provenStorageRoot == nil {
		return "", errors.New("STATE_STORAGE_ROOT_UNPROVEN")
	}
	if *c.provenStorageRoot == types.EmptyRootHash {
		return "0x0", nil
	}
	if value, ok := c.provenStorageValues[common.BytesToHash(decoded).Hex()]; ok {
		return value, nil
	}
	return "", errors.New("STATE_STORAGE_KEY_UNPROVEN")
}

// ContractStorageSource is an explicit known-chain fingerprint. The encoding
// comes from a verified execution anchor, never an RPC-supplied mapping rule.
type ContractStorageSource struct {
	ChainKey    string `json:"chain_key"`
	GenesisHash string `json:"genesis_hash"`
}
type ContractStorageBinding struct {
	LogicalKey  string `json:"logical_key"`
	PhysicalKey string `json:"physical_key"`
	TriePath    string `json:"trie_path"`
}
type VerifiedContractAccountCommitment struct {
	Account              VerifiedAccountCommitment `json:"physical_account"`
	Source               ContractStorageSource     `json:"source"`
	StorageEncodingID    string                    `json:"storage_encoding_id"`
	Scope                string                    `json:"scope"`
	Bindings             []ContractStorageBinding  `json:"bindings"`
	LogicalStorageValues map[string]string         `json:"logical_storage_values"`
	ProofBytesUsed       int                       `json:"proof_bytes_used"`
	sealed               *contractStorageSeal
}
type contractStorageSeal struct {
	encoding string
	account  VerifiedAccountCommitment
}
type contractStoragePlan struct {
	source       ContractStorageSource
	encoding     string
	bindings     []ContractStorageBinding
	physicalKeys []string
	selected     map[string]bool
}

func contractStorageEncoding(source ContractStorageSource, anchor ExecutionStateAnchor) (string, error) {
	genesis, err := knownBodyGenesis(source.ChainKey)
	if err != nil || !receiptHex(source.GenesisHash, 32) || !strings.EqualFold(source.GenesisHash, genesis) {
		return "", errors.New("STATE_CONTRACT_SOURCE_FINGERPRINT")
	}
	if source.ChainKey == "evm:43114" && anchor.AccountEncodingID == "coreth-account-held-xchain-rlp/v1" {
		return "coreth-partitioned-storage/v1", nil
	}
	if source.ChainKey != "evm:43114" && anchor.AccountEncodingID == "ethereum-account-rlp/v1" {
		return "ethereum-contract-storage-identity/v1", nil
	}
	return "", errors.New("STATE_CONTRACT_STORAGE_ENCODING_UNSUPPORTED")
}
func contractPhysicalKey(encoding, key string) (string, string, error) {
	if len(key) > 66 {
		return "", "", errors.New("STATE_STORAGE_KEY_INVALID")
	}
	decoded, err := storageProofKey(key)
	if err != nil {
		return "", "", err
	}
	logical := common.BytesToHash(decoded).Hex()
	switch encoding {
	case "coreth-partitioned-storage/v1":
		// Pinned C-Chain extstate.storageKey: contract SLOAD clears bit 0
		// of byte 0, not the low bit of the 256-bit integer. Asset partition
		// and SkipStateKeyTransformation are deliberately outside this API.
		decoded[0] &= 0xfe
	case "ethereum-contract-storage-identity/v1":
	default:
		return "", "", errors.New("STATE_CONTRACT_STORAGE_ENCODING_UNSUPPORTED")
	}
	return logical, common.BytesToHash(decoded).Hex(), nil
}
func planContractStorage(ctx context.Context, source ContractStorageSource, anchor ExecutionStateAnchor, keys []string, budget *receiptProofBudget) (contractStoragePlan, error) {
	var out contractStoragePlan
	encoding, err := contractStorageEncoding(source, anchor)
	if err != nil {
		return out, err
	}
	if len(keys) > 4096 {
		return out, errors.New("STATE_PROOF_REQUEST_INVALID")
	}
	out = contractStoragePlan{source: source, encoding: encoding, bindings: []ContractStorageBinding{}, physicalKeys: []string{}, selected: map[string]bool{}}
	logicalSeen := map[string]bool{}
	for _, key := range keys {
		if err = ctx.Err(); err != nil {
			return contractStoragePlan{}, err
		}
		if len(key) > 66 {
			return contractStoragePlan{}, errors.New("STATE_STORAGE_KEY_INVALID")
		}
		if err = budget.charge(len(key) + 256); err != nil {
			return contractStoragePlan{}, err
		}
		logical, physical, err := contractPhysicalKey(encoding, key)
		if err != nil {
			return contractStoragePlan{}, err
		}
		if logicalSeen[logical] {
			return contractStoragePlan{}, errors.New("STATE_STORAGE_PROOF_DUPLICATE")
		}
		logicalSeen[logical] = true
		out.bindings = append(out.bindings, ContractStorageBinding{LogicalKey: logical, PhysicalKey: physical, TriePath: crypto.Keccak256Hash(common.FromHex(physical)).Hex()})
		if !out.selected[physical] {
			out.selected[physical] = true
			out.physicalKeys = append(out.physicalKeys, physical)
		}
	}
	return out, ctx.Err()
}
func projectContractAccount(ctx context.Context, plan contractStoragePlan, account VerifiedAccountCommitment, budget *receiptProofBudget) (VerifiedContractAccountCommitment, error) {
	var out VerifiedContractAccountCommitment
	// Extra physical proof entries are never silently interpreted as logical
	// slots. Missing entries can only be derived by the sealed empty-root rule.
	for key := range account.provenStorageValues {
		if !plan.selected[key] {
			return out, errors.New("STATE_STORAGE_REQUEST_COVERAGE")
		}
	}
	values := map[string]string{}
	for _, binding := range plan.bindings {
		if err := ctx.Err(); err != nil {
			return out, err
		}
		value, err := account.StorageValue(binding.PhysicalKey)
		if err != nil {
			return out, err
		}
		if err = budget.charge(len(binding.LogicalKey) + len(binding.PhysicalKey) + len(binding.TriePath) + len(value) + 128); err != nil {
			return out, err
		}
		values[binding.LogicalKey] = value
	}
	return VerifiedContractAccountCommitment{Account: account, Source: plan.source, StorageEncodingID: plan.encoding, Scope: "contract-storage-only", Bindings: plan.bindings, LogicalStorageValues: values, sealed: &contractStorageSeal{encoding: plan.encoding, account: account}}, ctx.Err()
}

// VerifyContractAccountProof verifies the untouched physical EIP-1186 proof
// first, then exposes the explicit contract logical/physical binding. It never
// rewrites proof keys or trie paths to rescue an old mismatching proof.
func VerifyContractAccountProof(ctx context.Context, source ContractStorageSource, anchor ExecutionStateAnchor, logicalKeys []string, physicalProof json.RawMessage, maxBytes int) (VerifiedContractAccountCommitment, error) {
	var empty VerifiedContractAccountCommitment
	if err := ctx.Err(); err != nil {
		return empty, err
	}
	if maxBytes <= 0 || maxBytes > 64<<20 {
		return empty, errors.New("STATE_PROOF_BUDGET")
	}
	budget := receiptProofBudget{remaining: maxBytes}
	plan, err := planContractStorage(ctx, source, anchor, logicalKeys, &budget)
	if err != nil {
		return empty, err
	}
	account, err := VerifyExecutionAccountProof(ctx, anchor, physicalProof, budget.remaining)
	if err != nil {
		return empty, err
	}
	if err = budget.charge(account.proofBytesUsed); err != nil {
		return empty, err
	}
	out, err := projectContractAccount(ctx, plan, account, &budget)
	if err != nil {
		return empty, err
	}
	out.ProofBytesUsed = maxBytes - budget.remaining
	return out, nil
}

// StorageValue queries logical contract storage using sealed verified inputs.
// This is not an imported X-Chain asset balance or raw/skip-transform API.
func (c VerifiedContractAccountCommitment) StorageValue(logicalKey string) (string, error) {
	if c.sealed == nil {
		return "", errors.New("STATE_CONTRACT_PROOF_UNPROVEN")
	}
	_, physical, err := contractPhysicalKey(c.sealed.encoding, logicalKey)
	if err != nil {
		return "", err
	}
	return c.sealed.account.StorageValue(physical)
}
