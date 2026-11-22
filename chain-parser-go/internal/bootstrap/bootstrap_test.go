package bootstrap

import (
	"errors"
	"math"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestHistoryNeverBlocksHeadFirst(t *testing.T) {
	s := domain.Snapshot{Reference: domain.SnapshotRef{Block: domain.BlockRef{Number: 1000000, TimestampMS: 1000}}, HistoryGaps: []domain.Gap{{From: 1, To: 999999, Reason: "offline30days"}}}
	if e := CheckFresh(s, 1500, 1000); e != nil {
		t.Fatal(e)
	}
	if e := CheckFresh(s, 2501, 1000); e == nil {
		t.Fatal("stale baseline accepted")
	}
}

func TestTimestampFreshnessBoundaries(t *testing.T) {
	if CheckTimestampMS(40000, 10000, 10) != nil || CheckTimestampMS(math.MaxUint64, math.MaxUint64-2, 10) != nil {
		t.Fatal("allowed future skew rejected")
	}
	if err := CheckTimestampMS(40001, 10000, 10); err == nil || err.Error() != "BASELINE_FUTURE_TIMESTAMP" {
		t.Fatalf("future timestamp accepted or classified as stale: %v", err)
	}
	if CheckTimestampMS(100, 110, 10) != nil || !errors.Is(CheckTimestampMS(100, 111, 10), ErrBaselineTooOld) {
		t.Fatal("age boundary changed")
	}
}
