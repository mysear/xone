package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
)

const MinV3Tick int32 = -887272
const MaxV3Tick int32 = 887272

type V3BitmapWord struct {
	WordPos int32  `json:"word_pos"`
	Bitmap  Amount `json:"bitmap"`
}
type V3Tick struct {
	Tick           int32  `json:"tick"`
	LiquidityGross Amount `json:"liquidity_gross"`
	LiquidityNet   string `json:"liquidity_net"`
}
type V3State struct {
	Implementation           string         `json:"implementation,omitempty"`
	ImplementationCodeSHA256 string         `json:"implementation_code_sha256,omitempty"`
	FactoryCodeSHA256        string         `json:"factory_code_sha256,omitempty"`
	FeeProtocol              uint32         `json:"fee_protocol,omitempty"`
	SqrtPriceX96             Amount         `json:"sqrt_price_x96"`
	Tick                     int32          `json:"tick"`
	Liquidity                Amount         `json:"liquidity"`
	TickSpacing              int32          `json:"tick_spacing"`
	Factory                  string         `json:"factory"`
	CodeSHA256               string         `json:"code_sha256"`
	MaxSteps                 uint32         `json:"max_steps"`
	TickWords                []V3BitmapWord `json:"tick_words"`
	Ticks                    []V3Tick       `json:"ticks"`
}

var v3Factors = []string{"fffcb933bd6fad37aa2d162d1a594001", "fff97272373d413259a46990580e213a", "fff2e50f5f656932ef12357cf3c7fdcc", "ffe5caca7e10e4e61c3624eaa0941cd0", "ffcb9843d60f6159c9db58835c926644", "ff973b41fa98c081472e6896dfb254c0", "ff2ea16466c96a3843ec78b326b52861", "fe5dee046a99a2a811c461f1969c3053", "fcbe86c7900a88aedcffc83b479aa3a4", "f987a7253ac413176f2b074cf7815e54", "f3392b0822b70005940c7a398e4b70f3", "e7159475a2c29b7443b29c7fa6e889d9", "d097f3bdfd2022b8845ad8f792aa5825", "a9f746462d870fdf8a65dc1f90e061e5", "70d869a156d2a1b890bb3df62baf32f7", "31be135f97d08fd981231505542fcfa6", "9aa508b5b7a84e1c677de54f3e99bc9", "5d6af8dedb81196699c329225ee604", "2216e584f5fa1ea926041bedfe98", "48a170391f7dc42444e8fa2"}

