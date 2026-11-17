package config

import (
	"errors"
	"strconv"
	"strings"
)

// CheckpointStore is an explicitly configured conditional-PUT HTTP object API.
// Credentials use the exact base URL key in journal.source_auth_tokens.
type CheckpointStore struct {
	LocalRetention    *CheckpointRetention `json:"local_retention,omitempty"`
	LocalBudget       *CheckpointBudget    `json:"local_budget,omitempty"`
	PublishIntervalMS uint64               `json:"publish_interval_ms,omitempty"`
	BaseURL           string               `json:"base_url"`
	AllowPlaintext    bool                 `json:"allow_plaintext_development"`
}

func (c *CheckpointStore) Validate() error {
	if c == nil {
		return nil
	}
	if e := c.LocalBudget.Validate(); e != nil {
		return e
	}
	if e := c.LocalRetention.Validate(); e != nil {
		return e
	}
	fail := errors.New("CHECKPOINT_STORE_CONFIG_INVALID")
	if len(c.BaseURL) > 2048 || c.PublishIntervalMS > 3600000 || c.PublishIntervalMS > 0 && c.PublishIntervalMS < 100 {
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

type CheckpointBudget struct {
	MaxBytes   uint64 `json:"max_bytes"`
	MaxFiles   uint64 `json:"max_files"`
	MaxEntries uint64 `json:"max_entries"`
}

func (c *CheckpointBudget) Validate() error {
	if c == nil {
		return nil
	}
	if c.MaxBytes == 0 || c.MaxBytes > 1<<40 || c.MaxFiles == 0 || c.MaxFiles > 100000 || c.MaxEntries < c.MaxFiles || c.MaxEntries > 1000000 {
		return errors.New("CHECKPOINT_LOCAL_BUDGET_CONFIG")
	}
	return nil
}

type CheckpointRetention struct {
	KeepLatest   uint64 `json:"keep_latest"`
	MinAgeMS     uint64 `json:"min_age_ms"`
	MaxRemove    uint64 `json:"max_remove"`
	MaxObjects   uint64 `json:"max_objects"`
	MaxEntries   uint64 `json:"max_entries"`
	MaxReadBytes uint64 `json:"max_read_bytes"`
}

func (c *CheckpointRetention) Validate() error {
	if c == nil {
		return nil
	}
	if c.KeepLatest < 2 || c.KeepLatest > 1000 || c.MinAgeMS == 0 || c.MinAgeMS > 31536000000 || c.MaxRemove == 0 || c.MaxRemove > 64 || c.MaxObjects < c.KeepLatest || c.MaxObjects > 4096 || c.MaxEntries < c.MaxObjects || c.MaxEntries > 100000 || c.MaxReadBytes == 0 || c.MaxReadBytes > 256<<20 {
		return errors.New("CHECKPOINT_RETENTION_CONFIG")
	}
	return nil
}
