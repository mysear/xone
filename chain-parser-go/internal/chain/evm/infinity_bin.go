package evm

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// Solc0.8.26/viaIR/Cancun original optimizer settings at pinned 3c5ef1be.
const infinityBinRuntimeBytes = 23852
const infinityBinTemplateSHA256 = "5855fc3415d02c1b7b5a6c78abe09c9fa8d75e2f1c53e4f44f87308847260b84"
const infinityBinVaultSHA256 = "41e153c88b63adef8484268a24d4d5b1ed96587fc5d380022ec5feee842d088e"

var infinityBinVaultOffsets = []int{1921, 2767, 3756, 6803, 7198}

func (a *Adapter) readInfinityBin(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	fail := func(s string) (domain.Pool, error) { return domain.Pool{}, errors.New(s) }
	v := p.InfinityBin
	if v == nil || p.Protocol != "pancake-infinity-bin" || v.TokenPolicy != "standard" || v.ExpectedVaultCodeSHA256 != infinityBinVaultSHA256 || v.MaxBins == 0 || v.MaxBins > 1024 || v.MaxTreeWords == 0 || v.MaxTreeWords > 1024 {
		return fail("INFINITY_BIN_CONFIG")
	}
	runtime, e := a.verifyCode(ctx, v.Manager, v.ExpectedManagerCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if len(runtime) != infinityBinRuntimeBytes {
		return fail("INFINITY_BIN_MANAGER_RUNTIME")
	}
	normalized := append([]byte{}, runtime...)
	vaultWord := make([]byte, 32)
	vaultBytes, e := hex.DecodeString(strings.TrimPrefix(v.Vault, "0x"))
	if e != nil || len(vaultBytes) != 20 {
		return fail("INFINITY_BIN_VAULT_IDENTITY")
	}
	copy(vaultWord[12:], vaultBytes)
	for _, offset := range infinityBinVaultOffsets {
		if string(normalized[offset:offset+32]) != string(vaultWord) {
			return fail("INFINITY_BIN_VAULT_IMMUTABLE")
		}
		clear(normalized[offset : offset+32])
	}
	sum := sha256.Sum256(normalized)
	if hex.EncodeToString(sum[:]) != infinityBinTemplateSHA256 {
		return fail("INFINITY_BIN_MANAGER_TEMPLATE")
	}
	if _, e = a.verifyCode(ctx, v.Vault, v.ExpectedVaultCodeSHA256, anchor); e != nil {
		return domain.Pool{}, e
	}
	word := func(n uint64) string { return fmt.Sprintf("%064x", n) }
	addressWord := func(s string) string { return strings.Repeat("0", 24) + s[2:] }
	rawID, e := hex.DecodeString(strings.TrimPrefix(v.PoolID, "0x"))
	if e != nil || len(rawID) != 32 {
		return fail("INFINITY_BIN_POOL_ID")
	}
	id := hex.EncodeToString(rawID)
	call := func(to, data string, n int) ([]byte, error) { return a.stableCall(ctx, to, data, anchor, n) }
	scalar := func(to, signature, args string, bits int) (*big.Int, error) {
		b, e := call(to, singletonSelector(signature)+args, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(b, bits)
	}
	addressCall := func(to, signature, args string) (string, error) {
		b, e := scalar(to, signature, args, 160)
		if e != nil {
			return "", e
		}
		return fmt.Sprintf("0x%040x", b), nil
	}
	observed, e := addressCall(v.Manager, "vault()", "")
	if e != nil {
		return domain.Pool{}, e
	}
	if observed != v.Vault {
		return fail("INFINITY_BIN_VAULT_GETTER")
	}
	paused, e := scalar(v.Manager, "paused()", "", 1)
	if e != nil {
		return domain.Pool{}, e
	}
	if paused.Sign() != 0 {
		return fail("INFINITY_BIN_MANAGER_PAUSED")
	}
	registered, e := scalar(v.Vault, "isAppRegistered(address)", addressWord(v.Manager), 1)
	if e != nil {
		return domain.Pool{}, e
	}
	if registered.Sign() == 0 {
		return fail("INFINITY_BIN_APP_UNREGISTERED")
	}
	maxStep, e := scalar(v.Manager, "maxBinStep()", "", 16)
	if e != nil {
		return domain.Pool{}, e
	}
	// This owner limit applies only to initialization, not existing-pool swap.
	_ = maxStep
	key, e := call(v.Manager, singletonSelector("poolIdToPoolKey(bytes32)")+id, 6)
	if e != nil {
		return domain.Pool{}, e
	}
	expected := []string{p.Token0, p.Token1, domain.SingletonZeroAddress, v.Manager}
	for i, want := range expected {
		n, e := wordUint(key[i*32:(i+1)*32], 160)
		if e != nil || fmt.Sprintf("0x%040x", n) != want {
			return fail("INFINITY_BIN_KEY_IDENTITY")
		}
	}
	fee, e := wordUint(key[128:160], 24)
	if e != nil || fee.Uint64() != uint64(v.LPFee) {
		return fail("INFINITY_BIN_KEY_FEE")
	}
	parameters := fmt.Sprintf("0x%064x", new(big.Int).Lsh(new(big.Int).SetUint64(uint64(v.BinStep)), 16))
	if "0x"+hex.EncodeToString(key[160:]) != parameters {
		return fail("INFINITY_BIN_KEY_PARAMETERS")
	}
	s := &domain.InfinityBinState{Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, Vault: v.Vault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Hooks: domain.SingletonZeroAddress, Parameters: parameters, Token0CodeSHA256: v.ExpectedToken0CodeSHA256, Token1CodeSHA256: v.ExpectedToken1CodeSHA256, BinStep: v.BinStep, LPFee: v.LPFee, MaxBins: v.MaxBins, MaxTreeWords: v.MaxTreeWords, MaxSteps: v.MaxSteps, TreeLevel1: []domain.InfinityBinTreeWord{}, TreeLevel2: []domain.InfinityBinTreeWord{}, Bins: []domain.InfinityBinEntry{}, BlockNumber: block.Number, Timestamp: block.TimestampMS / 1000, BlockHash: block.Hash}
	for i, token := range []string{p.Token0, p.Token1} {
		expected := v.ExpectedToken0CodeSHA256
		if i == 1 {
			expected = v.ExpectedToken1CodeSHA256
		}
		code, e := a.verifyCode(ctx, token, expected, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(code) {
			return fail("INFINITY_BIN_TOKEN_PROXY")
		}
		decimals, e := scalar(token, "decimals()", "", 8)
		if e != nil {
			return domain.Pool{}, e
		}
		if decimals.Uint64() > 18 {
			return fail("INFINITY_BIN_TOKEN_DECIMALS")
		}
		reserve, e := scalar(v.Vault, "reservesOfApp(address,address)", addressWord(v.Manager)+addressWord(token), 256)
		if e != nil {
			return domain.Pool{}, e
		}
		fees, e := scalar(v.Manager, "protocolFeesAccrued(address)", addressWord(token), 256)
		if e != nil {
			return domain.Pool{}, e
		}
		if i == 0 {
			s.AppReserve0 = domain.Amount(reserve.String())
			s.ProtocolFees0 = domain.Amount(fees.String())
		} else {
			s.AppReserve1 = domain.Amount(reserve.String())
			s.ProtocolFees1 = domain.Amount(fees.String())
		}
	}
	base := singletonMappingSlot(rawID, singletonSlotOffset(make([]byte, 32), 5))
	ext := func(slot []byte) (*big.Int, error) {
		b, e := call(v.Manager, singletonSelector("extsload(bytes32)")+hex.EncodeToString(slot), 1)
		if e != nil {
			return nil, e
		}
		return wordUint(b, 256)
	}
	packed, e := ext(base)
	if e != nil {
		return domain.Pool{}, e
	}
	if packed.BitLen() > 72 {
		return fail("INFINITY_BIN_SLOT0_UNUSED")
	}
	mask := big.NewInt(0xffffff)
	s.ActiveID = uint32(new(big.Int).And(packed, mask).Uint64())
	protocol := new(big.Int).And(new(big.Int).Rsh(new(big.Int).Set(packed), 24), mask).Uint64()
	s.ProtocolFee0 = uint32(protocol & 4095)
	s.ProtocolFee1 = uint32(protocol >> 12)
	if uint32(new(big.Int).Rsh(new(big.Int).Set(packed), 48).Uint64()) != v.LPFee {
		return fail("INFINITY_BIN_LP_FEE_MISMATCH")
	}
	slot0, e := call(v.Manager, singletonSelector("getSlot0(bytes32)")+id, 3)
	if e != nil {
		return domain.Pool{}, e
	}
	for i, want := range []uint64{uint64(s.ActiveID), protocol, uint64(s.LPFee)} {
		got, e := wordUint(slot0[i*32:(i+1)*32], 24)
		if e != nil || got.Uint64() != want {
			return fail("INFINITY_BIN_SLOT0_GETTER_MISMATCH")
		}
	}
	words := uint32(0)
	treeRead := func(slot []byte) (*big.Int, error) {
		if words >= v.MaxTreeWords {
			return nil, errors.New("INFINITY_BIN_TREE_WORD_BUDGET")
		}
		words++
		return ext(slot)
	}
	root, e := treeRead(singletonSlotOffset(base, 4))
	if e != nil {
		return domain.Pool{}, e
	}
	s.TreeLevel0 = domain.Amount(root.String())
	binIDs := []uint32{}
	for hi := 0; hi < 256; hi++ {
		if root.Bit(hi) == 0 {
			continue
		}
		l1, e := treeRead(singletonMappingSlot(new(big.Int).SetUint64(uint64(hi)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 5)))
		if e != nil {
			return domain.Pool{}, e
		}
		if l1.Sign() == 0 {
			return fail("INFINITY_BIN_TREE_EMPTY_CHILD")
		}
		s.TreeLevel1 = append(s.TreeLevel1, domain.InfinityBinTreeWord{Index: uint32(hi), Value: domain.Amount(l1.String())})
		for mid := 0; mid < 256; mid++ {
			if l1.Bit(mid) == 0 {
				continue
			}
			index := (uint32(hi) << 8) | uint32(mid)
			l2, e := treeRead(singletonMappingSlot(new(big.Int).SetUint64(uint64(index)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 6)))
			if e != nil {
				return domain.Pool{}, e
			}
			if l2.Sign() == 0 {
				return fail("INFINITY_BIN_TREE_EMPTY_CHILD")
			}
			s.TreeLevel2 = append(s.TreeLevel2, domain.InfinityBinTreeWord{Index: index, Value: domain.Amount(l2.String())})
			for lo := 0; lo < 256; lo++ {
				if l2.Bit(lo) == 0 {
					continue
				}
				bid := (index << 8) | uint32(lo)
				if bid == 0 || bid == 0xffffff {
					return fail("INFINITY_BIN_SENTINEL_IN_TREE")
				}
				if uint32(len(binIDs)) >= v.MaxBins {
					return fail("INFINITY_BIN_BIN_BUDGET")
				}
				binIDs = append(binIDs, bid)
			}
		}
	}
	for i, bid := range binIDs {
		bin, e := call(v.Manager, singletonSelector("getBin(bytes32,uint24)")+id+word(uint64(bid)), 4)
		if e != nil {
			return domain.Pool{}, e
		}
		r0, e := wordUint(bin[:32], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		r1, e := wordUint(bin[32:64], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		shares, e := wordUint(bin[96:128], 256)
		if e != nil {
			return domain.Pool{}, e
		}
		packedReserve, e := ext(singletonMappingSlot(new(big.Int).SetUint64(uint64(bid)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 1)))
		if e != nil {
			return domain.Pool{}, e
		}
		want := new(big.Int).Or(new(big.Int).Set(r0), new(big.Int).Lsh(new(big.Int).Set(r1), 128))
		storedShares, e := ext(singletonMappingSlot(new(big.Int).SetUint64(uint64(bid)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 2)))
		if e != nil {
			return domain.Pool{}, e
		}
		if packedReserve.Cmp(want) != 0 || storedShares.Cmp(shares) != 0 {
			return fail("INFINITY_BIN_BIN_STORAGE_MISMATCH")
		}
		for _, left := range []bool{true, false} {
			direction := uint64(0)
			want := uint64(0)
			if left {
				direction = 1
				want = 0xffffff
				if i > 0 {
					want = uint64(binIDs[i-1])
				}
			} else if i+1 < len(binIDs) {
				want = uint64(binIDs[i+1])
			}
			got, e := scalar(v.Manager, "getNextNonEmptyBin(bytes32,bool,uint24)", id+word(direction)+word(uint64(bid)), 24)
			if e != nil {
				return domain.Pool{}, e
			}
			if got.Uint64() != want {
				return fail("INFINITY_BIN_TREE_GETTER_MISMATCH")
			}
		}
		s.Bins = append(s.Bins, domain.InfinityBinEntry{ID: bid, Reserve0: domain.Amount(r0.String()), Reserve1: domain.Amount(r1.String()), TotalShares: domain.Amount(shares.String())})
	}
	// Native burn removes membership at the locked minimum share while leaving
	// active ID and rounding dust. Read that first bin separately from the tree.
	found := false
	for _, bid := range binIDs {
		found = found || bid == s.ActiveID
	}
	if !found {
		bin, e := call(v.Manager, singletonSelector("getBin(bytes32,uint24)")+id+word(uint64(s.ActiveID)), 4)
		if e != nil {
			return domain.Pool{}, e
		}
		r0, e := wordUint(bin[:32], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		r1, e := wordUint(bin[32:64], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		shares, e := wordUint(bin[96:128], 256)
		if e != nil {
			return domain.Pool{}, e
		}
		if shares.Sign() != 0 || r0.Sign() != 0 || r1.Sign() != 0 {
			if shares.Cmp(big.NewInt(1000)) != 0 {
				return fail("INFINITY_BIN_DETACHED_SHARES")
			}
			stored, e := ext(singletonMappingSlot(new(big.Int).SetUint64(uint64(s.ActiveID)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 1)))
			if e != nil {
				return domain.Pool{}, e
			}
			want := new(big.Int).Or(new(big.Int).Set(r0), new(big.Int).Lsh(new(big.Int).Set(r1), 128))
			storedShare, e := ext(singletonMappingSlot(new(big.Int).SetUint64(uint64(s.ActiveID)).FillBytes(make([]byte, 32)), singletonSlotOffset(base, 2)))
			if e != nil {
				return domain.Pool{}, e
			}
			if stored.Cmp(want) != 0 || storedShare.Cmp(shares) != 0 {
				return fail("INFINITY_BIN_BIN_STORAGE_MISMATCH")
			}
			s.DetachedActive = &domain.InfinityBinEntry{ID: s.ActiveID, Reserve0: domain.Amount(r0.String()), Reserve1: domain.Amount(r1.String()), TotalShares: domain.Amount(shares.String())}
		}
	}
	profile, _ := domain.Protocol(p.Protocol)
	result := domain.Pool{InfinityBin: s, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
	return result, result.Validate()
}
