package evm

import (
	"errors"
	"fmt"
	"maps"
	"slices"
	"strings"
	"time"

	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/rpc"
)

// All probes share the proof reader's deadline, byte budget, request policy
// and raw recorder. The header count also bounds work on distinct stale peers.
type nativeHeadProbe struct {
	reader      *nativeProofReader
	headersLeft int
	maxAgeMS    uint64
}

func (p *nativeHeadProbe) header(tag string) (ChainHeaderCommitment, error) {
	var empty ChainHeaderCommitment
	if p.headersLeft == 0 {
		return empty, errors.New("NATIVE_HEAD_HEADER_BUDGET")
	}
	p.headersLeft--
	raw, err := p.reader.call("eth_getBlockByNumber", []any{tag, false})
	if err != nil {
		return empty, err
	}
	h, err := VerifyChainHeaderJSON(p.reader.ctx, p.reader.chainKey, raw)
	if err == nil && strings.HasPrefix(tag, "0x") && h.Block.Number != tag {
		return empty, errors.New("NATIVE_PROOF_HEIGHT")
	}
	return h, err
}

type nativeHeadView struct {
	tip     ChainHeaderCommitment
	settled RawBlock
}

// Two verified observations at one height must agree, even when a server
// changes its answer between a tag and a subsequent numeric lookup.
func (v nativeHeadView) remember(seen map[string]string) error {
	for _, b := range []RawBlock{v.tip.Block, v.settled} {
		if hash, ok := seen[b.Number]; ok && hash != b.Hash {
			return errors.New("NATIVE_HEAD_PROVIDER_DISAGREEMENT")
		}
		seen[b.Number] = b.Hash
	}
	return nil
}

func movingHeadTag(tag string) bool {
	return tag == "latest" || tag == "safe" || tag == "finalized"
}

func (p *nativeHeadProbe) observe(tag string) (nativeHeadView, error) {
	tip, err := p.header(tag)
	view := nativeHeadView{tip: tip, settled: tip.Block}
	if err != nil {
		return view, err
	}
	check := func(block RawBlock) error {
		ms, err := checkedEVMTimestampMS(block.Timestamp)
		if err != nil {
			return err
		}
		return bootstrap.CheckTimestampMS(ms, uint64(time.Now().UnixMilli()), p.maxAgeMS)
	}
	if err = check(tip.Block); err != nil {
		return view, err
	}
	if tip.SettledHeight != nil {
		number := fmt.Sprintf("0x%x", *tip.SettledHeight)
		if number != tip.Block.Number {
			settled, err := p.header(number)
			if err != nil {
				return view, err
			}
			view.settled = settled.Block
			return view, check(view.settled)
		}
	}
	return view, nil
}

func (p *nativeHeadProbe) confirm(number, hash string) error {
	h, err := p.header(number)
	if err != nil {
		return err
	}
	if h.Block.Hash != hash {
		return errors.New("NATIVE_HEAD_PROVIDER_DISAGREEMENT")
	}
	return nil
}

// Only a verified but stale moving tip can trigger selection of another
// endpoint's tag. Never replace a signed/numeric target, an invalid proof or
// a future clock. The original provider still supplies the full block, and
// the usual header/body/receipt witnesses and final re-anchor still run.
func (a *Adapter) selectNativeHead(r *nativeProofReader, tag string) (ChainHeaderCommitment, string, error) {
	var empty ChainHeaderCommitment
	probe := nativeHeadProbe{reader: r, headersLeft: r.limits.MaxHeaders, maxAgeMS: a.OnlineMaxAgeMS}
	if a.OnlineMaxAgeMS == 0 || !movingHeadTag(tag) {
		h, err := probe.header(tag)
		expected := h.Block.Hash
		if h.SettledHeight != nil {
			expected = ""
		}
		return h, expected, err
	}
	view, err := probe.observe(tag)
	if err == nil {
		return view.tip, view.settled.Hash, nil
	}
	if err != bootstrap.ErrBaselineTooOld {
		return empty, "", err
	}

	seen := map[string]string{}
	if err := view.remember(seen); err != nil {
		return empty, "", err
	}
	primary := r.client
	defer func() { r.client = primary }()
	_, peers := a.Client.ProviderClients()
	for _, peer := range peers {
		if peer.URLs[0] == primary.URLs[0] {
			continue
		}
		err = r.bindProvider(peer, nil)
		if err == nil {
			view, err = probe.observe(tag)
		}
		if err == bootstrap.ErrBaselineTooOld {
			if err = view.remember(seen); err != nil {
				return empty, "", err
			}
			continue
		}
		if err != nil {
			switch rpc.Classify(err) {
			case rpc.RateLimited, rpc.Transient, rpc.Unavailable:
				continue
			}
			return empty, "", err
		}
		freshHeight, _ := canonicalIndex(view.tip.Block.Number)
		staleHeights := slices.Sorted(maps.Keys(seen))
		if err = view.remember(seen); err != nil {
			return empty, "", err
		}
		for _, height := range staleHeights {
			n, _ := canonicalIndex(height)
			if n > freshHeight {
				return empty, "", errors.New("NATIVE_HEAD_PROVIDER_DISAGREEMENT")
			}
			if err = probe.confirm(height, seen[height]); err != nil {
				return empty, "", err
			}
		}
		r.client = primary
		if err = probe.confirm(view.tip.Block.Number, view.tip.Block.Hash); err != nil {
			return empty, "", err
		}
		return view.tip, view.settled.Hash, nil
	}
	if err := r.ctx.Err(); err != nil {
		return empty, "", err
	}
	return empty, "", bootstrap.ErrBaselineTooOld
}
