package rpc

import (
	"bytes"
	"context"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"math/big"
	"net/http"
	"reflect"
	"strings"
	"sync"
	"time"
)

type Client struct {
	HTTP        *http.Client
	URLs        []string
	MaxBytes    int
	mu          sync.Mutex
	quarantined map[string]bool
	states      map[string]*providerState
	policy      Policy
	recorder    *recorderState
}

func New(urls []string, timeout time.Duration, maxBytes int) *Client {
	transport := http.DefaultTransport.(*http.Transport).Clone()
	transport.Proxy = nil
	return &Client{HTTP: &http.Client{Timeout: timeout, Transport: transport, CheckRedirect: func(_ *http.Request, _ []*http.Request) error { return errors.New("RPC_REDIRECT_FORBIDDEN") }}, URLs: urls, MaxBytes: maxBytes, quarantined: map[string]bool{}}
}

type rpcResponse struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      int             `json:"id"`
	Result  json.RawMessage `json:"result"`
	Error   *struct {
		Code int `json:"code"`
	} `json:"error"`
}

func (c *Client) callURL(ctx context.Context, url, method string, params any, out any) error {
	b, e := json.Marshal(map[string]any{"jsonrpc": "2.0", "id": 1, "method": method, "params": params})
	if e != nil {
		return errors.New("RPC_REQUEST_ENCODING")
	}
	req, e := http.NewRequestWithContext(ctx, http.MethodPost, url, bytes.NewReader(b))
	if e != nil {
		return errors.New("RPC_URL_INVALID")
	}
	req.Header.Set("Content-Type", "application/json")
	res, e := c.HTTP.Do(req)
	if e != nil {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		if errors.Is(e, context.DeadlineExceeded) {
			return context.DeadlineExceeded
		}
		return errors.New("RPC_NETWORK")
	}
	defer res.Body.Close()
	requestBytes := b
	b, e = io.ReadAll(io.LimitReader(res.Body, int64(c.MaxBytes)+1))
	if e == nil && len(b) <= c.MaxBytes {
		if err := c.recordResponse(ctx, endpointID(url), method, requestBytes, b); err != nil {
			return err
		}
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if e != nil {
		return errors.New("RPC_NETWORK")
	}
	if len(b) > c.MaxBytes {
		return errors.New("RPC_RESPONSE_LIMIT")
	}
	if res.StatusCode != 200 {
		return fmt.Errorf("RPC_HTTP_%d", res.StatusCode)
	}
	var response rpcResponse
	if json.Unmarshal(b, &response) != nil || response.JSONRPC != "2.0" || response.ID != 1 {
		return errors.New("RPC_INVALID_ENVELOPE")
	}
	if response.Error != nil {
		return fmt.Errorf("RPC_ERROR_%d", response.Error.Code)
	}
	if len(response.Result) == 0 || string(response.Result) == "null" {
		return errors.New("RPC_NULL_RESULT")
	}
	// Validate in isolation before touching the caller. Standard JSON type
	// errors from a failed endpoint must not taint later failover results.
	// The final decode preserves the existing successful JSON merge semantics.
	destination := reflect.ValueOf(out)
	if !destination.IsValid() || destination.Kind() != reflect.Pointer || destination.IsNil() {
		return errors.New("RPC_RESULT_INVALID")
	}
	staged := reflect.New(destination.Elem().Type())
	if json.Unmarshal(response.Result, staged.Interface()) != nil {
		return errors.New("RPC_RESULT_INVALID")
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if json.Unmarshal(response.Result, out) != nil {
		return errors.New("RPC_RESULT_INVALID")
	}
	return nil
}
func (c *Client) Call(ctx context.Context, method string, params any, out any) error {
	_, err := c.CallWithProvider(ctx, method, params, out)
	return err
}

// CallWithProvider returns the endpoint ID that produced the result. Use this
// ID in raw evidence ownership; IDs do not certify independent failure domains.
func (c *Client) CallWithProvider(ctx context.Context, method string, params any, out any) (string, error) {
	ctx, cancel := callContext(ctx, c.HTTP.Timeout)
	defer cancel()
	if ctx.Err() != nil {
		return "", ctx.Err()
	}
	var urls []string
	seen := map[string]bool{}
	for _, url := range c.URLs {
		skip := c.isQuarantined(url)
		if !skip && !seen[url] {
			urls = append(urls, url)
			seen[url] = true
		}
	}
	if len(urls) == 0 {
		return "", errors.New("NO_HEALTHY_PROVIDER")
	}
	p := c.getPolicy()
	max := p.MaxAttempts
	if max == 0 {
		max = len(urls)
	}
	var last error
	var lastID string
	disabled := map[string]bool{}
	attempts := 0
	for round := 0; attempts < max; round++ {
		retryable := false
		for _, url := range urls {
			if disabled[url] || c.isQuarantined(url) {
				continue
			}
			if attempts >= max {
				break
			}
			attempts++
			lastID = endpointID(url)
			last = c.attempt(ctx, url, method, params, out)
			if last == nil {
				return endpointID(url), nil
			}
			if errors.Is(last, ErrRecordingFailed) {
				return lastID, last
			}
			switch Classify(last) {
			case Canceled, Deadline, InvalidRequest, Unsupported:
				return endpointID(url), last
			case Transient, RateLimited:
				retryable = true
			default:
				disabled[url] = true
			}
		}
		if !retryable || attempts >= max {
			break
		}
		if err := backoff(ctx, p, round); err != nil {
			return lastID, err
		}
	}
	return lastID, last
}
func (c *Client) VerifyEVM(ctx context.Context, chain, genesis string) error {
	ctx, cancel := callContext(ctx, c.HTTP.Timeout)
	defer cancel()
	if ctx.Err() != nil {
		return ctx.Err()
	}
	id := strings.TrimPrefix(chain, "evm:")
	expected, ok := new(big.Int).SetString(id, 10)
	if !ok || !strings.HasPrefix(chain, "evm:") || expected.Sign() < 0 {
		return errors.New("INVALID_CHAIN_KEY")
	}
	healthy := 0
	var lastIdentityError error
	seen := map[string]bool{}
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		if ctx.Err() != nil {
			return ctx.Err()
		}
		var got string
		e := c.attempt(ctx, url, "eth_chainId", []any{}, &got)
		if errors.Is(e, ErrRecordingFailed) {
			return e
		}
		if e == nil {
			n, valid := new(big.Int).SetString(strings.TrimPrefix(got, "0x"), 16)
			if !valid || n.Cmp(expected) != 0 {
				e = errors.New("RPC_WRONG_CHAIN")
			}
		}
		if e == nil {
			var block struct {
				Hash string `json:"hash"`
			}
			e = c.attempt(ctx, url, "eth_getBlockByNumber", []any{"0x0", false}, &block)
			if errors.Is(e, ErrRecordingFailed) {
				return e
			}
			if e == nil && !strings.EqualFold(block.Hash, genesis) {
				e = errors.New("RPC_WRONG_GENESIS")
			}
		}
		c.mu.Lock()
		if c.quarantined == nil {
			c.quarantined = map[string]bool{}
		}
		c.quarantined[url] = e != nil
		c.mu.Unlock()
		s := c.state(url)
		s.mu.Lock()
		s.obs.Quarantined = e != nil
		if Classify(e) == IdentityMismatch {
			s.obs.LastClass = IdentityMismatch
			s.obs.Failures++
		}
		s.mu.Unlock()
		if e != nil {
			lastIdentityError = e
		}
		if e == nil {
			healthy++
		}
	}
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if healthy == 0 {
		return &CallError{Class: Classify(lastIdentityError), Code: "RPC_IDENTITY_NO_HEALTHY_PROVIDER", Cause: lastIdentityError}
	}
	return nil
}

// SameEVMHash treats valid 32-byte hex hashes as bytes. Non-hash aliases used
// by explicit debug fixtures remain case-sensitive strings.
func SameEVMHash(a, b string) bool {
	if len(a) != 66 || len(b) != 66 || !strings.HasPrefix(a, "0x") || !strings.HasPrefix(b, "0x") {
		return a == b
	}
	if _, err := hex.DecodeString(a[2:]); err != nil {
		return a == b
	}
	if _, err := hex.DecodeString(b[2:]); err != nil {
		return a == b
	}
	return strings.EqualFold(a, b)
}

// ConfirmEVMBlock requires agreement from at least two distinct, non-quarantined
// configured endpoints. URL distinctness does not establish independent failure
// domains or consensus. It is used for fork recovery, never as a provider vote
// that can silently outvote a conflicting response.
func (c *Client) ConfirmEVMBlock(ctx context.Context, number uint64, hash, parent string) error {
	ctx, cancel := callContext(ctx, c.HTTP.Timeout)
	defer cancel()
	seen := map[string]bool{}
	confirmed := 0
	if ctx.Err() != nil {
		return ctx.Err()
	}
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		skip := c.isQuarantined(url)
		if skip {
			continue
		}
		var b struct {
			Number string `json:"number"`
			Hash   string `json:"hash"`
			Parent string `json:"parentHash"`
		}
		if e := c.attempt(ctx, url, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", number), false}, &b); e != nil {
			if errors.Is(e, ErrRecordingFailed) {
				return e
			}
			if ctx.Err() != nil {
				return ctx.Err()
			}
			return errors.New("FORK_CONFIRMATION_UNAVAILABLE")
		}
		if b.Number != fmt.Sprintf("0x%x", number) || !SameEVMHash(b.Hash, hash) || !SameEVMHash(b.Parent, parent) {
			return errors.New("FORK_PROVIDER_DISAGREEMENT")
		}
		confirmed++
	}
	if confirmed < 2 {
		// Legacy diagnostic retained for compatibility. This checks distinct
		// endpoints only; provider independence requires external evidence.
		return errors.New("FORK_INDEPENDENT_PROVIDER_REQUIRED")
	}
	return nil
}

// ProviderClients returns the distinct configured endpoint count and a snapshot
// of healthy, endpoint-bound clients. A child cannot fail over to another URL.
// Distinct URLs are not a claim of independent provider ownership.
func (c *Client) ProviderClients() (int, []*Client) {
	recorder := c.recorderState()
	for _, url := range c.URLs {
		c.state(url)
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	seen := map[string]bool{}
	var providers []*Client
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		s := c.states[url]
		s.mu.Lock()
		quarantined := s.obs.Quarantined
		s.mu.Unlock()
		if c.quarantined[url] || quarantined {
			continue
		}
		providers = append(providers, &Client{HTTP: c.HTTP, URLs: []string{url}, MaxBytes: c.MaxBytes, quarantined: map[string]bool{}, states: map[string]*providerState{url: c.states[url]}, policy: c.policy, recorder: recorder})
	}
	return len(seen), providers
}
