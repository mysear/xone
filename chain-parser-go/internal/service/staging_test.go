package service

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http/httptest"
	"path/filepath"
	"strconv"
	"strings"
	"testing"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
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

type pancakeStageReader struct{ stageReader }

func (r pancakeStageReader) ReadComplete(ctx context.Context, tag string) (domain.BlockRef, []domain.Pool, []domain.SwapFact, error) {
	block, pools, _, err := r.stageReader.ReadComplete(ctx, tag)
	if err != nil {
		return block, pools, nil, err
	}
	p := pools[0]
	swap := domain.SwapFact{EventID: domain.SwapEventID(block, 1, 2), TxHash: fmt.Sprintf("0x%064x", block.Number), TxIndex: 1, LogIndex: 2,
		PoolID: p.PoolID, Token0: p.Token0, Token1: p.Token1, Amount0In: "10", Amount1In: "0", Amount0Out: "0", Amount1Out: "8"}
	return block, pools, []domain.SwapFact{swap}, nil
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

func TestNativeBNBStagedFactsReachDurableHTTPJournal(t *testing.T) {
	ctx := context.Background()
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	cfg.Config.State.Path = t.TempDir()
	chain := &cfg.Config.Chains[0]
	chain.Key = "evm:56"
	chain.Genesis = "0x0d21840abff46b96c84b2ac9e10e4f5cdaeb5693cb665db62a2f3b02d2d57b5b"
	chain.BlockIntegrity = evm.ChainNativeCommitmentsV2
	chain.NativeProof = &config.NativeProofConfig{MaxBytes: 8 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}
	chain.Pools = []config.PoolConfig{{ID: "0x" + strings.Repeat("10", 20), Protocol: "pancake-v2",
		Token0: "0x" + strings.Repeat("20", 20), Token1: "0x" + strings.Repeat("30", 20), FeeNumerator: 25, FeeDenominator: 10000,
		V2: &config.V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("11", 20), ExpectedCodeSHA256: strings.Repeat("ab", 32)}}}
	source := testSnapshotSource(t)
	s, err := NewWithSource(cfg, func(ctx context.Context, ch config.Chain) (domain.Snapshot, error) {
		snapshot, err := source(ctx, ch)
		if err != nil {
			return snapshot, err
		}
		snapshot.Pools = snapshot.Pools[:1]
		p := &snapshot.Pools[0]
		admitted := ch.Pools[0]
		p.PoolID, p.Token0, p.Token1 = admitted.ID, admitted.Token0, admitted.Token1
		p.Protocol, p.ChainKey, p.AdapterVersion = admitted.Protocol, ch.Key, "pancake-v2/core-1"
		p.FeeNumerator, p.FeeDenominator = admitted.FeeNumerator, admitted.FeeDenominator
		p.V2 = &domain.V2State{Factory: admitted.V2.ExpectedFactory, CodeSHA256: admitted.V2.ExpectedCodeSHA256}
		snapshot.Reference.Block.ChainKey = ch.Key
		snapshot.Reference.AdapterSetHash, _ = domain.Hash([]string{p.AdapterVersion})
		snapshot.Reference.ScopeHash, _ = domain.ScopeHash(snapshot.Pools)
		return snapshot, nil
	})
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(s.Close)
	if err := s.Start(ctx); err != nil {
		t.Fatal(err)
	}
	initial, ready := s.stores[chain.Key].Current()
	if !ready {
		t.Fatal("missing baseline")
	}
	archive, err := ingest.OpenArchive(filepath.Join(t.TempDir(), "staging"), ingest.ArchiveLimits{MaxBytes: 8 << 20, MaxRecords: 10, MaxRecordBytes: 1 << 20})
	if err != nil {
		t.Fatal(err)
	}
	s.rawSources[chain.Key] = newRawRecorder(chain.Key, archive)
	reader := pancakeStageReader{stageReader{initial}}
	if err = s.syncStaged(ctx, *chain, initial, initial.Reference.Block.Number+2, reader); err != nil {
		t.Fatal(err)
	}
	response := httptest.NewRecorder()
	s.Handler().ServeHTTP(response, httptest.NewRequest("GET", "/internal/chains/evm:56/journal?after=0", nil))
	var records []domain.Record
	if response.Code != 200 || json.Unmarshal(response.Body.Bytes(), &records) != nil || len(records) != 3 || records[0].NormalizedFacts != nil {
		t.Fatal(response.Code, response.Body.String())
	}
	for _, row := range records[1:] {
		facts := row.NormalizedFacts
		if facts == nil || facts.DecoderVersion != domain.NativeBNBV2SwapDecoder || facts.RevertedBlocks == nil || len(facts.Blocks) != 1 ||
			len(facts.Blocks[0].Swaps) != 1 || facts.Blocks[0].Block != row.Reference.Block ||
			facts.Blocks[0].Swaps[0].EventID != domain.SwapEventID(row.Reference.Block, 1, 2) {
			t.Fatal(row)
		}
	}
	staged, err := archive.RawRecords()
	if err != nil || len(staged) != 2 {
		t.Fatal(staged, err)
	}
	for _, record := range staged {
		if record.Raw.Owner.ProviderID != rawStagedSwapProvider {
			t.Fatal("new facts share the legacy raw identity", record.Raw.Owner.ProviderID)
		}
	}
}
