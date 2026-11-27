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
