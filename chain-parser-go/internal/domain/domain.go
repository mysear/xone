// Package domain defines the versioned, language-neutral parser contract.
package domain

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"math/big"
	"regexp"
	"sort"
	"strings"
)

const SchemaVersion = 1

var decimalPattern = regexp.MustCompile(`^(0|[1-9][0-9]{0,77})$`)
var chainPattern = regexp.MustCompile(`^(evm:[1-9][0-9]*|solana:[a-zA-Z0-9_-]+)$`)

type Amount string

func ParseAmount(s string) (Amount, error) {
	if !decimalPattern.MatchString(s) {
		return "", errors.New("INVALID_AMOUNT")
	}
	n, ok := new(big.Int).SetString(s, 10)
	if !ok || n.BitLen() > 256 {
		return "", errors.New("AMOUNT_OVERFLOW")
	}
	return Amount(s), nil
}
func (a Amount) String() string { return string(a) }
func (a Amount) Big() *big.Int  { n, _ := new(big.Int).SetString(string(a), 10); return n }
func (a Amount) MarshalJSON() ([]byte, error) {
	if _, e := ParseAmount(string(a)); e != nil {
		return nil, e
	}
	return json.Marshal(string(a))
}
func (a *Amount) UnmarshalJSON(b []byte) error {
	var s string
	if e := json.Unmarshal(b, &s); e != nil {
		return errors.New("AMOUNT_MUST_BE_STRING")
	}
	n, e := ParseAmount(s)
	if e == nil {
		*a = n
	}
	return e
}
func AmountFromBytes(b []byte) (Amount, error) {
	if len(b) != 32 {
		return "", errors.New("UINT256_REQUIRES_32_BE_BYTES")
	}
	return ParseAmount(new(big.Int).SetBytes(b).String())
}
func (a Amount) Bytes() ([]byte, error) {
	n, e := ParseAmount(string(a))
	if e != nil {
		return nil, e
	}
	return n.Big().FillBytes(make([]byte, 32)), nil
}
func Hash(v any) (string, error) {
	raw, e := json.Marshal(v)
	if e != nil {
		return "", e
	}
	var value any
	d := json.NewDecoder(bytes.NewReader(raw))
	d.UseNumber()
	if e = d.Decode(&value); e != nil {
		return "", e
	}
	raw, e = json.Marshal(value)
	if e != nil {
		return "", e
	}
	h := sha256.Sum256(raw)
	return hex.EncodeToString(h[:]), nil
}
func ValidateChain(s string) error {
	if !chainPattern.MatchString(s) {
		return errors.New("INVALID_CHAIN_KEY")
	}
	return nil
}

