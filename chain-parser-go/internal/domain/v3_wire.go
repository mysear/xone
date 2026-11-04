package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	pb "xone/common/protoc/golang/dexpb"
)

func V3ToProto(v *V3State) (*pb.V3State, error) {
	if v == nil {
		return nil, nil
	}
	if e := v.Validate(); e != nil {
		return nil, e
	}
	amount := func(a Amount) *pb.Amount { b, _ := a.Bytes(); return &pb.Amount{Uint256Be: b} }
	code, _ := hex.DecodeString(v.CodeSHA256)
	factoryCode, _ := hex.DecodeString(v.FactoryCodeSHA256)
	implCode, _ := hex.DecodeString(v.ImplementationCodeSHA256)
	out := &pb.V3State{Implementation: v.Implementation, ImplementationCodeSha256: implCode, FactoryCodeSha256: factoryCode, FeeProtocol: v.FeeProtocol, SqrtPriceX96: amount(v.SqrtPriceX96), Tick: v.Tick, Liquidity: amount(v.Liquidity), TickSpacing: v.TickSpacing, Factory: v.Factory, CodeSha256: code, MaxSteps: v.MaxSteps}
	for _, w := range v.TickWords {
		out.TickWords = append(out.TickWords, &pb.V3BitmapWord{WordPos: w.WordPos, Bitmap: amount(w.Bitmap)})
	}
	for _, t := range v.Ticks {
		net, _ := ParseLiquidityNet(t.LiquidityNet)
		if net.Sign() < 0 {
			net.Add(net, new(big.Int).Lsh(big.NewInt(1), 128))
		}
		b := net.FillBytes(make([]byte, 16))
		out.Ticks = append(out.Ticks, &pb.V3Tick{Tick: t.Tick, LiquidityGross: amount(t.LiquidityGross), LiquidityNetI128Be: b})
	}
	return out, nil
}
func V3FromProto(v *pb.V3State) (*V3State, error) {
	if v == nil {
		return nil, nil
	}
	if len(v.CodeSha256) != 32 {
		return nil, errors.New("V3_CODE_IDENTITY")
	}
	amount := func(a *pb.Amount) (Amount, error) {
		if a == nil {
			return "", errors.New("V3_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(a.Uint256Be)
	}
	price, e := amount(v.SqrtPriceX96)
	if e != nil {
		return nil, e
	}
	liquidity, e := amount(v.Liquidity)
	if e != nil {
		return nil, e
	}
	if len(v.FactoryCodeSha256) != 0 && len(v.FactoryCodeSha256) != 32 {
		return nil, errors.New("V3_FACTORY_CODE")
	}
	if len(v.ImplementationCodeSha256) != 0 && len(v.ImplementationCodeSha256) != 32 {
		return nil, errors.New("V3_IMPLEMENTATION_IDENTITY")
	}
	out := &V3State{Implementation: v.Implementation, ImplementationCodeSHA256: hex.EncodeToString(v.ImplementationCodeSha256), FactoryCodeSHA256: hex.EncodeToString(v.FactoryCodeSha256), FeeProtocol: v.FeeProtocol, SqrtPriceX96: price, Tick: v.Tick, Liquidity: liquidity, TickSpacing: v.TickSpacing, Factory: v.Factory, CodeSHA256: hex.EncodeToString(v.CodeSha256), MaxSteps: v.MaxSteps, TickWords: []V3BitmapWord{}, Ticks: []V3Tick{}}
	if len(v.TickWords) > 33 || len(v.Ticks) > 4096 {
		return nil, errors.New("V3_STATE_LIMIT")
	}
	for _, w := range v.TickWords {
		if w == nil {
			return nil, errors.New("V3_BITMAP_REQUIRED")
		}
		bitmap, e := amount(w.Bitmap)
		if e != nil {
			return nil, e
		}
		out.TickWords = append(out.TickWords, V3BitmapWord{WordPos: w.WordPos, Bitmap: bitmap})
	}
	for _, t := range v.Ticks {
		if t == nil || len(t.LiquidityNetI128Be) != 16 {
			return nil, errors.New("V3_LIQUIDITY_NET_WIDTH")
		}
		gross, e := amount(t.LiquidityGross)
		if e != nil {
			return nil, e
		}
		net := new(big.Int).SetBytes(t.LiquidityNetI128Be)
		if t.LiquidityNetI128Be[0]&128 != 0 {
			net.Sub(net, new(big.Int).Lsh(big.NewInt(1), 128))
		}
		out.Ticks = append(out.Ticks, V3Tick{Tick: t.Tick, LiquidityGross: gross, LiquidityNet: net.String()})
	}
	return out, out.Validate()
}
