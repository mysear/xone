package rpc

import (
	"context"
	"os"
	"testing"
	"time"
)

func TestPublicEthereumHistoricalCapabilities(t *testing.T) {
	if os.Getenv("XONE_PUBLIC_EVM_HEADS") != "1" {
		t.Skip("requires live Ethereum mainnet RPC")
	}
	const genesis = "0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3"
	ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
	defer cancel()
	client := New([]string{"https://ethereum-rpc.publicnode.com", "https://eth.drpc.org"}, 24*time.Second, 2<<20)
	for _, sample := range []struct {
		height uint64
		hash   string
	}{
		{0, genesis},
		{15000000, "0x9a71a95be3fe957457b11817587e5af4c7e24836d5b383c430ff25b9286a457f"},
		{22000000, "0xfc429da12e414c0e8348fd9bb760b1a118b4dcf73419f021d4ccc2c3b793e05c"},
		{26510136, "0xa36de1658d366dd0073719b543bff4f624913f8bb402dde41f5f9fd6248615bb"},
	} {
		reports, err := client.ProbeEVM(ctx, ProbeRequest{
			Chain: "evm:1", Genesis: genesis, HistoricalHeight: sample.height, HistoricalHash: sample.hash,
			Account: "0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48",
		})
		if err != nil || len(reports) != len(client.URLs) {
			t.Errorf("height %d: %d providers (%v)", sample.height, len(reports), err)
		}
		for _, report := range reports {
			t.Logf("height=%d provider=%s head=%s header=%+v balance=%+v hash_pin=%+v safe=%+v finalized=%+v",
				sample.height, report.ProviderID, report.ObservedHead, report.HistoricalBlock,
				report.Historical, report.HashPinned, report.Safe, report.Finalized)
		}
	}
}
