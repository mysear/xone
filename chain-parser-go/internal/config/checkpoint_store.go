package config

import (
	"errors"
	"strconv"
	"strings"
)

// CheckpointStore is an explicitly configured conditional-PUT HTTP object API.
// Credentials use the exact base URL key in journal.source_auth_tokens.
type CheckpointStore struct {
	BaseURL        string `json:"base_url"`
	AllowPlaintext bool   `json:"allow_plaintext_development"`
}

func (c *CheckpointStore) Validate() error {
	if c == nil {
		return nil
	}
	fail := errors.New("CHECKPOINT_STORE_CONFIG_INVALID")
	if len(c.BaseURL) > 2048 {
		return fail
	}
	rest, secure := strings.CutPrefix(c.BaseURL, "https://")
	if !secure {
		var ok bool
		rest, ok = strings.CutPrefix(c.BaseURL, "http://")
		if !ok || !c.AllowPlaintext {
			return fail
		}
	}
	parts := strings.Split(rest, "/")
	authority := strings.Split(parts[0], ":")
	if len(authority) > 2 || authority[0] == "" || len(authority[0]) > 253 {
		return fail
	}
	for _, label := range strings.Split(authority[0], ".") {
		if len(label) < 1 || len(label) > 63 || label[0] == '-' || label[len(label)-1] == '-' {
			return fail
		}
		for _, v := range label {
			if !(v >= 'a' && v <= 'z' || v >= '0' && v <= '9' || v == '-') {
				return fail
			}
		}
	}
	if len(authority) == 2 {
		port, e := strconv.ParseUint(authority[1], 10, 16)
		if e != nil || port == 0 || strconv.FormatUint(port, 10) != authority[1] {
			return fail
		}
	}
	if !secure && authority[0] != "127.0.0.1" {
		return fail
	}
	for _, part := range parts[1:] {
		if part == "" || part == "." || part == ".." {
			return fail
		}
		for _, v := range part {
			if !(v >= 'a' && v <= 'z' || v >= 'A' && v <= 'Z' || v >= '0' && v <= '9' || strings.ContainsRune("._-", v)) {
				return fail
			}
		}
	}
	return nil
}
