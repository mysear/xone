package service

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	mathrand "math/rand/v2"
	"strconv"
	"strings"
	"sync"
	"sync/atomic"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

var ErrProviderGate = errors.New("PROVIDER_CAPABILITY_NOT_READY")
var ErrProviderBackoff = errors.New("PROVIDER_PROBE_BACKOFF")

type ProviderRuntimeStatus struct {
	ProviderID           string
	Identity             rpc.Capability
	Head                 rpc.Capability
	HTTPHead             ingest.CatalogAnchor
	HeadCheckedAt        time.Time
	CurrentHashPinned    rpc.Capability
	CurrentCodePinned    rpc.Capability
	CurrentCallPinned    rpc.Capability
	CurrentStoragePinned rpc.Capability
	CurrentControlHash   string
	Report               rpc.CapabilityReport
	Observation          rpc.ProviderObservation
	CheckedAt            time.Time
}
type ProviderSnapshot struct {
	Chain        string
	Independence rpc.CapabilityStatus // URL count never proves independent failure domains.
	Providers    []ProviderRuntimeStatus
	NextProbe    time.Time
	LastError    rpc.ErrorClass
}

// ProviderRuntime is one bounded probe actor. Refresh at startup and Run during
// the service lifetime; Snapshot is suitable for normal source status handlers.
// Capability evidence is method scoped: balance controls do not certify eth_call.
type ProviderRuntime struct {
	mu             sync.Mutex
	busy           atomic.Bool
	headBusy       atomic.Bool
	client         *rpc.Client
	chain, genesis string
	cfg            config.ProviderProbeConfig
	next           time.Time
	failures       uint
	snapshot       ProviderSnapshot
	anchorChecks   map[string]providerAnchorCheck
	normal         *runtimeSourceState
	observer       ProviderReadObserver
}

