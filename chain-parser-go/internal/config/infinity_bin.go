package config

import (
	"encoding/hex"
	"math/big"
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
	ExpectedToken0CodeSHA256  string `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256  string `json:"expected_token1_code_sha256"`
	TokenPolicy               string `json:"token_policy"`
	MaxBins                   uint32 `json:"max_bins"`
	MaxTreeWords              uint32 `json:"max_tree_words"`
	MaxSteps                  uint32 `json:"max_steps"`
}

func (p PoolConfig) validateInfinityBin(chain string) bool {
	v := p.InfinityBin
	if v == nil || !strings.HasPrefix(chain, "evm:") || domain.ValidateChain(chain) != nil || p.SingletonAdvanced != nil || p.MultiAsset != nil || p.Weighted != nil || p.RaydiumCLMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCPMM != nil || p.SingletonCL != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.ProgramID != "" || len(p.Accounts) != 0 || v.TokenPolicy != "standard" || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(v.LPFee) || p.ID != v.Manager+"/"+v.PoolID {
		return false
	}
	state := domain.InfinityBinState{Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, Vault: v.Vault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Hooks: domain.SingletonZeroAddress, Parameters: "0x" + hex.EncodeToString(new(big.Int).SetUint64(uint64(v.BinStep)<<16).FillBytes(make([]byte, 32))), BinStep: v.BinStep, LPFee: v.LPFee, Token0CodeSHA256: v.ExpectedToken0CodeSHA256, Token1CodeSHA256: v.ExpectedToken1CodeSHA256, MaxBins: v.MaxBins, MaxTreeWords: v.MaxTreeWords, MaxSteps: v.MaxSteps}
	return state.ValidateIdentity(p.Token0, p.Token1) == nil
}
