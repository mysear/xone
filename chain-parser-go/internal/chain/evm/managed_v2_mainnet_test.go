package evm

import (
	"context"
	"encoding/json"
	"fmt"
	"os"
	"testing"
	"time"

	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/rpc"
)

func TestManagedV2MainnetHistoricalPool(t *testing.T) {
	endpoint := os.Getenv("XONE_MANAGED_V2_ARCHIVE_RPC")
	if endpoint == "" {
		t.Skip("set XONE_MANAGED_V2_ARCHIVE_RPC for the Ethereum mainnet historical check")
	}
	raw, err := os.ReadFile("../../../test/contracts/managed-v2-mainnet-source.json")
	if err != nil {
		t.Fatal(err)
	}
	var pin struct {
		Number       uint64            `json:"block_number"`
		Hash         string            `json:"block_hash"`
		ParentHash   string            `json:"parent_hash"`
		TimestampMS  uint64            `json:"timestamp_ms"`
		Pool         config.PoolConfig `json:"pool"`
		SwapsEnabled bool              `json:"swaps_enabled"`
		Creation     struct {
			BlockHash string `json:"block_hash"`
			TxHash    string `json:"transaction_hash"`
			LogIndex  string `json:"log_index"`
		} `json:"creation"`
	}
	if err = json.Unmarshal(raw, &pin); err != nil {
		t.Fatal(err)
	}
	chain := config.Chain{Key: "evm:1", Pools: []config.PoolConfig{pin.Pool}}
	adapter := Adapter{Chain: chain, Client: rpc.New([]string{endpoint}, 2*time.Minute, 16<<20)}
	factory := pin.Pool.MultiAsset.ManagedV2.ExpectedFactory
	creationTopic := "0x83a48fbcfc991335314e74d0496aab6a1987e992ddc85dddbcc4d6dd6ef2e9fc"
	poolTopic := "0x000000000000000000000000" + pin.Pool.ID[2:]
	filter := map[string]any{
		"blockHash": pin.Creation.BlockHash, "address": factory,
		"topics": []string{creationTopic, poolTopic},
	}
	var logs []Log
	if err := adapter.Client.Call(context.Background(), "eth_getLogs", []any{filter}, &logs); err != nil {
		t.Fatal(err)
	}
	if len(logs) != 1 {
		t.Fatalf("expected one factory creation event, got %d", len(logs))
	}
	log := logs[0]
	if log.BlockHash != pin.Creation.BlockHash || log.TxHash != pin.Creation.TxHash ||
		log.Index != pin.Creation.LogIndex || log.Removed || log.Address != factory ||
		log.Data != "0x" || len(log.Topics) != 2 || log.Topics[0] != creationTopic || log.Topics[1] != poolTopic {
		t.Fatal("factory creation event differs from the pinned deployment")
	}
	expected := domain.BlockRef{ChainKey: chain.Key, Number: pin.Number, Hash: pin.Hash, ParentHash: pin.ParentHash, TimestampMS: pin.TimestampMS}
	ref, pools, err := adapter.ReadState(context.Background(), fmt.Sprintf("0x%x", pin.Number), false)
	if err != nil {
		t.Fatal(err)
	}
	if ref != expected || len(pools) != 1 {
		t.Fatalf("unexpected source block or pool count: %v, %d", ref, len(pools))
	}
	pool := pools[0]
	state := pool.MultiAsset.ManagedV2
	if pool.PoolID != pin.Pool.ID || pool.MultiAsset.PoolID != pin.Pool.MultiAsset.PoolID ||
		len(pool.MultiAsset.Assets) != 6 || state.SwapsEnabled != pin.SwapsEnabled {
		t.Fatal("unexpected deployed pool state")
	}
	if err = pool.Validate(); err != nil {
		t.Fatal(err)
	}
}