type BlockRef struct {
	ChainKey    string `json:"chain_key"`
	Number      uint64 `json:"number_or_slot"`
	Hash        string `json:"hash"`
	ParentHash  string `json:"parent_hash"`
	TimestampMS uint64 `json:"timestamp_ms"`
}
type SnapshotRef struct {
	Block                  BlockRef `json:"block"`
	CanonicalEpoch         uint64   `json:"canonical_epoch"`
	JournalSeq             uint64   `json:"journal_seq"`
	SchemaVersion          uint32   `json:"schema_version"`
	AdapterSetHash         string   `json:"adapter_set_hash"`
	OnlineGeneration       uint64   `json:"online_generation"`
	ScopeHash              string   `json:"scope_hash"`
	WriterEpoch            uint64   `json:"writer_epoch"`
	OwnerService           string   `json:"owner_service"`
	OwnerAssignmentVersion uint64   `json:"owner_assignment_version"`
}
type Pool struct {
	PancakeBscLm      *PancakeBscLmState      `json:"pancake_bsc_lm,omitempty"`
	PancakeLm         *PancakeLmState         `json:"pancake_lm,omitempty"`
	SlipstreamContext *SlipstreamState        `json:"slipstream_context,omitempty"`
	Lfj               *LfjState               `json:"lfj,omitempty"`
	InfinityBin       *InfinityBinState       `json:"infinity_bin,omitempty"`
	SingletonAdvanced *SingletonAdvancedState `json:"singleton_advanced,omitempty"`
	MultiAsset        *MultiAssetState        `json:"multi_asset,omitempty"`
	Weighted          *WeightedState          `json:"weighted,omitempty"`
	RaydiumCLMM       *RaydiumCLMMState       `json:"raydium_clmm,omitempty"`
	OrcaWhirlpool     *OrcaWhirlpoolState     `json:"orca_whirlpool,omitempty"`
	RaydiumCPMM       *RaydiumCPMMState       `json:"raydium_cpmm,omitempty"`
	SingletonCL       *SingletonCLState       `json:"singleton_cl,omitempty"`
	Stable            *StableState            `json:"stable,omitempty"`
	V2                *V2State                `json:"v2,omitempty"`
	V3                *V3State                `json:"v3,omitempty"`
	ChainKey          string                  `json:"chain_key"`
	PoolID            string                  `json:"pool_id"`
	Protocol          string                  `json:"protocol"`
	Token0            string                  `json:"token0"`
	Token1            string                  `json:"token1"`
	Reserve0          Amount                  `json:"reserve0"`
	Reserve1          Amount                  `json:"reserve1"`
	FeeNumerator      uint64                  `json:"fee_numerator"`
	FeeDenominator    uint64                  `json:"fee_denominator"`
	AdapterVersion    string                  `json:"adapter_version"`
	TokenBehavior     string                  `json:"token_behavior"`
}

