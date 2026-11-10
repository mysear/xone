package domain

import (
	"encoding/hex"
	"errors"
	"strings"
)

type ProtocolProfile struct {
	Family          string
	AdapterVersion  string
	FeeNumerator    uint64
	FeeDenominator  uint64
	FeeProtocolBits int
	RequireIdentity bool
}

func Protocol(name string) (ProtocolProfile, bool) {
	switch name {
	case V2CompleteProtocol:
		return ProtocolProfile{Family: "v2", AdapterVersion: V2CompleteAdapterVersion, FeeNumerator: 3, FeeDenominator: 1000, RequireIdentity: true}, true
	case "pancake-infinity-bin":
		return ProtocolProfile{Family: "infinity-bin", AdapterVersion: "pancake-infinity-bin/3c5ef1be-static-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-rewards":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-rewards-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-dynamic":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-dynamic-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-custom-seed":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-custom-seed-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-bitmap":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-bitmap-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-token-fee":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-token-fee-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-clmm-static":
		return ProtocolProfile{Family: "rayclmm", AdapterVersion: "raydium-clmm/6d7a0f82-spl-static-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "orca-whirlpools-adaptive-dynamic":
		return ProtocolProfile{Family: "orca", AdapterVersion: "orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "orca-whirlpools-adaptive-rewards":
		return ProtocolProfile{Family: "orca", AdapterVersion: "orca-whirlpools/cc683ec9-spl-adaptive-rewards-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "orca-whirlpools-adaptive":
		return ProtocolProfile{Family: "orca", AdapterVersion: "orca-whirlpools/cc683ec9-spl-adaptive-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "orca-whirlpools-rewards":
		return ProtocolProfile{Family: "orca", AdapterVersion: "orca-whirlpools/cc683ec9-spl-rewards-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "orca-whirlpools-static":
		return ProtocolProfile{Family: "orca", AdapterVersion: "orca-whirlpools/cc683ec9-spl-static-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "raydium-cpmm-creator":
		return ProtocolProfile{Family: "raydium", AdapterVersion: "raydium-cpmm/f4b9b263-spl-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "uniswap-v4-advanced":
		return ProtocolProfile{Family: "singleton-advanced", AdapterVersion: "uniswap-v4/core-advanced-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "pancake-infinity-cl-advanced":
		return ProtocolProfile{Family: "singleton-advanced", AdapterVersion: "pancake-infinity-cl/cl-advanced-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "uniswap-v4":
		return ProtocolProfile{Family: "singleton", AdapterVersion: "uniswap-v4/core-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "pancake-infinity-cl":
		return ProtocolProfile{Family: "singleton", AdapterVersion: "pancake-infinity-cl/cl-1", FeeDenominator: 1000000, RequireIdentity: true}, true

	case PancakeBscLmProtocol:
		return ProtocolProfile{Family: "pancake-bsc4", AdapterVersion: PancakeBscLmAdapter, FeeDenominator: 1000000, FeeProtocolBits: 32, RequireIdentity: true}, true
	case PancakeLmProtocol:
		return ProtocolProfile{Family: "pancake-lm", AdapterVersion: PancakeLmAdapter, FeeDenominator: 1000000, FeeProtocolBits: 32, RequireIdentity: true}, true
	case SlipstreamContextProtocol:
		return ProtocolProfile{Family: "slipstream-context", AdapterVersion: SlipstreamContextAdapter, FeeDenominator: 1000000, RequireIdentity: true}, true
	case LfjProtocol:
		return ProtocolProfile{Family: "lfj", AdapterVersion: LfjAdapter, FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "aerodrome-slipstream":
		return ProtocolProfile{Family: "v3", AdapterVersion: "aerodrome-slipstream/cl-1", FeeDenominator: 1000000, RequireIdentity: true}, true
	case "velodrome-v2-volatile", "velodrome-v2-stable":
		return ProtocolProfile{Family: "aero", AdapterVersion: name + "/b3065d8b-1", FeeDenominator: 10000, RequireIdentity: true}, true
	case "aerodrome-v2-volatile", "aerodrome-v2-stable":
		return ProtocolProfile{Family: "aero", AdapterVersion: name + "/pool-1", FeeDenominator: 10000, RequireIdentity: true}, true
	case "curve-plain2-basic":
		return ProtocolProfile{Family: "curve", AdapterVersion: "curve-plain2-basic/factory-5", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case ManagedV2BreakerProtocol:
		return ProtocolProfile{Family: "multi", AdapterVersion: ManagedV2BreakerAdapterVersion, FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case ManagedV2BptProtocol:
		return ProtocolProfile{Family: "multi", AdapterVersion: ManagedV2BptAdapterVersion, FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case ManagedV2ScheduledProtocol:
		return ProtocolProfile{Family: "multi", AdapterVersion: ManagedV2ScheduledAdapterVersion, FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case ManagedV2Protocol:
		return ProtocolProfile{Family: "multi", AdapterVersion: ManagedV2AdapterVersion, FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-managed-fees":
		return ProtocolProfile{Family: "multi", AdapterVersion: "balancer-v2-managed-fees/e281ff40-bpt-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-managed-snapshot":
		return ProtocolProfile{Family: "multi", AdapterVersion: "balancer-v2-managed-snapshot/no-bpt-fees-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-composable-regular":
		return ProtocolProfile{Family: "multi", AdapterVersion: "balancer-v2-composable-regular/no-rates-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-weighted-multi":
		return ProtocolProfile{Family: "multi", AdapterVersion: "balancer-v2-weighted-multi/static-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "curve-crypto2-ramp":
		return ProtocolProfile{Family: "multi", AdapterVersion: "curve-crypto2-ramp/d7d04cd9-ramp-1", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case "curve-crypto2-full":
		return ProtocolProfile{Family: "multi", AdapterVersion: "curve-crypto2-full/d7d04cd9-standard-1", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case "curve-metausd-direct":
		return ProtocolProfile{Family: "multi", AdapterVersion: "curve-metausd-direct/5272fd2b-fixed-base-1", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case "curve-stableswap-ng":
		return ProtocolProfile{Family: "multi", AdapterVersion: "curve-stableswap-ng/3332cd44-standard-1", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case "curve-plain3-basic", "curve-plain4-basic":
		return ProtocolProfile{Family: "multi", AdapterVersion: name + "/factory-1", FeeDenominator: 10000000000, RequireIdentity: true}, true
	case "balancer-v2-weighted2-static":
		return ProtocolProfile{Family: "weighted", AdapterVersion: "balancer-v2-weighted2/static-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-weighted2-equal":
		return ProtocolProfile{Family: "weighted", AdapterVersion: "balancer-v2-weighted2/equal-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "balancer-v2-stable2":
		return ProtocolProfile{Family: "balancer", AdapterVersion: "balancer-v2-stable2/stable-1", FeeDenominator: 1000000000000000000, RequireIdentity: true}, true
	case "uniswap-v2":
		return ProtocolProfile{Family: "v2", AdapterVersion: "uniswap-v2/1"}, true
	case "sushiswap-v2":
		return ProtocolProfile{Family: "v2", AdapterVersion: "sushiswap-v2/core-1", FeeNumerator: 3, FeeDenominator: 1000, RequireIdentity: true}, true
	case "pancake-v1":
		return ProtocolProfile{Family: "v2", AdapterVersion: "pancake-v1/core-1", FeeNumerator: 2, FeeDenominator: 1000, RequireIdentity: true}, true
	case "pancake-v2":
		return ProtocolProfile{Family: "v2", AdapterVersion: "pancake-v2/core-1", FeeNumerator: 25, FeeDenominator: 10000, RequireIdentity: true}, true
	case "uniswap-v3":
		return ProtocolProfile{Family: "v3", AdapterVersion: "uniswap-v3/core-1", FeeDenominator: 1000000, FeeProtocolBits: 8, RequireIdentity: true}, true
	case "pancake-v3":
		return ProtocolProfile{Family: "v3", AdapterVersion: "pancake-v3/core-1", FeeDenominator: 1000000, FeeProtocolBits: 32, RequireIdentity: true}, true
	default:
		return ProtocolProfile{}, false
	}
}

type V2State struct {
	Swap       *V2SwapState `json:"swap,omitempty"`
	Factory    string       `json:"factory"`
	CodeSHA256 string       `json:"code_sha256"`
}

func ValidateEVMIdentity(factory, code string) error {
	if !strings.HasPrefix(factory, "0x") || len(factory) != 42 || factory != strings.ToLower(factory) {
		return errors.New("POOL_FACTORY_IDENTITY")
	}
	if _, e := hex.DecodeString(factory[2:]); e != nil {
		return errors.New("POOL_FACTORY_IDENTITY")
	}
	if len(code) != 64 || code != strings.ToLower(code) {
		return errors.New("POOL_CODE_IDENTITY")
	}
	if _, e := hex.DecodeString(code); e != nil {
		return errors.New("POOL_CODE_IDENTITY")
	}
	return nil
}
func (v V2State) Validate() error {
	if e := ValidateEVMIdentity(v.Factory, v.CodeSHA256); e != nil {
		return e
	}
	if v.Swap != nil {
		return v.Swap.Validate()
	}
	return nil
}

func ValidateEVMAddress(address string) error {
	if len(address) != 42 || !strings.HasPrefix(address, "0x") || address != strings.ToLower(address) {
		return errors.New("POOL_ADDRESS_NOT_CANONICAL")
	}
	if _, e := hex.DecodeString(address[2:]); e != nil {
		return errors.New("POOL_ADDRESS_NOT_CANONICAL")
	}
	return nil
}
