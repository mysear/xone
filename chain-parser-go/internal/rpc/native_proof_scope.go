package rpc

import "net/http"

// CopyWithHTTP retains source identity/gating, policy and live recorder while
// giving one bounded read an independently wrapped transport.
func (c *Client) CopyWithHTTP(client *http.Client) *Client {
	recorder := c.recorderState()
	for _, url := range c.URLs {
		c.state(url)
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	quarantined := make(map[string]bool, len(c.quarantined))
	for url, value := range c.quarantined {
		quarantined[url] = value
	}
	states := make(map[string]*providerState, len(c.states))
	for url, state := range c.states {
		states[url] = state
	}
	return &Client{HTTP: client, URLs: append([]string(nil), c.URLs...), MaxBytes: c.MaxBytes, quarantined: quarantined, states: states, policy: c.policy, recorder: recorder}
}
