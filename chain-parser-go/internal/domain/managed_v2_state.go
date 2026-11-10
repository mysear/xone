package domain

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"math/big"
	"reflect"
	"strings"
	pb "xone/common/protoc/golang/dexpb"
)

const ManagedV2Protocol = "balancer-v2-managed-aum"
const ManagedV2AdapterVersion = "balancer-v2/20230411-managed-aum-3"

// Identities are observed runtime SHA-256 and separately verified template SHA-256.
// Format validation here is not proof of original-chain provenance.
type ManagedV2Dependency struct {
	Role           string `json:"role"`
	Address        string `json:"address"`
	CodeSHA256     string `json:"code_sha256"`
	TemplateSHA256 string `json:"template_sha256"`
}

func (s *ManagedV2Dependency) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2Dependency
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2Dependency(v)
	return nil
}

type ManagedV2TokenInfo struct {
	VaultActualBalance    Amount `json:"vault_actual_balance"`
	Cash                  Amount `json:"cash"`
	Managed               Amount `json:"managed"`
	AssetManager          string `json:"asset_manager"`
	LastChangeBlock       uint64 `json:"last_change_block"`
	CircuitBreakerEnabled bool   `json:"circuit_breaker_enabled"`
}

func (s *ManagedV2TokenInfo) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2TokenInfo
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2TokenInfo(v)
	return nil
}

// ManagedV2ExecutionContext is a noncanonical computation clock. The parent
// timestamp, block number and hash continue to identify the source bank.
type ManagedV2ExecutionContext struct {
	BlockNumber uint64 `json:"block_number"`
	Timestamp   uint64 `json:"timestamp"`
}

func (s *ManagedV2ExecutionContext) UnmarshalJSON(raw []byte) error {
	type plain ManagedV2ExecutionContext
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = ManagedV2ExecutionContext(v)
	return nil
}

type BalancerManagedV2State struct {
	CircuitBreakers             *ManagedV2CircuitState `json:"circuit_breakers,omitempty"`
	BPT                         *ManagedV2BptState     `json:"bpt,omitempty"`
	Schedule                    *ManagedV2Schedule     `json:"schedule,omitempty"`
	VaultBPTActualBalance       Amount                 `json:"vault_bpt_actual_balance"`
	Timestamp                   uint64                 `json:"timestamp"`
	BlockNumber                 uint64                 `json:"block_number"`
	BlockHash                   string                 `json:"block_hash"`
	Factory                     string                 `json:"factory"`
	FactoryCodeSHA256           string                 `json:"factory_code_sha256"`
	PoolRuntimeTemplateSHA256   string                 `json:"pool_runtime_template_sha256"`
	FeeProvider                 string                 `json:"fee_provider"`
	FeeProviderCodeSHA256       string                 `json:"fee_provider_code_sha256"`
	ProtocolCollector           string                 `json:"protocol_collector"`
	ProtocolCollectorCodeSHA256 string                 `json:"protocol_collector_code_sha256"`
	Dependencies                []ManagedV2Dependency  `json:"dependencies"`
	TotalSupply                 Amount                 `json:"total_supply"`
	VirtualSupply               Amount                 `json:"virtual_supply"`
	ActualSupply                Amount                 `json:"actual_supply"`
	PendingAUM                  Amount                 `json:"pending_aum"`
	VaultBPTCash                Amount                 `json:"vault_bpt_cash"`
	VaultBPTManaged             Amount                 `json:"vault_bpt_managed"`
	BPTIndex                    uint32                 `json:"bpt_index"`
	Manager                     string                 `json:"manager"`
	ManagerBPTBalance           Amount                 `json:"manager_bpt_balance"`
	ProtocolCollectorBPTBalance Amount                 `json:"protocol_collector_bpt_balance"`
	// Effective getManagementAumFeeParams value, which is zero in recovery mode.
	ManagementAUMFee            Amount                     `json:"management_aum_fee"`
	LastCollectionTimestamp     uint32                     `json:"last_collection_timestamp"`
	ProviderFeeID               Amount                     `json:"provider_fee_id"`
	LiveProviderAUMPercentage   Amount                     `json:"live_provider_aum_percentage"`
	CachedProtocolAUMPercentage Amount                     `json:"cached_protocol_aum_percentage"`
	SwapsEnabled                bool                       `json:"swaps_enabled"`
	Paused                      bool                       `json:"paused"`
	VaultPaused                 bool                       `json:"vault_paused"`
	VaultPauseWindowEnd         uint64                     `json:"vault_pause_window_end"`
	VaultBufferPeriodEnd        uint64                     `json:"vault_buffer_period_end"`
	RecoveryMode                bool                       `json:"recovery_mode"`
	AllowlistEnabled            bool                       `json:"allowlist_enabled"`
	PauseWindowEnd              uint64                     `json:"pause_window_end"`
	BufferPeriodEnd             uint64                     `json:"buffer_period_end"`
	GradualWeightEnd            uint64                     `json:"gradual_weight_end"`
	GradualFeeEnd               uint64                     `json:"gradual_fee_end"`
	TokenInfo                   []ManagedV2TokenInfo       `json:"token_info"`
	ExecutionContext            *ManagedV2ExecutionContext `json:"execution_context,omitempty"`
}

