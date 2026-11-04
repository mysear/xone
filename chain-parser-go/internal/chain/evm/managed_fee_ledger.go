package evm

import (
	"context"
	"encoding/hex"
	"errors"
	"strings"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

type managedFeeObservation struct {
	State     *domain.BalancerManagedState
	Delegated bool
}

// Only the existing effective fee cache is used: regular swaps do not refresh
// the global protocol cache or collect AUM fees.
func (a *Adapter) readManagedFeeLedger(ctx context.Context, p config.PoolConfig, anchor map[string]any, block domain.BlockRef) (managedFeeObservation, error) {
	fail := func(code string) (managedFeeObservation, error) { return managedFeeObservation{}, errors.New(code) }
	if p.MultiAsset == nil || block.TimestampMS == 0 || block.TimestampMS%1000 != 0 {
		return fail("MANAGED_FEE_SOURCE_TIME")
	}
	call := func(target, data string) ([]byte, error) { return a.stableCall(ctx, target, data, anchor, 1) }
	amount := func(target, data string) (domain.Amount, error) {
		raw, e := call(target, data)
		if e != nil {
			return "", e
		}
		value, e := wordUint(raw, 256)
		if e != nil {
			return "", e
		}
		return domain.Amount(value.String()), nil
	}
	address := func(target, data string) (string, error) {
		raw, e := call(target, data)
		if e != nil {
			return "", e
		}
		if _, e = wordUint(raw, 160); e != nil {
			return "", e
		}
		return "0x" + hex.EncodeToString(raw[12:]), nil
	}
	recovery, e := amount(p.ID, "0xb35056b8")
	if e != nil {
		return managedFeeObservation{}, e
	}
	if recovery != "0" {
		return fail("MANAGED_FEE_RECOVERY_UNSUPPORTED")
	}
	delegated, e := amount(p.ID, "0x619a6f2b")
	if e != nil {
		return managedFeeObservation{}, e
	}
	if delegated != "0" && delegated != "1" {
		return fail("MANAGED_FEE_DELEGATION_BOOL")
	}
	owner, e := address(p.ID, "0x893d20e8")
	if e != nil {
		return managedFeeObservation{}, e
	}
	collector, e := address(p.ID, "0xd2946c2b")
	if e != nil {
		return managedFeeObservation{}, e
	}
	vaultCollector, e := address(p.MultiAsset.ExpectedVault, "0xd2946c2b")
	if e != nil {
		return managedFeeObservation{}, e
	}
	if collector != vaultCollector {
		return fail("MANAGED_FEE_COLLECTOR_MISMATCH")
	}
	state := &domain.BalancerManagedState{Owner: owner, ProtocolCollector: collector, Timestamp: block.TimestampMS / 1000, BlockNumber: block.Number, BlockHash: block.Hash}
	state.TotalSupply, e = amount(p.ID, "0x18160ddd")
	if e != nil {
		return managedFeeObservation{}, e
	}
	state.ProtocolSwapFee, e = amount(p.ID, "0x77eef903")
	if e != nil {
		return managedFeeObservation{}, e
	}
	state.ManagementSwapFee, e = amount(p.ID, "0x90605f3a")
	if e != nil {
		return managedFeeObservation{}, e
	}
	balance := func(who string) (domain.Amount, error) {
		return amount(p.ID, "0x70a08231"+strings.Repeat("0", 24)+who[2:])
	}
	state.OwnerBalance, e = balance(owner)
	if e != nil {
		return managedFeeObservation{}, e
	}
	state.ProtocolCollectorBalance = state.OwnerBalance
	if collector != owner {
		state.ProtocolCollectorBalance, e = balance(collector)
		if e != nil {
			return managedFeeObservation{}, e
		}
	}
	return managedFeeObservation{State: state, Delegated: delegated == "1"}, nil
}
