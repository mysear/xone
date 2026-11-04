package domain

import (
	"encoding/hex"
	"errors"
	"golang.org/x/crypto/sha3"
	"math/big"
	"strings"
)

const SingletonDynamicFeeFlag uint32 = 0x800000

// SingletonAdvancedState is deliberately separate from legacy Pool admission.
// KeyFee is immutable; Snapshot.LPFee is the observed slot0 fee at SnapshotBlockHash.
type SingletonAdvancedState struct {
	Protocol          string           `json:"protocol"`
	Currency0         string           `json:"currency0"`
	Currency1         string           `json:"currency1"`
	KeyFee            uint32           `json:"key_fee"`
	SnapshotBlockHash string           `json:"snapshot_block_hash"`
	Snapshot          SingletonCLState `json:"snapshot"`
}

func singletonAdvancedBytes32(v string) ([]byte, error) {
	if len(v) != 66 || !strings.HasPrefix(v, "0x") || v != strings.ToLower(v) {
		return nil, errors.New("SINGLETON_DIGEST")
	}
	b, err := hex.DecodeString(v[2:])
	if err != nil {
		return nil, errors.New("SINGLETON_DIGEST")
	}
	return b, nil
}
func (v SingletonAdvancedState) KeyID() (string, error) {
	s := v.Snapshot
	for _, a := range []string{s.Manager, v.Currency0, v.Currency1, s.Hooks} {
		if ValidateEVMAddress(a) != nil {
			return "", errors.New("SINGLETON_ADDRESS")
		}
	}
	if s.Manager == SingletonZeroAddress || v.Currency0 >= v.Currency1 || (v.KeyFee > 1000000 && v.KeyFee != SingletonDynamicFeeFlag) || s.Core.TickSpacing < 1 || s.Core.TickSpacing >= 32768 {
		return "", errors.New("SINGLETON_ADVANCED_KEY")
	}
	word := func(a string) []byte { b, _ := hex.DecodeString(a[2:]); return append(make([]byte, 32-len(b)), b...) }
	num := func(n uint32) []byte { return new(big.Int).SetUint64(uint64(n)).FillBytes(make([]byte, 32)) }
	key := append(word(v.Currency0), word(v.Currency1)...)
	switch v.Protocol {
	case "uniswap-v4":
		if s.Parameters != "" {
			return "", errors.New("SINGLETON_PARAMETERS")
		}
		key = append(key, num(v.KeyFee)...)
		key = append(key, num(uint32(s.Core.TickSpacing))...)
		key = append(key, word(s.Hooks)...)
	case "pancake-infinity-cl":
		b, err := singletonAdvancedBytes32(s.Parameters)
		if err != nil {
			return "", err
		}
		expected := num(uint32(s.Core.TickSpacing) << 16)
		if string(b[:30]) != string(expected[:30]) || b[30]&0xc0 != 0 {
			return "", errors.New("SINGLETON_PARAMETERS")
		}
		key = append(key, word(s.Hooks)...)
		key = append(key, word(s.Manager)...)
		key = append(key, num(v.KeyFee)...)
		key = append(key, b...)
	default:
		return "", errors.New("SINGLETON_PROTOCOL_UNSUPPORTED")
	}
	h := sha3.NewLegacyKeccak256()
	_, _ = h.Write(key)
	return "0x" + hex.EncodeToString(h.Sum(nil)), nil
}

// ValidateIdentity checks collection preconditions without requiring core data yet.
func (v SingletonAdvancedState) ValidateIdentity() error {
	s := v.Snapshot
	if ValidateEVMIdentity(s.Manager, s.ManagerCodeSHA256) != nil {
		return errors.New("SINGLETON_MANAGER_IDENTITY")
	}
	b, err := singletonAdvancedBytes32(v.SnapshotBlockHash)
	if err != nil || new(big.Int).SetBytes(b).Sign() == 0 {
		return errors.New("SINGLETON_BLOCK_HASH_REQUIRED")
	}
	id, err := v.KeyID()
	if err != nil {
		return err
	}
	if id != s.PoolID {
		return errors.New("SINGLETON_POOL_ID")
	}
	if s.LPFee > 1000000 || (v.KeyFee != SingletonDynamicFeeFlag && s.LPFee != v.KeyFee) {
		return errors.New("SINGLETON_LP_FEE_MISMATCH")
	}
	limit := uint32(4000)
	if v.Protocol == "uniswap-v4" {
		limit = 1000
	}
	if s.ProtocolFee0 > limit || s.ProtocolFee1 > limit {
		return errors.New("SINGLETON_PROTOCOL_FEE")
	}
	var flags uint16
	if v.Protocol == "uniswap-v4" {
		hook, _ := hex.DecodeString(s.Hooks[2:])
		flags = (uint16(hook[18])<<8 | uint16(hook[19])) & 0x3fff
		if flags&0xcc != 0 {
			return errors.New("SINGLETON_HOOK_EXECUTION_REQUIRED")
		}
		if flags&2 != 0 && flags&(1<<10) == 0 || flags&1 != 0 && flags&(1<<8) == 0 || s.Hooks != SingletonZeroAddress && flags == 0 && v.KeyFee != SingletonDynamicFeeFlag {
			return errors.New("SINGLETON_HOOK_PERMISSIONS")
		}
	} else {
		p, _ := singletonAdvancedBytes32(s.Parameters)
		flags = uint16(p[30])<<8 | uint16(p[31])
		if flags&0xcc0 != 0 {
			return errors.New("SINGLETON_HOOK_EXECUTION_REQUIRED")
		}
		if flags&(1<<12) != 0 && flags&(1<<3) == 0 || flags&(1<<13) != 0 && flags&(1<<5) == 0 {
			return errors.New("SINGLETON_HOOK_PERMISSIONS")
		}
	}
	if s.Hooks == SingletonZeroAddress && (v.KeyFee == SingletonDynamicFeeFlag || flags != 0) {
		return errors.New("SINGLETON_HOOK_PERMISSIONS")
	}
	return nil
}
func (v SingletonAdvancedState) Validate() error {
	if err := v.ValidateIdentity(); err != nil {
		return err
	}
	return v.Snapshot.Core.Validate(v.Protocol)
}