func (s *BalancerManagedV2State) UnmarshalJSON(raw []byte) error {
	type plain BalancerManagedV2State
	var v plain
	if e := decodeManagedV2JSON(raw, &v, true); e != nil {
		return e
	}
	*s = BalancerManagedV2State(v)
	return nil
}

// Reject Go encoding/json's case-insensitive field matching and null defaults.
func decodeManagedV2JSON(raw []byte, out any, required bool) error {
	typ := reflect.TypeOf(out).Elem()
	allowed := map[string]bool{}
	mandatory := map[string]bool{}
	for i := 0; i < typ.NumField(); i++ {
		tag := strings.Split(typ.Field(i).Tag.Get("json"), ",")
		allowed[tag[0]] = true
		mandatory[tag[0]] = required && !strings.Contains(typ.Field(i).Tag.Get("json"), "omitempty")
	}
	dec := json.NewDecoder(bytes.NewReader(raw))
	start, e := dec.Token()
	if e != nil || start != json.Delim('{') {
		return errors.New("MANAGED_V2_JSON_OBJECT")
	}
	seen := map[string]bool{}
	for dec.More() {
		key, e := dec.Token()
		if e != nil {
			return e
		}
		name, ok := key.(string)
		if !ok || !allowed[name] || seen[name] {
			return errors.New("MANAGED_V2_JSON_FIELD")
		}
		seen[name] = true
		var v json.RawMessage
		if e = dec.Decode(&v); e != nil {
			return e
		}
		if (required || name == "managed_v2") && bytes.Equal(bytes.TrimSpace(v), []byte("null")) {
			return errors.New("MANAGED_V2_JSON_NULL")
		}
	}
	if _, e = dec.Token(); e != nil {
		return e
	}
	if _, e = dec.Token(); e != io.EOF {
		return errors.New("MANAGED_V2_JSON_TRAILING")
	}
	for k, need := range mandatory {
		if need && !seen[k] {
			return errors.New("MANAGED_V2_FIELD_REQUIRED")
		}
	}
	return json.Unmarshal(raw, out)
}
func (s *MultiAssetState) UnmarshalJSON(raw []byte) error {
	type plain MultiAssetState
	var v plain
	if e := decodeManagedV2JSON(raw, &v, false); e != nil {
		return e
	}
	*s = MultiAssetState(v)
	return nil
}
func ValidateManagedV2Dependencies(ds []ManagedV2Dependency) error {
	allowed := map[string]bool{"circuit-breaker-lib": true, "managed-pool-add-remove-token-lib": true, "managed-pool-amm-lib": true, "external-weighted-math": true, "recovery-mode-helper": true}
	seen := map[string]bool{}
	if len(ds) != len(allowed) {
		return errors.New("MANAGED_V2_DEPENDENCIES")
	}
	for _, d := range ds {
		if !allowed[d.Role] || seen[d.Role] || ValidateEVMIdentity(d.Address, d.CodeSHA256) != nil || !validDigest(d.TemplateSHA256) {
			return errors.New("MANAGED_V2_DEPENDENCIES")
		}
		seen[d.Role] = true
	}
	return nil
}
func (s BalancerManagedV2State) Validate() error {
	if c := s.ExecutionContext; c != nil {
		if c.BlockNumber < s.BlockNumber || c.Timestamp < s.Timestamp || c.Timestamp < uint64(s.LastCollectionTimestamp) || c.Timestamp > ^uint64(0)/1000 {
			return errors.New("MANAGED_V2_EXECUTION_CONTEXT")
		}
		for _, v := range s.TokenInfo {
			if v.LastChangeBlock > c.BlockNumber {
				return errors.New("MANAGED_V2_EXECUTION_CONTEXT")
			}
		}
	}
	if ValidateEVMIdentity(s.Factory, s.FactoryCodeSHA256) != nil || ValidateEVMIdentity(s.FeeProvider, s.FeeProviderCodeSHA256) != nil || ValidateEVMIdentity(s.ProtocolCollector, s.ProtocolCollectorCodeSHA256) != nil || ValidateEVMAddress(s.Manager) != nil || !validDigest(s.PoolRuntimeTemplateSHA256) || len(s.BlockHash) != 66 || !strings.HasPrefix(s.BlockHash, "0x") || !validDigest(s.BlockHash[2:]) || strings.Trim(s.BlockHash[2:], "0") == "" || s.Timestamp > ^uint64(0)/1000 {
		return errors.New("MANAGED_V2_IDENTITY")
	}
	if e := ValidateManagedV2Dependencies(s.Dependencies); e != nil {
		return e
	}
	for _, a := range []Amount{s.TotalSupply, s.VirtualSupply, s.ActualSupply, s.PendingAUM, s.VaultBPTCash, s.VaultBPTManaged, s.VaultBPTActualBalance, s.ManagerBPTBalance, s.ProtocolCollectorBPTBalance, s.ManagementAUMFee, s.ProviderFeeID, s.LiveProviderAUMPercentage, s.CachedProtocolAUMPercentage} {
		if _, e := ParseAmount(string(a)); e != nil {
			return e
		}
	}

	if s.ManagementAUMFee.Big().Cmp(big.NewInt(950000000000000000)) > 0 || s.LiveProviderAUMPercentage.Big().Cmp(big.NewInt(1000000000000000000)) > 0 || s.CachedProtocolAUMPercentage.Big().Cmp(big.NewInt(1000000000000000000)) > 0 {
		return errors.New("MANAGED_V2_FEE")
	}
	if s.VaultBPTActualBalance.Big().Cmp(s.VaultBPTCash.Big()) < 0 || s.VaultBPTActualBalance.Big().Cmp(s.TotalSupply.Big()) > 0 {
		return errors.New("MANAGED_V2_ACTUAL_BPT_BALANCE")
	}
	if s.BPTIndex != 0 || s.VaultBPTManaged.Big().Sign() != 0 {
		return errors.New("MANAGED_V2_BPT_REGISTRATION")
	}
	if s.ProviderFeeID.Big().BitLen() > 8 {
		return errors.New("MANAGED_V2_PROVIDER_FEE_ID_WIDTH")
	}
	if s.RecoveryMode && s.ManagementAUMFee != "0" {
		return errors.New("MANAGED_V2_RECOVERY_EFFECTIVE_FEE")
	}
	if s.ManagementAUMFee == "0" && s.PendingAUM != "0" {
		return errors.New("MANAGED_V2_ZERO_FEE_PENDING")
	}
	own := new(big.Int).Add(s.VaultBPTCash.Big(), s.VaultBPTManaged.Big())
	if own.BitLen() > 112 || new(big.Int).Add(s.VirtualSupply.Big(), own).Cmp(s.TotalSupply.Big()) != 0 || new(big.Int).Add(s.VirtualSupply.Big(), s.PendingAUM.Big()).Cmp(s.ActualSupply.Big()) != 0 || s.ManagerBPTBalance.Big().Cmp(s.TotalSupply.Big()) > 0 || s.ProtocolCollectorBPTBalance.Big().Cmp(s.TotalSupply.Big()) > 0 {
		return errors.New("MANAGED_V2_SUPPLY")
	}
	if s.Manager == s.ProtocolCollector {
		if s.ManagerBPTBalance != s.ProtocolCollectorBPTBalance {
			return errors.New("MANAGED_V2_RECIPIENT_ALIAS")
		}
	} else if new(big.Int).Add(s.ManagerBPTBalance.Big(), s.ProtocolCollectorBPTBalance.Big()).Cmp(s.TotalSupply.Big()) > 0 {
		return errors.New("MANAGED_V2_LEDGER")
	}
	if s.Dependencies == nil || s.TokenInfo == nil || len(s.TokenInfo) < 2 || len(s.TokenInfo) > 50 || int(s.BPTIndex) > len(s.TokenInfo) || s.PauseWindowEnd > s.BufferPeriodEnd || s.VaultPauseWindowEnd > s.VaultBufferPeriodEnd {
		return errors.New("MANAGED_V2_PARAMETERS")
	}
	// getPausedState returns an effective flag at the source or derived clock.
	pauseTimestamp := s.Timestamp
	if s.ExecutionContext != nil {
		pauseTimestamp = s.ExecutionContext.Timestamp
	}
	if (s.Paused && pauseTimestamp > s.BufferPeriodEnd) || (s.VaultPaused && pauseTimestamp > s.VaultBufferPeriodEnd) {
		return errors.New("MANAGED_V2_EFFECTIVE_PAUSE")
	}
	if s.CircuitBreakers != nil {
		if e := s.CircuitBreakers.Validate(); e != nil {
			return e
		}
		if s.Schedule == nil || s.BPT == nil || len(s.CircuitBreakers.Tokens) != len(s.TokenInfo) {
			return errors.New("MANAGED_V2_CIRCUIT_PARENT_REQUIRED")
		}
	}
	if s.BPT != nil {
		if e := s.BPT.Validate(); e != nil {
			return e
		}
		if s.Schedule == nil {
			return errors.New("MANAGED_V2_BPT_SCHEDULE_REQUIRED")
		}
		view, e := s.Schedule.At(pauseTimestamp)
		if e != nil {
			return e
		}
		block := s.BlockNumber
		if s.ExecutionContext != nil {
			block = s.ExecutionContext.BlockNumber
		}
		if view.JoinExitEnabled != s.BPT.JoinExitEnabled || s.BPT.BPTLastChangeBlock > block || s.BPT.MinimumBPTLocked.Big().Cmp(s.VirtualSupply.Big()) > 0 {
			return errors.New("MANAGED_V2_BPT_SOURCE_FIELDS")
		}
	}
	if s.Schedule == nil {
		if s.GradualWeightEnd > s.Timestamp || s.GradualFeeEnd > s.Timestamp {
			return errors.New("MANAGED_V2_ACTIVE_SCHEDULE_UNSUPPORTED")
		}
	} else {
		view, e := s.Schedule.At(pauseTimestamp)
		if e != nil {
			return e
		}
		if len(s.Schedule.Tokens) != len(s.TokenInfo) || view.WeightEnd != s.GradualWeightEnd || view.FeeEnd != s.GradualFeeEnd || view.SwapsEnabled != s.SwapsEnabled || view.RecoveryMode != s.RecoveryMode || view.MustAllowlistLPs != s.AllowlistEnabled {
			return errors.New("MANAGED_V2_SCHEDULE_SOURCE_FIELDS")
		}
	}
	for i, v := range s.TokenInfo {
		if v.LastChangeBlock > uint64(1)<<32-1 {
			return errors.New("MANAGED_V2_VAULT_BLOCK_WIDTH")
		}
		if ValidateEVMAddress(v.AssetManager) != nil {
			return errors.New("MANAGED_V2_TOKEN_INFO")
		}
		for _, a := range []Amount{v.Cash, v.Managed, v.VaultActualBalance} {
			if _, e := ParseAmount(string(a)); e != nil {
				return e
			}
		}
		if new(big.Int).Add(v.Cash.Big(), v.Managed.Big()).BitLen() > 112 {
			return errors.New("MANAGED_V2_TOKEN_WIDTH")
		}
		if v.Managed.Big().Sign() != 0 && v.AssetManager == "0x0000000000000000000000000000000000000000" {
			return errors.New("MANAGED_V2_TOKEN_ASSET_MANAGER")
		}
		if v.VaultActualBalance.Big().Cmp(v.Cash.Big()) < 0 {
			return errors.New("MANAGED_V2_ACTUAL_TOKEN_BALANCE")
		}
		if s.CircuitBreakers == nil {
			if v.CircuitBreakerEnabled {
				return errors.New("MANAGED_V2_CIRCUIT_BREAKER_UNSUPPORTED")
			}
		} else if v.CircuitBreakerEnabled != (s.CircuitBreakers.Tokens[i].StateWord != "0") {
			return errors.New("MANAGED_V2_CIRCUIT_ENABLED_MISMATCH")
		}
	}
	return nil
}
func (p Pool) validateManagedV2() error {
	s := p.MultiAsset
	if s == nil || s.ManagedV2 == nil {
		return errors.New("MANAGED_V2_STATE_REQUIRED")
	}
	m := s.ManagedV2
	isBreaker := p.Protocol == ManagedV2BreakerProtocol
	if isBreaker != (m.CircuitBreakers != nil) {
		return errors.New("MANAGED_V2_CIRCUIT_PROFILE_MISMATCH")
	}
	isBPT := p.Protocol == ManagedV2BptProtocol || isBreaker
	if isBPT != (m.BPT != nil) {
		return errors.New("MANAGED_V2_BPT_PROFILE_MISMATCH")
	}
	scheduled := p.Protocol == ManagedV2ScheduledProtocol || isBPT
	expectedAdapter := ManagedV2AdapterVersion
	if scheduled {
		expectedAdapter = ManagedV2ScheduledAdapterVersion
		if isBPT {
			expectedAdapter = ManagedV2BptAdapterVersion
		}
		if isBreaker {
			expectedAdapter = ManagedV2BreakerAdapterVersion
		}
	}
	if (p.Protocol != ManagedV2Protocol && !scheduled) || scheduled != (m.Schedule != nil) {
		return errors.New("MANAGED_V2_SCHEDULE_PROFILE")
	}
	if s.ManagedFees != nil || s.CurveCrypto != nil || s.CurveMeta != nil || s.CurveNG != nil || ValidateChain(p.ChainKey) != nil || !strings.HasPrefix(p.ChainKey, "evm:") || ValidateEVMIdentity(p.PoolID, s.CodeSHA256) != nil || ValidateEVMIdentity(s.Vault, s.VaultCodeSHA256) != nil || p.AdapterVersion != expectedAdapter || p.TokenBehavior != "standard" || p.FeeDenominator != 1000000000000000000 || p.FeeNumerator < 1000000000000 || p.FeeNumerator > 950000000000000000 || len(s.Assets) < 2 || len(s.Assets) > 50 || len(s.Weights) != len(s.Assets) || len(m.TokenInfo) != len(s.Assets) || s.Amplification != "0" || s.AdminFee != 0 || len(s.PoolID) != 66 || !strings.HasPrefix(s.PoolID, p.PoolID+"0001") || !validDigest(s.PoolID[2:]) {
		return errors.New("MANAGED_V2_PARAMETERS")
	}
	if e := m.Validate(); e != nil {
		return e
	}
	wallets := map[string]*big.Int{s.Vault: m.VaultBPTActualBalance.Big()}
	for _, role := range []struct {
		address string
		balance Amount
	}{{m.Manager, m.ManagerBPTBalance}, {m.ProtocolCollector, m.ProtocolCollectorBPTBalance}} {
		if previous, ok := wallets[role.address]; ok && previous.Cmp(role.balance.Big()) != 0 {
			return errors.New("MANAGED_V2_VAULT_WALLET_ALIAS")
		}
		wallets[role.address] = role.balance.Big()
	}
	observed := new(big.Int)
	for _, balance := range wallets {
		observed.Add(observed, balance)
	}
	if observed.Cmp(m.TotalSupply.Big()) > 0 {
		return errors.New("MANAGED_V2_WALLET_SUPPLY")
	}
	// Recipient wallets are ERC20 balances. A Vault recipient may also hold
	// BPT registered to other pools; only this pool's registration backs virtual supply.

	if p.Token0 != s.Assets[0].Token || p.Token1 != s.Assets[1].Token || p.Reserve0 != s.Assets[0].Balance || p.Reserve1 != s.Assets[1].Balance {
		return errors.New("MULTI_ASSET_DUPLICATE_FIELDS")
	}
	total := big.NewInt(0)
	for i, a := range s.Assets {
		if ValidateEVMIdentity(a.Token, a.CodeSHA256) != nil || a.Token == p.PoolID || a.Token == s.Vault || i > 0 && s.Assets[i-1].Token >= a.Token || !power10Between(a.Scaling, 18, 36) {
			return errors.New("MANAGED_V2_ASSET")
		}
		if _, e := ParseAmount(string(a.Balance)); e != nil {
			return e
		}
		if a.Balance.Big().Cmp(new(big.Int).Add(m.TokenInfo[i].Cash.Big(), m.TokenInfo[i].Managed.Big())) != 0 {
			return errors.New("MANAGED_V2_BALANCE")
		}
		w, e := ParseAmount(string(s.Weights[i]))
		if e != nil || w.Big().Cmp(big.NewInt(10000000000000000)) < 0 {
			return errors.New("MANAGED_V2_WEIGHT")
		}
		total.Add(total, w.Big())
	}
	if isBreaker {
		if e := m.CircuitBreakers.validatePool(s.Assets, s.Weights); e != nil {
			return e
		}
	}
	if scheduled {
		clock := m.Timestamp
		if m.ExecutionContext != nil {
			clock = m.ExecutionContext.Timestamp
		}
		view, e := m.Schedule.At(clock)
		if e != nil {
			return e
		}
		if len(view.Weights) != len(s.Assets) || view.SwapFee.Big().Uint64() != p.FeeNumerator {
			return errors.New("MANAGED_V2_SCHEDULE_FEE")
		}
		for i, asset := range s.Assets {
			if m.Schedule.Tokens[i].Token != asset.Token || view.Weights[i] != s.Weights[i] || view.Scaling[i] != asset.Scaling {
				return errors.New("MANAGED_V2_SCHEDULE_GETTER_MISMATCH")
			}
		}
	} else if total.Cmp(big.NewInt(1000000000000000000)) != 0 {
		return errors.New("MANAGED_V2_WEIGHT_SUM")
	}
	return nil
}
func (s ManagedV2Dependency) toProto() (*pb.ManagedV2Dependency, error) {
	out := &pb.ManagedV2Dependency{}
	out.Role = s.Role
	out.Address = s.Address
	bCodeSHA256, e := hex.DecodeString(strings.TrimPrefix(s.CodeSHA256, "0x"))
	if e != nil || len(bCodeSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.CodeSha256 = bCodeSHA256
	bTemplateSHA256, e := hex.DecodeString(strings.TrimPrefix(s.TemplateSHA256, "0x"))
	if e != nil || len(bTemplateSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.TemplateSha256 = bTemplateSHA256
	return out, nil
}
func ManagedV2DependencyFromProto(w *pb.ManagedV2Dependency) (*ManagedV2Dependency, error) {
	if w == nil {
		return nil, errors.New("MANAGED_V2_STATE_REQUIRED")
	}
	s := &ManagedV2Dependency{}
	s.Role = w.Role
	s.Address = w.Address
	if len(w.CodeSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.CodeSHA256 = hex.EncodeToString(w.CodeSha256)
	if len(w.TemplateSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.TemplateSHA256 = hex.EncodeToString(w.TemplateSha256)
	return s, nil
}
func (s ManagedV2TokenInfo) toProto() (*pb.ManagedV2TokenInfo, error) {
	out := &pb.ManagedV2TokenInfo{}
	bVaultActualBalance, e := s.VaultActualBalance.Bytes()
	if e != nil {
		return nil, e
	}
	out.VaultActualBalance = &pb.Amount{Uint256Be: bVaultActualBalance}
	bCash, e := s.Cash.Bytes()
	if e != nil {
		return nil, e
	}
	out.Cash = &pb.Amount{Uint256Be: bCash}
	bManaged, e := s.Managed.Bytes()
	if e != nil {
		return nil, e
	}
	out.Managed = &pb.Amount{Uint256Be: bManaged}
	out.AssetManager = s.AssetManager
	vLastChangeBlock := s.LastChangeBlock
	out.LastChangeBlock = &vLastChangeBlock
	vCircuitBreakerEnabled := s.CircuitBreakerEnabled
	out.CircuitBreakerEnabled = &vCircuitBreakerEnabled
	return out, nil
}
func ManagedV2TokenInfoFromProto(w *pb.ManagedV2TokenInfo) (*ManagedV2TokenInfo, error) {
	if w == nil {
		return nil, errors.New("MANAGED_V2_STATE_REQUIRED")
	}
	s := &ManagedV2TokenInfo{}
	if w.VaultActualBalance == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vVaultActualBalance, e := AmountFromBytes(w.VaultActualBalance.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.VaultActualBalance = vVaultActualBalance
	if w.Cash == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vCash, e := AmountFromBytes(w.Cash.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.Cash = vCash
	if w.Managed == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vManaged, e := AmountFromBytes(w.Managed.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.Managed = vManaged
	s.AssetManager = w.AssetManager
	if w.LastChangeBlock == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.LastChangeBlock = *w.LastChangeBlock
	if w.CircuitBreakerEnabled == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.CircuitBreakerEnabled = *w.CircuitBreakerEnabled
	return s, nil
}
func (s BalancerManagedV2State) toProto() (*pb.BalancerManagedV2State, error) {
	if s.ExecutionContext != nil {
		return nil, errors.New("MANAGED_V2_DERIVED_STATE_NOT_CANONICAL")
	}
	out := &pb.BalancerManagedV2State{}
	var circuitErr error
	out.CircuitBreakers, circuitErr = ManagedV2CircuitStateToProto(s.CircuitBreakers)
	if circuitErr != nil {
		return nil, circuitErr
	}
	if s.BPT != nil {
		var e error
		out.Bpt, e = s.BPT.toProto()
		if e != nil {
			return nil, e
		}
	}
	var scheduleErr error
	out.Schedule, scheduleErr = ManagedV2ScheduleToProto(s.Schedule)
	if scheduleErr != nil {
		return nil, scheduleErr
	}
	bVaultBPTActualBalance, e := s.VaultBPTActualBalance.Bytes()
	if e != nil {
		return nil, e
	}
	out.VaultBptActualBalance = &pb.Amount{Uint256Be: bVaultBPTActualBalance}
	vTimestamp := s.Timestamp
	out.Timestamp = &vTimestamp
	vBlockNumber := s.BlockNumber
	out.BlockNumber = &vBlockNumber
	bBlockHash, e := hex.DecodeString(strings.TrimPrefix(s.BlockHash, "0x"))
	if e != nil || len(bBlockHash) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.BlockHash = bBlockHash
	out.Factory = s.Factory
	bFactoryCodeSHA256, e := hex.DecodeString(strings.TrimPrefix(s.FactoryCodeSHA256, "0x"))
	if e != nil || len(bFactoryCodeSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.FactoryCodeSha256 = bFactoryCodeSHA256
	bPoolRuntimeTemplateSHA256, e := hex.DecodeString(strings.TrimPrefix(s.PoolRuntimeTemplateSHA256, "0x"))
	if e != nil || len(bPoolRuntimeTemplateSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.PoolRuntimeTemplateSha256 = bPoolRuntimeTemplateSHA256
	out.FeeProvider = s.FeeProvider
	bFeeProviderCodeSHA256, e := hex.DecodeString(strings.TrimPrefix(s.FeeProviderCodeSHA256, "0x"))
	if e != nil || len(bFeeProviderCodeSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.FeeProviderCodeSha256 = bFeeProviderCodeSHA256
	out.ProtocolCollector = s.ProtocolCollector
	bProtocolCollectorCodeSHA256, e := hex.DecodeString(strings.TrimPrefix(s.ProtocolCollectorCodeSHA256, "0x"))
	if e != nil || len(bProtocolCollectorCodeSHA256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	out.ProtocolCollectorCodeSha256 = bProtocolCollectorCodeSHA256
	for _, v := range s.Dependencies {
		w, e := v.toProto()
		if e != nil {
			return nil, e
		}
		out.Dependencies = append(out.Dependencies, w)
	}
	bTotalSupply, e := s.TotalSupply.Bytes()
	if e != nil {
		return nil, e
	}
	out.TotalSupply = &pb.Amount{Uint256Be: bTotalSupply}
	bVirtualSupply, e := s.VirtualSupply.Bytes()
	if e != nil {
		return nil, e
	}
	out.VirtualSupply = &pb.Amount{Uint256Be: bVirtualSupply}
	bActualSupply, e := s.ActualSupply.Bytes()
	if e != nil {
		return nil, e
	}
	out.ActualSupply = &pb.Amount{Uint256Be: bActualSupply}
	bPendingAUM, e := s.PendingAUM.Bytes()
	if e != nil {
		return nil, e
	}
	out.PendingAum = &pb.Amount{Uint256Be: bPendingAUM}
	bVaultBPTCash, e := s.VaultBPTCash.Bytes()
	if e != nil {
		return nil, e
	}
	out.VaultBptCash = &pb.Amount{Uint256Be: bVaultBPTCash}
	bVaultBPTManaged, e := s.VaultBPTManaged.Bytes()
	if e != nil {
		return nil, e
	}
	out.VaultBptManaged = &pb.Amount{Uint256Be: bVaultBPTManaged}
	vBPTIndex := uint64(s.BPTIndex)
	out.BptIndex = &vBPTIndex
	out.Manager = s.Manager
	bManagerBPTBalance, e := s.ManagerBPTBalance.Bytes()
	if e != nil {
		return nil, e
	}
	out.ManagerBptBalance = &pb.Amount{Uint256Be: bManagerBPTBalance}
	bProtocolCollectorBPTBalance, e := s.ProtocolCollectorBPTBalance.Bytes()
	if e != nil {
		return nil, e
	}
	out.ProtocolCollectorBptBalance = &pb.Amount{Uint256Be: bProtocolCollectorBPTBalance}
	bManagementAUMFee, e := s.ManagementAUMFee.Bytes()
	if e != nil {
		return nil, e
	}
	out.ManagementAumFee = &pb.Amount{Uint256Be: bManagementAUMFee}
	vLastCollectionTimestamp := uint64(s.LastCollectionTimestamp)
	out.LastCollectionTimestamp = &vLastCollectionTimestamp
	bProviderFeeID, e := s.ProviderFeeID.Bytes()
	if e != nil {
		return nil, e
	}
	out.ProviderFeeId = &pb.Amount{Uint256Be: bProviderFeeID}
	bLiveProviderAUMPercentage, e := s.LiveProviderAUMPercentage.Bytes()
	if e != nil {
		return nil, e
	}
	out.LiveProviderAumPercentage = &pb.Amount{Uint256Be: bLiveProviderAUMPercentage}
	bCachedProtocolAUMPercentage, e := s.CachedProtocolAUMPercentage.Bytes()
	if e != nil {
		return nil, e
	}
	out.CachedProtocolAumPercentage = &pb.Amount{Uint256Be: bCachedProtocolAUMPercentage}
	vSwapsEnabled := s.SwapsEnabled
	out.SwapsEnabled = &vSwapsEnabled
	vPaused := s.Paused
	out.Paused = &vPaused
	vVaultPaused := s.VaultPaused
	out.VaultPaused = &vVaultPaused
	vVaultPauseWindowEnd := s.VaultPauseWindowEnd
	out.VaultPauseWindowEnd = &vVaultPauseWindowEnd
	vVaultBufferPeriodEnd := s.VaultBufferPeriodEnd
	out.VaultBufferPeriodEnd = &vVaultBufferPeriodEnd
	vRecoveryMode := s.RecoveryMode
	out.RecoveryMode = &vRecoveryMode
	vAllowlistEnabled := s.AllowlistEnabled
	out.AllowlistEnabled = &vAllowlistEnabled
	vPauseWindowEnd := s.PauseWindowEnd
	out.PauseWindowEnd = &vPauseWindowEnd
	vBufferPeriodEnd := s.BufferPeriodEnd
	out.BufferPeriodEnd = &vBufferPeriodEnd
	vGradualWeightEnd := s.GradualWeightEnd
	out.GradualWeightEnd = &vGradualWeightEnd
	vGradualFeeEnd := s.GradualFeeEnd
	out.GradualFeeEnd = &vGradualFeeEnd
	for _, v := range s.TokenInfo {
		w, e := v.toProto()
		if e != nil {
			return nil, e
		}
		out.TokenInfo = append(out.TokenInfo, w)
	}
	return out, nil
}
func BalancerManagedV2StateFromProto(w *pb.BalancerManagedV2State) (*BalancerManagedV2State, error) {
	if w == nil {
		return nil, errors.New("MANAGED_V2_STATE_REQUIRED")
	}
	s := &BalancerManagedV2State{}
	var circuitErr error
	s.CircuitBreakers, circuitErr = ManagedV2CircuitStateFromProto(w.CircuitBreakers)
	if circuitErr != nil {
		return nil, circuitErr
	}
	var bptErr error
	s.BPT, bptErr = ManagedV2BptStateFromProto(w.Bpt)
	if bptErr != nil {
		return nil, bptErr
	}
	var scheduleErr error
	s.Schedule, scheduleErr = ManagedV2ScheduleFromProto(w.Schedule)
	if scheduleErr != nil {
		return nil, scheduleErr
	}
	if w.VaultBptActualBalance == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vVaultBPTActualBalance, e := AmountFromBytes(w.VaultBptActualBalance.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.VaultBPTActualBalance = vVaultBPTActualBalance
	if w.Timestamp == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.Timestamp = *w.Timestamp
	if w.BlockNumber == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.BlockNumber = *w.BlockNumber
	if len(w.BlockHash) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.BlockHash = "0x" + hex.EncodeToString(w.BlockHash)
	s.Factory = w.Factory
	if len(w.FactoryCodeSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.FactoryCodeSHA256 = hex.EncodeToString(w.FactoryCodeSha256)
	if len(w.PoolRuntimeTemplateSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.PoolRuntimeTemplateSHA256 = hex.EncodeToString(w.PoolRuntimeTemplateSha256)
	s.FeeProvider = w.FeeProvider
	if len(w.FeeProviderCodeSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.FeeProviderCodeSHA256 = hex.EncodeToString(w.FeeProviderCodeSha256)
	s.ProtocolCollector = w.ProtocolCollector
	if len(w.ProtocolCollectorCodeSha256) != 32 {
		return nil, errors.New("MANAGED_V2_HASH_WIDTH")
	}
	s.ProtocolCollectorCodeSHA256 = hex.EncodeToString(w.ProtocolCollectorCodeSha256)
	s.Dependencies = []ManagedV2Dependency{}
	for _, v := range w.Dependencies {
		x, e := ManagedV2DependencyFromProto(v)
		if e != nil {
			return nil, e
		}
		s.Dependencies = append(s.Dependencies, *x)
	}
	if w.TotalSupply == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vTotalSupply, e := AmountFromBytes(w.TotalSupply.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.TotalSupply = vTotalSupply
	if w.VirtualSupply == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vVirtualSupply, e := AmountFromBytes(w.VirtualSupply.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.VirtualSupply = vVirtualSupply
	if w.ActualSupply == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vActualSupply, e := AmountFromBytes(w.ActualSupply.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.ActualSupply = vActualSupply
	if w.PendingAum == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vPendingAUM, e := AmountFromBytes(w.PendingAum.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.PendingAUM = vPendingAUM
	if w.VaultBptCash == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vVaultBPTCash, e := AmountFromBytes(w.VaultBptCash.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.VaultBPTCash = vVaultBPTCash
	if w.VaultBptManaged == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vVaultBPTManaged, e := AmountFromBytes(w.VaultBptManaged.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.VaultBPTManaged = vVaultBPTManaged
	if w.BptIndex == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	if *w.BptIndex > uint64(^uint32(0)) {
		return nil, errors.New("MANAGED_V2_UINT32_WIDTH")
	}
	s.BPTIndex = uint32(*w.BptIndex)
	s.Manager = w.Manager
	if w.ManagerBptBalance == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vManagerBPTBalance, e := AmountFromBytes(w.ManagerBptBalance.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.ManagerBPTBalance = vManagerBPTBalance
	if w.ProtocolCollectorBptBalance == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vProtocolCollectorBPTBalance, e := AmountFromBytes(w.ProtocolCollectorBptBalance.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.ProtocolCollectorBPTBalance = vProtocolCollectorBPTBalance
	if w.ManagementAumFee == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vManagementAUMFee, e := AmountFromBytes(w.ManagementAumFee.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.ManagementAUMFee = vManagementAUMFee
	if w.LastCollectionTimestamp == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	if *w.LastCollectionTimestamp > uint64(^uint32(0)) {
		return nil, errors.New("MANAGED_V2_UINT32_WIDTH")
	}
	s.LastCollectionTimestamp = uint32(*w.LastCollectionTimestamp)
	if w.ProviderFeeId == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vProviderFeeID, e := AmountFromBytes(w.ProviderFeeId.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.ProviderFeeID = vProviderFeeID
	if w.LiveProviderAumPercentage == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vLiveProviderAUMPercentage, e := AmountFromBytes(w.LiveProviderAumPercentage.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.LiveProviderAUMPercentage = vLiveProviderAUMPercentage
	if w.CachedProtocolAumPercentage == nil {
		return nil, errors.New("MANAGED_V2_AMOUNT_REQUIRED")
	}
	vCachedProtocolAUMPercentage, e := AmountFromBytes(w.CachedProtocolAumPercentage.Uint256Be)
	if e != nil {
		return nil, e
	}
	s.CachedProtocolAUMPercentage = vCachedProtocolAUMPercentage
	if w.SwapsEnabled == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.SwapsEnabled = *w.SwapsEnabled
	if w.Paused == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.Paused = *w.Paused
	if w.VaultPaused == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.VaultPaused = *w.VaultPaused
	if w.VaultPauseWindowEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.VaultPauseWindowEnd = *w.VaultPauseWindowEnd
	if w.VaultBufferPeriodEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.VaultBufferPeriodEnd = *w.VaultBufferPeriodEnd
	if w.RecoveryMode == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.RecoveryMode = *w.RecoveryMode
	if w.AllowlistEnabled == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.AllowlistEnabled = *w.AllowlistEnabled
	if w.PauseWindowEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.PauseWindowEnd = *w.PauseWindowEnd
	if w.BufferPeriodEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.BufferPeriodEnd = *w.BufferPeriodEnd
	if w.GradualWeightEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.GradualWeightEnd = *w.GradualWeightEnd
	if w.GradualFeeEnd == nil {
		return nil, errors.New("MANAGED_V2_SCALAR_REQUIRED")
	}
	s.GradualFeeEnd = *w.GradualFeeEnd
	s.TokenInfo = []ManagedV2TokenInfo{}
	for _, v := range w.TokenInfo {
		x, e := ManagedV2TokenInfoFromProto(v)
		if e != nil {
			return nil, e
		}
		s.TokenInfo = append(s.TokenInfo, *x)
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	return s, nil
}
func ManagedV2ToProto(s *BalancerManagedV2State) (*pb.BalancerManagedV2State, error) {
	if s == nil {
		return nil, nil
	}
	if e := s.Validate(); e != nil {
		return nil, e
	}
	return s.toProto()
}
func ManagedV2FromProto(w *pb.BalancerManagedV2State) (*BalancerManagedV2State, error) {
	if w == nil {
		return nil, nil
	}
	return BalancerManagedV2StateFromProto(w)
}
