package evm

import (
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"xone/chain-parser-go/internal/domain"
)

func readComposableV6(call func(string, string, int) ([]byte, error), pool, vault string) (*domain.BalancerComposableV6State, error) {
	readAmount := func(target, data string, bits int) (domain.Amount, error) {
		word, err := call(target, data, 1)
		if err != nil {
			return "", err
		}
		value, err := wordUint(word, bits)
		if err != nil {
			return "", err
		}
		return domain.Amount(value.String()), nil
	}
	recovery, err := readAmount(pool, "0xb35056b8", 1) // inRecoveryMode()
	if err != nil {
		return nil, err
	}
	if recovery != "0" {
		return nil, errors.New("COMPOSABLE_V6_RECOVERY_MODE")
	}
	exempt, err := readAmount(pool, "0x77151bee", 1) // isExemptFromYieldProtocolFee()
	if err != nil {
		return nil, err
	}
	joinExit, err := call(pool, "0x3c975d51", 2) // getLastJoinExitData()
	if err != nil {
		return nil, err
	}
	amp, err := wordUint(joinExit[:32], 23)
	if err != nil {
		return nil, err
	}
	invariant, err := wordUint(joinExit[32:], 233)
	if err != nil {
		return nil, err
	}
	collector, err := call(vault, "0xd2946c2b", 1) // getProtocolFeesCollector()
	if err != nil {
		return nil, err
	}
	if _, err := wordUint(collector, 160); err != nil {
		return nil, err
	}
	state := &domain.BalancerComposableV6State{
		LastJoinExitAmp: domain.Amount(amp.String()), LastJoinExitInvariant: domain.Amount(invariant.String()),
		FeeCollector: "0x" + hex.EncodeToString(collector[12:]), YieldExempt: exempt == "1",
	}
	for _, field := range []struct {
		data string
		out  *domain.Amount
	}{
		{"0x18160ddd", &state.TotalSupply},                               // totalSupply()
		{"0x70464016" + fmt.Sprintf("%064x", 0), &state.ProtocolSwapFee}, // getProtocolFeePercentageCache(uint256)
		{"0x70464016" + fmt.Sprintf("%064x", 2), &state.ProtocolYieldFee},
		{"0x70a08231" + strings.Repeat("0", 24) + state.FeeCollector[2:], &state.CollectorBPT}, // balanceOf(address)
	} {
		*field.out, err = readAmount(pool, field.data, 256)
		if err != nil {
			return nil, err
		}
	}
	return state, nil
}

func readComposableV6Cache(call func(string, string, int) ([]byte, error), pool, token string, index uint32, provider, digest string, timestamp uint64, contextIndependent bool) (domain.BalancerComposableV6RateCache, error) {
	var cache domain.BalancerComposableV6RateCache
	data, err := call(pool, "0x7f1260d1"+strings.Repeat("0", 24)+token[2:], 4)
	if err != nil {
		return cache, err
	}
	current, err := wordUint(data[:32], 96)
	if err != nil {
		return cache, err
	}
	old, err := wordUint(data[32:64], 96)
	if err != nil {
		return cache, err
	}
	duration, err := wordUint(data[64:96], 32)
	if err != nil {
		return cache, err
	}
	expires, err := wordUint(data[96:], 32)
	if err != nil {
		return cache, err
	}
	cache = domain.BalancerComposableV6RateCache{TokenIndex: index, Provider: provider, ProviderCodeSHA256: digest,
		Current: domain.Amount(current.String()), Old: domain.Amount(old.String()), Duration: uint32(duration.Uint64()), Expires: uint32(expires.Uint64())}
	if timestamp > uint64(cache.Expires) {
		if !contextIndependent {
			return cache, errors.New("COMPOSABLE_V6_RATE_CACHE_EXPIRED")
		}
		if timestamp > uint64(^uint32(0)-cache.Duration) {
			return cache, errors.New("COMPOSABLE_V6_RATE_CACHE_TIMESTAMP_OVERFLOW")
		}
		word, err := call(provider, "0x679aefce", 1) // getRate()
		if err != nil {
			return cache, err
		}
		rate, err := wordUint(word, 96)
		if err != nil {
			return cache, err
		}
		cache.ObservedRefreshRate = domain.Amount(rate.String())
	}
	return cache, nil
}
