package domain

import (
	"errors"
	"math/big"
)

const RayCLMMMinTick int32 = -443636
const RayCLMMMaxTick int32 = 443636

func RayCLMMSqrtPriceAtTick(tick int32) (*big.Int, error) {
	if tick < RayCLMMMinTick || tick > RayCLMMMaxTick {
		return nil, errors.New("RAYCLMM_TICK_RANGE")
	}
	factors := []string{"fffcb933bd6fb800", "fff97272373d4000", "fff2e50f5f657000", "ffe5caca7e10f000", "ffcb9843d60f7000", "ff973b41fa98e800", "ff2ea16466c9b000", "fe5dee046a9a3800", "fcbe86c7900bb000", "f987a7253ac65800", "f3392b0822bb6000", "e7159475a2caf000", "d097f3bdfd2f2000", "a9f746462d9f8000", "70d869a156f31c00", "31be135f97ed3200", "9aa508b5b85a500", "5d6af8dedc582c", "2216e584f5fa"}
	ratio := new(big.Int).Lsh(big.NewInt(1), 64)
	abs := tick
	if abs < 0 {
		abs = -abs
	}
	for bit, n := range factors {
		if abs&(1<<bit) != 0 {
			v, _ := new(big.Int).SetString(n, 16)
			ratio.Rsh(new(big.Int).Mul(ratio, v), 64)
		}
	}
	if tick > 0 {
		ratio.Quo(new(big.Int).Sub(new(big.Int).Lsh(big.NewInt(1), 128), big.NewInt(1)), ratio)
	}
	return ratio, nil
}
