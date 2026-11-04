package domain

import (
	"errors"
	"math/big"
	"reflect"
)

type managedVaultKey struct{ chain, vault string }
type managedTokenKey struct{ chain, token string }
type managedWalletKey struct{ chain, vault, token string }
type managedVaultFacts struct {
	code                string
	number, timestamp   uint64
	hash                string
	clock               ManagedV2ExecutionContext
	paused              bool
	pauseEnd, bufferEnd uint64
}
type managedTokenIdentity struct {
	code    string
	scaling Amount
}

func managedV2State(p Pool) *BalancerManagedV2State {
	if p.MultiAsset == nil {
		return nil
	}
	return p.MultiAsset.ManagedV2
}

// ValidateManagedV2PoolDependencies validates declared facts, grouped by chain.
// Snapshot retains authority over canonical source headers and derived contexts.
func ValidateManagedV2PoolDependencies(pools []Pool) error {
	// Validate each Managed declaration before indexing aligned asset/token rows.
	for _, p := range pools {
		if p.Protocol == ManagedV2Protocol || managedV2State(p) != nil {
			if err := p.Validate(); err != nil {
				return err
			}
		}
	}
	type sourceAddress struct {
		chain, address, hash string
		number, timestamp    uint64
	}
	type sourceFee struct {
		sourceAddress
		id Amount
	}
	physical := map[struct{ chain, id string }]Pool{}
	codes, templates := map[sourceAddress]string{}, map[sourceAddress]string{}
	live := map[sourceFee]Amount{}
	for _, p := range pools {
		key := struct{ chain, id string }{p.ChainKey, p.PoolID}
		if previous, ok := physical[key]; ok && !reflect.DeepEqual(previous, p) {
			return errors.New("DUPLICATE_PHYSICAL_POOL_CONFLICT")
		}
		physical[key] = p
		state := managedV2State(p)
		if state == nil {
			continue
		}
		multi := p.MultiAsset
		addressKey := func(address string) sourceAddress {
			return sourceAddress{p.ChainKey, address, state.BlockHash, state.BlockNumber, state.Timestamp}
		}
		observe := func(address, code string) bool {
			key := addressKey(address)
			old, ok := codes[key]
			if ok && old != code {
				return false
			}
			codes[key] = code
			return true
		}
		for _, role := range []struct{ address, code string }{{p.PoolID, multi.CodeSHA256}, {multi.Vault, multi.VaultCodeSHA256}, {state.Factory, state.FactoryCodeSHA256}, {state.FeeProvider, state.FeeProviderCodeSHA256}, {state.ProtocolCollector, state.ProtocolCollectorCodeSHA256}} {
			if !observe(role.address, role.code) {
				if role.address == multi.Vault {
					return errors.New("SHARED_VAULT_IDENTITY")
				}
				return errors.New("SHARED_CONTRACT_IDENTITY")
			}
		}
		for _, asset := range multi.Assets {
			if !observe(asset.Token, asset.CodeSHA256) {
				return errors.New("SHARED_TOKEN_IDENTITY")
			}
		}
		for _, dependency := range state.Dependencies {
			if !observe(dependency.Address, dependency.CodeSHA256) {
				return errors.New("SHARED_CONTRACT_IDENTITY")
			}
			key := addressKey(dependency.Address)
			if old, ok := templates[key]; ok && old != dependency.TemplateSHA256 {
				return errors.New("SHARED_CONTRACT_TEMPLATE")
			}
			templates[key] = dependency.TemplateSHA256
		}
		fee := sourceFee{addressKey(state.FeeProvider), state.ProviderFeeID}
		if old, ok := live[fee]; ok && old != state.LiveProviderAUMPercentage {
			return errors.New("SHARED_PROVIDER_PERCENTAGE")
		}
		live[fee] = state.LiveProviderAUMPercentage
	}
	vaults := map[managedVaultKey]managedVaultFacts{}
	tokens := map[managedTokenKey]managedTokenIdentity{}
	wallets := map[managedWalletKey]Amount{}
	for _, p := range pools {
		m := managedV2State(p)
		if m == nil {
			continue
		}
		s := p.MultiAsset
		key := managedVaultKey{p.ChainKey, s.Vault}
		clock := ManagedV2ExecutionContext{BlockNumber: m.BlockNumber, Timestamp: m.Timestamp}
		if m.ExecutionContext != nil {
			clock = *m.ExecutionContext
		}
		facts := managedVaultFacts{s.VaultCodeSHA256, m.BlockNumber, m.Timestamp, m.BlockHash, clock, m.VaultPaused, m.VaultPauseWindowEnd, m.VaultBufferPeriodEnd}
		if previous, ok := vaults[key]; ok {
			if previous.code != facts.code {
				return errors.New("SHARED_VAULT_IDENTITY")
			}
			if previous.number != facts.number || previous.timestamp != facts.timestamp || previous.hash != facts.hash {
				return errors.New("SHARED_VAULT_SOURCE")
			}
			if previous.clock != facts.clock {
				return errors.New("SHARED_VAULT_CLOCK")
			}
			if previous.paused != facts.paused || previous.pauseEnd != facts.pauseEnd || previous.bufferEnd != facts.bufferEnd {
				return errors.New("SHARED_VAULT_PAUSE")
			}
		} else {
			vaults[key] = facts
		}
		// The own BPT wallet can also appear as another pool's underlying.
		own := managedWalletKey{p.ChainKey, s.Vault, p.PoolID}
		if previous, ok := wallets[own]; ok && previous != m.VaultBPTActualBalance {
			return errors.New("SHARED_VAULT_WALLET")
		}
		wallets[own] = m.VaultBPTActualBalance
		for i, asset := range s.Assets {
			actual := m.TokenInfo[i].VaultActualBalance
			key := managedWalletKey{p.ChainKey, s.Vault, asset.Token}
			if previous, ok := wallets[key]; ok && previous != actual {
				return errors.New("SHARED_VAULT_WALLET")
			}
			wallets[key] = actual
		}
		// Scaling comparison is limited to Managed underlying declarations.
		for _, asset := range s.Assets {
			key := managedTokenKey{p.ChainKey, asset.Token}
			identity := managedTokenIdentity{asset.CodeSHA256, asset.Scaling}
			if previous, ok := tokens[key]; ok && previous != identity {
				return errors.New("SHARED_TOKEN_IDENTITY")
			}
			tokens[key] = identity
		}
	}
	// A Vault wallet backs the sum of CASH in every declared registration,
	// plus possible undeclared registrations/internal balances. Managed amounts
	// are held externally and do not contribute to this necessary lower bound.
	cash := map[managedWalletKey]*big.Int{}
	registrations := map[struct{ chain, vault, pool, token string }]bool{}
	for _, p := range pools {
		state := managedV2State(p)
		if state == nil {
			continue
		}
		multi := p.MultiAsset
		add := func(token string, value Amount) {
			reg := struct{ chain, vault, pool, token string }{p.ChainKey, multi.Vault, p.PoolID, token}
			if registrations[reg] {
				return
			}
			registrations[reg] = true
			key := managedWalletKey{p.ChainKey, multi.Vault, token}
			if cash[key] == nil {
				cash[key] = new(big.Int)
			}
			cash[key].Add(cash[key], value.Big())
		}
		add(p.PoolID, state.VaultBPTCash)
		for i, asset := range multi.Assets {
			add(asset.Token, state.TokenInfo[i].Cash)
		}
	}
	for key, value := range cash {
		if value.BitLen() > 256 || value.Cmp(wallets[key].Big()) > 0 {
			return errors.New("SHARED_VAULT_REGISTERED_CASH")
		}
	}
	for _, p := range pools {
		if s := p.MultiAsset; s != nil {
			if facts, ok := vaults[managedVaultKey{p.ChainKey, s.Vault}]; ok && facts.code != s.VaultCodeSHA256 {
				return errors.New("SHARED_VAULT_IDENTITY")
			}
		}
	}
	return nil
}

