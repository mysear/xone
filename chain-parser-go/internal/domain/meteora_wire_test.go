package domain

import (
	"testing"
	pb "xone/common/protoc/golang/dexpb"
)

func TestMeteoraWireCannotBeSilentlyDroppedByEVMDecoder(t *testing.T) {
	wire := &pb.OnlineSnapshot{
		Reference: &pb.SnapshotRef{
			Block:          &pb.BlockRef{ChainKey: "evm:1", Hash: make([]byte, 32), ParentHash: make([]byte, 32)},
			AdapterSetHash: make([]byte, 32), ScopeHash: make([]byte, 32),
		},
		Pools: []*pb.PoolState{{Protocol: "uniswap-v2", Reserve0: &pb.Amount{}, Reserve1: &pb.Amount{}, Meteora: &pb.MeteoraState{}}},
	}
	if _, err := SnapshotFromProto(wire); err == nil || err.Error() != "METEORA_REQUIRES_SOLANA_DECODER" {
		t.Fatalf("unsupported model field was lost: %v", err)
	}
}
