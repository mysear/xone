package service

import (
	"context"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
)

// FrozenCatalogScope is a detached selection of admitted identities and current
// state. It is input to scope preparation, never authorization to publish a
// reducer generation. ValidateFrozen must run again under the chain actor just
// before that actor's owner-prepared/journal publication barrier.
type FrozenCatalogScope struct {
	Fence    CatalogFence
	Anchor   ingest.CatalogAnchor
	Revision uint64
	Entries  []RuntimeCatalogEntry
}

func (f FrozenCatalogScope) Configs() []config.PoolConfig {
	out := make([]config.PoolConfig, 0, len(f.Entries))
	for _, e := range f.Entries {
		p := e.Config
		if p.V2 != nil {
			v := *p.V2
			p.V2 = &v
		}
		if p.V3 != nil {
			v := *p.V3
			p.V3 = &v
		}
		out = append(out, p)
	}
	return out
}
func (r *CatalogRuntime) Freeze(ctx context.Context, f CatalogFence, ids []string) (FrozenCatalogScope, error) {
	if ctx.Err() != nil {
		return FrozenCatalogScope{}, ctx.Err()
	}
	s := r.CatalogSnapshot(ingest.CatalogOnline)
	configs, err := CatalogConfigs(s, f, ids)
	if err != nil {
		return FrozenCatalogScope{}, err
	}
	selected := make(map[string]RuntimeCatalogEntry, len(s.Entries))
	for _, e := range s.Entries {
		selected[e.Config.ID] = e
	}
	frozen := FrozenCatalogScope{Fence: s.Fence, Anchor: s.Anchor, Revision: s.Revision, Entries: make([]RuntimeCatalogEntry, 0, len(configs))}
	for _, p := range configs {
		frozen.Entries = append(frozen.Entries, selected[p.ID])
	}
	if err = r.ValidateFrozen(ctx, frozen); err != nil {
		return FrozenCatalogScope{}, err
	}
	return frozen, nil
}
func (r *CatalogRuntime) ValidateFrozen(ctx context.Context, f FrozenCatalogScope) error {
	if ctx.Err() != nil {
		return ctx.Err()
	}
	if len(f.Entries) == 0 || len(f.Entries) > 1024 {
		return ingest.ErrFactoryAdmission
	}
	if err := r.check(ctx, f.Revision); err != nil {
		return err
	}
	s := r.CatalogSnapshot(ingest.CatalogOnline)
	if !s.Ready || s.Fence != f.Fence || s.Anchor != f.Anchor || s.Revision != f.Revision {
		return ingest.ErrFactoryStale
	}
	current := map[string]RuntimeCatalogEntry{}
	for _, e := range s.Entries {
		current[e.Config.ID] = e
	}
	seen := map[string]bool{}
	for _, e := range f.Entries {
		if ctx.Err() != nil {
			return ctx.Err()
		}
		live, ok := current[e.Config.ID]
		if !ok || seen[e.Config.ID] {
			return ingest.ErrFactoryAdmission
		}
		seen[e.Config.ID] = true
		expected, err := domain.Hash(live)
		if err != nil {
			return err
		}
		actual, err := domain.Hash(e)
		if err != nil {
			return err
		}
		if actual != expected {
			return ingest.ErrFactoryAdmission
		}
	}
	return r.check(ctx, f.Revision)
}
