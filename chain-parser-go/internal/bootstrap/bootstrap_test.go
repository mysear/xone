package bootstrap

import (
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
