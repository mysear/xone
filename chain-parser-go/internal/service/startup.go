package service

import (
	"context"
	"errors"

	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
)

// Shared resources are opened before this call. A failed chain attempt must not
// prevent independent chains from acquiring and publishing their own baseline.
// false, nil denotes a legitimate writer standby, not an ignored source error.
func (s *Service) initializeChain(ctx context.Context, chain config.Chain) (bool, error) {
	if s.control != nil {
		session, err := s.openSession(ctx, chain)
		if err != nil {
			if err.Error() == "LEASE_NOT_OWNED" {
				s.stores[chain.Key].Suspend()
				return false, nil
			}
			return false, err
		}
		s.sessions[chain.Key] = session
	}
	if a := s.adapters[chain.Key]; a != nil {
		identity := func(observe context.Context) error { return a.Client.VerifyEVM(observe, chain.Key, chain.Genesis) }
		var err error
		if s.rawMaintenance != nil {
			err = s.rawMaintenance.Observe(ctx, chain.Key, RawObserveIdentity, identity)
		} else {
			err = identity(ctx)
		}
		if err != nil {
			return false, err
		}
	}
	return true, s.reanchor(ctx, chain, false)
}

// Revocation and writer cleanup are shared by startup and ordinary polling.
// Reanchor verifies identity again before state collection, so Tick can repair a
// source rejected at startup without ever publishing from an unverified source.
func (s *Service) failChain(ctx context.Context, chain config.Chain, cause error, event string) {
	// A failed source read must revoke other replicas as well. Only later
	// independent fork confirmation may advance canonical epoch.
	if current, ready := s.stores[chain.Key].Current(); ready {
		if err := s.revokeScope(ctx, chain, current); err != nil {
			s.log.Event("error", "scope_revocation_failed", map[string]any{"chain_key": chain.Key, "error_code": err.Error()})
		}
	}
	s.stores[chain.Key].Suspend()
	if err := s.revokeRuntimeSources(ctx, chain, nil); err != nil {
		s.log.Event("error", "source_revocation_failed", map[string]any{"chain_key": chain.Key, "error_code": err.Error()})
	}
	if tracker := s.rawSources[chain.Key]; tracker != nil {
		if err := tracker.quarantine("SOURCE_COLLECTION_FAILED", chain.Key); err != nil {
			s.log.Event("error", "source_dlq_failed", map[string]any{"chain_key": chain.Key, "error_code": "SOURCE_DLQ_WRITE"})
		}
	}
	s.lastError.Add(1)
	if w := s.sessions[chain.Key]; w != nil {
		w.close()
		delete(s.sessions, chain.Key)
	}
	fields := map[string]any{"chain_key": chain.Key, "error_code": cause.Error()}
	var witness *evm.NativeWitnessError
	if errors.As(cause, &witness) {
		fields["witness_phase"], fields["witness_index"] = witness.Phase, witness.WitnessIndex
		fields["block_tag"], fields["witness_error_class"] = witness.HeightTag, witness.Class
	}
	s.log.Event("error", event, fields)
}
