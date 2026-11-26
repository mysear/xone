package evm

import (
	"context"
	"encoding/json"
	"errors"
	"math/big"
	"testing"
	"time"

	"github.com/ethereum/go-ethereum/common"
	"github.com/ethereum/go-ethereum/core/types"
	"xone/chain-parser-go/internal/bootstrap"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/rpc"
)

// An empty, locally hashed body also lets ReadComplete exercise receipts, both
// numerical body witnesses and the closing anchor after a moving-tag selection.
func addHeadTestBlock(t *testing.T, f *normalNativeFixture, number int64, parent common.Hash, stamp int64) (string, common.Hash) {
	t.Helper()
	h := &types.Header{Number: big.NewInt(number), ParentHash: parent, UncleHash: types.EmptyUncleHash, Root: types.EmptyRootHash,
		TxHash: types.EmptyTxsHash, ReceiptHash: types.EmptyReceiptsHash, Difficulty: big.NewInt(0), GasLimit: 30000000,
		Time: uint64(stamp), BaseFee: big.NewInt(1), Extra: []byte{}}
	raw, err := json.Marshal(h)
	if err != nil {
		t.Fatal(err)
	}
	var fields map[string]json.RawMessage
	if err = json.Unmarshal(raw, &fields); err != nil {
		t.Fatal(err)
	}
	fields["transactions"] = json.RawMessage("[]")
	raw, err = json.Marshal(fields)
	if err != nil {
		t.Fatal(err)
	}
	key := "0x" + h.Number.Text(16)
	f.headers[key], f.full[key], f.receipts[h.Hash().Hex()] = raw, raw, []Receipt{}
	return key, h.Hash()
}

func newNativeHeadEndpoint() *normalNativeFixture {
	genesis, _ := knownBodyGenesis("evm:1")
	return &normalNativeFixture{chain: config.Chain{Key: "evm:1", Genesis: genesis,
		BlockIntegrity: ChainNativeCommitmentsV2, ReceiptsMode: "bulk-auto",
		NativeProof: &config.NativeProofConfig{MaxBytes: 16 << 20, MaxHeaders: 64, MaxWindowBlocks: 16, MaxTransactions: 4096, MaxLookahead: 32}},
		headers: map[string]json.RawMessage{}, full: map[string]json.RawMessage{}, receipts: map[string][]Receipt{}}
}

