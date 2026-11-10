package config

import (
	"encoding/hex"
	"errors"
	"strings"
	"time"
)

// ProviderProbeConfig uses public chain references. Authentication and claims
// about endpoint ownership are deliberately not prerequisites for probing.
// Suggested Chain field: ProviderProbe *ProviderProbeConfig `json:"provider_probe,omitempty"`.
type ProviderProbeConfig struct {
	HistoricalHeight uint64 `json:"historical_height"`
	HistoricalHash   string `json:"historical_hash"`
	Account          string `json:"account"`
	TimeoutMS        uint64 `json:"timeout_ms"`
	IntervalMS       uint64 `json:"interval_ms"`
	MaxBackoffMS     uint64 `json:"max_backoff_ms"`
	MaxAgeMS         uint64 `json:"max_age_ms"`
	MaxProviders     int    `json:"max_providers"`
}

func (p *ProviderProbeConfig) Validate(chain string) error {
	if p == nil {
		return nil
	}
	if !strings.HasPrefix(chain, "evm:") || !catalogHex(p.HistoricalHash, 32, true) || !catalogHex(p.Account, 20, true) || !catalogDuration(p.TimeoutMS) || !catalogDuration(p.IntervalMS) || !catalogDuration(p.MaxBackoffMS) || !catalogDuration(p.MaxAgeMS) || p.MaxBackoffMS < p.IntervalMS || p.MaxAgeMS < p.IntervalMS || p.MaxProviders < 1 || p.MaxProviders > 64 {
		return errors.New("PROVIDER_PROBE_CONFIG_INVALID")
	}
	return nil
}

// FactoryDeployment pins the original factory ABI version to deployed code.
// SHA256 fields use the same byte digest convention as the existing adapters.
// Only the two original Uniswap factory protocols are currently implemented.
type FactoryDeployment struct {
	PoolTemplate        *V3PoolTemplateConfig `json:"pool_template,omitempty"`
	Address             string                `json:"address"`
	Version             string                `json:"version"`
	ProvenanceID        string                `json:"provenance_id"`
	FactoryCodeSHA256   string                `json:"factory_code_sha256"`
	PoolCodeSHA256      string                `json:"pool_code_sha256"`
	Since               uint64                `json:"since"`
	Until               uint64                `json:"until,omitempty"`
	TickWordRadius      uint32                `json:"tick_word_radius,omitempty"`
	MaxInitializedTicks uint32                `json:"max_initialized_ticks,omitempty"`
	MaxSteps            uint32                `json:"max_steps,omitempty"`
}

// Suggested Chain field: DynamicCatalog *DynamicCatalogConfig `json:"dynamic_catalog,omitempty"`.
// Validate both optional fields from Config.Validate's existing per-chain loop.
type DynamicCatalogConfig struct {
	TimeoutMS     uint64              `json:"timeout_ms"`
	MaxCandidates int                 `json:"max_candidates"`
	MaxPools      int                 `json:"max_pools"`
	MaxBytes      int64               `json:"max_bytes"`
	MaxReceipts   int                 `json:"max_receipts"`
	MaxLogs       int                 `json:"max_logs"`
	Sources       []FactoryDeployment `json:"sources"`
}

func (c *DynamicCatalogConfig) Validate(chain string) error {
	if c == nil {
		return nil
	}
	if !strings.HasPrefix(chain, "evm:") || !catalogDuration(c.TimeoutMS) || c.MaxCandidates < 1 || c.MaxCandidates > 100000 || c.MaxPools < 1 || c.MaxPools > 100000 || c.MaxBytes < 1024 || c.MaxBytes > 64<<20 || c.MaxReceipts < 1 || c.MaxReceipts > 100000 || c.MaxLogs < 1 || c.MaxLogs > 1000000 || len(c.Sources) == 0 || len(c.Sources) > 256 {
		return errors.New("DYNAMIC_CATALOG_CONFIG_INVALID")
	}
	seen := map[string]bool{}
	for _, s := range c.Sources {
		if !catalogHex(s.Address, 20, true) || s.Address != strings.ToLower(s.Address) || seen[s.Address] || s.ProvenanceID == "" || len(s.ProvenanceID) > 256 || !catalogHex(s.FactoryCodeSHA256, 32, false) || ValidateFactoryPoolCode(chain, s.Version, s.Address, s.FactoryCodeSHA256, s.PoolCodeSHA256, s.PoolTemplate) != nil || s.Until != 0 && s.Until < s.Since {
			return errors.New("DYNAMIC_CATALOG_SOURCE_INVALID")
		}
		seen[s.Address] = true
		switch s.Version {
		case "uniswap-v2":
			if s.TickWordRadius != 0 || s.MaxInitializedTicks != 0 || s.MaxSteps != 0 {
				return errors.New("DYNAMIC_CATALOG_VERSION_INVALID")
			}
		case "uniswap-v3":
			if s.TickWordRadius > 32 || s.MaxInitializedTicks == 0 || s.MaxInitializedTicks > 65536 || s.MaxSteps == 0 || s.MaxSteps > 100000 {
				return errors.New("DYNAMIC_CATALOG_V3_BUDGET_INVALID")
			}
		default:
			return errors.New("DYNAMIC_CATALOG_VERSION_UNSUPPORTED")
		}
	}
	return nil
}
func catalogHex(s string, n int, prefix bool) bool {
	if prefix {
		if !strings.HasPrefix(s, "0x") {
			return false
		}
		s = s[2:]
	}
	if len(s) != 2*n || s != strings.ToLower(s) || strings.Trim(s, "0") == "" {
		return false
	}
	_, err := hex.DecodeString(s)
	return err == nil
}
func catalogDuration(ms uint64) bool { return ms > 0 && ms <= uint64((24*time.Hour)/time.Millisecond) }
