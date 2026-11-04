package evm

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"math/big"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

// One fixed EIP-1898 context owns the entire physical multi-token balance vector.
func (a *Adapter) readMultiAsset(ctx context.Context, p config.PoolConfig, anchor map[string]any, blockNumber uint64) (domain.Pool, error) {
	if p.Protocol == "balancer-v2-managed-fees" {
		return domain.Pool{}, errors.New("MANAGED_FEE_RUNTIME_UNAVAILABLE")
	}
	v := p.MultiAsset
	profile, known := domain.Protocol(p.Protocol)
	if !known || profile.Family != "multi" || v == nil || (v.TokenPolicy != "standard-no-bpt" && !(p.Protocol == "balancer-v2-composable-regular" && v.TokenPolicy == "standard-with-own-bpt")) || len(v.Tokens) < 2 || len(v.Tokens) > 38 {
		return domain.Pool{}, errors.New("MULTI_ASSET_CONFIG")
	}
	fail := func() (domain.Pool, error) { return domain.Pool{}, errors.New("MULTI_ASSET_RPC_STATE") }
	code, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	weighted := strings.HasPrefix(p.Protocol, "balancer-v2-")
	composable := p.Protocol == "balancer-v2-composable-regular"
	managed := p.Protocol == "balancer-v2-managed-snapshot"
	if weighted {
		if isStandardClone(code) || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" {
			return fail()
		}
	} else if e = a.verifyClone(ctx, code, v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256, anchor); e != nil {
		return domain.Pool{}, e
	}
	call := func(to, data string, words int) ([]byte, error) { return a.stableCall(ctx, to, data, anchor, words) }
	address := func(b []byte) (string, error) {
		if _, e := wordUint(b, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(b[12:]), nil
	}
	uintCall := func(to, data string) (*big.Int, error) {
		b, e := call(to, data, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(b, 256)
	}
	n := len(v.Tokens)
	s := &domain.MultiAssetState{CodeSHA256: v.ExpectedCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Assets: make([]domain.MultiAsset, n), Weights: []domain.Amount{}, Amplification: "0"}
	for i, t := range v.Tokens {
		code, e := a.verifyCode(ctx, t.Token, t.ExpectedCodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(code) {
			return fail()
		}
		s.Assets[i] = domain.MultiAsset{Token: t.Token, Balance: "0", Scaling: "0", CodeSHA256: t.ExpectedCodeSHA256}
	}
	var fee *big.Int
	if weighted {
		if len(v.PoolID) != 66 {
			return fail()
		}
		vaultCode, e := a.verifyCode(ctx, v.ExpectedVault, v.ExpectedVaultCodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(vaultCode) {
			return fail()
		}
		b, e := call(p.ID, selectorGetVault, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		got, e := address(b)
		if e != nil || got != v.ExpectedVault {
			return fail()
		}
		b, e = call(p.ID, selectorGetPoolID, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		if "0x"+hex.EncodeToString(b) != v.PoolID {
			return fail()
		}
		b, e = call(v.ExpectedVault, selectorGetPool+v.PoolID[2:], 2)
		if e != nil {
			return domain.Pool{}, e
		}
		got, e = address(b[:32])
		if e != nil || got != p.ID || new(big.Int).SetBytes(b[32:]).Cmp(func() *big.Int {
			if composable {
				return big.NewInt(0)
			}
			return big.NewInt(1)
		}()) != 0 {
			return fail()
		}
		for _, target := range []string{p.ID, v.ExpectedVault} {
			b, e = call(target, selectorPausedState, 3)
			if e != nil {
				return domain.Pool{}, e
			}
			if new(big.Int).SetBytes(b[:32]).Sign() != 0 {
				return fail()
			}
		}
		array := func(selector string) ([]domain.Amount, error) {
			b, e := call(p.ID, selector, -1)
			if e != nil {
				return nil, e
			}
			if len(b) != 64+32*n || new(big.Int).SetBytes(b[:32]).Cmp(big.NewInt(32)) != 0 || new(big.Int).SetBytes(b[32:64]).Cmp(big.NewInt(int64(n))) != 0 {
				return nil, errors.New("MULTI_ASSET_ARRAY_ABI")
			}
			out := make([]domain.Amount, n)
			for i := range out {
				out[i] = domain.Amount(new(big.Int).SetBytes(b[64+i*32 : 96+i*32]).String())
			}
			return out, nil
		}
		scales, e := array(selectorScaling)
		if e != nil {
			return domain.Pool{}, e
		}
		if !composable {
			s.Weights, e = array(selectorWeights)
			if e != nil {
				return domain.Pool{}, e
			}
		} else {
			bpt, e := uintCall(p.ID, "0x82687a56")
			if e != nil {
				return domain.Pool{}, e
			}
			if !bpt.IsUint64() || bpt.Uint64() >= uint64(n) || v.Tokens[bpt.Uint64()].Token != p.ID {
				return fail()
			}
			providers, e := call(p.ID, "0x238a2d59", -1)
			if e != nil {
				return domain.Pool{}, e
			}
			if len(providers) != 64+32*n || new(big.Int).SetBytes(providers[:32]).Cmp(big.NewInt(32)) != 0 || new(big.Int).SetBytes(providers[32:64]).Cmp(big.NewInt(int64(n))) != 0 {
				return fail()
			}
			for _, v := range providers[64:] {
				if v != 0 {
					return fail()
				}
			}
			amp, e := call(p.ID, selectorAmplification, 3)
			if e != nil {
				return domain.Pool{}, e
			}
			if new(big.Int).SetBytes(amp[64:96]).Cmp(big.NewInt(1000)) != 0 {
				return fail()
			}
			s.Amplification = domain.Amount(new(big.Int).SetBytes(amp[:32]).String())
		}
		if managed {
			enabled, e := uintCall(p.ID, "0x47bc4d92")
			if e != nil {
				return domain.Pool{}, e
			}
			if enabled.Cmp(big.NewInt(1)) != 0 {
				return fail()
			}
			for _, sel := range []string{"0x77eef903", "0x90605f3a"} {
				value, e := uintCall(p.ID, sel)
				if e != nil {
					return domain.Pool{}, e
				}
				if value.Sign() != 0 {
					return fail()
				}
			}
		}
		for i, t := range v.Tokens {
			d, e := uintCall(t.Token, selectorDecimalsWeighted)
			if e != nil {
				return domain.Pool{}, e
			}
			if !d.IsUint64() || d.Uint64() > 18 || scales[i].Big().Cmp(new(big.Int).Exp(big.NewInt(10), big.NewInt(36-int64(d.Uint64())), nil)) != 0 {
				return fail()
			}
			s.Assets[i].Scaling = scales[i]
		}
		b, e = call(v.ExpectedVault, selectorPoolTokens+v.PoolID[2:], -1)
		if e != nil {
			return domain.Pool{}, e
		}
		balancesOffset := 128 + n*32
		if len(b) != 160+64*n || new(big.Int).SetBytes(b[:32]).Cmp(big.NewInt(96)) != 0 || new(big.Int).SetBytes(b[32:64]).Cmp(big.NewInt(int64(balancesOffset))) != 0 || new(big.Int).SetBytes(b[96:128]).Cmp(big.NewInt(int64(n))) != 0 || new(big.Int).SetBytes(b[balancesOffset:balancesOffset+32]).Cmp(big.NewInt(int64(n))) != 0 {
			return fail()
		}
		last, e := wordUint(b[64:96], 64)
		if e != nil || last.Uint64() > blockNumber {
			return fail()
		}
		for i, t := range v.Tokens {
			got, e = address(b[128+i*32 : 160+i*32])
			if e != nil || got != t.Token {
				return fail()
			}
			balance, e := wordUint(b[balancesOffset+32+i*32:balancesOffset+64+i*32], 112)
			if e != nil {
				return fail()
			}
			s.Assets[i].Balance = domain.Amount(balance.String())
			info, e := call(v.ExpectedVault, selectorTokenInfo+v.PoolID[2:]+strings.Repeat("0", 24)+t.Token[2:], 4)
			if e != nil {
				return domain.Pool{}, e
			}
			cash, e := wordUint(info[:32], 112)
			if e != nil || cash.Cmp(balance) != 0 || new(big.Int).SetBytes(info[32:64]).Sign() != 0 || new(big.Int).SetBytes(info[64:96]).Cmp(last) > 0 || new(big.Int).SetBytes(info[96:]).Sign() != 0 {
				return fail()
			}
		}
		fee, e = uintCall(p.ID, selectorSwapFee)
		if e != nil {
			return domain.Pool{}, e
		}
	} else {
		// This reader is exclusively for the pinned Plain3Basic/Plain4Basic layout.
		// Their private rate_multipliers begin after coins,balances,fee and A ramp.
		for i, t := range v.Tokens {
			b, e := call(p.ID, selectorCoins+fmt.Sprintf("%064x", i), 1)
			if e != nil {
				return domain.Pool{}, e
			}
			got, e := address(b)
			if e != nil || got != t.Token {
				return fail()
			}
			balance, e := uintCall(p.ID, selectorBalances+fmt.Sprintf("%064x", i))
			if e != nil {
				return domain.Pool{}, e
			}
			s.Assets[i].Balance = domain.Amount(balance.String())
			var raw string
			if e = a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, fmt.Sprintf("0x%x", 2*n+7+i), anchor}, &raw); e != nil {
				return domain.Pool{}, e
			}
			value, e := hex.DecodeString(strings.TrimPrefix(raw, "0x"))
			if e != nil || len(value) != 32 {
				return fail()
			}
			s.Assets[i].Scaling = domain.Amount(new(big.Int).SetBytes(value).String())
		}
		amp, e := uintCall(p.ID, selectorAPrecise)
		if e != nil {
			return domain.Pool{}, e
		}
		s.Amplification = domain.Amount(amp.String())
		admin, e := uintCall(p.ID, selectorAdminFee)
		if e != nil {
			return domain.Pool{}, e
		}
		if !admin.IsUint64() {
			return fail()
		}
		s.AdminFee = admin.Uint64()
		fee, e = uintCall(p.ID, selectorFee)
		if e != nil {
			return domain.Pool{}, e
		}
	}
	if fee == nil || !fee.IsUint64() {
		return fail()
	}
	pool := domain.Pool{MultiAsset: s, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: s.Assets[0].Balance, Reserve1: s.Assets[1].Balance, FeeNumerator: fee.Uint64(), FeeDenominator: profile.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
	return pool, pool.Validate()
}
