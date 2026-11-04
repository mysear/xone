package domain

import (
	"errors"
	"math/big"
	"strings"
)

// BalancerManagedState tracks every BPT ledger entry modified by a regular swap.
// It is separate from the legacy profile which requires both fee shares to be zero.
type BalancerManagedState struct {
	TotalSupply              Amount `json:"total_supply"`
	ProtocolSwapFee          Amount `json:"protocol_swap_fee"`
	ManagementSwapFee        Amount `json:"management_swap_fee"`
	Owner                    string `json:"owner"`
	ProtocolCollector        string `json:"protocol_collector"`
	OwnerBalance             Amount `json:"owner_balance"`
	ProtocolCollectorBalance Amount `json:"protocol_collector_balance"`
	Timestamp                uint64 `json:"timestamp"`
	BlockNumber              uint64 `json:"block_number"`
	BlockHash                string `json:"block_hash"`
}

func (p Pool) validateManagedFees() error {
	if p.MultiAsset == nil || p.MultiAsset.ManagedFees == nil {
		return errors.New("MANAGED_FEE_STATE_REQUIRED")
	}
	s := p.MultiAsset.ManagedFees
	if ValidateEVMAddress(s.Owner) != nil || ValidateEVMAddress(s.ProtocolCollector) != nil || s.ProtocolCollector == "0x"+strings.Repeat("0", 40) || s.Timestamp == 0 || s.Timestamp > ^uint64(0)/1000 || len(s.BlockHash) != 66 || !strings.HasPrefix(s.BlockHash, "0x") || !validDigest(s.BlockHash[2:]) || strings.Trim(s.BlockHash[2:], "0") == "" {
		return errors.New("MANAGED_FEE_IDENTITY")
	}
	amounts := []Amount{s.TotalSupply, s.ProtocolSwapFee, s.ManagementSwapFee, s.OwnerBalance, s.ProtocolCollectorBalance}
	values := make([]*big.Int, len(amounts))
	for i, a := range amounts {
		v, e := ParseAmount(string(a))
		if e != nil {
			return e
		}
		values[i] = v.Big()
	}
	one := big.NewInt(1000000000000000000)
	if values[0].Sign() == 0 || values[1].Cmp(big.NewInt(500000000000000000)) > 0 || values[2].Cmp(one) > 0 || values[3].Cmp(values[0]) > 0 || values[4].Cmp(values[0]) > 0 {
		return errors.New("MANAGED_FEE_ACCOUNTING")
	}
	if s.Owner == s.ProtocolCollector {
		if values[3].Cmp(values[4]) != 0 {
			return errors.New("MANAGED_FEE_RECIPIENT_ALIAS")
		}
	} else if new(big.Int).Add(values[3], values[4]).Cmp(values[0]) > 0 {
		return errors.New("MANAGED_FEE_ACCOUNTING")
	}
	return nil
}