func NewProviderRuntime(client *rpc.Client, chain, genesis string, cfg config.ProviderProbeConfig) (*ProviderRuntime, error) {
	if err := cfg.Validate(chain); err != nil {
		return nil, err
	}
	if client == nil || client.HTTP == nil || genesis == "" || len(client.URLs) == 0 {
		return nil, ErrProviderGate
	}
	if len(client.Providers()) > cfg.MaxProviders {
		return nil, ingest.ErrBudget
	}
	return &ProviderRuntime{client: client, chain: chain, genesis: genesis, cfg: cfg, snapshot: ProviderSnapshot{Chain: chain, Independence: rpc.Unknown}}, nil
}
func (p *ProviderRuntime) Snapshot() ProviderSnapshot {
	p.mu.Lock()
	defer p.mu.Unlock()
	s := p.snapshot
	s.NextProbe = p.next
	s.Providers = append([]ProviderRuntimeStatus(nil), s.Providers...)
	return s
}
func (p *ProviderRuntime) refresh(ctx context.Context) error {
	if p.normal != nil && p.normal.closed.Load() {
		return ingest.ErrClosed
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if !p.busy.CompareAndSwap(false, true) {
		return ingest.ErrFactoryBusy
	}
	defer p.busy.Store(false)
	p.mu.Lock()
	early := time.Now().Before(p.next)
	p.mu.Unlock()
	if early {
		return ErrProviderBackoff
	}
	ctx, cancel := context.WithTimeout(ctx, time.Duration(p.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	ctx = context.WithValue(ctx, runtimeProbeContextKey{}, true)
	historyCtx, historyCancel := context.WithTimeout(ctx, time.Duration(p.cfg.TimeoutMS)*time.Millisecond/2)
	reports, probeErr := p.client.ProbeEVM(historyCtx, rpc.ProbeRequest{Chain: p.chain, Genesis: p.genesis, HistoricalHeight: p.cfg.HistoricalHeight, HistoricalHash: p.cfg.HistoricalHash, Account: p.cfg.Account})
	historyCancel()
	byID := map[string]rpc.CapabilityReport{}
	for _, r := range reports {
		byID[r.ProviderID] = r
	}
	_, clients := p.client.ProviderClients()
	heads := map[string]ProviderRuntimeStatus{}
	for i, client := range clients {
		obs := client.Providers()[0]
		s := ProviderRuntimeStatus{ProviderID: obs.ID, Identity: rpc.Capability{Status: rpc.Unknown}, Head: rpc.Capability{Status: rpc.Unknown}, Report: byID[obs.ID], CheckedAt: time.Now().UTC()}
		// The aggregate historical probe may stop before reaching this endpoint.
		// Confirm identity on this exact client; default quarantine state is not proof.
		deadline, _ := ctx.Deadline()
		endpointCtx, endpointCancel := context.WithTimeout(ctx, time.Until(deadline)/time.Duration(len(clients)-i))
		identityErr := client.VerifyEVM(endpointCtx, p.chain, p.genesis)
		if identityErr != nil {
			s.Identity.Error = rpc.Classify(identityErr)
			if s.Identity.Error == rpc.IdentityMismatch {
				s.Identity.Status = rpc.NotSupported
			}
			heads[obs.ID] = s
			endpointCancel()
			continue
		}
		s.Identity.Status = rpc.Supported
		var b struct{ Number, Hash, ParentHash string }
		err := client.Call(endpointCtx, "eth_getBlockByNumber", []any{"latest", false}, &b)
		h, e := strconv.ParseUint(trimRuntimeHex(b.Number), 16, 64)
		if err == nil && (e != nil || b.Number != fmt.Sprintf("0x%x", h) || !runtimeDigest(b.Hash) || !runtimeDigest(b.ParentHash)) {
			err = errors.New("RPC_RESULT_INVALID")
		}
		if err == nil {
			s.Head.Status = rpc.Supported
			s.HeadCheckedAt = time.Now().UTC()
			s.HTTPHead = ingest.CatalogAnchor{Chain: p.chain, Height: h, Hash: b.Hash}
		} else {
			s.Head.Error = rpc.Classify(err)
		}
		s.CurrentHashPinned = rpc.Capability{Status: rpc.Unknown}
		s.CurrentCodePinned = rpc.Capability{Status: rpc.Unknown}
		s.CurrentCallPinned = rpc.Capability{Status: rpc.Unknown}
		s.CurrentStoragePinned = rpc.Capability{Status: rpc.Unknown}
		if s.Head.Status == rpc.Supported {
			s.CurrentHashPinned, s.CurrentControlHash = probeCurrentBalance(endpointCtx, client, p.cfg.Account, b.Hash)
			if s.CurrentHashPinned.Status == rpc.Supported {
				s.CurrentCodePinned = probeCurrentMethod(endpointCtx, client, "eth_getCode", p.cfg.Account, b.Hash, s.CurrentControlHash)
				s.CurrentCallPinned = probeCurrentMethod(endpointCtx, client, "eth_call", map[string]string{"to": p.cfg.Account, "data": "0x"}, b.Hash, s.CurrentControlHash)
				s.CurrentStoragePinned = probeCurrentMethod(endpointCtx, client, "eth_getStorageAt", p.cfg.Account, b.Hash, s.CurrentControlHash)
			}
		}
		heads[obs.ID] = s
		endpointCancel()
	}
	result := ProviderSnapshot{Chain: p.chain, Independence: rpc.Unknown, LastError: rpc.Classify(probeErr)}
	anyHealthy := false
	for _, obs := range p.client.Providers() {
		s, ok := heads[obs.ID]
		if !ok {
			s = ProviderRuntimeStatus{ProviderID: obs.ID, Identity: rpc.Capability{Status: rpc.Unknown, Error: obs.LastClass}, Head: rpc.Capability{Status: rpc.Unknown}, CheckedAt: time.Now().UTC()}
			if obs.LastClass == rpc.IdentityMismatch {
				s.Identity.Status = rpc.NotSupported
			}
		}
		if s.CurrentHashPinned.Status == "" {
			s.CurrentHashPinned.Status = rpc.Unknown
		}
		if s.CurrentCodePinned.Status == "" {
			s.CurrentCodePinned.Status = rpc.Unknown
		}
		if s.CurrentCallPinned.Status == "" {
			s.CurrentCallPinned.Status = rpc.Unknown
		}
		if s.CurrentStoragePinned.Status == "" {
			s.CurrentStoragePinned.Status = rpc.Unknown
		}
		s.Observation = obs
		if s.Report.ProviderID == "" {
			s.Report = rpc.CapabilityReport{ProviderID: obs.ID, Safe: rpc.Capability{Status: rpc.Unknown}, Finalized: rpc.Capability{Status: rpc.Unknown}, Historical: rpc.Capability{Status: rpc.Unknown}, HistoricalBlock: rpc.Capability{Status: rpc.Unknown}, HashPinned: rpc.Capability{Status: rpc.Unknown}}
		}
		result.Providers = append(result.Providers, s)
		anyHealthy = anyHealthy || providerQualified(s)
	}
	if !anyHealthy && result.LastError == "" {
		result.LastError = rpc.Unavailable
		for _, s := range result.Providers {
			if s.CurrentHashPinned.Error != "" {
				result.LastError = s.CurrentHashPinned.Error
				break
			}
			if s.CurrentCodePinned.Error != "" {
				result.LastError = s.CurrentCodePinned.Error
				break
			}
			if s.CurrentCallPinned.Error != "" {
				result.LastError = s.CurrentCallPinned.Error
				break
			}
			if s.Head.Error != "" {
				result.LastError = s.Head.Error
				break
			}
		}
	}
	p.mu.Lock()
	defer p.mu.Unlock()
	delay := time.Duration(p.cfg.IntervalMS) * time.Millisecond
	if !anyHealthy {
		p.failures++
		for i := uint(1); i < p.failures && delay < time.Duration(p.cfg.MaxBackoffMS)*time.Millisecond; i++ {
			delay *= 2
		}
		maxDelay := time.Duration(p.cfg.MaxBackoffMS) * time.Millisecond
		if delay > maxDelay {
			delay = maxDelay
		}
		delay = delay/2 + time.Duration(mathrand.Int64N(int64(delay-delay/2)+1))
	} else {
		p.failures = 0
	}
	p.next = time.Now().Add(delay)
	p.snapshot = result
	p.anchorChecks = map[string]providerAnchorCheck{}
	return probeErr
}
func providerQualified(s ProviderRuntimeStatus) bool {
	return s.Identity.Status == rpc.Supported && s.Head.Status == rpc.Supported && s.CurrentHashPinned.Status == rpc.Supported && s.CurrentCodePinned.Status == rpc.Supported && s.CurrentCallPinned.Status == rpc.Supported
}

// Gate requires fresh endpoint evidence. Anchor identity is separately verified
// by CatalogRuntime at its exact number; a moving latest head is not a fence.
func (p *ProviderRuntime) Gate(anchor ingest.CatalogAnchor) error {
	if p.normal != nil && p.normal.closed.Load() {
		return ingest.ErrClosed
	}
	if anchor.Chain != p.chain || !runtimeDigest(anchor.Hash) || anchor.Generation == 0 {
		return ErrProviderGate
	}
	for _, s := range p.Snapshot().Providers {
		if providerQualified(s) && time.Since(s.CheckedAt) <= time.Duration(p.cfg.MaxAgeMS)*time.Millisecond && s.HTTPHead.Height >= anchor.Height {
			return nil
		}
	}
	return ErrProviderGate
}

// QualifiedClients never falls back to an endpoint which failed a control.
func (p *ProviderRuntime) QualifiedClients(anchor ingest.CatalogAnchor) []*rpc.Client {
	if p.Gate(anchor) != nil {
		return nil
	}
	eligible := map[string]bool{}
	for _, s := range p.Snapshot().Providers {
		eligible[s.ProviderID] = providerQualified(s) && time.Since(s.CheckedAt) <= time.Duration(p.cfg.MaxAgeMS)*time.Millisecond && s.HTTPHead.Height >= anchor.Height
	}
	_, clients := p.client.ProviderClients()
	var out []*rpc.Client
	for _, c := range clients {
		if eligible[c.Providers()[0].ID] {
			out = append(out, c)
		}
	}
	return out
}
func (p *ProviderRuntime) Run(ctx context.Context) {
	for {
		if ctx.Err() != nil {
			return
		}
		_ = p.Refresh(ctx)
		p.mu.Lock()
		delay := time.Until(p.next)
		p.mu.Unlock()
		if delay < time.Millisecond {
			delay = time.Millisecond
		}
		timer := time.NewTimer(delay)
		select {
		case <-ctx.Done():
			timer.Stop()
			return
		case <-timer.C:
		}
	}
}
func trimRuntimeHex(s string) string {
	if len(s) >= 2 && s[:2] == "0x" {
		return s[2:]
	}
	return ""
}

// Current controls have their own budget after the historical attempt. Old
// archive availability is never a prerequisite for the online capability gate.
func probeCurrentBalance(ctx context.Context, client *rpc.Client, account, hash string) (rpc.Capability, string) {
	unknown := rpc.Capability{Status: rpc.Unknown}
	var value string
	err := client.Call(ctx, "eth_getBalance", []any{account, map[string]any{"blockHash": hash, "requireCanonical": true}}, &value)
	if err != nil {
		unknown.Error = rpc.Classify(err)
		return unknown, ""
	}
	if !runtimeQuantity(value) {
		unknown.Error = rpc.InvalidResponse
		return unknown, ""
	}
	var random [32]byte
	if _, err = rand.Read(random[:]); err != nil {
		return unknown, ""
	}
	control := "0x" + hex.EncodeToString(random[:])
	var negative string
	err = client.Call(ctx, "eth_getBalance", []any{account, map[string]any{"blockHash": control, "requireCanonical": true}}, &negative)
	if err == nil {
		unknown.Error = rpc.InvalidResponse
		return unknown, control
	}
	if err.Error() != "RPC_ERROR_-32000" && err.Error() != "RPC_ERROR_-32001" {
		unknown.Error = rpc.Classify(err)
		if unknown.Error == rpc.Unsupported || unknown.Error == rpc.InvalidRequest {
			unknown.Status = rpc.NotSupported
		}
		return unknown, control
	}
	return rpc.Capability{Status: rpc.Supported}, control
}
func runtimeQuantity(value string) bool {
	if len(value) < 3 || !strings.HasPrefix(value, "0x") || len(value) > 3 && value[2] == '0' {
		return false
	}
	for _, c := range value[2:] {
		if !(c >= '0' && c <= '9' || c >= 'a' && c <= 'f') {
			return false
		}
	}
	return true
}

// providerAnchorCheck is bounded to one item per configured endpoint. Fast
// checks never renew CheckedAt: identity and capability controls still expire.
type providerAnchorCheck struct {
	Anchor    ingest.CatalogAnchor
	CheckedAt time.Time
	RetryAt   time.Time
	Error     error
}

// VerifyAnchor verifies one exact HTTP block on an endpoint with fresh positive
// identity/current selector evidence. It advances head eligibility without any
// chainId, safe/finalized or historical probes, even inside the full-probe
// interval. One total timeout and a nonqueued actor bound work and concurrency.
func (p *ProviderRuntime) verifyAnchor(ctx context.Context, a ingest.CatalogAnchor) error {
	if p.normal != nil && p.normal.closed.Load() {
		return ingest.ErrClosed
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if a.Chain != p.chain || a.Generation == 0 || !runtimeDigest(a.Hash) {
		return ErrProviderGate
	}
	if !p.headBusy.CompareAndSwap(false, true) {
		return ingest.ErrFactoryBusy
	}
	defer p.headBusy.Store(false)
	ctx, cancel := context.WithTimeout(ctx, time.Duration(p.cfg.TimeoutMS)*time.Millisecond)
	defer cancel()
	snapshot := p.Snapshot()
	eligible := map[string]ProviderRuntimeStatus{}
	for _, s := range snapshot.Providers {
		if s.Identity.Status == rpc.Supported && s.CurrentHashPinned.Status == rpc.Supported && s.CurrentCodePinned.Status == rpc.Supported && s.CurrentCallPinned.Status == rpc.Supported && time.Since(s.CheckedAt) <= time.Duration(p.cfg.MaxAgeMS)*time.Millisecond {
			eligible[s.ProviderID] = s
		}
	}
	_, clients := p.client.ProviderClients()
	last := error(ErrProviderGate)
	for _, client := range clients {
		obs := client.Providers()[0]
		if _, ok := eligible[obs.ID]; !ok {
			continue
		}
		p.mu.Lock()
		cached, exists := p.anchorChecks[obs.ID]
		p.mu.Unlock()
		if exists && cached.Error == nil && cached.Anchor.Chain == a.Chain && cached.Anchor.Height == a.Height && rpc.SameEVMHash(cached.Anchor.Hash, a.Hash) && time.Since(cached.CheckedAt) < time.Second {
			return p.Gate(a)
		}
		if exists && time.Now().Before(cached.RetryAt) {
			last = cached.Error
			continue
		}
		var b struct{ Number, Hash, ParentHash string }
		err := client.Call(ctx, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", a.Height), false}, &b)
		if err == nil && (b.Number != fmt.Sprintf("0x%x", a.Height) || !runtimeDigest(b.Hash) || !runtimeDigest(b.ParentHash) || !rpc.SameEVMHash(b.Hash, a.Hash)) {
			err = ingest.ErrFactoryStale
		}
		now := time.Now().UTC()
		p.mu.Lock()
		evidenceValid := false
		for _, live := range p.snapshot.Providers {
			if live.ProviderID == obs.ID && live.Identity.Status == rpc.Supported && live.CurrentHashPinned.Status == rpc.Supported && live.CurrentCodePinned.Status == rpc.Supported && live.CurrentCallPinned.Status == rpc.Supported && time.Since(live.CheckedAt) <= time.Duration(p.cfg.MaxAgeMS)*time.Millisecond {
				evidenceValid = true
				break
			}
		}
		if err == nil && !evidenceValid {
			p.mu.Unlock()
			return ErrProviderGate
		}
		if p.anchorChecks == nil {
			p.anchorChecks = map[string]providerAnchorCheck{}
		}
		check := providerAnchorCheck{Anchor: a, CheckedAt: now, Error: err}
		if err != nil {
			delay := time.Duration(p.cfg.IntervalMS) * time.Millisecond
			if delay > time.Second {
				delay = time.Second
			}
			check.RetryAt = now.Add(delay)
			p.snapshot.LastError = rpc.Classify(err)
		}
		p.anchorChecks[obs.ID] = check
		for i := range p.snapshot.Providers {
			if p.snapshot.Providers[i].ProviderID != obs.ID {
				continue
			}
			s := &p.snapshot.Providers[i]
			s.Observation = client.Providers()[0]
			if err == nil {
				s.Head = rpc.Capability{Status: rpc.Supported}
				s.HeadCheckedAt = now
				if a.Height >= s.HTTPHead.Height {
					s.HTTPHead = a
					s.HTTPHead.Generation = 0
				}
			} else {
				s.Head = rpc.Capability{Status: rpc.Unknown, Error: rpc.Classify(err)}
			}
		}
		p.mu.Unlock()
		if err == nil {
			return nil
		}
		last = err
		if ctx.Err() != nil {
			return ctx.Err()
		}
	}
	if last == nil {
		return ErrProviderGate
	}
	return last
}

type runtimeProbeContextKey struct{}

func probeCurrentMethod(ctx context.Context, client *rpc.Client, method string, target any, hash, control string) rpc.Capability {
	result := rpc.Capability{Status: rpc.Unknown}
	params := func(block string) []any {
		selector := map[string]any{"blockHash": block, "requireCanonical": true}
		if method == "eth_getStorageAt" {
			return []any{target, "0x0", selector}
		}
		return []any{target, selector}
	}
	var value string
	err := client.Call(ctx, method, params(hash), &value)
	if err == nil {
		if len(value) < 2 || !strings.HasPrefix(value, "0x") {
			err = errors.New("RPC_RESULT_INVALID")
		} else {
			_, err = hex.DecodeString(value[2:])
			if err == nil && method == "eth_getStorageAt" && len(value) != 66 {
				err = errors.New("RPC_RESULT_INVALID")
			}
		}
	}
	if err != nil {
		result.Error = rpc.Classify(err)
		if result.Error == rpc.Unsupported || result.Error == rpc.InvalidRequest {
			result.Status = rpc.NotSupported
		}
		return result
	}
	var negative string
	err = client.Call(ctx, method, params(control), &negative)
	if err == nil {
		result.Error = rpc.InvalidResponse
		return result
	}
	if err.Error() != "RPC_ERROR_-32000" && err.Error() != "RPC_ERROR_-32001" {
		result.Error = rpc.Classify(err)
		return result
	}
	return rpc.Capability{Status: rpc.Supported}
}

type ProviderReadPurpose string

const (
	ProviderReadProbe  ProviderReadPurpose = "probe"
	ProviderReadAnchor ProviderReadPurpose = "anchor"
)

type ProviderReadObserver func(context.Context, ProviderReadPurpose, func(context.Context) error) error

// SetObserver is optional and carries no Service dependency. Normal services
// register raw responsibility; standalone probes run the same operation directly.
func (p *ProviderRuntime) SetObserver(observer ProviderReadObserver) {
	p.mu.Lock()
	p.observer = observer
	p.mu.Unlock()
}
func (p *ProviderRuntime) observed(ctx context.Context, purpose ProviderReadPurpose, operation func(context.Context) error) error {
	p.mu.Lock()
	observer := p.observer
	p.mu.Unlock()
	if observer == nil {
		return operation(ctx)
	}
	return observer(ctx, purpose, operation)
}
func (p *ProviderRuntime) Refresh(ctx context.Context) error {
	return p.observed(ctx, ProviderReadProbe, p.refresh)
}
func (p *ProviderRuntime) VerifyAnchor(ctx context.Context, a ingest.CatalogAnchor) error {
	return p.observed(ctx, ProviderReadAnchor, func(observed context.Context) error { return p.verifyAnchor(observed, a) })
}
