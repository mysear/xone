package evm

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/provenance"
)

// readState has already verified the block body and receipts. The sealed beacon
// proof stays with this read through the final canonical block recheck.
func (a *Adapter) verifyOnlineOPOrigin(ctx context.Context, b RawBlock, now func() time.Time) (func() error, error) {
	p := a.Chain.OPOrigin
	if p == nil || a.Chain.Key != "evm:10" || b.NativeBodyProof == nil || b.nativeSession == nil || b.NativeCoverage == nil || len(b.NativeBodyProof.RawTransactions) == 0 {
		return nil, errors.New("OP_ORIGIN_NATIVE_BODY_REQUIRED")
	}
	timestamp, err := canonicalIndex(b.Timestamp)
	if err != nil {
		return nil, err
	}
	_, origin, err := parseOPOrigin(common.FromHex(b.NativeBodyProof.RawTransactions[0]), timestamp)
	if err != nil {
		return nil, err
	}
	proof, err := provenance.ReadOnlineBeaconProof(a.OPOriginProofFile, p.CheckpointRoot)
	if err != nil {
		return nil, err
	}
	fresh := func() error {
		if err := ctx.Err(); err != nil {
			return err
		}
		return proof.CheckOnlineAge(now(), p.MaxCheckpointAgeMS, a.OPOriginMaxAgeMS)
	}
	if err = fresh(); err != nil {
		return nil, err
	}
	head, hash := proof.ExecutionBlock()
	session := b.nativeSession
	session.mu.Lock()
	defer session.mu.Unlock()
	limits, err := session.available()
	if err != nil {
		return nil, err
	}
	if origin.Number > head || head-origin.Number >= uint64(limits.MaxHeaders) || head-origin.Number > 128 {
		return nil, errors.New("OP_ORIGIN_ANCESTRY_WINDOW")
	}
	genesis, err := knownBodyGenesis("evm:1")
	if err != nil {
		return nil, err
	}
	l1 := Adapter{Client: a.Client, Chain: config.Chain{Key: "evm:1", Genesis: genesis, BlockIntegrity: ChainNativeCommitmentsV2}}
	reader, err := openNativeProofReader(ctx, &l1, limits, p.L1RPCURL, nil)
	if err != nil {
		session.reject()
		return nil, err
	}
	defer reader.cancel()
	err = verifyOriginAncestry(ctx, head, hash, origin, func(height uint64) (json.RawMessage, error) {
		return reader.call("eth_getBlockByNumber", []any{fmt.Sprintf("0x%x", height), false})
	})
	if err != nil {
		session.reject()
		return nil, err
	}
	if err = session.consume(limits.MaxBytes - reader.budget.remaining); err != nil {
		return nil, err
	}
	return fresh, nil
}