// ManagedV2ExecutionCapabilities reports unsupported pool IDs after validating
// facts. Callers retain all siblings in the source bank and filter candidates.
func ManagedV2ExecutionCapabilities(pools []Pool) (map[string]string, error) {
	if err := ValidateManagedV2PoolDependencies(pools); err != nil {
		return nil, err
	}
	groups := map[managedVaultKey][]Pool{}
	for _, p := range pools {
		if s := p.MultiAsset; s != nil && s.Vault != "" {
			key := managedVaultKey{p.ChainKey, s.Vault}
			groups[key] = append(groups[key], p)
		}
	}
	blocked := map[string]string{}
	for _, group := range groups {
		hasManaged, mixed, bpt := false, false, false
		physical := map[string]bool{}
		for _, p := range group {
			physical[p.PoolID] = true
			hasManaged = hasManaged || managedV2State(p) != nil
			mixed = mixed || managedV2State(p) == nil
		}
		if !hasManaged {
			continue
		}
		for _, p := range group {
			if managedV2State(p) == nil {
				continue
			}
			for _, asset := range p.MultiAsset.Assets {
				bpt = bpt || asset.Token != p.PoolID && physical[asset.Token]
			}
		}
		reason := ""
		if bpt {
			reason = "SHARED_VAULT_BPT_UNSUPPORTED"
		} else if mixed {
			reason = "SHARED_VAULT_MIXED_ADAPTER_UNSUPPORTED"
		} else {
			continue
		}
		for _, p := range group {
			blocked[p.PoolID] = reason
		}
	}
	return blocked, nil
}
