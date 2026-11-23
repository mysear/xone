// replay-beacon-window runs the normal EVM decoder against recorded-response
// servers. It supports a signed Ethereum head or an explicitly numbered window
// whose provider finality observations are checked by the caller.
package main

import (
	"context"
	"crypto/sha256"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"net/url"
	"os"
	"strconv"
	"time"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/provenance"
	"xone/chain-parser-go/internal/rpc"
)

type observation struct {
	Block  domain.BlockRef `json:"block"`
	Digest string          `json:"staged_sha256"`
}

type windowReport struct {
	SignedHeight uint64        `json:"signed_height"`
	SignedHash   string        `json:"signed_hash"`
	Rows         []observation `json:"rows"`
}

func checkReplayEndpoints(chain config.Chain) error {
	urls := append([]string(nil), chain.RPCURLs...)
	if chain.ProofRPCURL != "" {
		urls = append(urls, chain.ProofRPCURL)
	}
	for _, raw := range urls {
		u, err := url.Parse(raw)
		if err != nil || u.Scheme != "http" || u.Hostname() != "127.0.0.1" ||
			u.User != nil || u.Path != "" || u.ForceQuery || u.RawQuery != "" || u.Fragment != "" {
			return errors.New("REPLAY_LOOPBACK_REQUIRED")
		}
		port, err := strconv.Atoi(u.Port())
		if err != nil || port < 1 || port > 65535 || raw != "http://127.0.0.1:"+strconv.Itoa(port) {
			return errors.New("REPLAY_LOOPBACK_REQUIRED")
		}
	}
	return nil
}

func stagedDigest(block domain.BlockRef, pools []domain.Pool, swaps []domain.SwapFact) (string, error) {
	raw, err := json.Marshal(struct {
		Block domain.BlockRef   `json:"block"`
		Pools []domain.Pool     `json:"pools"`
		Swaps []domain.SwapFact `json:"swaps"`
	}{block, pools, swaps})
	if err != nil {
		return "", err
	}
	return fmt.Sprintf("%x", sha256.Sum256(raw)), nil
}

func replay(ctx context.Context, cfg *config.Snapshot, base, last uint64) (*windowReport, error) {
	if len(cfg.Config.Chains) != 1 {
		return nil, errors.New("REPLAY_MAINNET_PROFILE")
	}
	chain := cfg.Config.Chains[0]
	policy := chain.BeaconFinality
	if chain.Key != "evm:1" || policy == nil || len(chain.RPCURLs) != 2 || chain.RPCURLs[0] == chain.RPCURLs[1] {
		return nil, errors.New("REPLAY_MAINNET_PROFILE")
	}
	if err := checkReplayEndpoints(chain); err != nil {
		return nil, err
	}
	proof, err := provenance.ReadOnlineBeaconProof(cfg.Resolve(policy.ProofFile), policy.CheckpointRoot)
	if err != nil {
		return nil, err
	}
	height, hash := proof.ExecutionBlock()
	if base == 0 || base > last || last > height || height-base > 128 || height-base > cfg.Config.Startup.MaxTail {
		return nil, errors.New("REPLAY_WINDOW")
	}
	client := rpc.New(chain.RPCURLs, time.Duration(cfg.Config.Journal.TimeoutMS)*time.Millisecond, cfg.Config.Journal.MaxBytes)
	defer client.HTTP.CloseIdleConnections()
	adapter := &evm.Adapter{Chain: chain, Client: client}
	reader := evm.BeaconAncestorReader{Adapter: adapter}
	// The owner reads the signed tip with the native body path and the intervening ancestors as headers.
	expected := map[uint64]string{height: hash}
	for h := height; h > base; h-- {
		var ref domain.BlockRef
		tag := fmt.Sprintf("0x%x", h)
		if h == height {
			ref, err = adapter.Header(ctx, tag)
		} else {
			ref, err = reader.Header(ctx, tag)
		}
		if err != nil {
			return nil, fmt.Errorf("ancestor %d: %w", h, err)
		}
		if ref.Number != h || ref.Hash != expected[h] {
			return nil, errors.New("REPLAY_ANCESTRY")
		}
		expected[h-1] = ref.ParentHash
	}
	var rows []observation
	for h := base; ; h++ {
		block, pools, swaps, err := adapter.ReadComplete(ctx, fmt.Sprintf("0x%x", h))
		if err != nil {
			return nil, fmt.Errorf("decode %d: %w", h, err)
		}
		if block.Number != h || block.Hash != expected[h] {
			return nil, errors.New("REPLAY_BLOCK")
		}
		digest, err := stagedDigest(block, pools, swaps)
		if err != nil {
			return nil, err
		}
		rows = append(rows, observation{block, digest})
		if h == last {
			break
		}
	}
	return &windowReport{SignedHeight: height, SignedHash: hash, Rows: rows}, nil
}
func run() error {
	path := flag.String("config", "", "local replay config")
	base := flag.Uint64("first", 0, "first height")
	last := flag.Uint64("last", 0, "last height")
	providerFinalized := flag.Bool("provider-finalized", false, "decode a numbered window from a provider-finalized capture on loopback")
	flag.Parse()
	cfg, err := config.Load(*path, "chain-parser-go")
	if err != nil {
		return err
	}
	ctx, cancel := context.WithTimeout(context.Background(), 10*time.Minute)
	defer cancel()
	if *providerFinalized {
		window, err := replayProviderWindow(ctx, cfg, *base, *last)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(window)
	}
	rows, err := replay(ctx, cfg, *base, *last)
	if err != nil {
		return err
	}
	return json.NewEncoder(os.Stdout).Encode(rows)
}
func main() {
	if err := run(); err != nil {
		fmt.Fprintln(os.Stderr, err)
		os.Exit(1)
	}
}
