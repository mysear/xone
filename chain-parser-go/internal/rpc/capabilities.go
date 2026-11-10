package rpc

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"time"
)

type CapabilityStatus string

const (
	Supported    CapabilityStatus = "supported"
	NotSupported CapabilityStatus = "unsupported"
	Unknown      CapabilityStatus = "unknown"
)

type Capability struct {
	Status CapabilityStatus
	Error  ErrorClass
}
type ProbeRequest struct {
	Chain            string
	Genesis          string
	HistoricalHeight uint64
	HistoricalHash   string
	Account          string
}

// CapabilityReport describes exact successful probes at CheckedAt, not a claimed
// contiguous archive range, consensus proof, or provider ownership identity.
// HistoricalBlock checks the fixed header; Historical additionally checks
// eth_getBalance at that exact historical number for the requested account.
// HashPinned is limited to eth_getBalance with EIP-1898; other methods need their
// own probes. A positive read must also pass a negative control using a fresh
// unknown block hash; this detects servers ignoring the selector, not dishonesty.
type CapabilityReport struct {
	HashPinnedMethod      string
	HashPinnedControlHash string
	ProviderID            string
	CheckedAt             time.Time
	HistoricalHeight      uint64
	HistoricalHash        string
	ObservedHead          string
	Safe                  Capability
	Finalized             Capability
	HistoricalBlock       Capability
	Historical            Capability
	HashPinned            Capability
}

func capability(err error) Capability {
	if err == nil {
		return Capability{Status: Supported}
	}
	class := Classify(err)
	status := Unknown
	if class == Unsupported || class == InvalidRequest {
		status = NotSupported
	}
	return Capability{Status: status, Error: class}
}

// ProbeEVM first verifies chain identity and probes each healthy endpoint without
// failover under one total timeout. HistoricalHash must come from the caller's
// fixed reference. No downgrade to number-based state reads is performed.
func (c *Client) ProbeEVM(ctx context.Context, request ProbeRequest) ([]CapabilityReport, error) {
	if request.HistoricalHash == "" || request.Account == "" {
		return nil, errors.New("RPC_PROBE_REFERENCE_REQUIRED")
	}
	ctx, cancel := callContext(ctx, c.HTTP.Timeout)
	defer cancel()
	if err := c.VerifyEVM(ctx, request.Chain, request.Genesis); err != nil {
		return nil, err
	}
	_, providers := c.ProviderClients()
	reports := make([]CapabilityReport, 0, len(providers))
	for _, provider := range providers {
		url := provider.URLs[0]
		report := CapabilityReport{ProviderID: endpointID(url), CheckedAt: time.Now().UTC(), HistoricalHeight: request.HistoricalHeight, HistoricalHash: request.HistoricalHash, HashPinned: Capability{Status: Unknown}}
		var head string
		err := provider.attempt(ctx, url, "eth_blockNumber", []any{}, &head)
		if errors.Is(err, ErrRecordingFailed) {
			return reports, err
		}
		if err == nil {
			report.ObservedHead = head
		}
		for _, tag := range []string{"safe", "finalized"} {
			var block struct{ Number, Hash, ParentHash string }
			err := provider.attempt(ctx, url, "eth_getBlockByNumber", []any{tag, false}, &block)
			if errors.Is(err, ErrRecordingFailed) {
				return reports, err
			}
			if err == nil && (block.Hash == "" || block.ParentHash == "" || block.Number == "") {
				err = errors.New("RPC_RESULT_INVALID")
			}
			if tag == "safe" {
				report.Safe = capability(err)
			} else {
				report.Finalized = capability(err)
			}
		}
		var history struct{ Number, Hash, ParentHash string }
		err = provider.attempt(ctx, url, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", request.HistoricalHeight), false}, &history)
		if errors.Is(err, ErrRecordingFailed) {
			return reports, err
		}
		if err == nil && (history.Number != fmt.Sprintf("0x%x", request.HistoricalHeight) || !SameEVMHash(history.Hash, request.HistoricalHash) || history.ParentHash == "") {
			err = errors.New("RPC_RESULT_INVALID")
		}
		report.HistoricalBlock = capability(err)
		var historicalBalance string
		if err == nil {
			err = provider.attempt(ctx, url, "eth_getBalance", []any{request.Account, fmt.Sprintf("0x%x", request.HistoricalHeight)}, &historicalBalance)
			if errors.Is(err, ErrRecordingFailed) {
				return reports, err
			}
			if err == nil && !hexQuantity(historicalBalance) {
				err = errors.New("RPC_RESULT_INVALID")
			}
		}
		report.Historical = capability(err)
		if err == nil {
			var balance string
			err = provider.attempt(ctx, url, "eth_getBalance", []any{request.Account, map[string]any{"blockHash": history.Hash, "requireCanonical": true}}, &balance)
			if errors.Is(err, ErrRecordingFailed) {
				return reports, err
			}
			if err == nil && (!hexQuantity(balance) || balance != historicalBalance) {
				err = errors.New("RPC_RESULT_INVALID")
			}
			report.HashPinnedMethod = "eth_getBalance"
			if err == nil {
				var control [32]byte
				if _, randomErr := rand.Read(control[:]); randomErr != nil {
					return reports, randomErr
				}
				report.HashPinnedControlHash = "0x" + hex.EncodeToString(control[:])
				var ignored string
				controlErr := provider.attempt(ctx, url, "eth_getBalance", []any{request.Account, map[string]any{"blockHash": report.HashPinnedControlHash, "requireCanonical": true}}, &ignored)
				if errors.Is(controlErr, ErrRecordingFailed) {
					return reports, controlErr
				}
				if controlErr == nil {
					err = errors.New("RPC_PIN_SELECTOR_IGNORED")
				} else if controlErr.Error() != "RPC_ERROR_-32000" && controlErr.Error() != "RPC_ERROR_-32001" {
					err = controlErr
				}
			}
			report.HashPinned = capability(err)
		}
		reports = append(reports, report)
		if ctx.Err() != nil {
			return reports, ctx.Err()
		}
	}
	return reports, nil
}
func hexQuantity(s string) bool {
	if len(s) < 3 || s[:2] != "0x" || (len(s) > 3 && s[2] == '0') {
		return false
	}
	for _, r := range s[2:] {
		if !(r >= '0' && r <= '9' || r >= 'a' && r <= 'f') {
			return false
		}
	}
	return true
}
