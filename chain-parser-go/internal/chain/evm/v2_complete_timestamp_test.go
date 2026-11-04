package evm

import (
	"context"
	"fmt"
	"testing"

	"xone/chain-parser-go/internal/domain"
)

func TestV2CompleteAndLegacyCheckedBlockTimestamp(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	maxSeconds := ^uint64(0) / 1000
	tests := []struct {
		name, encoded string
		seconds       uint64
		valid         bool
	}{
		{"zero", "0x0", 0, true},
		{"one-second", "0x1", 1, true},
		{"max-safe-seconds", fmt.Sprintf("0x%x", maxSeconds), maxSeconds, true},
		{"first-overflow", fmt.Sprintf("0x%x", maxSeconds+1), 0, false},
		{"wrap-to-one-second", "0x2000000000000001", 0, false},
		{"max-u64", "0xffffffffffffffff", 0, false},
		{"outside-u64", "0x10000000000000000", 0, false},
		{"missing", "", 0, false},
		{"non-quantity", "0x1.1", 0, false},
	}
	for _, protocol := range []string{"uniswap-v2", completeV2Protocol} {
		for _, entry := range []string{"Header", "ReadState", "ReadComplete"} {
			for _, tc := range tests {
				t.Run(protocol+"/"+entry+"/"+tc.name, func(t *testing.T) {
					a, b := newCompleteV2RPC(t, c, func(method, to, data string, out any) any {
						if method == "eth_getBlockByNumber" {
							out.(map[string]any)["timestamp"] = tc.encoded
						}
						return out
					})
					a.Chain.Pools[0].Protocol = protocol
					var ref domain.BlockRef
					var pools []domain.Pool
					var err error
					switch entry {
					case "Header":
						ref, err = a.Header(context.Background(), "latest")
					case "ReadState":
						ref, pools, err = a.ReadState(context.Background(), "latest", false)
					case "ReadComplete":
						ref, pools, _, err = a.ReadComplete(context.Background(), "latest")
					}
					if !tc.valid {
						if err == nil || ref != (domain.BlockRef{}) || pools != nil {
							t.Fatalf("invalid timestamp %s became usable ref=%+v pools=%d err=%v", tc.encoded, ref, len(pools), err)
						}
						b.mu.Lock()
						defer b.mu.Unlock()
						if b.headers != 1 || len(b.seen) != 0 {
							t.Fatalf("invalid timestamp reached dependencies: headers=%d reads=%v", b.headers, b.seen)
						}
						return
					}
					if err != nil {
						t.Fatalf("representable timestamp %s rejected: %v", tc.encoded, err)
					}
					if ref.TimestampMS != tc.seconds*1000 || ref.TimestampMS/1000 != tc.seconds {
						t.Fatalf("timestamp conversion changed: seconds=%d ref=%+v", tc.seconds, ref)
					}
					if entry != "Header" {
						if len(pools) != 1 {
							t.Fatalf("missing pool for valid timestamp: %d", len(pools))
						}
						if protocol == completeV2Protocol && pools[0].V2.Swap.BankUnixTimestamp != tc.seconds {
							t.Fatalf("bank source clock changed: %+v", pools[0].V2.Swap)
						}
					}
				})
			}
		}
	}
}

func TestV2CompleteAndLegacyRawBlockKeepsLargeTimestamp(t *testing.T) {
	c := loadCompleteV2OriginalBank(t)
	for _, protocol := range []string{"uniswap-v2", completeV2Protocol} {
		t.Run(protocol, func(t *testing.T) {
			a, b := newCompleteV2RPC(t, c, func(method, to, data string, out any) any {
				if method == "eth_getBlockByNumber" {
					out.(map[string]any)["timestamp"] = "0x2000000000000001"
				}
				return out
			})
			a.Chain.Pools[0].Protocol = protocol
			block, err := a.Block(context.Background(), "latest")
			if err != nil || block.Timestamp != "0x2000000000000001" {
				t.Fatalf("raw block timestamp changed: %s %v", block.Timestamp, err)
			}
			b.mu.Lock()
			defer b.mu.Unlock()
			if b.headers != 1 || len(b.seen) != 0 {
				t.Fatal("raw block read reached pool dependencies")
			}
		})
	}
}

func TestV2CompleteTimestampConversionIndependentOfProfile(t *testing.T) {
	// Conversion also protects header readers that have no configured pools.
	a, _ := newCompleteV2RPC(t, loadCompleteV2OriginalBank(t), func(method, to, data string, out any) any {
		if method == "eth_getBlockByNumber" {
			out.(map[string]any)["timestamp"] = "0x2000000000000001"
		}
		return out
	})
	a.Chain.Pools = nil
	if ref, err := a.Header(context.Background(), "latest"); err == nil || ref != (domain.BlockRef{}) {
		t.Fatalf("unscoped Header accepted overflow: %+v %v", ref, err)
	}
	if ref, pools, err := a.ReadState(context.Background(), "latest", false); err == nil || ref != (domain.BlockRef{}) || pools != nil {
		t.Fatalf("unscoped ReadState accepted overflow: %+v %v", ref, err)
	}
	// Guard against accidentally checking only wrapped millisecond alignment:
	// this source value overflows to exactly 1000ms, still divisible by 1000.
	seconds := uint64(1)<<61 | 1
	if wrapped := seconds * 1000; wrapped != 1000 || wrapped%1000 != 0 {
		t.Fatal("overflow regression vector changed")
	}
}
