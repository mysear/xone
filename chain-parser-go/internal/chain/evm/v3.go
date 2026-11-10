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

func wordUint(b []byte, bits int) (*big.Int, error) {
	if len(b) != 32 {
		return nil, errors.New("V3_ABI_WORD")
	}
	n := new(big.Int).SetBytes(b)
	if n.BitLen() > bits {
		return nil, errors.New("V3_ABI_UNSIGNED_WIDTH")
	}
	return n, nil
}
func wordInt(b []byte, bits int) (*big.Int, error) {
	if len(b) != 32 {
		return nil, errors.New("V3_ABI_WORD")
	}
	n := new(big.Int).SetBytes(b)
	if b[0]&128 != 0 {
		n.Sub(n, new(big.Int).Lsh(big.NewInt(1), 256))
	}
	min := new(big.Int).Neg(new(big.Int).Lsh(big.NewInt(1), uint(bits-1)))
	max := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), uint(bits-1)), big.NewInt(1))
	if n.Cmp(min) < 0 || n.Cmp(max) > 0 {
		return nil, errors.New("V3_ABI_SIGNED_WIDTH")
	}
	return n, nil
}
func signedArg(n int32) string {
	v := big.NewInt(int64(n))
	if n < 0 {
		v.Add(v, new(big.Int).Lsh(big.NewInt(1), 256))
	}
	return fmt.Sprintf("%064x", v)
}
func floorDiv(n, d int32) int32 {
	q := n / d
	if n < 0 && n%d != 0 {
		q--
	}
	return q
}

type v3ReadContext struct {
	allowKnownPancakeLm         bool
	allowKnownSlipstreamModules bool
	origin                      string
}

