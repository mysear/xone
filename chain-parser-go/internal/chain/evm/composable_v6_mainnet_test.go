package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"reflect"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestComposableV6MainnetHistoricalPool(t *testing.T) {
	endpoint := os.Getenv("XONE_COMPOSABLE_V6_ARCHIVE_RPC")
	if endpoint == "" {
		t.Skip("set XONE_COMPOSABLE_V6_ARCHIVE_RPC for the Ethereum mainnet historical check")
	}
	raw, err := os.ReadFile("../../../test/contracts/composable-v6-mainnet-source.json")
	if err != nil {
		t.Fatal(err)
	}
	var pin struct {
		Factory     string            `json:"factory"`
		Number      uint64            `json:"block_number"`
		Hash        string            `json:"block_hash"`
		ParentHash  string            `json:"parent_hash"`
		TimestampMS uint64            `json:"timestamp_ms"`
		Pool        config.PoolConfig `json:"pool"`
		Creation    struct {
			Number uint64 `json:"block_number"`
			Hash   string `json:"block_hash"`
			TxHash string `json:"transaction_hash"`
			Index  string `json:"log_index"`
		}
		Observed struct {
			Balances     []domain.Amount
			Scales       []domain.Amount
			Amp          domain.Amount
			ActualSupply domain.Amount                    `json:"actual_supply"`
			State        domain.BalancerComposableV6State `json:"composable_v6"`
		}
	}
	if err := json.Unmarshal(raw, &pin); err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	chain := config.Chain{Key: "evm:1", Pools: []config.PoolConfig{pin.Pool}}
	adapter := Adapter{Chain: chain, Client: rpc.New([]string{endpoint}, 2*time.Minute, 16<<20)}
	creation, err := adapter.Block(ctx, fmt.Sprintf("0x%x", pin.Creation.Number))
	if err != nil || creation.Hash != pin.Creation.Hash {
		t.Fatalf("creation block differs from pin: %v", err)
	}
	topics := []string{
		"0x83a48fbcfc991335314e74d0496aab6a1987e992ddc85dddbcc4d6dd6ef2e9fc",
		"0x000000000000000000000000" + pin.Pool.ID[2:],
	}
	filter := map[string]any{"blockHash": creation.Hash, "address": pin.Factory, "topics": topics}
	var logs []Log
	if err := adapter.Client.Call(ctx, "eth_getLogs", []any{filter}, &logs); err != nil {
		t.Fatal(err)
	}
	if len(logs) != 1 {
		t.Fatalf("expected one factory creation event, got %d", len(logs))
	}
	log := logs[0]
	if log.BlockHash != creation.Hash || log.TxHash != pin.Creation.TxHash || log.Index != pin.Creation.Index ||
		log.Removed || log.Address != pin.Factory || log.Data != "0x" || !reflect.DeepEqual(log.Topics, topics) {
		t.Fatal("factory creation event differs from the pinned deployment")
	}
	want := domain.BlockRef{ChainKey: chain.Key, Number: pin.Number, Hash: pin.Hash, ParentHash: pin.ParentHash, TimestampMS: pin.TimestampMS}
	ref, pools, err := adapter.ReadState(ctx, fmt.Sprintf("0x%x", pin.Number), false)
	if err != nil || ref != want || len(pools) != 1 {
		t.Fatalf("fixed-block parse: %v, %v, %d pools", err, ref, len(pools))
	}
	pool := pools[0]
	state := pool.MultiAsset
	observed := pin.Observed
	if pool.AdapterVersion != domain.ComposableV6CachedRatesAdapter || pool.FeeNumerator != pin.Pool.FeeNumerator ||
		state == nil || state.Amplification != observed.Amp || !reflect.DeepEqual(state.ComposableV6, &observed.State) ||
		len(state.Assets) != len(observed.Balances) || len(state.Assets) != len(observed.Scales) {
		t.Fatal("unexpected deployed pool state")
	}
	for i, asset := range state.Assets {
		if asset.Balance != observed.Balances[i] || asset.Scaling != observed.Scales[i] {
			t.Fatalf("asset %d differs from pin", i)
		}
	}
	anchor := map[string]any{"blockHash": ref.Hash, "requireCanonical": true}
	word, err := adapter.stableCall(ctx, pool.PoolID, "0x876f303b", anchor, 1) // getActualSupply()
	if err != nil {
		t.Fatal(err)
	}
	actual, err := wordUint(word, 256)
	if err != nil || actual.String() != string(observed.ActualSupply) {
		t.Fatalf("actual supply differs from pin: %v, %v", err, actual)
	}
}
