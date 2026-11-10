package rpc

import (
	"context"
	"crypto/sha256"
	"errors"
	"fmt"
	"math"
	"math/rand/v2"
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
	ID           string
	Quarantined  bool
	Attempts     uint64
	Failures     uint64
	LastClass    ErrorClass
	LastLatency  time.Duration
	TotalLatency time.Duration
}
type providerState struct {
	mu      sync.Mutex
	tokens  float64
	updated time.Time
	obs     ProviderObservation
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
		s = &providerState{obs: ProviderObservation{ID: endpointID(url)}}
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
	seen := map[string]bool{}
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		s := c.state(url)
		s.mu.Lock()
		obs := s.obs
		s.mu.Unlock()
		obs.Quarantined = c.isQuarantined(url)
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
	s.mu.Lock()
	if p.RequestsPerSecond > 0 {
		now := time.Now()
		if s.updated.IsZero() {
			s.tokens = float64(p.Burst)
		} else {
			s.tokens = math.Min(float64(p.Burst), s.tokens+now.Sub(s.updated).Seconds()*p.RequestsPerSecond)
		}
		s.updated = now
		if s.tokens < 1 {
			s.mu.Unlock()
			return &CallError{ProviderID: endpointID(url), Class: BudgetExceeded, Code: "RPC_RATE_BUDGET"}
		}
		s.tokens--
	}
	s.mu.Unlock()
	start := time.Now()
	err := c.callURL(ctx, url, method, params, out)
	s.mu.Lock()
	s.obs.Attempts++
	s.obs.LastLatency = time.Since(start)
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
