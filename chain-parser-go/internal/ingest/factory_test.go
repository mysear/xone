package ingest

import (
	"context"
	"encoding/hex"
	"errors"
	"fmt"
	"golang.org/x/crypto/sha3"
	"sync/atomic"
	"testing"
	"time"
)

func factoryAddr(n int) string { return fmt.Sprintf("0x%040x", n) }
func factoryWord(n int) string { return fmt.Sprintf("%064x", n) }
func factoryFixture(t *testing.T) (*FactoryDiscovery, FactoryBlock, *atomic.Int32) {
	t.Helper()
	_, archive := fixture(t)
	raw, err := archive.Put(Owner{Chain: "evm:1", Height: 7, Hash: headHash("ab"), ProviderID: "http-source"}, []byte("fixed block+receipts"))
	if err != nil {
		t.Fatal(err)
	}
	log := FactoryLog{Address: factoryAddr(10), BlockHash: headHash("ab"), BlockNumber: 7, TransactionHash: headHash("cd"), TransactionIndex: 0, LogIndex: 0, Topics: []string{V2PairCreatedTopic, "0x" + factoryWord(1), "0x" + factoryWord(2)}, Data: "0x" + factoryWord(20) + factoryWord(1)}
	batch := FactoryBlock{Chain: "evm:1", Anchor: Cursor{Height: 7, Hash: headHash("ab")}, Raw: raw, Receipts: []FactoryReceipt{{BlockHash: log.BlockHash, BlockNumber: 7, TransactionHash: log.TransactionHash, TransactionIndex: 0, Logs: []FactoryLog{log}}}}
	var published atomic.Int32
	options := FactoryOptions{Sources: []FactorySource{{Chain: "evm:1", Address: factoryAddr(10), Version: UniswapV2Factory, ProvenanceID: "verified-deployment"}}, Archive: archive,
		Limits:      FactoryLimits{Timeout: time.Second, MaxReceipts: 16, MaxLogs: 32, MaxBytes: 1 << 20, MaxCandidates: 16, MaxPools: 16},
		VerifyBlock: func(context.Context, FactoryBlock) error { return nil }, VerifySource: func(context.Context, FactorySource, Cursor) error { return nil }, VerifyCurrent: func(context.Context, CatalogAnchor) error { return nil },
		ReadCurrent: func(ctx context.Context, c FactoryCandidate, a CatalogAnchor) (PoolAdmission, error) {
			return PoolAdmission{Candidate: c, At: a, StateDigest: headHash("ef")}, nil
		},
		Publish: func(ctx context.Context, b CatalogBatch) error { published.Add(1); return nil },
	}
	d, err := NewFactoryDiscovery(options)
	if err != nil {
		t.Fatal(err)
	}
	return d, batch, &published
}
func TestFactoryOriginalSignaturesFixedLocationsDedupAndAdmission(t *testing.T) {
	for signature, topic := range map[string]string{V2PairCreatedSignature: V2PairCreatedTopic, V3PoolCreatedSignature: V3PoolCreatedTopic} {
		h := sha3.NewLegacyKeccak256()
		h.Write([]byte(signature))
		if got := "0x" + hex.EncodeToString(h.Sum(nil)); got != topic {
			t.Fatal("original ABI signature/topic mismatch", signature, got)
		}
	}
	if V2PairCreatedTopic != "0x0d3648bd0f6ba80134a33ba9275ac585d9d315f0ad8355cddefde31afa28d0e9" || V3PoolCreatedTopic != "0x783cca1c0412dd0d695e784568c96da2e9c22ff989357a2e8b1d9b2b4e6b7118" {
		t.Fatal("original event signature changed")
	}
	d, batch, published := factoryFixture(t)
	batch.Receipts[0].Logs = append(batch.Receipts[0].Logs, batch.Receipts[0].Logs[0])
	candidates, err := d.Discover(context.Background(), batch)
	if err != nil || len(candidates) != 1 || candidates[0].Pool != factoryAddr(20) || candidates[0].TransactionIndex != 0 || candidates[0].LogIndex != 0 || len(d.Snapshot(CatalogOnline)) != 0 || published.Load() != 0 {
		t.Fatal(candidates, err)
	}
	anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err != nil {
		t.Fatal(err)
	}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err != nil {
		t.Fatal(err)
	}
	if len(d.Snapshot(CatalogOnline)) != 1 || published.Load() != 1 {
		t.Fatal(d.Snapshot(CatalogOnline), published.Load())
	}
}
func TestFactoryRejectsFalseSourcesLocationsVersionsAndRemoved(t *testing.T) {
	for _, mode := range []string{"source", "provenance", "block-proof", "removed", "blockhash", "receipt-position", "log-position", "conflict", "version", "abi-padding"} {
		t.Run(mode, func(t *testing.T) {
			d, b, published := factoryFixture(t)
			log := &b.Receipts[0].Logs[0]
			switch mode {
			case "source":
				log.Address = factoryAddr(99)
			case "provenance":
				d.options.VerifySource = func(context.Context, FactorySource, Cursor) error { return errors.New("code proof mismatch") }
			case "block-proof":
				d.options.VerifyBlock = func(context.Context, FactoryBlock) error { return errors.New("receipt proof mismatch") }
			case "removed":
				log.Removed = true
			case "blockhash":
				log.BlockHash = headHash("ee")
			case "receipt-position":
				b.Receipts[0].TransactionIndex = 1
			case "log-position":
				log.LogIndex = 1
			case "conflict":
				other := *log
				other.Data = "0x" + factoryWord(21) + factoryWord(1)
				b.Receipts[0].Logs = append(b.Receipts[0].Logs, other)
			case "version":
				log.Topics[0] = V3PoolCreatedTopic
			case "abi-padding":
				log.Topics[1] = "0x01" + log.Topics[1][4:]
			}
			if candidates, err := d.Discover(context.Background(), b); err == nil || len(candidates) != 0 || published.Load() != 0 || len(d.Snapshot(CatalogOnline)) != 0 {
				t.Fatal(candidates, err)
			}
		})
	}
	d, _, _ := factoryFixture(t)
	options := d.options
	options.Sources[0].Version = "unknown-v99"
	if _, err := NewFactoryDiscovery(options); err == nil {
		t.Fatal("unknown version admitted")
	}
}
func TestFactoryFailedCurrentReadsAndHistoryCannotOverwriteOnline(t *testing.T) {
	d, b, published := factoryFixture(t)
	candidates, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
	read := d.options.ReadCurrent
	d.options.ReadCurrent = func(context.Context, FactoryCandidate, CatalogAnchor) (PoolAdmission, error) {
		return PoolAdmission{}, errors.New("current state unavailable")
	}
	if err := d.Admit(context.Background(), CatalogOnline, anchor, candidates); err == nil || published.Load() != 0 || len(d.Snapshot(CatalogOnline)) != 0 {
		t.Fatal(err)
	}
	d.options.ReadCurrent = read
	if err := d.Admit(context.Background(), CatalogOnline, anchor, candidates); err != nil {
		t.Fatal(err)
	}
	historyAnchor := anchor
	historyAnchor.Height = 11
	historyAnchor.Hash = headHash("ff")
	if err := d.Admit(context.Background(), CatalogHistory, historyAnchor, candidates); err != nil {
		t.Fatal(err)
	}
	if d.Snapshot(CatalogOnline)[0].At != anchor || d.Snapshot(CatalogHistory)[0].At != historyAnchor {
		t.Fatal("history overwrote online")
	}
	forged := candidates[0]
	forged.Pool = factoryAddr(500)
	if err := d.Admit(context.Background(), CatalogOnline, historyAnchor, []FactoryCandidate{forged}); err == nil {
		t.Fatal("forged candidate promoted")
	}
}
func TestFactoryV3DecodeSortAndAtomicBatchPublish(t *testing.T) {
	d, b, published := factoryFixture(t)
	options := d.options
	options.Sources = append(options.Sources, FactorySource{Chain: "evm:1", Address: factoryAddr(11), Version: UniswapV3Factory, ProvenanceID: "v3-code-proof"})
	d, err := NewFactoryDiscovery(options)
	if err != nil {
		t.Fatal(err)
	}
	v3 := b.Receipts[0].Logs[0]
	v3.LogIndex = 1
	v3.Address = factoryAddr(11)
	v3.Topics = []string{V3PoolCreatedTopic, "0x" + factoryWord(1), "0x" + factoryWord(2), "0x" + factoryWord(3000)}
	v3.Data = "0x" + factoryWord(60) + factoryWord(21)
	b.Receipts[0].Logs = []FactoryLog{v3, b.Receipts[0].Logs[0]}
	candidates, err := d.Discover(context.Background(), b)
	if err != nil || len(candidates) != 2 || candidates[0].LogIndex != 0 || candidates[1].TickSpacing != 60 || candidates[1].Fee != 3000 {
		t.Fatal(candidates, err)
	}
	read := d.options.ReadCurrent
	d.options.ReadCurrent = func(ctx context.Context, c FactoryCandidate, a CatalogAnchor) (PoolAdmission, error) {
		if c.Pool == factoryAddr(21) {
			return PoolAdmission{}, errors.New("second pool incomplete")
		}
		return read(ctx, c, a)
	}
	anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err == nil || published.Load() != 0 || len(d.Snapshot(CatalogOnline)) != 0 {
		t.Fatal(err)
	}
	d.options.ReadCurrent = read
	d.options.Publish = func(context.Context, CatalogBatch) error { return errors.New("transaction aborted") }
	if err = d.Admit(context.Background(), CatalogOnline, anchor, candidates); err == nil || len(d.Snapshot(CatalogOnline)) != 0 {
		t.Fatal(err)
	}
}
func TestFactoryBudgetsAndCurrentAnchorBinding(t *testing.T) {
	for _, mode := range []string{"bytes", "logs", "state-anchor", "state-digest", "stale", "cancel"} {
		t.Run(mode, func(t *testing.T) {
			d, b, _ := factoryFixture(t)
			switch mode {
			case "bytes":
				d.options.Limits.MaxBytes = 1
			case "logs":
				d.options.Limits.MaxLogs = 0
			}
			candidates, err := d.Discover(context.Background(), b)
			if mode == "bytes" || mode == "logs" {
				if err == nil {
					t.Fatal("budget ignored")
				}
				return
			}
			if err != nil {
				t.Fatal(err)
			}
			anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
			ctx := context.Background()
			read := d.options.ReadCurrent
			if mode == "stale" {
				if err := d.Admit(ctx, CatalogOnline, anchor, candidates); err != nil {
					t.Fatal(err)
				}
				anchor.Height = 9
			}
			if mode == "cancel" {
				c, cancel := context.WithCancel(ctx)
				cancel()
				ctx = c
			}
			d.options.ReadCurrent = func(ctx context.Context, c FactoryCandidate, a CatalogAnchor) (PoolAdmission, error) {
				p, e := read(ctx, c, a)
				if mode == "state-anchor" {
					p.At.Hash = headHash("ff")
				}
				if mode == "state-digest" {
					p.StateDigest = ""
				}
				return p, e
			}
			if err = d.Admit(ctx, CatalogOnline, anchor, candidates); err == nil {
				t.Fatal("admission barrier ignored")
			}
		})
	}
}

