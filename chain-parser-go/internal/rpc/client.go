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
		return errors.New("RPC_NETWORK")
	}
	defer res.Body.Close()
	b, e = io.ReadAll(io.LimitReader(res.Body, int64(c.MaxBytes)+1))
	if e != nil || len(b) > c.MaxBytes {
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
	if json.Unmarshal(response.Result, out) != nil {
		return errors.New("RPC_RESULT_INVALID")
	}
	return nil
}
func (c *Client) Call(ctx context.Context, method string, params any, out any) error {
	// One total budget, one attempt per provider. No nested unbounded retries.
	ctx, cancel := context.WithTimeout(ctx, c.HTTP.Timeout)
	defer cancel()
	var last error = errors.New("NO_HEALTHY_PROVIDER")
	for _, url := range c.URLs {
		c.mu.Lock()
		skip := c.quarantined[url]
		c.mu.Unlock()
		if skip {
			continue
		}
		if ctx.Err() != nil {
			return errors.New("RPC_DEADLINE")
		}
		last = c.callURL(ctx, url, method, params, out)
		if last == nil {
			return nil
		}
		if strings.HasPrefix(last.Error(), "RPC_ERROR_-3260") {
			return last
		}
	}
	return last
}
func (c *Client) VerifyEVM(ctx context.Context, chain, genesis string) error {
	id := strings.TrimPrefix(chain, "evm:")
	expected, ok := new(big.Int).SetString(id, 10)
	if !ok {
		return errors.New("INVALID_CHAIN_KEY")
	}
	healthy := 0
	for _, url := range c.URLs {
		var got string
		ctxCall, cancel := context.WithTimeout(ctx, c.HTTP.Timeout)
		e := c.callURL(ctxCall, url, "eth_chainId", []any{}, &got)
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
			e = c.callURL(ctxCall, url, "eth_getBlockByNumber", []any{"0x0", false}, &block)
			if e == nil && !strings.EqualFold(block.Hash, genesis) {
				e = errors.New("RPC_WRONG_GENESIS")
			}
		}
		cancel()
		c.mu.Lock()
		c.quarantined[url] = e != nil
		c.mu.Unlock()
		if e == nil {
			healthy++
		}
	}
	if healthy == 0 {
		return errors.New("RPC_IDENTITY_NO_HEALTHY_PROVIDER")
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
// configured endpoints. It is used for fork recovery, never as a provider vote
// that can silently outvote a conflicting response.
func (c *Client) ConfirmEVMBlock(ctx context.Context, number uint64, hash, parent string) error {
	ctx, cancel := context.WithTimeout(ctx, c.HTTP.Timeout)
	defer cancel()
	seen := map[string]bool{}
	confirmed := 0
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		c.mu.Lock()
		skip := c.quarantined[url]
		c.mu.Unlock()
		if skip {
			continue
		}
		var b struct {
			Number string `json:"number"`
			Hash   string `json:"hash"`
			Parent string `json:"parentHash"`
		}
		if e := c.callURL(ctx, url, "eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", number), false}, &b); e != nil {
			return errors.New("FORK_CONFIRMATION_UNAVAILABLE")
		}
		if b.Number != fmt.Sprintf("0x%x", number) || !SameEVMHash(b.Hash, hash) || !SameEVMHash(b.Parent, parent) {
			return errors.New("FORK_PROVIDER_DISAGREEMENT")
		}
		confirmed++
	}
	if confirmed < 2 {
		return errors.New("FORK_INDEPENDENT_PROVIDER_REQUIRED")
	}
	return nil
}

// ProviderClients returns the distinct configured endpoint count and a snapshot
// of healthy, endpoint-bound clients. A child cannot fail over to another URL.
// Distinct URLs are not a claim of independent provider ownership.
func (c *Client) ProviderClients() (int, []*Client) {
	c.mu.Lock()
	defer c.mu.Unlock()
	seen := make(map[string]bool, len(c.URLs))
	providers := make([]*Client, 0, len(c.URLs))
	for _, url := range c.URLs {
		if seen[url] {
			continue
		}
		seen[url] = true
		if c.quarantined[url] {
			continue
		}
		providers = append(providers, &Client{HTTP: c.HTTP, URLs: []string{url}, MaxBytes: c.MaxBytes, quarantined: map[string]bool{}})
	}
	return len(seen), providers
}
