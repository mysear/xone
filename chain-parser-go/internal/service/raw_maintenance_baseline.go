package service

import (
	"context"
	"encoding/json"
	"errors"

	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/reducer"
)

// baselineScopeProof is deliberately not a Current()/source-read shortcut. It
// verifies the immutable scope job written by the same unit as the owner record.
func (m *RawMaintenance) baselineScopeProof(key string, p rawOwnerProof, recordKey string) (*reducer.ScopeOwnerCommit, error) {
	proof, e := m.s.stores[key].ScopeCommit(p.ScopeJob)
	if e != nil {
		// Once a proof was verified and handed off, pruning a scope catalog row must
		// not destroy its historical acknowledgement. The full copied scope proof is
		// part of the digest-verified archive ledger, not a new inferred commit.
		m.mu.Lock()
		entry, ok := m.chains[key].ledgers[recordKey]
		m.mu.Unlock()
		if !ok || entry.Ledger.ScopeCommit == nil {
			return nil, errors.Join(errors.New("RAW_BASELINE_SCOPE_UNCONFIRMED"), e)
		}
		ctx, cancel := context.WithTimeout(context.Background(), m.operationTimeout())
		defer cancel()
		raw, err := m.readEvidence(ctx, key, entry.Raw)
		if err != nil {
			return nil, err
		}
		var ledger rawOwnerLedger
		if json.Unmarshal(raw, &ledger) != nil || ledger.ScopeCommit == nil {
			return nil, ingest.ErrCorrupt
		}
		proof = *ledger.ScopeCommit
	}
	digest, e := fenceDigest(proof.Fence)
	if e != nil || p.ScopeJob == "" || proof.JobID != p.ScopeJob || proof.CommitID != p.CommitID || proof.Reference != p.Reference || digest != p.ScopeFenceDigest {
		return nil, errors.New("RAW_BASELINE_SCOPE_PROOF_MISMATCH")
	}
	return &proof, nil
}