func (p Pool) Validate() error {
	if e := validateCompleteV2Pool(p); e != nil {
		return e
	}
	count := 0
	for _, present := range []bool{p.PancakeBscLm != nil, p.PancakeLm != nil, p.SlipstreamContext != nil, p.Lfj != nil, p.InfinityBin != nil, p.SingletonAdvanced != nil, p.MultiAsset != nil, p.Weighted != nil, p.Stable != nil, p.V2 != nil, p.V3 != nil, p.SingletonCL != nil, p.RaydiumCPMM != nil, p.OrcaWhirlpool != nil, p.RaydiumCLMM != nil} {
		if present {
			count++
		}
	}
	if count > 1 {
		return errors.New("POOL_MODEL_AMBIGUOUS")
	}

	if p.Protocol == PancakeBscLmProtocol {
		if p.PancakeBscLm == nil {
			return errors.New("BSC4_STATE_REQUIRED")
		}
		return p.PancakeBscLm.Validate(p)
	}
	if p.PancakeBscLm != nil {
		return errors.New("BSC4_PROTOCOL_MISMATCH")
	}
	if p.Protocol == PancakeLmProtocol {
		if p.PancakeLm == nil {
			return errors.New("PANCAKE_LM_CONTEXT_REQUIRED")
		}
		return p.PancakeLm.Validate(p)
	}
	if p.PancakeLm != nil {
		return errors.New("PANCAKE_LM_PROTOCOL_MISMATCH")
	}
	if p.Protocol == SlipstreamContextProtocol {
		if p.SlipstreamContext == nil {
			return errors.New("SLIPSTREAM_CONTEXT_REQUIRED")
		}
		return p.SlipstreamContext.Validate(p)
	}
	if p.SlipstreamContext != nil {
		return errors.New("SLIPSTREAM_CONTEXT_PROTOCOL_MISMATCH")
	}
	if p.Protocol == LfjProtocol {
		if p.Lfj == nil {
			return errors.New("LFJ_STATE_REQUIRED")
		}
		return p.Lfj.Validate(p)
	}
	if p.Lfj != nil {
		return errors.New("LFJ_PROTOCOL_MISMATCH")
	}
	if profile, ok := Protocol(p.Protocol); ok && profile.Family == "infinity-bin" {
		return p.validateInfinityBin()
	}
	if p.InfinityBin != nil {
		return errors.New("INFINITY_BIN_PROTOCOL_MISMATCH")
	}
	if profile, ok := Protocol(p.Protocol); ok && profile.Family == "singleton-advanced" {
		return p.validateSingletonAdvanced()
	}
	if p.SingletonAdvanced != nil {
		return errors.New("SINGLETON_ADVANCED_PROTOCOL_MISMATCH")
	}
	if profile, ok := Protocol(p.Protocol); ok && profile.Family == "multi" {
		return p.validateMultiAsset()
	}
	if p.MultiAsset != nil {
		return errors.New("MULTI_ASSET_PROTOCOL_MISMATCH")
	}
	if p.Protocol == "balancer-v2-weighted2-equal" || p.Protocol == "balancer-v2-weighted2-static" {
		return p.validateWeighted()
	}
	if p.Weighted != nil {
		return errors.New("WEIGHTED_PROTOCOL_MISMATCH")
	}
	if p.Protocol == "raydium-clmm-rewards" || p.Protocol == "raydium-clmm-static" || p.Protocol == "raydium-clmm-token-fee" || p.Protocol == "raydium-clmm-bitmap" || p.Protocol == "raydium-clmm-custom-seed" || p.Protocol == "raydium-clmm-dynamic" {
		if p.RaydiumCLMM == nil {
			return errors.New("RAYCLMM_STATE_REQUIRED")
		}
		return p.RaydiumCLMM.Validate(p)
	}
	if p.RaydiumCLMM != nil {
		return errors.New("RAYCLMM_PROTOCOL_MISMATCH")
	}
	if p.Protocol == "orca-whirlpools-static" || p.Protocol == "orca-whirlpools-rewards" || p.Protocol == "orca-whirlpools-adaptive" || p.Protocol == "orca-whirlpools-adaptive-rewards" || p.Protocol == "orca-whirlpools-adaptive-dynamic" {
		if p.OrcaWhirlpool == nil {
			return errors.New("ORCA_STATE_REQUIRED")
		}
		return p.OrcaWhirlpool.Validate(p)
	}
	if p.OrcaWhirlpool != nil {
		return errors.New("ORCA_PROTOCOL_MISMATCH")
	}
	if p.Protocol == "raydium-cpmm-creator" {
		if p.RaydiumCPMM == nil {
			return errors.New("RAYDIUM_STATE_REQUIRED")
		}
		return p.RaydiumCPMM.Validate(p)
	}
	if p.RaydiumCPMM != nil {
		return errors.New("RAYDIUM_PROTOCOL_MISMATCH")
	}
	profile, known := Protocol(p.Protocol)
	if known {
		if profile.Family != "singleton" && (profile.RequireIdentity || p.V2 != nil || strings.HasPrefix(strings.ToLower(p.PoolID), "0x")) {
			if e := ValidateEVMAddress(p.PoolID); e != nil {
				return e
			}
		}
		if !strings.HasPrefix(p.ChainKey, "evm:") {
			return errors.New("PROTOCOL_CHAIN_FAMILY_MISMATCH")
		}
		if p.AdapterVersion != profile.AdapterVersion {
			return errors.New("ADAPTER_VERSION_MISMATCH")
		}
		if profile.Family == "singleton" {
			if p.SingletonCL == nil || p.V3 != nil || p.V2 != nil || p.Stable != nil || p.Reserve0 != "0" || p.Reserve1 != "0" || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(p.SingletonCL.LPFee) {
				return errors.New("SINGLETON_STATE_REQUIRED")
			}
			if e := p.SingletonCL.Validate(p.Protocol, p.Token0, p.Token1); e != nil {
				return e
			}
			if p.PoolID != p.SingletonCL.Manager+"/"+p.SingletonCL.PoolID {
				return errors.New("SINGLETON_PHYSICAL_ID")
			}
		} else if p.SingletonCL != nil {
			return errors.New("SINGLETON_PROTOCOL_MISMATCH")
		} else if profile.Family == "aero" || profile.Family == "curve" || profile.Family == "balancer" {
			if p.Stable == nil || p.V2 != nil || p.V3 != nil || p.FeeDenominator != profile.FeeDenominator {
				return errors.New("STABLE_STATE_REQUIRED")
			}
			if e := p.Stable.Validate(profile.Family); e != nil {
				return e
			}
			if profile.Family == "curve" && (p.FeeNumerator < 4000000 || p.FeeNumerator > 100000000) {
				return errors.New("CURVE_FEE_RANGE")
			}
			if profile.Family == "balancer" && (p.FeeNumerator < 1000000000000 || p.FeeNumerator > 100000000000000000) {
				return errors.New("BALANCER_FEE_RANGE")
			}
			if profile.Family == "aero" && p.FeeNumerator > 300 {
				return errors.New("AERO_FEE_RANGE")
			}
		} else if p.Stable != nil {
			return errors.New("STABLE_PROTOCOL_MISMATCH")
		} else if profile.Family == "v3" {
			if p.V3 == nil || p.V2 != nil || p.Reserve0 != "0" || p.Reserve1 != "0" || p.FeeDenominator != 1000000 {
				return errors.New("V3_STATE_REQUIRED")
			}
			if e := p.V3.Validate(); e != nil {
				return e
			}
			if p.Protocol == "aerodrome-slipstream" && (ValidateEVMIdentity(p.V3.Factory, p.V3.FactoryCodeSHA256) != nil || p.V3.FeeProtocol != 0 || p.FeeNumerator > 100000) {
				return errors.New("SLIPSTREAM_STATE_IDENTITY")
			}
			if profile.FeeProtocolBits == 8 && p.V3.FeeProtocol > 255 {
				return errors.New("V3_FEE_PROTOCOL_WIDTH")
			}
			if p.Protocol == "pancake-v3" && (p.V3.FeeProtocol&65535 > 10000 || p.V3.FeeProtocol>>16 > 10000) {
				return errors.New("V3_PROTOCOL_FEE_RANGE")
			}
		} else {
			if p.V3 != nil {
				return errors.New("V3_PROTOCOL_MISMATCH")
			}
			if profile.RequireIdentity && p.V2 == nil {
				return errors.New("V2_IDENTITY_REQUIRED")
			}
			if p.V2 != nil {
				if e := p.V2.Validate(); e != nil {
					return e
				}
			}
			if profile.FeeDenominator != 0 && (p.FeeNumerator != profile.FeeNumerator || p.FeeDenominator != profile.FeeDenominator) {
				return errors.New("PROTOCOL_FEE_MISMATCH")
			}
		}
	} else if p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil {
		return errors.New("PROTOCOL_STATE_MISMATCH")
	}
	if ValidateChain(p.ChainKey) != nil || p.PoolID == "" || p.Token0 == p.Token1 || p.Token0 == "" || p.Token1 == "" || p.AdapterVersion == "" {
		return errors.New("INVALID_POOL_IDENTITY")
	}
	if _, e := ParseAmount(p.Reserve0.String()); e != nil {
		return e
	}
	if _, e := ParseAmount(p.Reserve1.String()); e != nil {
		return e
	}
	if p.FeeDenominator == 0 || p.FeeNumerator >= p.FeeDenominator {
		return errors.New("INVALID_FEE")
	}
	return nil
}

