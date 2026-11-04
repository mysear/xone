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

// Fixed original CurveCryptoSwap2/Vyper0.3.3 plus five immutable words; original
// CurveTokenV4/Vyper0.3.1. Operator token admission still requires standard ERC20s.
const curveCryptoRuntimeBytes = 20814
const curveCryptoImmutableBytes = 160
const curveCryptoRuntimeSHA256 = "c19b756213feac7b03ee10b12888c3abe427bd10f9ec125004091b56dd995a80"

func (a *Adapter) readCurveCrypto(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (domain.Pool, error) {
	fail := func(s string) (domain.Pool, error) { return domain.Pool{}, errors.New(s) }
	v := p.MultiAsset
	if (p.Protocol != "curve-crypto2-full" && p.Protocol != "curve-crypto2-ramp") || v == nil || v.TokenPolicy != "standard-no-bpt" || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" || v.ExpectedVault != "" || v.ExpectedVaultCodeSHA256 != "" || v.PoolID != "" || v.ExpectedBasePoolCodeSHA256 != "" || len(v.Tokens) != 2 || v.ExpectedLPTokenCodeSHA256 != domain.CurveCryptoLPCodeSHA256 || domain.ValidateEVMAddress(v.ExpectedLPToken) != nil {
		return fail("CURVE_CRYPTO_CONFIG")
	}
	code, e := a.verifyCode(ctx, p.ID, v.ExpectedCodeSHA256, anchor)
	if e != nil {
		return domain.Pool{}, e
	}
	if len(code) != curveCryptoRuntimeBytes+curveCryptoImmutableBytes {
		return fail("CURVE_CRYPTO_RUNTIME_LAYOUT")
	}
	hash := sha256.Sum256(code[:curveCryptoRuntimeBytes])
	if hex.EncodeToString(hash[:]) != curveCryptoRuntimeSHA256 {
		return fail("CURVE_CRYPTO_RUNTIME_TEMPLATE")
	}
	tail := code[curveCryptoRuntimeBytes:]
	addr := func(b []byte) (string, error) {
		if _, e := wordUint(b, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(b[12:]), nil
	}
	lp, e := addr(tail[:32])
	if e != nil || lp != v.ExpectedLPToken {
		return fail("CURVE_CRYPTO_IMMUTABLE_LP")
	}
	call := func(to, data string) (*big.Int, error) {
		b, e := a.stableCall(ctx, to, data, anchor, 1)
		if e != nil {
			return nil, e
		}
		return wordUint(b, 256)
	}
	addressCall := func(to, data string) (string, error) {
		b, e := a.stableCall(ctx, to, data, anchor, 1)
		if e != nil {
			return "", e
		}
		return addr(b)
	}
	got, e := addressCall(p.ID, "0xfc0c546a")
	if e != nil {
		return domain.Pool{}, e
	}
	if got != lp {
		return fail("CURVE_CRYPTO_LP_GETTER")
	}
	if _, e = a.verifyCode(ctx, lp, v.ExpectedLPTokenCodeSHA256, anchor); e != nil {
		return domain.Pool{}, e
	}
	got, e = addressCall(lp, "0x07546172")
	if e != nil {
		return domain.Pool{}, e
	}
	if got != p.ID {
		return fail("CURVE_CRYPTO_LP_MINTER")
	}
	c := &domain.CurveCryptoState{LPToken: lp, LPTokenCodeSHA256: v.ExpectedLPTokenCodeSHA256, RawBalances: make([]domain.Amount, 2), Timestamp: block.TimestampMS / 1000, BlockNumber: block.Number, BlockHash: block.Hash}
	s := &domain.MultiAssetState{CurveCrypto: c, CodeSHA256: v.ExpectedCodeSHA256, Assets: make([]domain.MultiAsset, 2), Weights: []domain.Amount{}}
	storage := func(slot uint64) (*big.Int, error) {
		var out string
		e := a.Client.Call(ctx, "eth_getStorageAt", []any{p.ID, fmt.Sprintf("0x%x", slot), anchor}, &out)
		if e != nil {
			return nil, e
		}
		if !strings.HasPrefix(out, "0x") {
			return nil, errors.New("CURVE_CRYPTO_STORAGE_ABI")
		}
		b, e := hex.DecodeString(out[2:])
		if e != nil || len(b) != 32 {
			return nil, errors.New("CURVE_CRYPTO_STORAGE_ABI")
		}
		return new(big.Int).SetBytes(b), nil
	}
	amountSlot := func(slot uint64, out *domain.Amount) error {
		b, e := storage(slot)
		if e != nil {
			return e
		}
		*out = domain.Amount(b.String())
		return nil
	}
	u64Slot := func(slot uint64) (uint64, error) {
		b, e := storage(slot)
		if e != nil {
			return 0, e
		}
		if !b.IsUint64() {
			return 0, errors.New("CURVE_CRYPTO_STATE_WIDTH")
		}
		return b.Uint64(), nil
	}
	for i, t := range v.Tokens {
		got, e := addr(tail[32+i*32 : 64+i*32])
		if e != nil || got != t.Token {
			return fail("CURVE_CRYPTO_IMMUTABLE_TOKEN")
		}
		got, e = addressCall(p.ID, selectorCoins+fmt.Sprintf("%064x", i))
		if e != nil {
			return domain.Pool{}, e
		}
		if got != t.Token {
			return fail("CURVE_CRYPTO_TOKEN_ORDER")
		}
		code, e := a.verifyCode(ctx, t.Token, t.ExpectedCodeSHA256, anchor)
		if e != nil {
			return domain.Pool{}, e
		}
		if isStandardClone(code) {
			return fail("CURVE_CRYPTO_TOKEN_PROXY")
		}
		decimals, e := call(t.Token, selectorDecimalsWeighted)
		if e != nil {
			return domain.Pool{}, e
		}
		if !decimals.IsUint64() || decimals.Uint64() > 18 {
			return fail("CURVE_CRYPTO_DECIMALS")
		}
		precision := new(big.Int).Exp(big.NewInt(10), big.NewInt(18-int64(decimals.Uint64())), nil)
		if precision.Cmp(new(big.Int).SetBytes(tail[96+i*32:128+i*32])) != 0 {
			return fail("CURVE_CRYPTO_PRECISION")
		}
		scaling := new(big.Int).Mul(precision, big.NewInt(1000000000000000000))
		s.Assets[i] = domain.MultiAsset{Token: t.Token, Scaling: domain.Amount(scaling.String()), CodeSHA256: t.ExpectedCodeSHA256}
		if e = amountSlot(uint64(23+i), &s.Assets[i].Balance); e != nil {
			return domain.Pool{}, e
		}
		raw, e := call(t.Token, "0x70a08231"+strings.Repeat("0", 24)+p.ID[2:])
		if e != nil {
			return domain.Pool{}, e
		}
		c.RawBalances[i] = domain.Amount(raw.String())
	}
	packed, e := storage(6)
	if e != nil {
		return domain.Pool{}, e
	}
	s.Amplification = domain.Amount(new(big.Int).Rsh(new(big.Int).Set(packed), 128).String())
	c.Gamma = domain.Amount(new(big.Int).And(packed, new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))).String())
	for _, f := range []struct {
		slot uint64
		out  *domain.Amount
	}{{1, &c.PriceScale}, {2, &c.PriceOracle}, {3, &c.LastPrices}, {9, &c.AllowedExtraProfit}, {11, &c.FeeGamma}, {13, &c.AdjustmentStep}, {18, &c.OutFee}, {25, &c.Invariant}, {28, &c.XCPProfit}, {29, &c.XCPProfitA}, {30, &c.VirtualPrice}} {
		if e = amountSlot(f.slot, f.out); e != nil {
			return domain.Pool{}, e
		}
	}
	if c.LastPricesTimestamp, e = u64Slot(4); e != nil {
		return domain.Pool{}, e
	}
	if c.FutureAGammaTime, e = u64Slot(8); e != nil {
		return domain.Pool{}, e
	}
	if p.Protocol == "curve-crypto2-full" && c.FutureAGammaTime > 1 {
		return fail("CURVE_CRYPTO_RAMP_UNSUPPORTED")
	}
	if p.Protocol == "curve-crypto2-ramp" {
		initial, err := storage(5)
		if err != nil {
			return domain.Pool{}, err
		}
		initialTime, err := u64Slot(7)
		if err != nil {
			return domain.Pool{}, err
		}
		c.Ramp = &domain.CurveCryptoRamp{
			InitialAmp:   domain.Amount(new(big.Int).Rsh(new(big.Int).Set(initial), 128).String()),
			InitialGamma: domain.Amount(new(big.Int).And(initial, new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1))).String()),
			InitialTime:  initialTime,
		}
	}
	if c.MAHalfTime, e = u64Slot(15); e != nil {
		return domain.Pool{}, e
	}
	mid, e := u64Slot(17)
	if e != nil {
		return domain.Pool{}, e
	}
	if mid != p.FeeNumerator {
		return fail("CURVE_CRYPTO_CONFIG_FEE")
	}
	if s.AdminFee, e = u64Slot(19); e != nil {
		return domain.Pool{}, e
	}
	notAdjusted, e := u64Slot(31)
	if e != nil || notAdjusted > 1 {
		return fail("CURVE_CRYPTO_NOT_ADJUSTED_ABI")
	}
	c.NotAdjusted = notAdjusted == 1
	killed, e := u64Slot(32)
	if e != nil {
		return domain.Pool{}, e
	}
	if killed != 0 {
		return fail("CURVE_CRYPTO_KILLED")
	}
	receiver, e := storage(36)
	if e != nil {
		return domain.Pool{}, e
	}
	receiverBytes := make([]byte, 32)
	receiver.FillBytes(receiverBytes)
	c.AdminReceiver, e = addr(receiverBytes)
	if e != nil {
		return domain.Pool{}, e
	}
	supply, e := call(lp, "0x18160ddd")
	if e != nil {
		return domain.Pool{}, e
	}
	c.TotalSupply = domain.Amount(supply.String())
	receiverBalance, e := call(lp, "0x70a08231"+strings.Repeat("0", 24)+c.AdminReceiver[2:])
	if e != nil {
		return domain.Pool{}, e
	}
	c.AdminReceiverBalance = domain.Amount(receiverBalance.String())
	profile, _ := domain.Protocol(p.Protocol)
	pool := domain.Pool{ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: s.Assets[0].Balance, Reserve1: s.Assets[1].Balance, FeeNumerator: mid, FeeDenominator: 10000000000, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard", MultiAsset: s}
	return pool, pool.Validate()
}
