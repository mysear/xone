package config

import (
	"errors"
	"strings"
	"xone/chain-parser-go/internal/domain"
)

type SlipstreamModuleConfig struct {
	Kind               string `json:"kind"`
	Address            string `json:"address"`
	ExpectedCodeSHA256 string `json:"expected_code_sha256"`
}
type SlipstreamContextConfig struct {
	ExpectedCodeSHA256               string                 `json:"expected_code_sha256"`
	ExpectedImplementation           string                 `json:"expected_implementation"`
	ExpectedImplementationCodeSHA256 string                 `json:"expected_implementation_code_sha256"`
	ExpectedFactory                  string                 `json:"expected_factory"`
	ExpectedFactoryCodeSHA256        string                 `json:"expected_factory_code_sha256"`
	ExpectedVoter                    string                 `json:"expected_voter"`
	ExpectedVoterCodeSHA256          string                 `json:"expected_voter_code_sha256"`
	ExpectedToken0CodeSHA256         string                 `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256         string                 `json:"expected_token1_code_sha256"`
	TokenPolicy                      string                 `json:"token_policy"`
	Origins                          []string               `json:"origins"`
	SwapFeeModule                    SlipstreamModuleConfig `json:"swap_fee_module"`
	UnstakedFeeModule                SlipstreamModuleConfig `json:"unstaked_fee_module"`
	TickWordRadius                   uint32                 `json:"tick_word_radius"`
	MaxInitializedTicks              uint32                 `json:"max_initialized_ticks"`
	MaxSteps                         uint32                 `json:"max_steps"`
	MaxObservations                  uint32                 `json:"max_observations"`
}

func (m SlipstreamModuleConfig) Validate(swap bool) error {
	if m.Kind == "none" {
		if m.Address != domain.SingletonZeroAddress || m.ExpectedCodeSHA256 != "" {
			return errors.New("SLIPSTREAM_ABSENT_MODULE_CONFIG")
		}
		return nil
	}
	if m.Kind != "custom" && (m.Kind != "dynamic" || !swap) {
		return errors.New("SLIPSTREAM_MODULE_KIND_CONFIG")
	}
	if m.Address == domain.SingletonZeroAddress || domain.ValidateEVMIdentity(m.Address, m.ExpectedCodeSHA256) != nil {
		return errors.New("SLIPSTREAM_MODULE_IDENTITY_CONFIG")
	}
	return nil
}
func (p PoolConfig) validateSlipstreamContext(chain string) bool {
	c := p.SlipstreamContext
	if c == nil || p.Protocol != domain.SlipstreamContextProtocol || !strings.HasPrefix(chain, "evm:") || domain.ValidateChain(chain) != nil || c.TokenPolicy != "standard" || p.FeeNumerator != 0 || p.FeeDenominator != 1000000 || p.ProgramID != "" || len(p.Accounts) != 0 {
		return false
	}
	for _, present := range []bool{p.Lfj != nil, p.InfinityBin != nil, p.SingletonAdvanced != nil, p.MultiAsset != nil, p.Weighted != nil, p.RaydiumCLMM != nil, p.OrcaWhirlpool != nil, p.RaydiumCPMM != nil, p.SingletonCL != nil, p.Stable != nil, p.V2 != nil, p.V3 != nil} {
		if present {
			return false
		}
	}
	distinct := map[string]bool{}
	for _, a := range []string{p.ID, p.Token0, p.Token1, c.ExpectedImplementation, c.ExpectedFactory} {
		if domain.ValidateEVMAddress(a) != nil || a == domain.SingletonZeroAddress || distinct[a] {
			return false
		}
		distinct[a] = true
	}
	if p.Token0 >= p.Token1 {
		return false
	}
	if domain.ValidateEVMIdentity(c.ExpectedVoter, c.ExpectedVoterCodeSHA256) != nil || c.ExpectedVoterCodeSHA256 != domain.SlipstreamVoterSHA256 {
		return false
	}
	for _, d := range []string{c.ExpectedCodeSHA256, c.ExpectedImplementationCodeSHA256, c.ExpectedFactoryCodeSHA256, c.ExpectedToken0CodeSHA256, c.ExpectedToken1CodeSHA256} {
		if domain.ValidateEVMIdentity(p.ID, d) != nil {
			return false
		}
	}
	if c.TickWordRadius > 16 || c.MaxInitializedTicks == 0 || c.MaxInitializedTicks > 4096 || c.MaxSteps == 0 || c.MaxSteps > 8192 || c.MaxObservations == 0 || c.MaxObservations > 4096 || len(c.Origins) > 16 {
		return false
	}
	if c.SwapFeeModule.Validate(true) != nil || c.UnstakedFeeModule.Validate(false) != nil {
		return false
	}
	if c.SwapFeeModule.Kind == "dynamic" && len(c.Origins) == 0 {
		return false
	}
	origins := map[string]bool{}
	for _, o := range c.Origins {
		if domain.ValidateEVMAddress(o) != nil || o == domain.SingletonZeroAddress || origins[o] {
			return false
		}
		origins[o] = true
	}
	return true
}
