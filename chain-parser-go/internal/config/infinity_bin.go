package config

import (
	"strings"
	"xone/chain-parser-go/internal/domain"
)

// InfinityBinConfig pins the manager, vault, pool key, tokens and bounded state reads.
type InfinityBinConfig struct {
	Manager                   string `json:"manager"`
	ExpectedManagerCodeSHA256 string `json:"expected_manager_code_sha256"`
	Vault                     string `json:"vault"`
	ExpectedVaultCodeSHA256   string `json:"expected_vault_code_sha256"`
	PoolID                    string `json:"pool_id"`
	BinStep                   uint32 `json:"bin_step"`
	LPFee                     uint32 `json:"lp_fee"`
	Hooks                     string `json:"hooks,omitempty"`
	HookModel                 string `json:"hook_model,omitempty"`
	ExpectedToken0CodeSHA256  string `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256  string `json:"expected_token1_code_sha256"`
	TokenPolicy               string `json:"token_policy"`
	MaxBins                   uint32 `json:"max_bins"`
	MaxTreeWords              uint32 `json:"max_tree_words"`
	MaxSteps                  uint32 `json:"max_steps"`
}

func (v InfinityBinConfig) SnapshotIdentity() domain.InfinityBinState {
	hooks := v.Hooks
	if hooks == "" {
		hooks = domain.SingletonZeroAddress
	}
	state := domain.InfinityBinState{
		Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, Vault: v.Vault,
		VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Hooks: hooks, HookModel: v.HookModel,
		BinStep: v.BinStep, LPFee: v.LPFee, Token0CodeSHA256: v.ExpectedToken0CodeSHA256,
		Token1CodeSHA256: v.ExpectedToken1CodeSHA256, MaxBins: v.MaxBins,
		MaxTreeWords: v.MaxTreeWords, MaxSteps: v.MaxSteps,
	}
	state.Parameters = state.KeyParameters()
	return state
}

func (p PoolConfig) validateInfinityBin(chain string) bool {
	v := p.InfinityBin
	if v == nil || !strings.HasPrefix(chain, "evm:") || domain.ValidateChain(chain) != nil || p.SingletonAdvanced != nil || p.MultiAsset != nil || p.Weighted != nil || p.RaydiumCLMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCPMM != nil || p.SingletonCL != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.ProgramID != "" || len(p.Accounts) != 0 || v.TokenPolicy != "standard" || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(v.LPFee) || p.ID != v.Manager+"/"+v.PoolID {
		return false
	}
	state := v.SnapshotIdentity()
	return state.ValidateIdentity(p.Token0, p.Token1) == nil
}
