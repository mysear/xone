package service

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

// Epochs fence writers; block ancestry determines whether committed events are
// canonical. Missing/pruned undo is unknown and leaves the discovery cursor put.
type runtimeCanonicalView struct {
	service        *Service
	chain          string
	lowest         domain.Snapshot
	blocks         map[uint64]domain.BlockRef
	remaining      uint64
	journalParents map[string]domain.BlockRef
}

func (s *Service) runtimeCanonicalView(ch config.Chain) (*runtimeCanonicalView, error) {
	current, ok := s.stores[ch.Key].Current()
	if !ok {
		return nil, ErrRuntimeDiscoveryGap
	}
	limit := ch.ReorgWindow
	if limit == 0 {
		limit = 128
	}
	if limit > 1024 {
		limit = 1024
	}
	return &runtimeCanonicalView{service: s, chain: ch.Key, lowest: current, blocks: map[uint64]domain.BlockRef{current.Reference.Block.Number: current.Reference.Block}, remaining: limit}, nil
}
func (v *runtimeCanonicalView) contains(ctx context.Context, height uint64, hash string) (bool, error) {
	if height > v.lowest.Reference.Block.Number {
		b, ok := v.blocks[height]
		return ok && strings.EqualFold(b.Hash, hash), nil
	}
	for v.lowest.Reference.Block.Number > height {
		// The child's durable parent hash is sufficient for the immediate
		// ancestor even when a new baseline intentionally pruned old undo.
		if v.lowest.Reference.Block.Number-height == 1 {
			return strings.EqualFold(v.lowest.Reference.Block.ParentHash, hash), nil
		}
		if err := ctx.Err(); err != nil {
			return false, err
		}
		if v.remaining == 0 {
			return false, ErrRuntimeDiscoveryGap
		}
		parent, err := v.service.stores[v.chain].UndoParent(v.lowest)
		if err != nil {
			if v.journalParents == nil {
				v.journalParents = map[string]domain.BlockRef{}
				seq := v.service.stores[v.chain].DurableRef().JournalSeq
				after := uint64(0)
				if seq > 1000 {
					after = seq - 1000
				}
				records, e := v.service.stores[v.chain].Records(after, 1000)
				if e != nil {
					return false, errors.Join(ErrRuntimeDiscoveryGap, e)
				}
				for _, record := range records {
					b := record.Reference.Block
					if old, ok := v.journalParents[b.Hash]; ok && old != b {
						return false, ingest.ErrCorrupt
					}
					v.journalParents[b.Hash] = b
				}
			}
			b, ok := v.journalParents[v.lowest.Reference.Block.ParentHash]
			if !ok || b.ChainKey != v.chain || b.Number+1 != v.lowest.Reference.Block.Number {
				return false, errors.Join(ErrRuntimeDiscoveryGap, fmt.Errorf("canonical ancestor %d from %d: %w", height, v.lowest.Reference.Block.Number, err))
			}
			parent = v.lowest
			parent.Reference.Block = b
		}
		v.remaining--
		v.lowest = parent
		v.blocks[parent.Reference.Block.Number] = parent.Reference.Block
	}
	return strings.EqualFold(v.lowest.Reference.Block.Hash, hash), nil
}

// Retire observations from superseded read fences only after comparing them to
// the durable canonical chain. Release authority is saved before touching pins.
// No reducer method is called while holding the catalog mutex.
func (s *Service) reconcileRuntimeDiscovery(ctx context.Context, ch config.Chain, owner domain.SnapshotRef) error {
	c := s.catalogRuntime[ch.Key]
	c.mu.Lock()
	q, err := loadRuntimeDiscoveryLocked(c)
	c.mu.Unlock()
	if err != nil {
		return err
	}
	obsolete := map[runtimeObservedBlock]bool{}
	var view *runtimeCanonicalView
	for _, entry := range q.Observed {
		if err = ctx.Err(); err != nil {
			return err
		}
		if entry.Source == runtimeFence(owner) {
			continue
		}
		if view == nil {
			view, err = s.runtimeCanonicalView(ch)
			if err != nil {
				return err
			}
		}
		canonical, e := view.contains(ctx, entry.Height, entry.Hash)
		if e != nil {
			return e
		}
		if !canonical {
			obsolete[entry] = true
		}
	}
	if len(obsolete) == 0 {
		return nil
	}
	c.mu.Lock()
	defer c.mu.Unlock()
	q, err = loadRuntimeDiscoveryLocked(c)
	if err != nil {
		return err
	}
	kept := make([]runtimeObservedBlock, 0, len(q.Observed))
	for _, entry := range q.Observed {
		if !obsolete[entry] {
			kept = append(kept, entry)
			continue
		}
		q.Orphaned++
		if entry.Raw.ID != "" {
			q.Releases = append(q.Releases, runtimeRawRelease{entry.Raw, entry.Pin})
		}
	}
	q.Observed = kept
	return saveRuntimeDiscoveryLocked(c, q)
}
