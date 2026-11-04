package domain

import (
	"encoding/hex"
	"errors"
	pb "xone/common/protoc/golang/dexpb"
)

func RaydiumCPMMToProto(s *RaydiumCPMMState) (*pb.RaydiumCPMMState, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.validateWire(); e != nil {
		return nil, e
	}
	raw := func(v string) []byte { b, _ := hex.DecodeString(v); return b }
	return &pb.RaydiumCPMMState{
		ProgramId: s.ProgramID, Programdata: s.Programdata, ProgramCodeSha256: raw(s.ProgramCodeSHA256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSha256: raw(s.TokenProgramCodeSHA256), Authority: s.Authority, AuthorityBump: s.AuthorityBump, AmmConfig: s.AmmConfig, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, ProtocolFeeRate: s.ProtocolFeeRate, FundFeeRate: s.FundFeeRate, CreatorFeeRate: s.CreatorFeeRate, CreatorFeeShareRate: s.CreatorFeeShareRate, CreatorFeeOn: s.CreatorFeeOn, CreatorFeeEnabled: s.CreatorFeeEnabled, ProtocolFees0: s.ProtocolFees0, ProtocolFees1: s.ProtocolFees1, FundFees0: s.FundFees0, FundFees1: s.FundFees1, CreatorFees0: s.CreatorFees0, CreatorFees1: s.CreatorFees1, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, OpenTime: s.OpenTime, RecentEpoch: s.RecentEpoch, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals}, nil
}
func RaydiumCPMMFromProto(s *pb.RaydiumCPMMState) (*RaydiumCPMMState, error) {
	if s == nil {
		return nil, nil
	}
	if len(s.ProgramCodeSha256) != 32 || len(s.TokenProgramCodeSha256) != 32 {
		return nil, errors.New("RAYDIUM_DIGEST_WIDTH")
	}
	out := &RaydiumCPMMState{
		ProgramID: s.ProgramId, Programdata: s.Programdata, ProgramCodeSHA256: hex.EncodeToString(s.ProgramCodeSha256), DeployedSlot: s.DeployedSlot, TokenProgramCodeSHA256: hex.EncodeToString(s.TokenProgramCodeSha256), Authority: s.Authority, AuthorityBump: s.AuthorityBump, AmmConfig: s.AmmConfig, Vault0: s.Vault0, Vault1: s.Vault1, RawVault0: s.RawVault0, RawVault1: s.RawVault1, ProtocolFeeRate: s.ProtocolFeeRate, FundFeeRate: s.FundFeeRate, CreatorFeeRate: s.CreatorFeeRate, CreatorFeeShareRate: s.CreatorFeeShareRate, CreatorFeeOn: s.CreatorFeeOn, CreatorFeeEnabled: s.CreatorFeeEnabled, ProtocolFees0: s.ProtocolFees0, ProtocolFees1: s.ProtocolFees1, FundFees0: s.FundFees0, FundFees1: s.FundFees1, CreatorFees0: s.CreatorFees0, CreatorFees1: s.CreatorFees1, BankSlot: s.BankSlot, BankUnixTimestamp: s.BankUnixTimestamp, BankEpoch: s.BankEpoch, OpenTime: s.OpenTime, RecentEpoch: s.RecentEpoch, Mint0Decimals: s.Mint0Decimals, Mint1Decimals: s.Mint1Decimals}
	if e := out.validateWire(); e != nil {
		return nil, e
	}
	return out, nil
}