func (a *Adapter) readV3(ctx context.Context, p config.PoolConfig, anchor map[string]any) (domain.Pool, error) {
	return a.readV3Context(ctx, p, anchor, v3ReadContext{})
}
func (a *Adapter) readV3Context(ctx context.Context, p config.PoolConfig, anchor map[string]any, options v3ReadContext) (domain.Pool, error) {
	if options.allowKnownPancakeLm && (p.Protocol != "pancake-v3" || options.allowKnownSlipstreamModules) {
		return domain.Pool{}, errors.New("V3_PRIVATE_READ_CONTEXT")
	}
	if options.allowKnownSlipstreamModules && (p.Protocol != "aerodrome-slipstream" || domain.ValidateEVMAddress(options.origin) != nil) {
		return domain.Pool{}, errors.New("V3_PRIVATE_READ_CONTEXT")
	}

	invalid := func() (domain.Pool, error) { return domain.Pool{}, errors.New("V3_POOL_IDENTITY") }
	profile, known := domain.Protocol(p.Protocol)
	if !known || profile.Family != "v3" {
		return domain.Pool{}, errors.New("PROTOCOL_UNSUPPORTED")
	}
	v := p.V3
	if v == nil || v.TickWordRadius > 16 || v.MaxInitializedTicks == 0 || v.MaxInitializedTicks > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 {
		return domain.Pool{}, errors.New("V3_CONFIG_REQUIRED")
	}
	var code string
	if e := a.Client.Call(ctx, "eth_getCode", []any{p.ID, anchor}, &code); e != nil {
		return domain.Pool{}, e
	}
	raw, e := hex.DecodeString(strings.TrimPrefix(code, "0x"))
	if e != nil || len(raw) == 0 {
		return invalid()
	}
	h := sha256.Sum256(raw)
	if e = a.verifyClone(ctx, raw, v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256, anchor); e != nil {
		return domain.Pool{}, e
	}
	if hex.EncodeToString(h[:]) != v.ExpectedCodeSHA256 {
		return domain.Pool{}, errors.New("V3_CODE_HASH_MISMATCH")
	}
	call := func(data string, words int) ([]byte, error) {
		var result string
		request := map[string]any{"to": p.ID, "data": data}
		if options.origin != "" {
			request["from"] = options.origin
		}
		if e := a.Client.Call(ctx, "eth_call", []any{request, anchor}, &result); e != nil {
			return nil, e
		}
		raw, e := hex.DecodeString(strings.TrimPrefix(result, "0x"))
		if e != nil || len(raw) != words*32 {
			return nil, errors.New("V3_ABI_SIZE")
		}
		return raw, nil
	}
	for _, item := range []struct{ selector, expected string }{{"0x0dfe1681", p.Token0}, {"0xd21220a7", p.Token1}, {"0xc45a0155", v.ExpectedFactory}} {
		b, e := call(item.selector, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		if strings.Trim(hex.EncodeToString(b[:12]), "0") != "" || "0x"+hex.EncodeToString(b[12:]) != item.expected {
			return invalid()
		}
	}
	if v.ExpectedFactoryCodeSHA256 != "" || p.Protocol == "aerodrome-slipstream" {
		if dependency, err := a.verifyCode(ctx, v.ExpectedFactory, v.ExpectedFactoryCodeSHA256, anchor); err != nil || isStandardClone(dependency) {
			if err == nil {
				err = errors.New("STABLE_DEPENDENCY_PROXY_UNSUPPORTED")
			}
			return domain.Pool{}, err
		}
	}
	b, e := call("0xddca3f43", 1)
	if e != nil {
		return domain.Pool{}, e
	}
	fee, e := wordUint(b, 24)
	if e != nil || (p.Protocol != "aerodrome-slipstream" && fee.Uint64() != p.FeeNumerator) || p.FeeDenominator != 1000000 || fee.Uint64() >= 1000000 || p.Protocol == "aerodrome-slipstream" && fee.Uint64() > 100000 {
		return domain.Pool{}, errors.New("V3_FEE_MISMATCH")
	}
	b, e = call("0xd0c93a7c", 1)
	if e != nil {
		return domain.Pool{}, e
	}
	spacing, e := wordInt(b, 24)
	if e != nil || spacing.Sign() <= 0 || spacing.Int64() >= 16384 {
		return domain.Pool{}, errors.New("V3_SPACING_MISMATCH")
	}
	space := int32(spacing.Int64())
	slotWords := 7
	if p.Protocol == "aerodrome-slipstream" {
		slotWords = 6
	}
	slot, e := call("0x3850c7bd", slotWords)
	if e != nil {
		return domain.Pool{}, e
	}
	price, e := wordUint(slot[:32], 160)
	if e != nil {
		return domain.Pool{}, e
	}
	tick, e := wordInt(slot[32:64], 24)
	if e != nil || tick.Int64() < int64(domain.MinV3Tick) || tick.Int64() >= int64(domain.MaxV3Tick) {
		return domain.Pool{}, errors.New("V3_TICK_RANGE")
	}
	checks := []struct{ index, bits int }{{2, 16}, {3, 16}, {4, 16}, {5, profile.FeeProtocolBits}, {6, 1}}
	unlockedIndex := 6
	if p.Protocol == "aerodrome-slipstream" {
		checks = []struct{ index, bits int }{{2, 16}, {3, 16}, {4, 16}, {5, 1}}
		unlockedIndex = 5
		module, e := a.stableCall(ctx, v.ExpectedFactory, selectorSwapFeeModule, anchor, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		unstaked, e := a.stableCall(ctx, v.ExpectedFactory, selectorUnstakedFeeModule, anchor, 1)
		if e != nil {
			return domain.Pool{}, e
		}
		if !options.allowKnownSlipstreamModules && (new(big.Int).SetBytes(unstaked).Sign() != 0 || new(big.Int).SetBytes(module).Sign() != 0) {
			return domain.Pool{}, errors.New("SLIPSTREAM_FEE_MODULE_UNSUPPORTED")
		}
	}
	for _, x := range checks {
		if _, e = wordUint(slot[x.index*32:(x.index+1)*32], x.bits); e != nil {
			return domain.Pool{}, e
		}
	}
	if new(big.Int).SetBytes(slot[unlockedIndex*32:(unlockedIndex+1)*32]).Uint64() != 1 {
		return domain.Pool{}, errors.New("V3_POOL_LOCKED")
	}
	b, e = call("0x1a686502", 1)
	if e != nil {
		return domain.Pool{}, e
	}
	liquidity, e := wordUint(b, 128)
	if e != nil {
		return domain.Pool{}, e
	}
	feeProtocol := uint32(0)
	if p.Protocol != "aerodrome-slipstream" {
		feeProtocol = uint32(new(big.Int).SetBytes(slot[160:192]).Uint64())
	}
	if p.Protocol == "pancake-v3" {
		if feeProtocol&65535 > 10000 || feeProtocol>>16 > 10000 {
			return domain.Pool{}, errors.New("V3_PROTOCOL_FEE_RANGE")
		}
		lm, e := call("0x540d4918", 1)
		if e != nil {
			return domain.Pool{}, e
		}
		if !options.allowKnownPancakeLm && new(big.Int).SetBytes(lm).Sign() != 0 {
			return domain.Pool{}, errors.New("PANCAKE_LM_POOL_UNSUPPORTED")
		}
	}
	state := &domain.V3State{Implementation: v.ExpectedImplementation, ImplementationCodeSHA256: v.ExpectedImplementationCodeSHA256, FactoryCodeSHA256: v.ExpectedFactoryCodeSHA256, FeeProtocol: feeProtocol, SqrtPriceX96: domain.Amount(price.String()), Tick: int32(tick.Int64()), Liquidity: domain.Amount(liquidity.String()), TickSpacing: space, Factory: v.ExpectedFactory, CodeSHA256: v.ExpectedCodeSHA256, MaxSteps: v.MaxSteps, TickWords: []domain.V3BitmapWord{}, Ticks: []domain.V3Tick{}}
	center := floorDiv(floorDiv(state.Tick, space), 256)
	lower := center - int32(v.TickWordRadius)
	upper := center + int32(v.TickWordRadius)
	minWord := floorDiv(floorDiv(domain.MinV3Tick, space), 256)
	maxWord := floorDiv(floorDiv(domain.MaxV3Tick, space), 256)
	if lower < minWord {
		lower = minWord
	}
	if upper > maxWord {
		upper = maxWord
	}
	initialized := []int32{}
	for pos := lower; pos <= upper; pos++ {
		b, e = call("0x5339c296"+signedArg(pos), 1)
		if e != nil {
			return domain.Pool{}, e
		}
		bitmap := new(big.Int).SetBytes(b)
		state.TickWords = append(state.TickWords, domain.V3BitmapWord{WordPos: pos, Bitmap: domain.Amount(bitmap.String())})
		for bit := 0; bit < 256; bit++ {
			if bitmap.Bit(bit) != 0 {
				t := (int64(pos)*256 + int64(bit)) * int64(space)
				if t < int64(domain.MinV3Tick) || t > int64(domain.MaxV3Tick) {
					return domain.Pool{}, errors.New("V3_BITMAP_TICK_RANGE")
				}
				initialized = append(initialized, int32(t))
				if len(initialized) > int(v.MaxInitializedTicks) {
					return domain.Pool{}, errors.New("V3_INITIALIZED_TICK_BUDGET")
				}
			}
		}
	}
	for _, index := range initialized {
		tickWords := 8
		if p.Protocol == "aerodrome-slipstream" {
			tickWords = 10
		}
		b, e = call("0xf30dba93"+signedArg(index), tickWords)
		if e != nil {
			return domain.Pool{}, e
		}
		gross, e := wordUint(b[:32], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		net, e := wordInt(b[32:64], 128)
		if e != nil {
			return domain.Pool{}, e
		}
		offset := 0
		if p.Protocol == "aerodrome-slipstream" {
			offset = 64
			if _, e = wordInt(b[64:96], 128); e != nil {
				return domain.Pool{}, e
			}
		}
		if _, e = wordInt(b[128+offset:160+offset], 56); e != nil {
			return domain.Pool{}, e
		}
		if _, e = wordUint(b[160+offset:192+offset], 160); e != nil {
			return domain.Pool{}, e
		}
		if _, e = wordUint(b[192+offset:224+offset], 32); e != nil {
			return domain.Pool{}, e
		}
		flag, e := wordUint(b[224+offset:256+offset], 1)
		if e != nil || flag.Uint64() != 1 || gross.Sign() == 0 {
			return domain.Pool{}, errors.New("V3_INCOMPLETE_TICKS")
		}
		state.Ticks = append(state.Ticks, domain.V3Tick{Tick: index, LiquidityGross: domain.Amount(gross.String()), LiquidityNet: net.String()})
	}
	pool := domain.Pool{ChainKey: a.Chain.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: fee.Uint64(), FeeDenominator: 1000000, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard", V3: state}
	return pool, pool.Validate()
}
