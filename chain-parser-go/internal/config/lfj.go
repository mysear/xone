package config

import (
	"strings"
	"xone/chain-parser-go/internal/domain"
)

type LfjConfig struct {
	ExpectedCodeSHA256               string `json:"expected_code_sha256"`
	ExpectedImplementation           string `json:"expected_implementation"`
	ExpectedImplementationCodeSHA256 string `json:"expected_implementation_code_sha256"`
	ExpectedFactory                  string `json:"expected_factory"`
	ExpectedFactoryCodeSHA256        string `json:"expected_factory_code_sha256"`
	ExpectedToken0CodeSHA256         string `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256         string `json:"expected_token1_code_sha256"`
	BinStep                          uint32 `json:"bin_step"`
	TokenPolicy                      string `json:"token_policy"`
	MaxBins                          uint32 `json:"max_bins"`
	MaxSteps                         uint32 `json:"max_steps"`
}

func (p PoolConfig) validateLfj(chain string) bool {
	v := p.Lfj
	if v == nil || p.Protocol != domain.LfjProtocol || !strings.HasPrefix(chain, "evm:") || domain.ValidateChain(chain) != nil || p.ProgramID != "" || len(p.Accounts) != 0 || p.FeeNumerator != 0 || p.FeeDenominator != 1000000000000000000 || v.TokenPolicy != "standard" || v.BinStep == 0 || v.BinStep > 65535 || v.MaxBins == 0 || v.MaxBins > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 {
		return false
	}
	for _, present := range []bool{p.InfinityBin != nil, p.SlipstreamContext != nil || p.SingletonAdvanced != nil, p.MultiAsset != nil, p.Weighted != nil, p.RaydiumCLMM != nil, p.OrcaWhirlpool != nil, p.RaydiumCPMM != nil, p.SingletonCL != nil, p.Stable != nil, p.V2 != nil, p.V3 != nil} {
		if present {
			return false
		}
	}
	seen := map[string]bool{}
	for _, a := range []string{p.ID, p.Token0, p.Token1, v.ExpectedImplementation, v.ExpectedFactory} {
		if domain.ValidateEVMAddress(a) != nil || a == domain.SingletonZeroAddress || seen[a] {
			return false
		}
		seen[a] = true
	}
	for _, d := range []string{v.ExpectedCodeSHA256, v.ExpectedImplementationCodeSHA256, v.ExpectedFactoryCodeSHA256, v.ExpectedToken0CodeSHA256, v.ExpectedToken1CodeSHA256} {
		if domain.ValidateEVMIdentity(p.ID, d) != nil {
			return false
		}
	}
	return true
}