func TestFactoryRejectsRemovedNonCreationLogInFixedReceipts(t *testing.T) {
	d, b, _ := factoryFixture(t)
	b.Receipts[0].Logs[0].Topics[0] = headHash("ee")
	b.Receipts[0].Logs[0].Removed = true
	if _, err := d.Discover(context.Background(), b); err == nil {
		t.Fatal("removed receipt accepted")
	}
}

func TestFactorySameEventAcrossHTTPProvidersIsIdempotent(t *testing.T) {
	d, b, published := factoryFixture(t)
	first, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
	if err := d.Admit(context.Background(), CatalogOnline, anchor, first); err != nil {
		t.Fatal(err)
	}
	raw, err := d.options.Archive.Read(b.Raw)
	if err != nil {
		t.Fatal(err)
	}
	owner := b.Raw.Owner
	owner.ProviderID = "backup-http-source"
	b.Raw, err = d.options.Archive.Put(owner, raw)
	if err != nil {
		t.Fatal(err)
	}
	repeated, err := d.Discover(context.Background(), b)
	if err != nil || len(repeated) != 1 || repeated[0].ID != first[0].ID {
		t.Fatal("provider identity changed logical event identity", repeated, err)
	}
	if err = d.Admit(context.Background(), CatalogOnline, anchor, repeated); err != nil || published.Load() != 1 {
		t.Fatal(err, published.Load())
	}
}

