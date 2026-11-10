package config

import (
	"strings"
	"xone/chain-parser-go/internal/domain"
)

type PancakeLmConfig struct {
	ExpectedCodeSHA256           string `json:"expected_code_sha256"`
	ExpectedFactory              string `json:"expected_factory"`
	ExpectedFactoryCodeSHA256    string `json:"expected_factory_code_sha256"`
	ExpectedLmPool               string `json:"expected_lm_pool"`
	ExpectedLmPoolCodeSHA256     string `json:"expected_lm_pool_code_sha256"`
	ExpectedMasterChef           string `json:"expected_master_chef"`
	ExpectedMasterChefCodeSHA256 string `json:"expected_master_chef_code_sha256"`
	ExpectedCake                 string `json:"expected_cake"`
	ExpectedWeth                 string `json:"expected_weth"`
	ExpectedPositionManager      string `json:"expected_position_manager"`
	ExpectedToken0CodeSHA256     string `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256     string `json:"expected_token1_code_sha256"`
	TokenPolicy                  string `json:"token_policy"`
	TickWordRadius               uint32 `json:"tick_word_radius"`
	MaxInitializedTicks          uint32 `json:"max_initialized_ticks"`
	MaxSteps                     uint32 `json:"max_steps"`
	MaxObservations              uint32 `json:"max_observations"`
}

func (p PoolConfig) validatePancakeLm(chain string) bool {
	c := p.PancakeLm
	if c == nil || p.Protocol != domain.PancakeLmProtocol || !strings.HasPrefix(chain, "evm:") || domain.ValidateChain(chain) != nil || c.TokenPolicy != "standard" || p.FeeNumerator >= 1000000 || p.FeeDenominator != 1000000 || p.ProgramID != "" || len(p.Accounts) != 0 {
		return false
	}
	for _, present := range []bool{p.SlipstreamContext != nil, p.Lfj != nil, p.InfinityBin != nil, p.SingletonAdvanced != nil, p.MultiAsset != nil, p.Weighted != nil, p.RaydiumCLMM != nil, p.OrcaWhirlpool != nil, p.RaydiumCPMM != nil, p.SingletonCL != nil, p.Stable != nil, p.V2 != nil, p.V3 != nil} {
		if present {
			return false
		}
	}
	distinct := map[string]bool{}
	for _, a := range []string{p.ID, p.Token0, p.Token1, c.ExpectedFactory, c.ExpectedLmPool, c.ExpectedMasterChef} {
		if domain.ValidateEVMAddress(a) != nil || a == domain.SingletonZeroAddress || distinct[a] {
			return false
		}
		distinct[a] = true
	}
	if p.Token0 >= p.Token1 {
		return false
	}
	for _, a := range []string{c.ExpectedCake, c.ExpectedWeth, c.ExpectedPositionManager} {
		if domain.ValidateEVMAddress(a) != nil || a == domain.SingletonZeroAddress {
			return false
		}
	}
	for _, d := range []string{c.ExpectedCodeSHA256, c.ExpectedFactoryCodeSHA256, c.ExpectedLmPoolCodeSHA256, c.ExpectedMasterChefCodeSHA256, c.ExpectedToken0CodeSHA256, c.ExpectedToken1CodeSHA256} {
		if domain.ValidateEVMIdentity(p.ID, d) != nil {
			return false
		}
	}
	return c.TickWordRadius <= 16 && c.MaxInitializedTicks > 0 && c.MaxInitializedTicks <= 4096 && c.MaxSteps > 0 && c.MaxSteps <= 8192 && c.MaxObservations > 0 && c.MaxObservations <= 4096
}
