package domain

import (
	"encoding/hex"
	"errors"
	pb "xone/common/protoc/golang/dexpb"
)

func CurveCryptoToProto(s *CurveCryptoState) (*pb.CurveCryptoState, error) {
	if s == nil {
		return nil, nil
	}
	out := &pb.CurveCryptoState{}
	amount := func(a Amount) (*pb.Amount, error) { b, e := a.Bytes(); return &pb.Amount{Uint256Be: b}, e }
	var e error
	out.Gamma, e = amount(s.Gamma)
	if e != nil {
		return nil, e
	}
	out.OutFee, e = amount(s.OutFee)
	if e != nil {
		return nil, e
	}
	out.FeeGamma, e = amount(s.FeeGamma)
	if e != nil {
		return nil, e
	}
	out.AllowedExtraProfit, e = amount(s.AllowedExtraProfit)
	if e != nil {
		return nil, e
	}
	out.AdjustmentStep, e = amount(s.AdjustmentStep)
	if e != nil {
		return nil, e
	}
	out.Invariant, e = amount(s.Invariant)
	if e != nil {
		return nil, e
	}
	out.PriceScale, e = amount(s.PriceScale)
	if e != nil {
		return nil, e
	}
	out.PriceOracle, e = amount(s.PriceOracle)
	if e != nil {
		return nil, e
	}
	out.LastPrices, e = amount(s.LastPrices)
	if e != nil {
		return nil, e
	}
	out.XcpProfit, e = amount(s.XCPProfit)
	if e != nil {
		return nil, e
	}
	out.XcpProfitA, e = amount(s.XCPProfitA)
	if e != nil {
		return nil, e
	}
	out.VirtualPrice, e = amount(s.VirtualPrice)
	if e != nil {
		return nil, e
	}
	out.TotalSupply, e = amount(s.TotalSupply)
	if e != nil {
		return nil, e
	}
	out.AdminReceiverBalance, e = amount(s.AdminReceiverBalance)
	if e != nil {
		return nil, e
	}
	if s.Ramp != nil {
		out.Ramp = &pb.CurveCryptoRamp{InitialTime: s.Ramp.InitialTime}
		out.Ramp.InitialAmp, e = amount(s.Ramp.InitialAmp)
		if e != nil {
			return nil, e
		}
		out.Ramp.InitialGamma, e = amount(s.Ramp.InitialGamma)
		if e != nil {
			return nil, e
		}
	}
	out.MaHalfTime = s.MAHalfTime
	out.LastPricesTimestamp = s.LastPricesTimestamp
	out.NotAdjusted = s.NotAdjusted
	out.FutureAGammaTime = s.FutureAGammaTime
	out.LpToken = s.LPToken
	out.AdminReceiver = s.AdminReceiver
	out.Timestamp = s.Timestamp
	out.BlockNumber = s.BlockNumber
	out.LpTokenCodeSha256, e = cryptoBytes(s.LPTokenCodeSHA256)
	if e != nil {
		return nil, e
	}
	out.BlockHash, e = cryptoBytes(s.BlockHash)
	if e != nil {
		return nil, e
	}
	for _, a := range s.RawBalances {
		v, e := amount(a)
		if e != nil {
			return nil, e
		}
		out.RawBalances = append(out.RawBalances, v)
	}
	return out, nil
}
func CurveCryptoFromProto(s *pb.CurveCryptoState) (*CurveCryptoState, error) {
	if s == nil {
		return nil, nil
	}
	out := &CurveCryptoState{RawBalances: []Amount{}}
	amount := func(a *pb.Amount) (Amount, error) {
		if a == nil {
			return "", errors.New("CURVE_CRYPTO_AMOUNT_REQUIRED")
		}
		return AmountFromBytes(a.Uint256Be)
	}
	var e error
	out.Gamma, e = amount(s.Gamma)
	if e != nil {
		return nil, e
	}
	out.OutFee, e = amount(s.OutFee)
	if e != nil {
		return nil, e
	}
	out.FeeGamma, e = amount(s.FeeGamma)
	if e != nil {
		return nil, e
	}
	out.AllowedExtraProfit, e = amount(s.AllowedExtraProfit)
	if e != nil {
		return nil, e
	}
	out.AdjustmentStep, e = amount(s.AdjustmentStep)
	if e != nil {
		return nil, e
	}
	out.Invariant, e = amount(s.Invariant)
	if e != nil {
		return nil, e
	}
	out.PriceScale, e = amount(s.PriceScale)
	if e != nil {
		return nil, e
	}
	out.PriceOracle, e = amount(s.PriceOracle)
	if e != nil {
		return nil, e
	}
	out.LastPrices, e = amount(s.LastPrices)
	if e != nil {
		return nil, e
	}
	out.XCPProfit, e = amount(s.XcpProfit)
	if e != nil {
		return nil, e
	}
	out.XCPProfitA, e = amount(s.XcpProfitA)
	if e != nil {
		return nil, e
	}
	out.VirtualPrice, e = amount(s.VirtualPrice)
	if e != nil {
		return nil, e
	}
	out.TotalSupply, e = amount(s.TotalSupply)
	if e != nil {
		return nil, e
	}
	out.AdminReceiverBalance, e = amount(s.AdminReceiverBalance)
	if e != nil {
		return nil, e
	}
	if s.Ramp != nil {
		out.Ramp = &CurveCryptoRamp{InitialTime: s.Ramp.InitialTime}
		out.Ramp.InitialAmp, e = amount(s.Ramp.InitialAmp)
		if e != nil {
			return nil, e
		}
		out.Ramp.InitialGamma, e = amount(s.Ramp.InitialGamma)
		if e != nil {
			return nil, e
		}
	}
	out.MAHalfTime = s.MaHalfTime
	out.LastPricesTimestamp = s.LastPricesTimestamp
	out.NotAdjusted = s.NotAdjusted
	out.FutureAGammaTime = s.FutureAGammaTime
	out.LPToken = s.LpToken
	out.AdminReceiver = s.AdminReceiver
	out.Timestamp = s.Timestamp
	out.BlockNumber = s.BlockNumber
	if len(s.LpTokenCodeSha256) != 32 || len(s.BlockHash) != 32 {
		return nil, errors.New("CURVE_CRYPTO_BYTES_WIDTH")
	}
	out.LPTokenCodeSHA256 = hex.EncodeToString(s.LpTokenCodeSha256)
	out.BlockHash = "0x" + hex.EncodeToString(s.BlockHash)
	for _, a := range s.RawBalances {
		v, e := amount(a)
		if e != nil {
			return nil, e
		}
		out.RawBalances = append(out.RawBalances, v)
	}
	return out, nil
}
