package rpc

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"math"
	"math/rand/v2"
	"slices"
	"strconv"
	"strings"
	"sync"
	"time"
)

// ErrorClass is deliberately independent of provider-controlled error messages.
type ErrorClass string

const (
	InvalidRequest   ErrorClass = "invalid_request"
	Unsupported      ErrorClass = "unsupported"
	AccessDenied     ErrorClass = "access_denied"
	RateLimited      ErrorClass = "rate_limited"
	Transient        ErrorClass = "transient"
	Unavailable      ErrorClass = "unavailable"
	InvalidResponse  ErrorClass = "invalid_response"
	IdentityMismatch ErrorClass = "identity_mismatch"
	BudgetExceeded   ErrorClass = "budget_exceeded"
	Canceled         ErrorClass = "canceled"
	Deadline         ErrorClass = "deadline"
)

type CallError struct {
	ProviderID string
	Class      ErrorClass
	Code       string
	Cause      error
}

func (e *CallError) Error() string { return e.Code }
func (e *CallError) Unwrap() error { return e.Cause }
func Classify(err error) ErrorClass {
	if err == nil {
		return ""
	}
	if errors.Is(err, ErrRecordingFailed) {
		return RecordingFailed
	}
	if errors.Is(err, context.Canceled) {
		return Canceled
	}
	if errors.Is(err, context.DeadlineExceeded) {
		return Deadline
	}
	var typed *CallError
	if errors.As(err, &typed) {
		return typed.Class
	}
	code := err.Error()
	switch {
	case code == "RPC_RATE_BUDGET":
		return BudgetExceeded
	case code == "RPC_HTTP_429" || code == "RPC_ERROR_-32005":
		return RateLimited
	case code == "RPC_HTTP_401" || code == "RPC_HTTP_403":
		return AccessDenied
	case code == "RPC_NETWORK" || code == "RPC_ERROR_-32603":
		return Transient
	case strings.HasPrefix(code, "RPC_HTTP_"):
		status, _ := strconv.Atoi(strings.TrimPrefix(code, "RPC_HTTP_"))
		if status >= 500 {
			return Transient
		}
		if status == 408 {
			return Transient
		}
		return InvalidRequest
	case code == "RPC_ERROR_-32601":
		return Unsupported
	case code == "RPC_ERROR_-32600" || code == "RPC_ERROR_-32602" || code == "RPC_REQUEST_ENCODING" || code == "RPC_URL_INVALID":
		return InvalidRequest
	case code == "RPC_WRONG_CHAIN" || code == "RPC_WRONG_GENESIS":
		return IdentityMismatch
	case code == "RPC_NULL_RESULT" || strings.HasPrefix(code, "RPC_ERROR_"):
		return Unavailable
	default:
		return InvalidResponse
	}
}

// MaxAttempts bounds the entire logical Call across all endpoints and retries.
// Zero keeps the legacy one-attempt-per-distinct-endpoint behavior. Rate limits
// are per endpoint and shared with ProviderClients, including probes/identity.
type Policy struct {
	MaxAttempts       int
	BaseBackoff       time.Duration
	MaxBackoff        time.Duration
	RequestsPerSecond float64
	Burst             int
}

type ProviderObservation struct {
	ID                string
	Quarantined       bool
	Attempts          uint64
	Failures          uint64
	BudgetRejections  uint64
	RequestsPerSecond float64
	Burst             int
	LastClass         ErrorClass
	LastLatency       time.Duration
	TotalLatency      time.Duration
	LatencySamples    int
	LatencyP50        time.Duration
	LatencyP95        time.Duration
	LatencyP99        time.Duration
}

const providerLatencyWindow = 128

type providerState struct {
	mu      sync.Mutex
	obs     ProviderObservation
	latency [providerLatencyWindow]time.Duration
	budget  *providerRateBudget
}

type providerRateBudget struct {
	mu      sync.Mutex
	tokens  float64
	updated time.Time
}

