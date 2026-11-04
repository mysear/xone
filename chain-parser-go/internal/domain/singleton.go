package domain

import (
	"encoding/hex"
	"errors"
	"golang.org/x/crypto/sha3"
	"math/big"
	"strings"
)

const SingletonZeroAddress = "0x0000000000000000000000000000000000000000"

type ConcentratedState struct {
	SqrtPriceX96 Amount         `json:"sqrt_price_x96"`
	Tick         int32          `json:"tick"`
	Liquidity    Amount         `json:"liquidity"`
	TickSpacing  int32          `json:"tick_spacing"`
	MaxSteps     uint32         `json:"max_steps"`
	TickWords    []V3BitmapWord `json:"tick_words"`
	Ticks        []V3Tick       `json:"ticks"`
}
type SingletonCLState struct {
	Manager           string            `json:"manager"`
	ManagerCodeSHA256 string            `json:"manager_code_sha256"`
	PoolID            string            `json:"pool_id"`
	LPFee             uint32            `json:"lp_fee"`
	ProtocolFee0      uint32            `json:"protocol_fee0"`
	ProtocolFee1      uint32            `json:"protocol_fee1"`
	Hooks             string            `json:"hooks"`
	Parameters        string            `json:"parameters"`
	Core              ConcentratedState `json:"core"`
}

func (v ConcentratedState) Validate(protocol string) error {
	if v.Tick < MinV3Tick || v.Tick >= MaxV3Tick || v.TickSpacing <= 0 || v.TickSpacing >= 32768 || v.MaxSteps == 0 || v.MaxSteps > 8192 || len(v.TickWords) == 0 || len(v.TickWords) > 33 || len(v.Ticks) > 4096 {
		return errors.New("SINGLETON_STATE_LIMIT")
	}
	price, e := ParseAmount(string(v.SqrtPriceX96))
	if e != nil || price.Big().BitLen() > 160 || price.Big().Sign() == 0 {
		return errors.New("SINGLETON_PRICE_WIDTH")
	}
	liquidity, e := ParseAmount(string(v.Liquidity))
	if e != nil || liquidity.Big().BitLen() > 128 {
		return errors.New("SINGLETON_LIQUIDITY_WIDTH")
	}
	lower, _ := SqrtRatioAtTick(v.Tick)
	upper, _ := SqrtRatioAtTick(v.Tick + 1)
	min, _ := SqrtRatioAtTick(MinV3Tick)
	max, _ := SqrtRatioAtTick(MaxV3Tick)
	if price.Big().Cmp(lower) < 0 || price.Big().Cmp(upper) > 0 || price.Big().Cmp(min) < 0 || price.Big().Cmp(max) >= 0 {
		return errors.New("SINGLETON_PRICE_TICK_MISMATCH")
	}
	expected := []int32{}
	for i, w := range v.TickWords {
		if w.WordPos < -32768 || w.WordPos > 32767 || i > 0 && w.WordPos != v.TickWords[i-1].WordPos+1 {
			return errors.New("SINGLETON_BITMAP_SEQUENCE")
		}
		bitmap, e := ParseAmount(string(w.Bitmap))
		if e != nil {
			return e
		}
		for bit := 0; bit < 256; bit++ {
			if bitmap.Big().Bit(bit) != 0 {
				tick := (int64(w.WordPos)*256 + int64(bit)) * int64(v.TickSpacing)
				if tick < int64(MinV3Tick) || tick > int64(MaxV3Tick) {
					return errors.New("SINGLETON_BITMAP_TICK_RANGE")
				}
				expected = append(expected, int32(tick))
			}
		}
	}
	lo := int64(v.TickWords[0].WordPos) * 256 * int64(v.TickSpacing)
	hi := (int64(v.TickWords[len(v.TickWords)-1].WordPos) + 1) * 256 * int64(v.TickSpacing)
	if int64(v.Tick) < lo-1 || int64(v.Tick) >= hi {
		return errors.New("SINGLETON_CURRENT_TICK_UNPROVEN")
	}
	if len(expected) != len(v.Ticks) {
		return errors.New("SINGLETON_INCOMPLETE_TICKS")
	}
	minCompressed := MinV3Tick / v.TickSpacing
	if MinV3Tick%v.TickSpacing != 0 {
		minCompressed--
	}
	numTicks := (MaxV3Tick / v.TickSpacing) - minCompressed + 1
	maxGross := new(big.Int).Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), big.NewInt(int64(numTicks)))
	for i, t := range v.Ticks {
		if t.Tick != expected[i] {
			return errors.New("SINGLETON_INCOMPLETE_TICKS")
		}
		gross, e := ParseAmount(string(t.LiquidityGross))
		if e != nil || gross.Big().Sign() == 0 || gross.Big().Cmp(maxGross) > 0 {
			return errors.New("SINGLETON_TICK_GROSS")
		}
		net, e := ParseLiquidityNet(t.LiquidityNet)
		if e != nil {
			return e
		}
		if new(big.Int).Abs(net).Cmp(gross.Big()) > 0 {
			return errors.New("SINGLETON_TICK_NET")
		}
	}
	return nil
}

