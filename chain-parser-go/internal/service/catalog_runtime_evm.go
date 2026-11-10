package service

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"strings"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/ingest"
	"xone/chain-parser-go/internal/rpc"
)

func (r *CatalogRuntime) methodControls(ctx context.Context, c *rpc.Client, a ingest.CatalogAnchor) error {
	s := r.options.Config.Sources[0]
	if err := r.verifyFactoryCode(ctx, c, s.Address, a.Hash); err != nil {
		return err
	}
	data := "0x574f2ba3" // original V2 allPairsLength()
	if s.Version == ingest.UniswapV3Factory {
		data = "0x22afcccb" + fmt.Sprintf("%064x", 3000)
	} // feeAmountTickSpacing(uint24)
	params := map[string]string{"to": s.Address, "data": data}
	var value string
	if err := c.Call(ctx, "eth_call", []any{params, map[string]any{"blockHash": a.Hash, "requireCanonical": true}}, &value); err != nil {
		return err
	}
	if _, err := catalogWord(value); err != nil {
		return err
	}
	var random [32]byte
	if _, err := rand.Read(random[:]); err != nil {
		return err
	}
	negative := map[string]any{"blockHash": "0x" + hex.EncodeToString(random[:]), "requireCanonical": true}
	for _, test := range []struct {
		method string
		target any
	}{{"eth_getCode", s.Address}, {"eth_call", params}} {
		var ignored string
		err := c.Call(ctx, test.method, []any{test.target, negative}, &ignored)
		if err == nil {
			return errors.New("CATALOG_BLOCK_SELECTOR_IGNORED")
		}
		if err.Error() != "RPC_ERROR_-32000" && err.Error() != "RPC_ERROR_-32001" {
			return err
		}
	}
	return nil
}
func catalogWord(value string) ([]byte, error) {
	if !strings.HasPrefix(value, "0x") || len(value) != 66 {
		return nil, ingest.ErrFactoryAdmission
	}
	raw, err := hex.DecodeString(value[2:])
	if err != nil {
		return nil, ingest.ErrFactoryAdmission
	}
	return raw, nil
}
func catalogAddressWord(value string) (string, error) {
	raw, err := catalogWord(value)
	if err != nil {
		return "", err
	}
	if strings.Trim(hex.EncodeToString(raw[:12]), "0") != "" {
		return "", ingest.ErrFactoryAdmission
	}
	return "0x" + hex.EncodeToString(raw[12:]), nil
}
func (r *CatalogRuntime) readCurrent(ctx context.Context, client *rpc.Client, c ingest.FactoryCandidate, a ingest.CatalogAnchor) (RuntimeCatalogEntry, error) {
	fail := func(err error) (RuntimeCatalogEntry, error) { return RuntimeCatalogEntry{}, err }
	s, err := r.deployment(c.Factory)
	if err != nil {
		return fail(err)
	}
	if s.Version != c.Version || s.ProvenanceID != c.ProvenanceID {
		return fail(ingest.ErrFactorySource)
	}
	if err = r.verifyFactoryCode(ctx, client, c.Factory, a.Hash); err != nil {
		return fail(err)
	}
	poolConfig := config.PoolConfig{ID: c.Pool, Protocol: c.Version, Token0: c.Token0, Token1: c.Token1}
	var templateBinding *V3TemplateBinding
	data := "0xe6a43905" + strings.Repeat("0", 24) + c.Token0[2:] + strings.Repeat("0", 24) + c.Token1[2:]
	if c.Version == ingest.UniswapV2Factory {
		poolConfig.FeeNumerator = 3
		poolConfig.FeeDenominator = 1000
		poolConfig.V2 = &config.V2PoolConfig{ExpectedFactory: c.Factory, ExpectedCodeSHA256: s.PoolCodeSHA256}
	} else if c.Version == ingest.UniswapV3Factory {
		data = "0x1698ee82" + strings.Repeat("0", 24) + c.Token0[2:] + strings.Repeat("0", 24) + c.Token1[2:] + fmt.Sprintf("%064x", c.Fee)
		poolConfig.FeeNumerator = uint64(c.Fee)
		poolConfig.FeeDenominator = 1000000
		poolSHA := s.PoolCodeSHA256
		if s.PoolTemplate != nil {
			poolSHA, err = r.readOfficialV3Runtime(ctx, client, s, c, a)
			if err != nil {
				return fail(err)
			}
			deployment, err := s.PoolTemplate.Deployment(r.options.Chain.Key, r.options.Chain.Genesis, c.Factory, s.FactoryCodeSHA256)
			if err != nil {
				return fail(err)
			}
			templateBinding = officialV3Binding(*s.PoolTemplate, deployment, poolSHA)

		}
		poolConfig.V3 = &config.V3PoolConfig{ExpectedFactory: c.Factory, ExpectedFactoryCodeSHA256: s.FactoryCodeSHA256, ExpectedCodeSHA256: poolSHA, TickWordRadius: s.TickWordRadius, MaxInitializedTicks: s.MaxInitializedTicks, MaxSteps: s.MaxSteps}
	} else {
		return fail(ingest.ErrFactorySource)
	}
	var member string
	if err = client.Call(ctx, "eth_call", []any{map[string]string{"to": c.Factory, "data": data}, map[string]any{"blockHash": a.Hash, "requireCanonical": true}}, &member); err != nil {
		return fail(err)
	}
	address, err := catalogAddressWord(member)
	if err != nil || address != c.Pool {
		return fail(ingest.ErrFactoryAdmission)
	}
	chain := r.options.Chain
	chain.Pools = []config.PoolConfig{poolConfig}
	adapter := evm.Adapter{Client: client, Chain: chain}
	block, pools, err := adapter.ReadState(ctx, fmt.Sprintf("0x%x", a.Height), false)
	if err != nil {
		return fail(err)
	}
	if block.Number != a.Height || !rpc.SameEVMHash(block.Hash, a.Hash) || len(pools) != 1 {
		return fail(ingest.ErrFactoryStale)
	}
	pool := pools[0]
	if pool.PoolID != c.Pool || pool.Token0 != c.Token0 || pool.Token1 != c.Token1 || pool.Protocol != c.Version {
		return fail(ingest.ErrFactoryAdmission)
	}
	if c.Version == ingest.UniswapV3Factory && (pool.V3 == nil || pool.FeeNumerator != uint64(c.Fee) || pool.V3.TickSpacing != c.TickSpacing) {
		return fail(ingest.ErrFactoryAdmission)
	}
	if err = pool.Validate(); err != nil {
		return fail(err)
	}
	// Existing adapter validation is the authority for reserve/tick/fee state.
	// The digest commits all of that actual state rather than a config or event.
	digest, err := domain.Hash(struct {
		Anchor ingest.CatalogAnchor
		Pool   domain.Pool
	}{a, pool})
	if err != nil {
		return fail(err)
	}
	if !strings.HasPrefix(digest, "0x") {
		digest = "0x" + digest
	}
	return RuntimeCatalogEntry{Admission: ingest.PoolAdmission{Candidate: c, At: a, StateDigest: digest}, Config: poolConfig, Pool: pool, TemplateBinding: templateBinding}, nil
}

// CatalogConfigs returns only admitted entries at the exact owner fence. The
// caller still joins them to one complete reducer snapshot before scope ready.
func CatalogConfigs(s RuntimeCatalogSnapshot, f CatalogFence, ids []string) ([]config.PoolConfig, error) {
	if !s.Ready || s.Namespace != ingest.CatalogOnline || s.Fence != f || s.Anchor.Generation != f.Generation || len(ids) == 0 || len(ids) > 1024 {
		return nil, ingest.ErrFactoryStale
	}
	byID := map[string]config.PoolConfig{}
	for _, entry := range s.Entries {
		byID[entry.Config.ID] = entry.Config
	}
	seen := map[string]bool{}
	configs := make([]config.PoolConfig, 0, len(ids))
	for _, id := range ids {
		p, ok := byID[id]
		if !ok || seen[id] {
			return nil, ingest.ErrFactoryAdmission
		}
		seen[id] = true
		// Deep-copy nested config pointers even when a caller passed its own snapshot.
		if p.V2 != nil {
			v := *p.V2
			p.V2 = &v
		}
		if p.V3 != nil {
			v := *p.V3
			p.V3 = &v
		}
		configs = append(configs, p)
	}
	return configs, nil
}
