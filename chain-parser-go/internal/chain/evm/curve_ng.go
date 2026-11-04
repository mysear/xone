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

// Compiler-owned code_layout at pinned NG revision/Vyper0.3.10. This is an
// admission layout, not a proof that an operator's arbitrary runtime is official.
const curveNGImmutableBytes = 2144
const curveNGRuntimeTemplateBytes = 21491
const curveNGRuntimeTemplateSHA256 = "390b83f661547f817358df2b503210646c1749400f7eb4cad3a9547f3c462810"

func (a *Adapter) readCurveNG(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	v := p.MultiAsset
	fail := func(code string) (domain.Pool, error) { return domain.Pool{}, errors.New(code) }
	if p.Protocol != "curve-stableswap-ng" || v == nil || v.TokenPolicy != "standard-no-bpt" || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" || v.ExpectedVault != "" || v.ExpectedVaultCodeSHA256 != "" || v.PoolID != "" || len(v.Tokens) < 2 || len(v.Tokens) > 8 {
		return fail("CURVE_NG_CONFIG")
	}
	code, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if isStandardClone(code) || len(code) != curveNGRuntimeTemplateBytes+curveNGImmutableBytes {
		return fail("CURVE_NG_RUNTIME_LAYOUT")
	}
	templateHash := sha256.Sum256(code[:curveNGRuntimeTemplateBytes])
	if hex.EncodeToString(templateHash[:]) != curveNGRuntimeTemplateSHA256 {
		return fail("CURVE_NG_RUNTIME_TEMPLATE")
	}
	n := len(v.Tokens)
	immutable := code[len(code)-curveNGImmutableBytes:]
	word := func(at int) *big.Int { return new(big.Int).SetBytes(immutable[at : at+32]) }
	for _, offset := range []int{0, 32} {
		if word(offset).Cmp(big.NewInt(int64(n))) != 0 {
			return fail("CURVE_NG_IMMUTABLE_COIN_COUNT")
		}
	}
	// Arrays use one length word followed by up to eight ABI words.
	for _, offset := range []int{96, 384, 704, 992, 1280, 1568} {
		if word(offset).Cmp(big.NewInt(int64(n))) != 0 {
			return fail("CURVE_NG_IMMUTABLE_ARRAY")
		}
	}
	if word(672).Sign() != 0 {
		return fail("CURVE_NG_REBASING_UNSUPPORTED")
	}
	state := &domain.MultiAssetState{CodeSHA256: v.ExpectedCodeSHA256, Assets: make([]domain.MultiAsset, n), Weights: []domain.Amount{}, Amplification: "0", AdminFee: 5000000000}
	ng := &domain.CurveNGState{StoredBalances: make([]domain.Amount, n), AdminBalances: make([]domain.Amount, n), LastPricesPacked: make([]domain.Amount, n-1), Timestamp: block.TimestampMS / 1000, BlockNumber: block.Number, BlockHash: block.Hash}
	state.CurveNG = ng
	call := func(to, data string, words int) ([]byte, error) { return a.stableCall(ctx, to, data, anchor, words) }
	uintCall := func(target, data string) (*big.Int, error) {
		raw, e := call(target, data, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(raw, 256)
	}
	address := func(raw []byte) (string, error) {
		if _, e := wordUint(raw, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(raw[12:]), nil
	}
	array := func(selector string, count int) ([]domain.Amount, error) {
		raw, e := call(p.ID, selector, -1)
		if e != nil {
			return nil, e
		}
		if len(raw) != 64+32*count || new(big.Int).SetBytes(raw[:32]).Cmp(big.NewInt(32)) != 0 || new(big.Int).SetBytes(raw[32:64]).Cmp(big.NewInt(int64(count))) != 0 {
			return nil, errors.New("CURVE_NG_ARRAY_ABI")
		}
		out := make([]domain.Amount, count)
		for i := range out {
			out[i] = domain.Amount(new(big.Int).SetBytes(raw[64+i*32 : 96+i*32]).String())
		}
		return out, nil
	}
	storage := func(slot uint64) (domain.Amount, error) {
		var result string
		e := a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, fmt.Sprintf("0x%x", slot), anchor}, &result)
		if e != nil {
			return "", e
		}
		if !strings.HasPrefix(result, "0x") {
			return "", errors.New("CURVE_NG_STORAGE_ABI")
		}
		raw, e := hex.DecodeString(result[2:])
		if e != nil || len(raw) != 32 {
			return "", errors.New("CURVE_NG_STORAGE_ABI")
		}
		return domain.AmountFromBytes(raw)
	}
	readU64 := func(selector string) (uint64, error) {
		v, e := uintCall(p.ID, selector)
		if e != nil {
			return 0, e
		}
		if !v.IsUint64() {
			return 0, errors.New("CURVE_NG_TIME_WIDTH")
		}
		return v.Uint64(), nil
	}
	onchainN, e := readU64("0x29357750")
	if e != nil {
		return domain.Pool{}, e
	}
	if onchainN != uint64(n) {
		return fail("CURVE_NG_COIN_COUNT")
	}
	rates, e := array("0xfd0684b1", n)
	if e != nil {
		return domain.Pool{}, e
	}
	effective, e := array("0x14f05979", n)
	if e != nil {
		return domain.Pool{}, e
	}
	for i, token := range v.Tokens {
		// Admit standard tokens only. Unknown oracle/4626/rebasing semantics do not
		// become standard merely because a current rate happens to equal one.
		if word(416+i*32).Sign() != 0 {
			return fail("CURVE_NG_ASSET_TYPE_UNSUPPORTED")
		}
		for _, offset := range []int{1024, 1312, 1600} {
			if word(offset+i*32).Sign() != 0 {
				return fail("CURVE_NG_RATE_DEPENDENCY_UNSUPPORTED")
			}
		}
		got, e := address(immutable[128+i*32 : 160+i*32])
		if e != nil || got != token.Token {
			return fail("CURVE_NG_IMMUTABLE_TOKEN")
		}
		rate := domain.Amount(word(736 + i*32).String())
		if rate != rates[i] {
			return fail("CURVE_NG_RATE_MISMATCH")
		}
		raw, e := call(p.ID, selectorCoins+fmt.Sprintf("%064x", i), 1)
		if e != nil {
			return domain.Pool{}, e
		}
		got, e = address(raw)
		if e != nil || got != token.Token {
			return fail("CURVE_NG_TOKEN_ORDER")
		}
		tokenCode, e := a.verifyCode(ctx, token.Token, token.ExpectedCodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(tokenCode) {
			return fail("CURVE_NG_TOKEN_PROXY_UNSUPPORTED")
		}
		decimals, e := uintCall(token.Token, selectorDecimalsWeighted)
		if e != nil {
			return domain.Pool{}, e
		}
		if !decimals.IsUint64() || decimals.Uint64() > 18 || rates[i].Big().Cmp(new(big.Int).Exp(big.NewInt(10), big.NewInt(36-int64(decimals.Uint64())), nil)) != 0 {
			return fail("CURVE_NG_DECIMAL_RATE")
		}
		admin, e := uintCall(p.ID, "0xe2e7d264"+fmt.Sprintf("%064x", i))
		if e != nil {
			return domain.Pool{}, e
		}
		ng.AdminBalances[i] = domain.Amount(admin.String())
		ng.StoredBalances[i], e = storage(uint64(2 + i))
		if e != nil {
			return domain.Pool{}, e
		}
		state.Assets[i] = domain.MultiAsset{Token: token.Token, Balance: effective[i], Scaling: rates[i], CodeSHA256: token.ExpectedCodeSHA256}
	}
	for _, entry := range []struct {
		slot  uint64
		count int
	}{{1, n}, {16, n}, {25, n - 1}} {
		length, e := storage(entry.slot)
		if e != nil {
			return domain.Pool{}, e
		}
		if length.Big().Cmp(big.NewInt(int64(entry.count))) != 0 {
			return fail("CURVE_NG_STORAGE_ARRAY_LENGTH")
		}
	}
	for i := range ng.LastPricesPacked {
		ng.LastPricesPacked[i], e = storage(uint64(26 + i))
		if e != nil {
			return domain.Pool{}, e
		}
	}
	ng.LastDPacked, e = storage(34)
	if e != nil {
		return domain.Pool{}, e
	}
	ng.MALastTime, e = storage(37)
	if e != nil {
		return domain.Pool{}, e
	}
	// Selector constants are generated from the fixed original ABI and tested.
	ng.MAExpTime, e = readU64(curveNGSelectorMAExpTime)
	if e != nil {
		return domain.Pool{}, e
	}
	ng.DMaTime, e = readU64(curveNGSelectorDMaTime)
	if e != nil {
		return domain.Pool{}, e
	}
	amp, e := uintCall(p.ID, selectorAPrecise)
	if e != nil {
		return domain.Pool{}, e
	}
	state.Amplification = domain.Amount(amp.String())
	fee, e := uintCall(p.ID, selectorFee)
	if e != nil {
		return domain.Pool{}, e
	}
	if !fee.IsUint64() {
		return fail("CURVE_NG_FEE_WIDTH")
	}
	admin, e := uintCall(p.ID, selectorAdminFee)
	if e != nil {
		return domain.Pool{}, e
	}
	if !admin.IsUint64() {
		return fail("CURVE_NG_ADMIN_WIDTH")
	}
	state.AdminFee = admin.Uint64()
	offpeg, e := uintCall(p.ID, "0x8edfdd5f")
	if e != nil {
		return domain.Pool{}, e
	}
	ng.OffpegFeeMultiplier = domain.Amount(offpeg.String())
	pool := domain.Pool{MultiAsset: state, ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: state.Assets[0].Balance, Reserve1: state.Assets[1].Balance, FeeNumerator: fee.Uint64(), FeeDenominator: 10000000000, AdapterVersion: "curve-stableswap-ng/3332cd44-standard-1", TokenBehavior: "standard"}
	return pool, pool.Validate()
}

const curveNGSelectorMAExpTime = "0x1be913a5"
const curveNGSelectorDMaTime = "0x9c4258c4"
