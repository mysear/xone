package domain

import "errors"

// All Bin pools on one manager share currency-scoped app reserves and protocol
// fees. Their snapshots cannot disagree before route execution propagates them.
func ValidateInfinityBinDependencies(pools []Pool) error {
	type identity struct {
		Code, Vault, VaultCode, BlockHash string
		Block, Timestamp                  uint64
	}
	type currency struct {
		Code          string
		Reserve, Fees Amount
	}
	managers := map[string]identity{}
	tokens := map[string]currency{}
	for _, p := range pools {
		b := p.InfinityBin
		if b == nil {
			continue
		}
		id := identity{b.ManagerCodeSHA256, b.Vault, b.VaultCodeSHA256, b.BlockHash, b.BlockNumber, b.Timestamp}
		if old, ok := managers[b.Manager]; ok && old != id {
			return errors.New("INFINITY_BIN_MANAGER_DEPENDENCY_CONFLICT")
		}
		managers[b.Manager] = id
		for _, c := range []struct {
			Token, Code  string
			Reserve, Fee Amount
		}{{p.Token0, b.Token0CodeSHA256, b.AppReserve0, b.ProtocolFees0}, {p.Token1, b.Token1CodeSHA256, b.AppReserve1, b.ProtocolFees1}} {
			key := b.Manager + "/" + c.Token
			v := currency{c.Code, c.Reserve, c.Fee}
			if old, ok := tokens[key]; ok && old != v {
				return errors.New("INFINITY_BIN_CURRENCY_DEPENDENCY_CONFLICT")
			}
			tokens[key] = v
		}
	}
	return nil
}