func TestFactoryRawEvidenceIncludedInByteBudget(t *testing.T) {
	d, b, _ := factoryFixture(t)
	owner := b.Raw.Owner
	owner.ProviderID = "large-raw"
	var err error
	b.Raw, err = d.options.Archive.Put(owner, make([]byte, 1500))
	if err != nil {
		t.Fatal(err)
	}
	d.options.Limits.MaxBytes = 1000
	if _, err := d.Discover(context.Background(), b); !errors.Is(err, ErrBudget) {
		t.Fatal("raw evidence escaped input budget", err)
	}
}

func TestFactoryCanceledAdmissionDoesNotWaitBehindAnotherActor(t *testing.T) {
	d, b, _ := factoryFixture(t)
	candidates, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	anchor := CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}
	entered := make(chan struct{})
	release := make(chan struct{})
	read := d.options.ReadCurrent
	d.options.ReadCurrent = func(ctx context.Context, c FactoryCandidate, a CatalogAnchor) (PoolAdmission, error) {
		close(entered)
		<-release
		return read(ctx, c, a)
	}
	first := make(chan error, 1)
	go func() { first <- d.Admit(context.Background(), CatalogOnline, anchor, candidates) }()
	<-entered
	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	second := make(chan error, 1)
	go func() { second <- d.Admit(ctx, CatalogOnline, anchor, candidates) }()
	select {
	case err := <-second:
		if !errors.Is(err, context.Canceled) {
			t.Error(err)
		}
	case <-time.After(50 * time.Millisecond):
		t.Error("canceled caller blocked behind current-state callback")
	}
	close(release)
	if err := <-first; err != nil {
		t.Fatal(err)
	}
}
func TestFactoryPublisherCanInspectLastPublishedSnapshot(t *testing.T) {
	d, b, _ := factoryFixture(t)
	candidates, err := d.Discover(context.Background(), b)
	if err != nil {
		t.Fatal(err)
	}
	d.options.Publish = func(ctx context.Context, b CatalogBatch) error {
		if len(d.Snapshot(CatalogOnline)) != 0 {
			return errors.New("half published catalog")
		}
		return nil
	}
	done := make(chan error, 1)
	go func() {
		done <- d.Admit(context.Background(), CatalogOnline, CatalogAnchor{Chain: "evm:1", Height: 10, Hash: headHash("ee"), Generation: 1}, candidates)
	}()
	select {
	case err := <-done:
		if err != nil {
			t.Fatal(err)
		}
	case <-time.After(100 * time.Millisecond):
		t.Fatal("publisher callback deadlocked on snapshot")
	}
}
