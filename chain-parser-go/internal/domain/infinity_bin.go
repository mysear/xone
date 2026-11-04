package domain

import (
	"encoding/hex"
	"errors"
	"golang.org/x/crypto/sha3"
	"math/big"
	"strings"
)

type InfinityBinTreeWord struct {
	Index uint32 `json:"index"`
	Value Amount `json:"value"`
}
type InfinityBinEntry struct {
	ID          uint32 `json:"id"`
	Reserve0    Amount `json:"reserve0"`
	Reserve1    Amount `json:"reserve1"`
	TotalShares Amount `json:"total_shares"`
}
type InfinityBinState struct {
	Manager           string                `json:"manager"`
	ManagerCodeSHA256 string                `json:"manager_code_sha256"`
	Vault             string                `json:"vault"`
	VaultCodeSHA256   string                `json:"vault_code_sha256"`
	PoolID            string                `json:"pool_id"`
	Hooks             string                `json:"hooks"`
	Parameters        string                `json:"parameters"`
	Token0CodeSHA256  string                `json:"token0_code_sha256"`
	Token1CodeSHA256  string                `json:"token1_code_sha256"`
	BinStep           uint32                `json:"bin_step"`
	ActiveID          uint32                `json:"active_id"`
	LPFee             uint32                `json:"lp_fee"`
	ProtocolFee0      uint32                `json:"protocol_fee0"`
	ProtocolFee1      uint32                `json:"protocol_fee1"`
	MaxBins           uint32                `json:"max_bins"`
	MaxTreeWords      uint32                `json:"max_tree_words"`
	MaxSteps          uint32                `json:"max_steps"`
	TreeLevel0        Amount                `json:"tree_level0"`
	TreeLevel1        []InfinityBinTreeWord `json:"tree_level1"`
	TreeLevel2        []InfinityBinTreeWord `json:"tree_level2"`
	Bins              []InfinityBinEntry    `json:"bins"`
	// DetachedActive retains the active bin dust after burn removes its tree bit.
	DetachedActive *InfinityBinEntry `json:"detached_active,omitempty"`
	// These balances are shared by every pool using the manager and currency.
	AppReserve0   Amount `json:"app_reserve0"`
	AppReserve1   Amount `json:"app_reserve1"`
	ProtocolFees0 Amount `json:"protocol_fees0"`
	ProtocolFees1 Amount `json:"protocol_fees1"`
	BlockNumber   uint64 `json:"block_number"`
	Timestamp     uint64 `json:"timestamp"`
	BlockHash     string `json:"block_hash"`
}

// KeyID returns the raw bytes32 key hash used by the Infinity bin manager.
func (s InfinityBinState) KeyID(token0, token1 string) (string, error) {
	if ValidateEVMAddress(s.Manager) != nil || s.Manager == SingletonZeroAddress || ValidateEVMAddress(token0) != nil || ValidateEVMAddress(token1) != nil || token0 == SingletonZeroAddress || token0 >= token1 || s.Manager == token0 || s.Manager == token1 || s.BinStep < 1 || s.BinStep > 100 || s.LPFee > 100000 || s.Hooks != SingletonZeroAddress {
		return "", errors.New("INFINITY_BIN_KEY_UNSUPPORTED")
	}
	parameters := new(big.Int).SetUint64(uint64(s.BinStep) << 16).FillBytes(make([]byte, 32))
	if s.Parameters != "0x"+hex.EncodeToString(parameters) {
		return "", errors.New("INFINITY_BIN_PARAMETERS")
	}
	key := make([]byte, 0, 192)
	for _, address := range []string{token0, token1, SingletonZeroAddress, s.Manager} {
		raw, _ := hex.DecodeString(address[2:])
		key = append(key, make([]byte, 12)...)
		key = append(key, raw...)
	}
	key = append(key, new(big.Int).SetUint64(uint64(s.LPFee)).FillBytes(make([]byte, 32))...)
	key = append(key, parameters...)
	hash := sha3.NewLegacyKeccak256()
	_, _ = hash.Write(key)
	return "0x" + hex.EncodeToString(hash.Sum(nil)), nil
}

