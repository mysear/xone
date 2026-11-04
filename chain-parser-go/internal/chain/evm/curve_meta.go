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

func (a *Adapter) readCurveMeta(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	v := p.MultiAsset
	if p.Protocol != "curve-metausd-direct" || v == nil || v.TokenPolicy != "standard-base-lp" || len(v.Tokens) != 2 || v.Tokens[1].Token != domain.CurveMetaBaseLP || v.ExpectedImplementation == "" || v.ExpectedImplementationCodeSHA256 != domain.CurveMetaRuntimeSHA256 || v.ExpectedBasePoolCodeSHA256 == "" || v.ExpectedVault != "" || v.ExpectedVaultCodeSHA256 != "" || v.PoolID != "" || block.TimestampMS == 0 || block.TimestampMS%1000 != 0 {
		return fail("CURVE_META_CONFIG")
	}
	code, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if !isStandardClone(code) || "0x"+hex.EncodeToString(code[10:30]) != v.ExpectedImplementation {
		return fail("CURVE_META_CLONE_CODE")
	}
	implementation, e := a.verifyCode(ctx, v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	hash := sha256.Sum256(implementation)
	if len(implementation) != 20060 || hex.EncodeToString(hash[:]) != domain.CurveMetaRuntimeSHA256 {
		return fail("CURVE_META_IMPLEMENTATION")
	}
	base, e := a.verifyCode(ctx, domain.CurveMetaBasePool, v.ExpectedBasePoolCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if isStandardClone(base) {
		return fail("CURVE_META_BASE_PROXY_UNSUPPORTED")
	}
	call := func(target, data string) ([]byte, error) { return a.stableCall(ctx, target, data, anchor, 1) }
	amount := func(target, data string) (domain.Amount, error) {
		raw, e := call(target, data)
		if e != nil {
			return "", e
		}
		value, e := wordUint(raw, 256)
		if e != nil {
			return "", e
		}
		return domain.Amount(value.String()), nil
	}
	m := &domain.CurveMetaState{Implementation: v.ExpectedImplementation, ImplementationCodeSHA256: v.ExpectedImplementationCodeSHA256, BasePool: domain.CurveMetaBasePool, BasePoolCodeSHA256: v.ExpectedBasePoolCodeSHA256, StoredBalances: make([]domain.Amount, 2), AdminBalances: make([]domain.Amount, 2), Timestamp: block.TimestampMS / 1000, BlockNumber: block.Number, BlockHash: block.Hash}
	m.BaseVirtualPrice, e = amount(m.BasePool, "0xbb7b8b80")
	if e != nil {
		return domain.Pool{}, e
	}
	var rateHex string
	if e = a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, "0xb", anchor}, &rateHex); e != nil {
		return domain.Pool{}, e
	}
	rateRaw, e := hex.DecodeString(strings.TrimPrefix(rateHex, "0x"))
	if e != nil || len(rateRaw) != 32 || !strings.HasPrefix(rateHex, "0x") {
		return fail("CURVE_META_RATE_ABI")
	}
	rate := new(big.Int).SetBytes(rateRaw)
	s := &domain.MultiAssetState{CurveMeta: m, CodeSHA256: v.ExpectedCodeSHA256, Weights: []domain.Amount{}, Assets: make([]domain.MultiAsset, 2), Amplification: "0"}
	for i, t := range v.Tokens {
		tokenCode, e := a.verifyCode(ctx, t.Token, t.ExpectedCodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(tokenCode) {
			return fail("CURVE_META_TOKEN_PROXY_UNSUPPORTED")
		}
		raw, e := call(p.ID, selectorCoins+fmt.Sprintf("%064x", i))
		if e != nil {
			return domain.Pool{}, e
		}
		if _, e = wordUint(raw, 160); e != nil {
			return domain.Pool{}, e
		}
		if "0x"+hex.EncodeToString(raw[12:]) != t.Token {
			return fail("CURVE_META_COIN_ORDER")
		}
		decimals, e := amount(t.Token, selectorDecimalsWeighted)
		if e != nil {
			return domain.Pool{}, e
		}
		d := decimals.Big()
		if !d.IsUint64() || d.Uint64() > 18 || i == 1 && d.Uint64() != 18 {
			return fail("CURVE_META_DECIMALS")
		}
		scale := new(big.Int).Exp(big.NewInt(10), big.NewInt(36-int64(d.Uint64())), nil)
		if i == 0 && rate.Cmp(scale) != 0 {
			return fail("CURVE_META_RATE_MISMATCH")
		}
		balance, e := amount(p.ID, selectorBalances+fmt.Sprintf("%064x", i))
		if e != nil {
			return domain.Pool{}, e
		}
		m.StoredBalances[i], e = amount(t.Token, "0x70a08231"+strings.Repeat("0", 24)+p.ID[2:])
		if e != nil {
			return domain.Pool{}, e
		}
		m.AdminBalances[i], e = amount(p.ID, "0xe2e7d264"+fmt.Sprintf("%064x", i))
		if e != nil {
			return domain.Pool{}, e
		}
		s.Assets[i] = domain.MultiAsset{Token: t.Token, Balance: balance, Scaling: domain.Amount(scale.String()), CodeSHA256: t.ExpectedCodeSHA256}
	}
	s.Amplification, e = amount(p.ID, selectorAPrecise)
	if e != nil {
		return domain.Pool{}, e
	}
	fee, e := amount(p.ID, selectorFee)
	if e != nil {
		return domain.Pool{}, e
	}
	admin, e := amount(p.ID, selectorAdminFee)
	if e != nil {
		return domain.Pool{}, e
	}
	if !fee.Big().IsUint64() || !admin.Big().IsUint64() {
		return fail("CURVE_META_FEE_WIDTH")
	}
	s.AdminFee = admin.Big().Uint64()
	pool := domain.Pool{MultiAsset: s, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: s.Assets[0].Balance, Reserve1: s.Assets[1].Balance, FeeNumerator: fee.Big().Uint64(), FeeDenominator: 10000000000, AdapterVersion: "curve-metausd-direct/5272fd2b-fixed-base-1", TokenBehavior: "standard"}
	return pool, pool.Validate()
}