func SingletonPoolID(protocol, manager, token0, token1 string, lpFee uint32, tickSpacing int32, parameters string) (string, error) {
	if ValidateEVMAddress(manager) != nil || manager == SingletonZeroAddress || ValidateEVMAddress(token0) != nil || ValidateEVMAddress(token1) != nil || token0 == SingletonZeroAddress || token0 >= token1 || lpFee >= 1000000 || tickSpacing <= 0 || tickSpacing >= 32768 {
		return "", errors.New("SINGLETON_KEY_UNSUPPORTED")
	}
	word := func(s string) []byte {
		b, _ := hex.DecodeString(strings.TrimPrefix(s, "0x"))
		return append(make([]byte, 32-len(b)), b...)
	}
	number := func(n uint64) []byte { return new(big.Int).SetUint64(n).FillBytes(make([]byte, 32)) }
	key := append(word(token0), word(token1)...)
	switch protocol {
	case "uniswap-v4":
		if parameters != "" {
			return "", errors.New("SINGLETON_PARAMETERS")
		}
		key = append(key, number(uint64(lpFee))...)
		key = append(key, number(uint64(tickSpacing))...)
		key = append(key, make([]byte, 32)...)
	case "pancake-infinity-cl":
		want := "0x" + hex.EncodeToString(new(big.Int).Lsh(big.NewInt(int64(tickSpacing)), 16).FillBytes(make([]byte, 32)))
		if parameters != want {
			return "", errors.New("SINGLETON_PARAMETERS")
		}
		key = append(key, make([]byte, 32)...)
		key = append(key, word(manager)...)
		key = append(key, number(uint64(lpFee))...)
		key = append(key, word(parameters)...)
	default:
		return "", errors.New("SINGLETON_PROTOCOL_UNSUPPORTED")
	}
	h := sha3.NewLegacyKeccak256()
	_, _ = h.Write(key)
	return "0x" + hex.EncodeToString(h.Sum(nil)), nil
}
func (s SingletonCLState) Validate(protocol, token0, token1 string) error {
	if ValidateEVMIdentity(s.Manager, s.ManagerCodeSHA256) != nil || s.Hooks != SingletonZeroAddress {
		return errors.New("SINGLETON_MANAGER_OR_HOOK")
	}
	id, e := SingletonPoolID(protocol, s.Manager, token0, token1, s.LPFee, s.Core.TickSpacing, s.Parameters)
	if e != nil {
		return e
	}
	if id != s.PoolID {
		return errors.New("SINGLETON_POOL_ID")
	}
	max := uint32(1000)
	if protocol == "pancake-infinity-cl" {
		max = 4000
	}
	if s.ProtocolFee0 > max || s.ProtocolFee1 > max {
		return errors.New("SINGLETON_PROTOCOL_FEE")
	}
	return s.Core.Validate(protocol)
}