func TestOnlineNativeHeadSelection(t *testing.T) {
	for _, tc := range []struct {
		scenario string
		wantErr  string
		peerTags int32
	}{
		{"healthy", "", 0},
		{"stale-primary", "", 1},
		{"safe-tag", "", 1},
		{"latest-tag", "", 1},
		{"record-failed", "RPC_RECORDING_FAILED", 1},
		{"canceled", "context canceled", 1},
		{"byte-limit", "RECEIPT_PROOF_BUDGET", 1},
		{"exact-history", "", 0},
		{"both-stale", "BASELINE_TOO_OLD", 1},
		{"only-duplicate", "BASELINE_TOO_OLD", 0},
		{"wrong-genesis", "BLOCK_BODY_FINGERPRINT", 0},
		{"future-primary", "BASELINE_FUTURE_TIMESTAMP", 0},
		{"future-peer", "BASELINE_FUTURE_TIMESTAMP", 1},
		{"invalid-hash", "BLOCK_INTEGRITY_HEADER_HASH", 1},
		{"same-height-fork", "NATIVE_HEAD_PROVIDER_DISAGREEMENT", 1},
		{"peer-rewrote-old", "NATIVE_HEAD_PROVIDER_DISAGREEMENT", 1},
		{"primary-rewrote-fresh", "NATIVE_HEAD_PROVIDER_DISAGREEMENT", 1},
		{"header-limit", "NATIVE_HEAD_HEADER_BUDGET", 1},
	} {
		t.Run(tc.scenario, func(t *testing.T) {
			first, peer := newNativeHeadEndpoint(), newNativeHeadEndpoint()
			now := time.Now().Unix()
			parent := common.HexToHash(first.chain.Genesis)
			old, oldHash := addHeadTestBlock(t, first, 25000001, parent, now-180)
			fresh, freshHash := addHeadTestBlock(t, first, 25000002, oldHash, now-2)
			addHeadTestBlock(t, peer, 25000001, parent, now-180)
			addHeadTestBlock(t, peer, 25000002, oldHash, now-2)
			first.observed, peer.observed = old, fresh
			tag, expected := "finalized", freshHash
			switch tc.scenario {
			case "healthy":
				first.observed = fresh
			case "safe-tag":
				tag = "safe"
			case "latest-tag":
				tag = "latest"
			case "exact-history":
				tag, expected = old, oldHash
			case "both-stale":
				peer.observed = old
			case "wrong-genesis":
				peer.chain.Genesis = common.Hash{}.Hex()
			case "future-primary":
				first.observed, _ = addHeadTestBlock(t, first, 25000003, freshHash, now+90)
			case "future-peer":
				peer.observed, _ = addHeadTestBlock(t, peer, 25000003, freshHash, now+90)
			case "invalid-hash":
				peer.corruptSettling.Store(true)
			case "same-height-fork":
				peer.observed, _ = addHeadTestBlock(t, peer, 25000001, parent, now-2)
			case "peer-rewrote-old":
				addHeadTestBlock(t, peer, 25000001, parent, now-181)
			case "primary-rewrote-fresh":
				addHeadTestBlock(t, first, 25000002, oldHash, now-3)
			case "header-limit":
				first.chain.NativeProof.MaxHeaders = 2
			}
			primaryURL := first.serve(t).URL
			urls := []string{primaryURL, peer.serve(t).URL}
			if tc.scenario == "only-duplicate" {
				urls[1] = primaryURL
			}
			a := &Adapter{Client: rpc.New(urls, 5*time.Second, 16<<20), Chain: first.chain, OnlineMaxAgeMS: 60000}
			ctx, cancel := context.WithCancel(context.Background())
			defer cancel()
			if tc.scenario == "byte-limit" {
				limits := nativeProofTestLimits()
				reader, err := newNativeProofReader(ctx, a, limits)
				if err != nil {
					t.Fatal(err)
				}
				_, _, err = a.selectNativeHead(reader, tag)
				reader.cancel()
				if err != nil {
					t.Fatal(err)
				}
				a.Chain.NativeProof.MaxBytes = limits.MaxBytes - reader.budget.remaining - 1
				peer.tagReads.Store(0)
			}
			tagCalls := 0
			a.Client.SetRecorder(func(_ context.Context, _, method string, params json.RawMessage, _ []byte) error {
				if method != "eth_getBlockByNumber" {
					return nil
				}
				var p []json.RawMessage
				var observed string
				if err := json.Unmarshal(params, &p); err != nil || len(p) != 2 || json.Unmarshal(p[0], &observed) != nil {
					return errors.New("invalid block request")
				}
				if movingHeadTag(observed) {
					if observed != tag {
						return errors.New("confirmation tag changed")
					}
					tagCalls++
					if tagCalls == 2 {
						switch tc.scenario {
						case "record-failed":
							// A wrapped recorder failure remains fatal even if its own
							// backend happens to report a stale timestamp.
							return bootstrap.ErrBaselineTooOld
						case "canceled":
							cancel()
						}
					}
				}
				return nil
			})
			ref, _, _, err := a.ReadComplete(ctx, tag)
			if tc.wantErr != "" {
				if err == nil || err.Error() != tc.wantErr || first.fullReads.Load()+peer.fullReads.Load() != 0 {
					t.Fatalf("expected %s before body I/O, got %v, full reads %d/%d", tc.wantErr, err, first.fullReads.Load(), peer.fullReads.Load())
				}
			} else if err != nil || ref.Hash != expected.Hex() || first.fullReads.Load() == 0 || peer.fullReads.Load() == 0 {
				t.Fatalf("expected %s with both body witnesses, got %s, error %v", expected, ref.Hash, err)
			}
			if got := peer.tagReads.Load(); got != tc.peerTags {
				t.Fatalf("peer moving-tag reads %d, expected %d", got, tc.peerTags)
			}
		})
	}
}