func endpointID(url string) string {
	d := sha256.Sum256([]byte(url))
	return fmt.Sprintf("rpc-%x", d[:12])
}
func (c *Client) state(url string) *providerState {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.states == nil {
		c.states = map[string]*providerState{}
	}
	s := c.states[url]
	if s == nil {
		budget := &providerRateBudget{}
		if c.rateSource != nil {
			budget = c.rateSource.state(url).budget
		}
		s = &providerState{obs: ProviderObservation{ID: endpointID(url)}, budget: budget}
		c.states[url] = s
	}
	return s
}
func (c *Client) SetPolicy(p Policy) error {
	if p.MaxAttempts < 0 || p.MaxAttempts > 1024 || p.BaseBackoff < 0 || p.MaxBackoff < 0 || p.BaseBackoff > p.MaxBackoff || p.RequestsPerSecond < 0 || math.IsNaN(p.RequestsPerSecond) || math.IsInf(p.RequestsPerSecond, 0) || p.Burst < 0 || (p.RequestsPerSecond > 0 && p.Burst == 0) {
		return errors.New("RPC_POLICY_INVALID")
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	c.policy = p
	return nil
}
func (c *Client) getPolicy() Policy { c.mu.Lock(); defer c.mu.Unlock(); return c.policy }
func (c *Client) isQuarantined(url string) bool {
	c.mu.Lock()
	local := c.quarantined[url]
	state := c.states[url]
	c.mu.Unlock()
	if local {
		return true
	}
	if state == nil {
		return false
	}
	state.mu.Lock()
	defer state.mu.Unlock()
	return state.obs.Quarantined
}

func (c *Client) Providers() []ProviderObservation {
	var result []ProviderObservation
	p := c.getPolicy()
	seen := map[string]bool{}
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		s := c.state(url)
		s.mu.Lock()
		obs := s.obs
		samples := s.latency
		s.mu.Unlock()
		obs.Quarantined = c.isQuarantined(url)
		obs.RequestsPerSecond, obs.Burst = p.RequestsPerSecond, p.Burst
		obs.LatencySamples = int(min(obs.Attempts, providerLatencyWindow))
		if n := obs.LatencySamples; n > 0 {
			slices.Sort(samples[:n])
			obs.LatencyP50 = samples[(n*50+99)/100-1]
			obs.LatencyP95 = samples[(n*95+99)/100-1]
			obs.LatencyP99 = samples[(n*99+99)/100-1]
		}
		result = append(result, obs)
	}
	return result
}
func (c *Client) attempt(ctx context.Context, url, method string, params, out any) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	s := c.state(url)
	p := c.getPolicy()
	if p.RequestsPerSecond > 0 {
		budget := s.budget
		budget.mu.Lock()
		now := time.Now()
		if budget.updated.IsZero() {
			budget.tokens = float64(p.Burst)
		} else {
			budget.tokens = math.Min(float64(p.Burst), budget.tokens+now.Sub(budget.updated).Seconds()*p.RequestsPerSecond)
		}
		budget.updated = now
		cost := 1.0
		if batch, ok := params.(ethCallBatch); ok {
			cost = float64(len(batch))
		}
		allowed := budget.tokens >= cost
		if allowed {
			budget.tokens -= cost
		}
		budget.mu.Unlock()
		if !allowed {
			s.mu.Lock()
			s.obs.BudgetRejections++
			s.mu.Unlock()
			return &CallError{ProviderID: endpointID(url), Class: BudgetExceeded, Code: "RPC_RATE_BUDGET"}
		}
	}
	start := time.Now()
	err := c.callURL(ctx, url, method, params, out)
	s.mu.Lock()
	s.obs.LastLatency = time.Since(start)
	s.latency[s.obs.Attempts%providerLatencyWindow] = s.obs.LastLatency
	s.obs.Attempts++
	s.obs.TotalLatency += s.obs.LastLatency
	s.obs.LastClass = Classify(err)
	if err != nil && !errors.Is(err, ErrRecordingFailed) {
		s.obs.Failures++
	}
	s.mu.Unlock()
	if err != nil {
		return &CallError{ProviderID: endpointID(url), Class: Classify(err), Code: err.Error(), Cause: err}
	}
	return nil
}
func callContext(ctx context.Context, timeout time.Duration) (context.Context, context.CancelFunc) {
	// Preserve the legacy fail-closed behavior for zero/negative timeouts.
	return context.WithTimeout(ctx, timeout)
}
func backoff(ctx context.Context, p Policy, round int) error {
	delay := p.BaseBackoff
	for i := 0; i < round && delay < p.MaxBackoff; i++ {
		if delay > p.MaxBackoff/2 {
			delay = p.MaxBackoff
		} else {
			delay *= 2
		}
	}
	if delay > p.MaxBackoff {
		delay = p.MaxBackoff
	}
	if delay > 1 {
		delay = delay/2 + time.Duration(rand.Int64N(int64(delay-delay/2)))
	}
	timer := time.NewTimer(delay)
	defer timer.Stop()
	select {
	case <-ctx.Done():
		return ctx.Err()
	case <-timer.C:
		return nil
	}
}
