package service

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"strings"

	"github.com/cockroachdb/pebble"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/ingest"
)

var runtimeDiscoveryIntentKey = []byte("normal-discovery/pin-intent/v1")

// One bounded write-ahead body lives outside the ref-only observation queue.
// It permits replay both before and after Archive.PutPinned on process restart.
// Queue+intent serialized bytes must fit the catalog budget before acquiring a pin.
type runtimeDiscoveryIntent struct {
	Version     int
	Observation runtimeObservedBlock
	Proof       json.RawMessage
}

func runtimeDiscoveryPin(c *CatalogRuntime, source CatalogFence, hash string) string {
	identity := sha256.Sum256([]byte(c.options.Path))
	return fmt.Sprintf("normal-discovery:%x:%d:%d:%s", identity[:12], source.CanonicalEpoch, source.Generation, strings.ToLower(hash))
}
func runtimeDiscoveryEnvelope(value any, maxBytes int64) ([]byte, error) {
	data, err := json.Marshal(value)
	if err != nil {
		return nil, err
	}
	sum := sha256.Sum256(data)
	raw, err := json.Marshal(catalogEnvelope{Digest: hex.EncodeToString(sum[:]), Data: data})
	if err != nil {
		return nil, err
	}
	if int64(len(raw)) > maxBytes {
		return nil, ingest.ErrBudget
	}
	return raw, nil
}
func loadRuntimeDiscoveryIntentLocked(c *CatalogRuntime) (*runtimeDiscoveryIntent, error) {
	raw, closer, err := c.db.Get(runtimeDiscoveryIntentKey)
	if errors.Is(err, pebble.ErrNotFound) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	defer closer.Close()
	if int64(len(raw)) > c.options.Config.MaxBytes {
		return nil, ingest.ErrBudget
	}
	var envelope catalogEnvelope
	if json.Unmarshal(raw, &envelope) != nil {
		return nil, ingest.ErrCorrupt
	}
	sum := sha256.Sum256(envelope.Data)
	if envelope.Digest != hex.EncodeToString(sum[:]) {
		return nil, ingest.ErrCorrupt
	}
	var intent runtimeDiscoveryIntent
	if json.Unmarshal(envelope.Data, &intent) != nil || intent.Version != 1 {
		return nil, ingest.ErrCorrupt
	}
	o := intent.Observation
	expected, err := ingest.FactoryProofReference(o.Raw.Owner, intent.Proof)
	if err != nil || expected != o.Raw || o.Raw.Owner.Chain != c.options.Chain.Key || o.Raw.Owner.Height != o.Height || !strings.EqualFold(o.Raw.Owner.Hash, o.Hash) || o.Raw.Owner.ProviderID != "verified-normal-complete" || !catalogFenceFollows(o.Source, o.Source) || o.Pin != runtimeDiscoveryPin(c, o.Source, o.Hash) {
		return nil, ingest.ErrCorrupt
	}
	return &intent, nil
}

func prepareRuntimeDiscoveryIntentLocked(c *CatalogRuntime, q runtimeDiscoveryQueue, intent runtimeDiscoveryIntent) error {
	if c.closed {
		return ingest.ErrClosed
	}
	if old, err := loadRuntimeDiscoveryIntentLocked(c); err != nil {
		return err
	} else if old != nil {
		return ErrRuntimeDiscoveryPending
	}
	proposed := q
	proposed.Observed = append(append([]runtimeObservedBlock(nil), q.Observed...), intent.Observation)
	queueRaw, err := runtimeDiscoveryEnvelope(proposed, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	intentRaw, err := runtimeDiscoveryEnvelope(intent, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	if int64(len(queueRaw)+len(intentRaw)) > c.options.Config.MaxBytes {
		return ingest.ErrBudget
	}
	baseRaw, err := runtimeDiscoveryEnvelope(q, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	batch := c.db.NewBatch()
	defer batch.Close()
	if err = batch.Set(runtimeDiscoveryKey, baseRaw, nil); err != nil {
		return err
	}
	if err = batch.Set(runtimeDiscoveryIntentKey, intentRaw, nil); err != nil {
		return err
	}
	return batch.Commit(pebble.Sync)
}

func finishRuntimeDiscoveryIntentLocked(ctx context.Context, c *CatalogRuntime, q runtimeDiscoveryQueue, intent runtimeDiscoveryIntent) error {
	if err := ctx.Err(); err != nil {
		return err
	}
	found := false
	for _, entry := range q.Observed {
		if entry.Height == intent.Observation.Height && strings.EqualFold(entry.Hash, intent.Observation.Hash) {
			if entry != intent.Observation {
				return ingest.ErrCorrupt
			}
			found = true
		}
	}
	if !found {
		q.Observed = append(q.Observed, intent.Observation)
	}
	queueRaw, err := runtimeDiscoveryEnvelope(q, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	intentRaw, err := runtimeDiscoveryEnvelope(intent, c.options.Config.MaxBytes)
	if err != nil {
		return err
	}
	if int64(len(queueRaw)+len(intentRaw)) > c.options.Config.MaxBytes {
		return ingest.ErrBudget
	}
	ref, err := c.options.Archive.PutPinned(intent.Observation.Raw.Owner, intent.Proof, intent.Observation.Pin)
	if err != nil {
		return err
	} // Durable intent retains responsibility for uncertain writes.
	if ref != intent.Observation.Raw {
		return ingest.ErrCorrupt
	}
	if err = ctx.Err(); err != nil {
		return err
	}
	batch := c.db.NewBatch()
	defer batch.Close()
	if err = batch.Set(runtimeDiscoveryKey, queueRaw, nil); err != nil {
		return err
	}
	if err = batch.Delete(runtimeDiscoveryIntentKey, nil); err != nil {
		return err
	}
	return batch.Commit(pebble.Sync)
}
func (s *Service) recoverRuntimeDiscoveryIntent(ctx context.Context, ch config.Chain) error {
	c := s.catalogRuntime[ch.Key]
	if c == nil {
		return nil
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.closed {
		return ingest.ErrClosed
	}
	intent, err := loadRuntimeDiscoveryIntentLocked(c)
	if err != nil || intent == nil {
		return err
	}
	q, err := loadRuntimeDiscoveryLocked(c)
	if err != nil {
		return err
	}
	return finishRuntimeDiscoveryIntentLocked(ctx, c, q, *intent)
}
