package domain

import (
	"encoding/hex"
	"errors"
	"math/big"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

func (s SingletonCLState) validateWire() error {
	if ValidateEVMIdentity(s.Manager, s.ManagerCodeSHA256) != nil || s.Manager == SingletonZeroAddress || s.Hooks != SingletonZeroAddress || s.LPFee >= 1000000 || s.ProtocolFee0 > 4000 || s.ProtocolFee1 > 4000 {
		return errors.New("SINGLETON_WIRE_IDENTITY")
	}
	for i, v := range []string{s.PoolID, s.Parameters} {
		if i == 1 && v == "" {
			continue
		}
		b, e := hex.DecodeString(strings.TrimPrefix(v, "0x"))
		if e != nil || len(b) != 32 || !strings.HasPrefix(v, "0x") || v != strings.ToLower(v) {
			return errors.New("SINGLETON_WIRE_DIGEST")
		}
	}
	return s.Core.Validate("pancake-infinity-cl")
}
func SingletonCLToProto(s *SingletonCLState) (*pb.SingletonCLState, error) {
	return singletonCLToProto(s, false)
}
func singletonCLToProto(s *SingletonCLState, advanced bool) (*pb.SingletonCLState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.validateWire(); !advanced && e != nil {
		return nil, e
	}
	amount := func(a Amount) *pb.Amount { b, _ := a.Bytes(); return &pb.Amount{Uint256Be: b} }
	raw := func(v string) []byte { b, _ := hex.DecodeString(strings.TrimPrefix(v, "0x")); return b }
	c := s.Core
	core := &pb.ConcentratedState{SqrtPriceX96: amount(c.SqrtPriceX96), Tick: c.Tick, Liquidity: amount(c.Liquidity), TickSpacing: c.TickSpacing, MaxSteps: c.MaxSteps}
	for _, w := range c.TickWords {
		core.TickWords = append(core.TickWords, &pb.V3BitmapWord{WordPos: w.WordPos, Bitmap: amount(w.Bitmap)})
	}
	for _, t := range c.Ticks {
		n, _ := ParseLiquidityNet(t.LiquidityNet)
		if n.Sign() < 0 {
			n.Add(n, new(big.Int).Lsh(big.NewInt(1), 128))
		}
		core.Ticks = append(core.Ticks, &pb.V3Tick{Tick: t.Tick, LiquidityGross: amount(t.LiquidityGross), LiquidityNetI128Be: n.FillBytes(make([]byte, 16))})
	}
	return &pb.SingletonCLState{Manager: s.Manager, ManagerCodeSha256: raw(s.ManagerCodeSHA256), PoolId: raw(s.PoolID), LpFee: s.LPFee, ProtocolFee0: s.ProtocolFee0, ProtocolFee1: s.ProtocolFee1, Hooks: s.Hooks, Parameters: raw(s.Parameters), Core: core}, nil
}
func SingletonCLFromProto(s *pb.SingletonCLState) (*SingletonCLState, error) {
	return singletonCLFromProto(s, false)
}
func singletonCLFromProto(s *pb.SingletonCLState, advanced bool) (*SingletonCLState, error) {
	if s == nil {
		return nil, nil
	}
	if s.Core == nil || len(s.ManagerCodeSha256) != 32 || len(s.PoolId) != 32 || (len(s.Parameters) != 0 && len(s.Parameters) != 32) {
		return nil, errors.New("SINGLETON_WIRE_REQUIRED")
	}
	a := func(v *pb.Amount) (Amount, error) {
		if v == nil {
			return "", errors.New("SINGLETON_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(v.Uint256Be)
	}
	c := s.Core
	price, e := a(c.SqrtPriceX96)
	if e != nil {
		return nil, e
	}
	liq, e := a(c.Liquidity)
	if e != nil {
		return nil, e
	}
	out := &SingletonCLState{Manager: s.Manager, ManagerCodeSHA256: hex.EncodeToString(s.ManagerCodeSha256), PoolID: "0x" + hex.EncodeToString(s.PoolId), LPFee: s.LpFee, ProtocolFee0: s.ProtocolFee0, ProtocolFee1: s.ProtocolFee1, Hooks: s.Hooks, Core: ConcentratedState{SqrtPriceX96: price, Tick: c.Tick, Liquidity: liq, TickSpacing: c.TickSpacing, MaxSteps: c.MaxSteps, TickWords: []V3BitmapWord{}, Ticks: []V3Tick{}}}
	if len(s.Parameters) > 0 {
		out.Parameters = "0x" + hex.EncodeToString(s.Parameters)
	}
	for _, w := range c.TickWords {
		if w == nil {
			return nil, errors.New("SINGLETON_BITMAP_REQUIRED")
		}
		b, e := a(w.Bitmap)
		if e != nil {
			return nil, e
		}
		out.Core.TickWords = append(out.Core.TickWords, V3BitmapWord{WordPos: w.WordPos, Bitmap: b})
	}
	for _, t := range c.Ticks {
		if t == nil || len(t.LiquidityNetI128Be) != 16 {
			return nil, errors.New("SINGLETON_NET_WIDTH")
		}
		gross, e := a(t.LiquidityGross)
		if e != nil {
			return nil, e
		}
		net := new(big.Int).SetBytes(t.LiquidityNetI128Be)
		if t.LiquidityNetI128Be[0]&128 != 0 {
			net.Sub(net, new(big.Int).Lsh(big.NewInt(1), 128))
		}
		out.Core.Ticks = append(out.Core.Ticks, V3Tick{Tick: t.Tick, LiquidityGross: gross, LiquidityNet: net.String()})
	}
	if e = out.validateWire(); !advanced && e != nil {
		return nil, e
	}
	return out, nil
}
