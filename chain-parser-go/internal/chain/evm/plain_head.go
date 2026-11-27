package evm

import (
	"context"
	"errors"
	"fmt"
	"time"

	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func (a *Adapter) selectFreshPlainHead(ctx context.Context, tag, originalProvider string, original domain.BlockRef) (RawBlock, error) {
	_, providers := a.Client.ProviderClients()
	anchors := []domain.BlockRef{original}
	for _, provider := range providers {
		if provider.Providers()[0].ID == originalProvider {
			continue
		}
		if err := provider.VerifyEVM(ctx, a.Chain.Key, a.Chain.Genesis); err != nil {
			switch rpc.Classify(err) {
			case rpc.IdentityMismatch, rpc.Transient, rpc.Unavailable, rpc.RateLimited:
				continue
			}
			return RawBlock{}, err
		}
		peer := *a
		peer.Client = provider
		block, _, err := peer.plainBlock(ctx, tag)
		if err != nil {
			switch rpc.Classify(err) {
			case rpc.Transient, rpc.Unavailable, rpc.RateLimited:
				continue
			}
			return RawBlock{}, err
		}
		ref, err := blockReference(a.Chain.Key, block)
		if err != nil {
			return RawBlock{}, err
		}
		switch err = bootstrap.CheckTimestampMS(ref.TimestampMS, uint64(time.Now().UnixMilli()), a.OnlineMaxAgeMS); err {
		case bootstrap.ErrBaselineTooOld:
			anchors = append(anchors, ref)
			continue
		case nil:
		default:
			return RawBlock{}, err
		}
		for _, anchor := range anchors {
			if ref.Number < anchor.Number {
				return RawBlock{}, errors.New("HEAD_PROVIDER_DISAGREEMENT")
			}
			confirmed := ref
			if ref.Number > anchor.Number {
				confirmed, err = peer.Header(ctx, fmt.Sprintf("0x%x", anchor.Number))
				if err != nil {
					return RawBlock{}, err
				}
			}
			if confirmed.Number != anchor.Number || confirmed.TimestampMS != anchor.TimestampMS ||
				!rpc.SameEVMHash(confirmed.Hash, anchor.Hash) || !rpc.SameEVMHash(confirmed.ParentHash, anchor.ParentHash) {
				return RawBlock{}, errors.New("HEAD_PROVIDER_DISAGREEMENT")
			}
		}
		return block, nil
	}
	if err := ctx.Err(); err != nil {
		return RawBlock{}, err
	}
	return RawBlock{}, bootstrap.ErrBaselineTooOld
}