// sqrt(1.0001^tick) in Q64.96, using the protocol's Q128.128
// mathematical constants and final upward conversion. No floating point.
func SqrtRatioAtTick(tick int32) (*big.Int, error) {
	if tick < MinV3Tick || tick > MaxV3Tick {
		return nil, errors.New("V3_TICK_RANGE")
	}
	abs := tick
	if abs < 0 {
		abs = -abs
	}
	ratio := new(big.Int).Lsh(big.NewInt(1), 128)
	for i, c := range v3Factors {
		if uint32(abs)&(1<<i) != 0 {
			factor, _ := new(big.Int).SetString(c, 16)
			ratio.Rsh(new(big.Int).Mul(ratio, factor), 128)
		}
	}
	if tick > 0 {
		max := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 256), big.NewInt(1))
		ratio.Quo(max, ratio)
	}
	rem := new(big.Int).And(new(big.Int).Set(ratio), new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 32), big.NewInt(1)))
	ratio.Rsh(ratio, 32)
	if rem.Sign() != 0 {
		ratio.Add(ratio, big.NewInt(1))
	}
	return ratio, nil
}
func ParseLiquidityNet(s string) (*big.Int, error) {
	n, ok := new(big.Int).SetString(s, 10)
	min := new(big.Int).Neg(new(big.Int).Lsh(big.NewInt(1), 127))
	max := new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 127), big.NewInt(1))
	if !ok || n.String() != s || n.Cmp(min) < 0 || n.Cmp(max) > 0 {
		return nil, errors.New("V3_LIQUIDITY_NET_WIDTH")
	}
	return n, nil
}
func (v V3State) Validate() error {
	if v.FactoryCodeSHA256 != "" {
		if e := ValidateEVMIdentity(v.Factory, v.FactoryCodeSHA256); e != nil {
			return e
		}
	}
	if v.Implementation != "" {
		if e := ValidateEVMIdentity(v.Implementation, v.ImplementationCodeSHA256); e != nil {
			return e
		}
	} else if v.ImplementationCodeSHA256 != "" {
		return errors.New("V3_IMPLEMENTATION_IDENTITY")
	}
	if v.Tick < MinV3Tick || v.Tick >= MaxV3Tick || v.TickSpacing <= 0 || v.TickSpacing >= 16384 || v.MaxSteps == 0 || v.MaxSteps > 8192 || len(v.TickWords) == 0 || len(v.TickWords) > 33 || len(v.Ticks) > 4096 {
		return errors.New("V3_STATE_LIMIT")
	}
	if _, e := hex.DecodeString(v.CodeSHA256); e != nil || len(v.CodeSHA256) != 64 || v.CodeSHA256 != strings.ToLower(v.CodeSHA256) {
		return errors.New("V3_CODE_IDENTITY")
	}
	if !strings.HasPrefix(v.Factory, "0x") || len(v.Factory) != 42 || v.Factory != strings.ToLower(v.Factory) {
		return errors.New("V3_FACTORY_IDENTITY")
	}
	if _, e := hex.DecodeString(v.Factory[2:]); e != nil {
		return errors.New("V3_FACTORY_IDENTITY")
	}
	price, e := ParseAmount(string(v.SqrtPriceX96))
	if e != nil || price.Big().BitLen() > 160 || price.Big().Sign() == 0 {
		return errors.New("V3_PRICE_WIDTH")
	}
	liquidity, e := ParseAmount(string(v.Liquidity))
	if e != nil || liquidity.Big().BitLen() > 128 {
		return errors.New("V3_LIQUIDITY_WIDTH")
	}
	lower, _ := SqrtRatioAtTick(v.Tick)
	upper, _ := SqrtRatioAtTick(v.Tick + 1)
	min, _ := SqrtRatioAtTick(MinV3Tick)
	max, _ := SqrtRatioAtTick(MaxV3Tick)
	if price.Big().Cmp(lower) < 0 || price.Big().Cmp(upper) > 0 || price.Big().Cmp(min) < 0 || price.Big().Cmp(max) >= 0 {
		return errors.New("V3_PRICE_TICK_MISMATCH")
	}
	expected := []int32{}
	for i, w := range v.TickWords {
		if w.WordPos < -32768 || w.WordPos > 32767 || i > 0 && w.WordPos != v.TickWords[i-1].WordPos+1 {
			return errors.New("V3_BITMAP_SEQUENCE")
		}
		bitmap, e := ParseAmount(string(w.Bitmap))
		if e != nil {
			return e
		}
		for bit := 0; bit < 256; bit++ {
			if bitmap.Big().Bit(bit) != 0 {
				tick := (int64(w.WordPos)*256 + int64(bit)) * int64(v.TickSpacing)
				if tick < int64(MinV3Tick) || tick > int64(MaxV3Tick) {
					return errors.New("V3_BITMAP_TICK_RANGE")
				}
				expected = append(expected, int32(tick))
			}
		}
	}
	lo := int64(v.TickWords[0].WordPos) * 256 * int64(v.TickSpacing)
	hi := (int64(v.TickWords[len(v.TickWords)-1].WordPos) + 1) * 256 * int64(v.TickSpacing)
	if int64(v.Tick) < lo-1 || int64(v.Tick) >= hi {
		return errors.New("V3_CURRENT_TICK_UNPROVEN")
	}
	if len(expected) != len(v.Ticks) {
		return errors.New("V3_INCOMPLETE_TICKS")
	}
	numTicks := (MaxV3Tick / v.TickSpacing) - (MinV3Tick / v.TickSpacing) + 1
	maxGross := new(big.Int).Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), big.NewInt(int64(numTicks)))
	for i, t := range v.Ticks {
		if t.Tick != expected[i] {
			return errors.New("V3_INCOMPLETE_TICKS")
		}
		gross, e := ParseAmount(string(t.LiquidityGross))
		if e != nil || gross.Big().Sign() == 0 || gross.Big().Cmp(maxGross) > 0 {
			return errors.New("V3_TICK_GROSS")
		}
		net, e := ParseLiquidityNet(t.LiquidityNet)
		if e != nil {
			return e
		}
		if new(big.Int).Abs(net).Cmp(gross.Big()) > 0 {
			return errors.New("V3_TICK_NET")
		}
	}
	return nil
}
func (p Pool) Clone() Pool {
	if p.InfinityBin != nil {
		c := p.InfinityBin.Clone()
		c.TreeLevel1 = append([]InfinityBinTreeWord{}, c.TreeLevel1...)
		c.TreeLevel2 = append([]InfinityBinTreeWord{}, c.TreeLevel2...)
		c.Bins = append([]InfinityBinEntry{}, c.Bins...)
		p.InfinityBin = &c
	}
	if p.RaydiumCLMM != nil {
		c := *p.RaydiumCLMM
		c.RewardInfos = append([]RaydiumCLMMRewardInfo(nil), c.RewardInfos...)
		if c.DynamicFee != nil {
			d := *c.DynamicFee
			c.DynamicFee = &d
		}
		c.Bitmap = append([]uint64{}, c.Bitmap...)
		if c.BitmapExtension != nil {
			e := *c.BitmapExtension
			e.Positive = append([]uint64{}, e.Positive...)
			e.Negative = append([]uint64{}, e.Negative...)
			c.BitmapExtension = &e
		}
		c.Observations = append([]RaydiumCLMMObservation{}, c.Observations...)
		c.TickArrays = append([]RaydiumCLMMArray{}, c.TickArrays...)
		for i := range c.TickArrays {
			c.TickArrays[i].Ticks = append([]RaydiumCLMMTick{}, c.TickArrays[i].Ticks...)
			for j := range c.TickArrays[i].Ticks {
				c.TickArrays[i].Ticks[j].RewardGrowthsOutside = append([]Amount(nil), c.TickArrays[i].Ticks[j].RewardGrowthsOutside...)
			}
		}
		p.RaydiumCLMM = &c
	}
	if p.OrcaWhirlpool != nil {
		c := *p.OrcaWhirlpool
		if c.Adaptive != nil {
			a := *c.Adaptive
			a.ConstantsReserved = append([]uint32(nil), a.ConstantsReserved...)
			a.VariablesReserved = append([]uint32(nil), a.VariablesReserved...)
			a.Reserved = append([]uint32(nil), a.Reserved...)
			c.Adaptive = &a
		}
		c.RewardInfos = append([]OrcaRewardInfo(nil), c.RewardInfos...)
		for i := range c.RewardInfos {
			c.RewardInfos[i].Extension = append([]uint32(nil), c.RewardInfos[i].Extension...)
		}
		c.TickArrays = append([]OrcaTickArray{}, c.TickArrays...)
		for i := range c.TickArrays {
			c.TickArrays[i].Ticks = append([]OrcaTick{}, c.TickArrays[i].Ticks...)
			for j := range c.TickArrays[i].Ticks {
				c.TickArrays[i].Ticks[j].RewardGrowthsOutside = append([]Amount(nil), c.TickArrays[i].Ticks[j].RewardGrowthsOutside...)
			}
		}
		p.OrcaWhirlpool = &c
	}
	if p.RaydiumCPMM != nil {
		copy := *p.RaydiumCPMM
		p.RaydiumCPMM = &copy
	}
	if p.SingletonCL != nil {
		c := *p.SingletonCL
		c.Core.TickWords = append([]V3BitmapWord{}, c.Core.TickWords...)
		c.Core.Ticks = append([]V3Tick{}, c.Core.Ticks...)
		p.SingletonCL = &c
	}

	if p.SingletonAdvanced != nil {
		c := *p.SingletonAdvanced
		c.Snapshot.Core.TickWords = append([]V3BitmapWord(nil), c.Snapshot.Core.TickWords...)
		c.Snapshot.Core.Ticks = append([]V3Tick(nil), c.Snapshot.Core.Ticks...)
		p.SingletonAdvanced = &c
	}
	if p.MultiAsset != nil {
		c := *p.MultiAsset
		if c.ManagedV2 != nil {
			m := *c.ManagedV2
			if m.ExecutionContext != nil {
				context := *m.ExecutionContext
				m.ExecutionContext = &context
			}
			m.Dependencies = append([]ManagedV2Dependency{}, m.Dependencies...)
			m.TokenInfo = append([]ManagedV2TokenInfo{}, m.TokenInfo...)
			c.ManagedV2 = &m
		}
		if c.ManagedFees != nil {
			fees := *c.ManagedFees
			c.ManagedFees = &fees
		}
		if c.CurveCrypto != nil {
			crypto := *c.CurveCrypto
			crypto.RawBalances = append([]Amount{}, crypto.RawBalances...)
			if crypto.Ramp != nil {
				ramp := *crypto.Ramp
				crypto.Ramp = &ramp
			}
			c.CurveCrypto = &crypto
		}
		if c.CurveMeta != nil {
			meta := *c.CurveMeta
			meta.StoredBalances = append([]Amount{}, meta.StoredBalances...)
			meta.AdminBalances = append([]Amount{}, meta.AdminBalances...)
			c.CurveMeta = &meta
		}
		if c.CurveNG != nil {
			ng := *c.CurveNG
			ng.StoredBalances = append([]Amount{}, ng.StoredBalances...)
			ng.AdminBalances = append([]Amount{}, ng.AdminBalances...)
			ng.LastPricesPacked = append([]Amount{}, ng.LastPricesPacked...)
			c.CurveNG = &ng
		}
		c.Assets = append([]MultiAsset(nil), c.Assets...)
		c.Weights = append([]Amount{}, c.Weights...)
		p.MultiAsset = &c
	}
	if p.Weighted != nil {
		copy := *p.Weighted
		p.Weighted = &copy
	}
	if p.Stable != nil {
		copy := *p.Stable
		p.Stable = &copy
	}
	if p.V2 != nil {
		copy := *p.V2
		if copy.Swap != nil {
			state := *copy.Swap
			copy.Swap = &state
		}
		p.V2 = &copy
	}
	if p.V3 != nil {
		copy := *p.V3
		copy.TickWords = append([]V3BitmapWord{}, p.V3.TickWords...)
		copy.Ticks = append([]V3Tick{}, p.V3.Ticks...)
		p.V3 = &copy
	}
	return p
}
