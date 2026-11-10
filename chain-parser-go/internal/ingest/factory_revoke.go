package ingest

import (
	"context"
	"strings"
)

// RevokeBlock forgets orphaned evidence and both namespaces' admissions. The
// caller must retry busy actors after cancellation; no queued unbounded work.
// Tombstones are bounded by MaxCandidates and prevent replay of removed logs.
func (d *FactoryDiscovery) RevokeBlock(ctx context.Context, chain, hash string) error {
	if err := d.enter(ctx); err != nil {
		return err
	}
	defer d.busy.Store(false)
	if chain == "" || !validHeadHash(hash) {
		return ErrFactoryEvent
	}
	key := chain + ":" + strings.ToLower(hash)
	if !d.revoked[key] && len(d.revoked) >= d.options.Limits.MaxCandidates {
		return ErrBudget
	}
	d.revoked[key] = true
	for id, c := range d.verified {
		if c.Chain == chain && strings.EqualFold(c.BlockHash, hash) {
			delete(d.verified, id)
		}
	}
	d.mu.Lock()
	defer d.mu.Unlock()
	for _, directory := range []map[string]PoolAdmission{d.online, d.history} {
		for key, a := range directory {
			if a.Candidate.Chain == chain && (strings.EqualFold(a.Candidate.BlockHash, hash) || strings.EqualFold(a.At.Hash, hash)) {
				delete(directory, key)
			}
		}
	}
	return nil
}
