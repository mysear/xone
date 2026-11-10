package service

import (
	"context"
	"fmt"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

type stageReader struct{ initial domain.Snapshot }

func (r stageReader) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	h, _ := strconv.ParseUint(strings.TrimPrefix(tag, "0x"), 16, 64)
	if h == r.initial.Reference.Block.Number+1 {
		select {
		case <-time.After(25 * time.Millisecond):
		case <-ctx.Done():
			return domain.BlockRef{}, nil, nil, ctx.Err()
		}
	}
	b := r.initial.Reference.Block
	b.Number = h
	b.Hash = fmt.Sprintf("0x%064x", h+1000)
	b.ParentHash = fmt.Sprintf("0x%064x", h-1+1000)
	b.TimestampMS = uint64(time.Now().UnixMilli())
	if h == r.initial.Reference.Block.Number+1 {
		b.ParentHash = r.initial.Reference.Block.Hash
	}
	return b, r.initial.Pools, nil, nil
}
func TestNormalServiceStagingCommitsCompleteBlocksInOrder(t *testing.T) {
	s, ch, _, _ := forkService(t)
	initial, ok := s.stores[ch.Key].Current()
	if !ok {
		t.Fatal("baseline")
	}
	archive, e := ingest.OpenArchive(filepath.Join(t.TempDir(), "staging"), ingest.ArchiveLimits{MaxBytes: 10 * 1024 * 1024, MaxRecords: 100, MaxRecordBytes: 1024 * 1024})
	if e != nil {
		t.Fatal(e)
	}
	s.rawSources = map[string]*rawRecorder{ch.Key: newRawRecorder(ch.Key, archive)}
	if e = s.syncStaged(context.Background(), ch, initial, initial.Reference.Block.Number+2, stageReader{initial}); e != nil {
		t.Fatal(e)
	}
	got, ok := s.stores[ch.Key].Current()
	if !ok || got.Reference.Block.Number != initial.Reference.Block.Number+2 || got.Reference.JournalSeq != initial.Reference.JournalSeq+2 {
		t.Fatal("not completely committed", got.Reference)
	}
}
