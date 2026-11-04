package evm

import (
	"context"
	"strings"
	"testing"

	"xone/chain-parser-go/internal/domain"
)

func completeV2GenesisBank(t *testing.T) completeV2Case {
	t.Helper()
	c := loadCompleteV2OriginalBank(t)
	c.StateAnchor.Number = 0
	c.StateAnchor.ParentHash = completeV2Word(0)
	c.StateAnchor.Timestamp = 0
	// Re-anchor the captured state as an explicitly initialized genesis bank.
	// Keep reserves intact while representing a known zero reserve timestamp.
	for i := range c.Accounts {
		if c.Accounts[i].Address == c.Pool.PoolID {
			for j := range c.Accounts[i].Storage {
				slot := &c.Accounts[i].Storage[j]
				if slot.Slot == completeV2Word(8) {
					slot.Value = "0x00000000" + slot.Value[10:]
				}
			}
		}
	}
	return c
}

func TestV2CompleteGenesisHeaderParentBoundary(t *testing.T) {
	nonzero := "0x" + strings.Repeat("a", 64)
	for _, tc := range []struct {
		name, number, hash, parent string
		valid                      bool
	}{
		{"genesis-zero-parent", "0x0", nonzero, completeV2Word(0), true},
		{"later-zero-parent", "0x1", nonzero, completeV2Word(0), false},
		{"later-nonzero-parent", "0x1", nonzero, "0x" + strings.Repeat("b", 64), true},
		{"genesis-zero-block-hash", "0x0", completeV2Word(0), completeV2Word(0), false},
		{"later-zero-block-hash", "0x1", completeV2Word(0), nonzero, false},
		{"genesis-short-parent", "0x0", nonzero, "0x0", false},
		{"genesis-missing-parent", "0x0", nonzero, "", false},
		{"genesis-unprefixed-parent", "0x0", nonzero, strings.Repeat("0", 64), false},
		{"genesis-uppercase-prefix-parent", "0x0", nonzero, "0X" + strings.Repeat("0", 64), false},
		{"genesis-overlong-parent", "0x0", nonzero, "0x" + strings.Repeat("0", 66), false},
		{"genesis-bad-hex-parent", "0x0", nonzero, "0x" + strings.Repeat("0", 63) + "z", false},
		{"noncanonical-genesis-number", "0x00", nonzero, completeV2Word(0), false},
	} {
		t.Run(tc.name, func(t *testing.T) {
			err := validateV2CompleteHeader(RawBlock{Number: tc.number, Hash: tc.hash, ParentHash: tc.parent, Timestamp: "0x0"})
			if (err == nil) != tc.valid {
				t.Fatalf("valid=%v err=%v", tc.valid, err)
			}
		})
	}
}

func TestV2CompleteGenesisBankThroughAdapterAndDirectRef(t *testing.T) {
	for _, mode := range []string{"adapter", "direct-ref"} {
		t.Run(mode, func(t *testing.T) {
			c := completeV2GenesisBank(t)
			a, b := newCompleteV2RPC(t, c, nil)
			ref := domain.BlockRef{ChainKey: c.Pool.ChainKey, Number: 0, Hash: c.StateAnchor.Hash, ParentHash: c.StateAnchor.ParentHash, TimestampMS: 0}
			var pool domain.Pool
			if mode == "adapter" {
				header, err := a.Block(context.Background(), "0x0")
				if err != nil || header.ParentHash != completeV2Word(0) {
					t.Fatalf("genesis Block(0x0): header=%+v err=%v", header, err)
				}
				observed, pools, err := a.ReadState(context.Background(), "0x0", false)
				if err != nil || len(pools) != 1 {
					t.Fatalf("genesis bank read: pools=%d err=%v", len(pools), err)
				}
				if observed != ref {
					t.Fatalf("genesis reference changed: %+v", observed)
				}
				pool = pools[0]
			} else {
				var err error
				pool, err = a.readV2Complete(context.Background(), c.config(), map[string]any{"blockHash": ref.Hash, "requireCanonical": true}, ref)
				if err != nil {
					t.Fatalf("explicit zero-bank reference: %v", err)
				}
			}
			assertCompleteV2Projection(t, c, pool)
			if pool.V2.Swap.BankBlockNumber != 0 || pool.V2.Swap.BankUnixTimestamp != 0 || pool.V2.Swap.BlockTimestampLast != 0 {
				t.Fatal("explicit zero bank fields lost")
			}
			b.mu.Lock()
			defer b.mu.Unlock()
			if len(b.seen) != 13 {
				t.Fatalf("missing pinned dependencies: %v", b.seen)
			}
		})
	}
}

func TestV2CompleteRejectsInvalidParentRefBeforeRPC(t *testing.T) {
	for _, tc := range []struct {
		name         string
		number       uint64
		hash, parent string
	}{
		{"later-zero-parent", 1, "", completeV2Word(0)},
		{"genesis-short-parent", 0, "", "0x0"},
		{"genesis-uppercase-parent-prefix", 0, "", "0X" + strings.Repeat("0", 64)},
		{"genesis-overlong-parent", 0, "", "0x" + strings.Repeat("0", 66)},
		{"genesis-missing-parent", 0, "", ""},
		{"genesis-zero-block-hash", 0, completeV2Word(0), completeV2Word(0)},
	} {
		t.Run(tc.name, func(t *testing.T) {
			c := completeV2GenesisBank(t)
			a, b := newCompleteV2RPC(t, c, nil)
			hash := tc.hash
			if hash == "" {
				hash = c.StateAnchor.Hash
			}
			ref := domain.BlockRef{ChainKey: c.Pool.ChainKey, Number: tc.number, Hash: hash, ParentHash: tc.parent, TimestampMS: 0}
			_, err := a.readV2Complete(context.Background(), c.config(), map[string]any{"blockHash": ref.Hash, "requireCanonical": true}, ref)
			if err == nil {
				t.Fatal("invalid parent/block identity accepted")
			}
			b.mu.Lock()
			defer b.mu.Unlock()
			if len(b.seen) != 0 || b.headers != 0 {
				t.Fatal("invalid reference reached RPC")
			}
		})
	}
}
