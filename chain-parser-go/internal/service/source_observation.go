package service

import (
	"context"
	"errors"
	"time"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func (s *Service) readObservedHeader(ctx context.Context, ch config.Chain, reader interface {
	Header(context.Context, string) (domain.BlockRef, error)
}, tag string) (domain.BlockRef, error) {
	var block domain.BlockRef
	read := func(observe context.Context) error {
		var err error
		block, err = readHeaderWithLease(observe, reader, s.sessions[ch.Key], tag)
		return err
	}
	if s.rawMaintenance != nil {
		err := s.rawMaintenance.Observe(ctx, ch.Key, RawObserveHeader, read)
		return block, err
	}
	err := read(ctx)
	return block, err
}

// An empty catalog can establish chain identity/head observations, but has no
// qualified pool Snapshot. The observation ticket owns only these HTTP inputs.
func (s *Service) observeChainHeader(ctx context.Context, chain config.Chain) error {
	base := s.adapters[chain.Key]
	if base == nil {
		return errors.New("RUNTIME_NORMAL_ADAPTER_REQUIRED")
	}
	adapter := base.CopyWithChain(chain)
	var head domain.BlockRef
	observe := func(source context.Context) error {
		if err := adapter.Client.VerifyEVM(source, chain.Key, chain.Genesis); err != nil {
			return err
		}
		var err error
		head, err = readHeaderWithLease(source, adapter, s.sessions[chain.Key], chain.Confirmation)
		if err != nil {
			return err
		}
		return bootstrap.CheckFresh(domain.Snapshot{Reference: domain.SnapshotRef{Block: head}}, uint64(time.Now().UnixMilli()), s.cfg.Config.Startup.MaxAge)
	}
	if err := s.rawMaintenance.Observe(ctx, chain.Key, RawObserveChainBaseline, observe); err != nil {
		return err
	}
	s.log.Event("info", "chain_header_observed", map[string]any{"chain_key": chain.Key, "height": head.Number, "pool_scope_ready": false})
	return nil
}
