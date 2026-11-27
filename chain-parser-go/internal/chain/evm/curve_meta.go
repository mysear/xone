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
	if v.EnableUnderlying && (a.Chain.Key != "evm:1" ||
		v.ExpectedBasePoolCodeSHA256 != domain.CurveMetaBaseRuntimeSHA256 ||
		v.Tokens[1].ExpectedCodeSHA256 != domain.CurveMetaBaseLPRuntimeSHA256) {
		return fail("CURVE_META_BASE_IDENTITY")
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
	if v.EnableUnderlying {
		m.BasePoolState, e = a.readCurveMetaBasePool(ctx, anchor, amount)
		if e != nil {
			return domain.Pool{}, e
		}
	}
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

// The pinned 3pool runtime uses its own A precision and a separate LP token.
func (a *Adapter) readCurveMetaBasePool(ctx context.Context, anchor map[string]any, amount func(string, string) (domain.Amount, error)) (*domain.CurveMetaBasePoolState, error) {
	pool := domain.CurveMetaBasePool
	// The Vyper storage layout places the LP token at slot 5 and is_killed at slot 15.
	for _, field := range []struct{ slot, want string }{
		{"0x5", "0x" + strings.Repeat("0", 24) + domain.CurveMetaBaseLP[2:]},
		{"0xf", "0x" + strings.Repeat("0", 64)},
	} {
		var got string
		if err := a.Client.Call(ctx, "eth_getStorageAt", []any{pool, field.slot, anchor}, &got); err != nil {
			return nil, err
		}
		if got != field.want {
			return nil, errors.New("CURVE_META_BASE_STORAGE")
		}
	}
	base := &domain.CurveMetaBasePoolState{
		Balances: make([]domain.Amount, 3), AdminBalances: make([]domain.Amount, 3),
		Rates: []domain.Amount{"1000000000000000000", "1000000000000000000000000000000", "1000000000000000000000000000000"},
	}
	for i, token := range []string{
		"0x6b175474e89094c44da98b954eedeac495271d0f",
		"0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
		"0xdac17f958d2ee523a2206206994597c13d831ec7",
	} {
		index := fmt.Sprintf("%064x", i)
		coin, err := a.stableCall(ctx, pool, selectorCoins+index, anchor, 1)
		if err != nil {
			return nil, err
		}
		if "0x"+hex.EncodeToString(coin) != "0x"+strings.Repeat("0", 24)+token[2:] {
			return nil, errors.New("CURVE_META_BASE_COIN_ORDER")
		}
		base.Balances[i], err = amount(pool, selectorBalances+index)
		if err != nil {
			return nil, err
		}
		base.AdminBalances[i], err = amount(pool, "0xe2e7d264"+index) // admin_balances(uint256)
		if err != nil {
			return nil, err
		}
	}
	amp, err := amount(pool, "0xf446c1d0") // A()
	if err != nil {
		return nil, err
	}
	base.Amplification = domain.Amount(new(big.Int).Mul(amp.Big(), big.NewInt(100)).String())
	fee, err := amount(pool, selectorFee)
	if err != nil {
		return nil, err
	}
	admin, err := amount(pool, selectorAdminFee)
	if err != nil {
		return nil, err
	}
	if !fee.Big().IsUint64() || !admin.Big().IsUint64() {
		return nil, errors.New("CURVE_META_BASE_FEE_WIDTH")
	}
	base.Fee = fee.Big().Uint64()
	base.AdminFee = admin.Big().Uint64()
	base.TotalSupply, err = amount(domain.CurveMetaBaseLP, "0x18160ddd") // totalSupply()
	if err != nil {
		return nil, err
	}
	return base, nil
}