type Gap struct {
	From   uint64 `json:"from"`
	To     uint64 `json:"to"`
	Reason string `json:"reason"`
}
type Snapshot struct {
	Reference   SnapshotRef `json:"reference"`
	Pools       []Pool      `json:"pools"`
	Source      string      `json:"source"`
	HistoryGaps []Gap       `json:"history_gaps"`
}

func ScopeHash(pools []Pool) (string, error) {
	ids := make([]string, 0, len(pools))
	for _, p := range pools {
		ids = append(ids, p.PoolID)
	}
	sort.Strings(ids)
	return Hash(ids)
}
func (s Snapshot) Validate(owner string, assignment uint64) error {
	r := s.Reference
	if len(s.Pools) == 0 {
		return errors.New("EMPTY_POOL_SCOPE")
	}
	if r.SchemaVersion != SchemaVersion {
		return errors.New("UNKNOWN_SCHEMA")
	}
	if r.OwnerService != owner || r.OwnerAssignmentVersion != assignment {
		return errors.New("OWNER_MISMATCH")
	}
	if r.WriterEpoch == 0 || r.CanonicalEpoch == 0 || r.OnlineGeneration == 0 || r.JournalSeq == 0 || r.Block.Hash == "" || r.AdapterSetHash == "" || ValidateChain(r.Block.ChainKey) != nil {
		return errors.New("INVALID_SNAPSHOT")
	}
	seen := map[string]bool{}
	if e := ValidatePoolDependencies(s.Pools); e != nil {
		return e
	}
	for _, p := range s.Pools {
		if p.V2 != nil && p.V2.Swap != nil {
			v := p.V2.Swap
			if v.BankBlockNumber != r.Block.Number || v.BankBlockHash != r.Block.Hash || v.BankUnixTimestamp > ^uint64(0)/1000 || v.BankUnixTimestamp*1000 != r.Block.TimestampMS {
				return errors.New("V2_SWAP_SNAPSHOT_BANK")
			}
		}
		if s := p.PancakeBscLm; s != nil {
			if s.Ledger.Derived || s.Ledger.BlockHash != r.Block.Hash || s.Ledger.BlockNumber != r.Block.Number || s.Ledger.Timestamp > ^uint64(0)/1000 || s.Ledger.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("BSC4_CANONICAL_BANK")
			}
		}
		if s := p.PancakeLm; s != nil {
			if s.Derived {
				return errors.New("PANCAKE_LM_DERIVED_STATE_NOT_CANONICAL")
			}
			if s.BlockHash != r.Block.Hash || s.BlockNumber != r.Block.Number || s.Timestamp > ^uint64(0)/1000 || s.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("PANCAKE_LM_SNAPSHOT_BANK")
			}
		}
		if s := p.SlipstreamContext; s != nil {
			if s.Derived {
				return errors.New("SLIPSTREAM_DERIVED_STATE_NOT_CANONICAL")
			}
			if s.BlockHash != r.Block.Hash || s.BlockNumber != r.Block.Number || s.Timestamp > ^uint64(0)/1000 || s.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("SLIPSTREAM_CONTEXT_SNAPSHOT_BANK")
			}
		}
		if p.Lfj != nil {
			l := p.Lfj
			if l.BlockHash != r.Block.Hash || l.BlockNumber != r.Block.Number || l.Timestamp > ^uint64(0)/1000 || l.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("LFJ_SNAPSHOT_BANK")
			}
		}
		if b := p.InfinityBin; b != nil {
			if b.BlockNumber != r.Block.Number || b.BlockHash != r.Block.Hash || b.Timestamp > ^uint64(0)/1000 || b.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("INFINITY_BIN_SNAPSHOT_BANK")
			}
		}
		if p.MultiAsset != nil && p.MultiAsset.CurveCrypto != nil {
			c := p.MultiAsset.CurveCrypto
			if c.BlockNumber != r.Block.Number || c.BlockHash != r.Block.Hash || c.Timestamp > ^uint64(0)/1000 || c.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("CURVE_CRYPTO_SNAPSHOT_BANK")
			}
		}
		if p.MultiAsset != nil && p.MultiAsset.CurveMeta != nil {
			m := p.MultiAsset.CurveMeta
			if m.BlockHash != r.Block.Hash || m.BlockNumber != r.Block.Number || m.Timestamp > ^uint64(0)/1000 || m.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("CURVE_META_SNAPSHOT_BANK")
			}
		}
		if p.MultiAsset != nil && p.MultiAsset.ManagedV2 != nil {
			m := p.MultiAsset.ManagedV2
			if m.ExecutionContext != nil {
				return errors.New("MANAGED_V2_DERIVED_STATE_NOT_CANONICAL")
			}
			if uint64(m.LastCollectionTimestamp) > m.Timestamp {
				return errors.New("MANAGED_V2_SNAPSHOT_COLLECTION_CLOCK")
			}
			for _, v := range m.TokenInfo {
				if v.LastChangeBlock > r.Block.Number {
					return errors.New("MANAGED_V2_SNAPSHOT_TOKEN_BANK")
				}
			}
			if m.BlockHash != r.Block.Hash || m.BlockNumber != r.Block.Number || m.Timestamp > ^uint64(0)/1000 || m.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("MANAGED_V2_SNAPSHOT_BANK")
			}
		}
		if p.MultiAsset != nil && p.MultiAsset.ManagedFees != nil {
			m := p.MultiAsset.ManagedFees
			if m.BlockHash != r.Block.Hash || m.BlockNumber != r.Block.Number || m.Timestamp > ^uint64(0)/1000 || m.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("MANAGED_FEE_SNAPSHOT_BANK")
			}
		}
		if p.MultiAsset != nil && p.MultiAsset.CurveNG != nil {
			ng := p.MultiAsset.CurveNG
			if ng.BlockHash != r.Block.Hash || ng.BlockNumber != r.Block.Number || ng.Timestamp > ^uint64(0)/1000 || ng.Timestamp*1000 != r.Block.TimestampMS {
				return errors.New("CURVE_NG_SNAPSHOT_BANK")
			}
		}
		if p.SingletonAdvanced != nil && p.SingletonAdvanced.SnapshotBlockHash != r.Block.Hash {
			return errors.New("SINGLETON_ADVANCED_SNAPSHOT_BANK")
		}
		if p.RaydiumCLMM != nil && p.RaydiumCLMM.BankSlot != r.Block.Number {
			return errors.New("RAYCLMM_SNAPSHOT_BANK")
		}
		if p.OrcaWhirlpool != nil && (p.OrcaWhirlpool.BankSlot != r.Block.Number || p.OrcaWhirlpool.Adaptive != nil && (p.OrcaWhirlpool.BankUnixTimestamp > ^uint64(0)/1000 || p.OrcaWhirlpool.BankUnixTimestamp*1000 != r.Block.TimestampMS)) {
			return errors.New("ORCA_SNAPSHOT_BANK")
		}
		if p.RaydiumCPMM != nil && p.RaydiumCPMM.BankSlot != r.Block.Number {
			return errors.New("RAYDIUM_SNAPSHOT_BANK")
		}
		if e := p.Validate(); e != nil {
			return e
		}
		if p.ChainKey != r.Block.ChainKey || seen[p.PoolID] {
			return errors.New("POOL_SCOPE_MISMATCH")
		}
		seen[p.PoolID] = true
	}
	hash, e := ScopeHash(s.Pools)
	if e != nil {
		return e
	}
	if hash != r.ScopeHash {
		return errors.New("SCOPE_HASH_MISMATCH")
	}
	return nil
}

type Record struct {
	Reference   SnapshotRef `json:"reference"`
	Kind        string      `json:"kind"`
	RecordID    string      `json:"record_id"`
	PayloadHash string      `json:"payload_hash"`
	Pools       []Pool      `json:"pools"`
}

func NewRecord(s Snapshot, kind string) (Record, error) {
	h, e := Hash(s.Pools)
	if e != nil {
		return Record{}, e
	}
	return Record{s.Reference, kind, fmt.Sprintf("%s/%d/%d", s.Reference.Block.ChainKey, s.Reference.OnlineGeneration, s.Reference.JournalSeq), h, s.Pools}, nil
}