// ValidateIdentity is shared with config validation; it does not require a bank or bins.
func (s InfinityBinState) ValidateIdentity(token0, token1 string) error {
	if ValidateEVMIdentity(s.Manager, s.ManagerCodeSHA256) != nil || s.Manager == SingletonZeroAddress || ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || s.Vault == SingletonZeroAddress || s.Vault == s.Manager || s.Vault == token0 || s.Vault == token1 || ValidateEVMIdentity(token0, s.Token0CodeSHA256) != nil || ValidateEVMIdentity(token1, s.Token1CodeSHA256) != nil {
		return errors.New("INFINITY_BIN_IDENTITY")
	}
	id, err := s.KeyID(token0, token1)
	if err != nil {
		return err
	}
	if id != s.PoolID {
		return errors.New("INFINITY_BIN_POOL_ID")
	}
	return s.validateLimits()
}
func (s InfinityBinState) validateLimits() error {
	if s.BinStep < 1 || s.BinStep > 100 || s.LPFee > 100000 || s.ProtocolFee0 > 4000 || s.ProtocolFee1 > 4000 || s.MaxBins < 1 || s.MaxBins > 1024 || s.MaxTreeWords < 1 || s.MaxTreeWords > 1024 || s.MaxSteps < 1 || s.MaxSteps > 4096 {
		return errors.New("INFINITY_BIN_STATE_LIMIT")
	}
	return nil
}
func infinityBinHash(s string) bool {
	if len(s) != 66 || !strings.HasPrefix(s, "0x") || s != strings.ToLower(s) {
		return false
	}
	_, err := hex.DecodeString(s[2:])
	return err == nil
}
func (s InfinityBinState) validateWire() error {
	if err := s.validateLimits(); err != nil {
		return err
	}
	if ValidateEVMIdentity(s.Manager, s.ManagerCodeSHA256) != nil || s.Manager == SingletonZeroAddress || ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || s.Vault == SingletonZeroAddress || s.Vault == s.Manager || ValidateEVMIdentity(s.Manager, s.Token0CodeSHA256) != nil || ValidateEVMIdentity(s.Manager, s.Token1CodeSHA256) != nil || s.Hooks != SingletonZeroAddress || !infinityBinHash(s.PoolID) || s.Parameters != "0x"+hex.EncodeToString(new(big.Int).SetUint64(uint64(s.BinStep)<<16).FillBytes(make([]byte, 32))) {
		return errors.New("INFINITY_BIN_IDENTITY")
	}
	if s.Timestamp == 0 || s.Timestamp > ^uint64(0)/1000 || !infinityBinHash(s.BlockHash) || s.BlockHash == "0x"+strings.Repeat("0", 64) {
		return errors.New("INFINITY_BIN_SOURCE_BANK")
	}
	for _, amount := range []Amount{s.AppReserve0, s.AppReserve1, s.ProtocolFees0, s.ProtocolFees1} {
		if _, err := ParseAmount(string(amount)); err != nil {
			return err
		}
	}
	actualCount := len(s.Bins)
	if s.DetachedActive != nil {
		actualCount++
		if s.DetachedActive.ID != s.ActiveID || s.DetachedActive.TotalShares != "1000" {
			return errors.New("INFINITY_BIN_DETACHED_ACTIVE")
		}
		if err := s.DetachedActive.validate(); err != nil {
			return err
		}
	}
	if s.ActiveID < 1 || s.ActiveID > 0xfffffe || actualCount == 0 || actualCount > int(s.MaxBins) || 1+len(s.TreeLevel1)+len(s.TreeLevel2) > int(s.MaxTreeWords) {
		return errors.New("INFINITY_BIN_STATE_LIMIT")
	}
	root := new(big.Int)
	level1 := map[uint32]*big.Int{}
	level2 := map[uint32]*big.Int{}
	for i, bin := range s.Bins {
		if bin.ID < 1 || bin.ID > 0xfffffe || i > 0 && bin.ID <= s.Bins[i-1].ID {
			return errors.New("INFINITY_BIN_ID_SEQUENCE")
		}
		if err := bin.validate(); err != nil {
			return err
		}
		if s.DetachedActive != nil && bin.ID == s.DetachedActive.ID {
			return errors.New("INFINITY_BIN_DETACHED_IN_TREE")
		}

		high, middle := bin.ID>>16, bin.ID>>8
		root.SetBit(root, int(high), 1)
		if level1[high] == nil {
			level1[high] = new(big.Int)
		}
		level1[high].SetBit(level1[high], int(middle&255), 1)
		if level2[middle] == nil {
			level2[middle] = new(big.Int)
		}
		level2[middle].SetBit(level2[middle], int(bin.ID&255), 1)
	}
	a, err := ParseAmount(string(s.TreeLevel0))
	if err != nil || a.Big().Cmp(root) != 0 {
		return errors.New("INFINITY_BIN_TREE_ROOT")
	}
	for i, words := range [][]InfinityBinTreeWord{s.TreeLevel1, s.TreeLevel2} {
		expected, maxIndex := level1, uint32(255)
		if i == 1 {
			expected, maxIndex = level2, 65535
		}
		if len(words) != len(expected) {
			return errors.New("INFINITY_BIN_TREE_INCOMPLETE")
		}
		for j, word := range words {
			value, err := ParseAmount(string(word.Value))
			want := expected[word.Index]
			if word.Index > maxIndex || j > 0 && word.Index <= words[j-1].Index || err != nil || value.Big().Sign() == 0 || want == nil || value.Big().Cmp(want) != 0 {
				return errors.New("INFINITY_BIN_TREE_WORD")
			}
		}
	}
	return nil
}
func (s InfinityBinState) Validate(token0, token1 string) error {
	if err := s.ValidateIdentity(token0, token1); err != nil {
		return err
	}
	return s.validateWire()
}
func (p Pool) validateInfinityBin() error {
	profile, _ := Protocol(p.Protocol)
	if ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || p.AdapterVersion != profile.AdapterVersion || p.TokenBehavior != "standard" || p.InfinityBin == nil || p.Reserve0 != "0" || p.Reserve1 != "0" || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(p.InfinityBin.LPFee) {
		return errors.New("INFINITY_BIN_POOL_STATE")
	}
	if err := p.InfinityBin.Validate(p.Token0, p.Token1); err != nil {
		return err
	}
	if p.PoolID != p.InfinityBin.Manager+"/"+p.InfinityBin.PoolID {
		return errors.New("INFINITY_BIN_PHYSICAL_ID")
	}
	return nil
}

// Clone copies every mutable collection and the optional detached active bin.
func (s InfinityBinState) Clone() InfinityBinState {
	s.TreeLevel1 = append([]InfinityBinTreeWord{}, s.TreeLevel1...)
	s.TreeLevel2 = append([]InfinityBinTreeWord{}, s.TreeLevel2...)
	s.Bins = append([]InfinityBinEntry{}, s.Bins...)
	if s.DetachedActive != nil {
		entry := *s.DetachedActive
		s.DetachedActive = &entry
	}
	return s
}
func (b InfinityBinEntry) validate() error {
	if b.ID < 1 || b.ID > 0xfffffe {
		return errors.New("INFINITY_BIN_ID_SEQUENCE")
	}
	for _, reserve := range []Amount{b.Reserve0, b.Reserve1} {
		value, err := ParseAmount(string(reserve))
		if err != nil || value.Big().BitLen() > 128 {
			return errors.New("INFINITY_BIN_RESERVE_WIDTH")
		}
	}
	shares, err := ParseAmount(string(b.TotalShares))
	if err != nil || shares.Big().Sign() == 0 {
		return errors.New("INFINITY_BIN_SHARES")
	}
	return nil
}
