package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"time"
	"xone/chain-parser-go/internal/app"
	"xone/chain-parser-go/internal/chain/evm"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/provenance"
)

func beaconRelease(cfg *config.Snapshot) (provenance.BeaconRelease, error) {
	for _, chain := range cfg.Config.Chains {
		if b := chain.BeaconFinality; b != nil {
			return provenance.BeaconRelease{Root: b.CheckpointRoot, ProofFile: cfg.Resolve(b.ProofFile), CheckpointAgeMS: b.MaxCheckpointAgeMS, FinalizedAgeMS: cfg.Config.Startup.MaxAge}, nil
		}
	}
	for _, chain := range cfg.Config.Chains {
		if b := chain.OPOrigin; b != nil {
			return provenance.BeaconRelease{Root: b.CheckpointRoot, ProofFile: cfg.Resolve(b.ProofFile), CheckpointAgeMS: b.MaxCheckpointAgeMS, FinalizedAgeMS: cfg.Config.Startup.MaxAge}, nil
		}
	}
	return provenance.BeaconRelease{}, fmt.Errorf("CONFIG_BEACON_FINALITY_REQUIRED")
}

func run() error {
	flags := flag.NewFlagSet("chain-parser-go", flag.ContinueOnError)
	path := flags.String("config", "", "environment configuration file")
	check := flags.Bool("check-config", false, "validate configuration and exit")
	refresh := flags.String("refresh-beacon-proof", "", "verify and publish an owner proof from this HTTPS beacon API, then exit")
	rotate := flags.String("prepare-beacon-rotation", "", "prepare a new proof file for this successor owner configuration")
	bootstrap := flags.String("beacon-bootstrap", "", "bootstrap response for the successor checkpoint root")
	span := flags.String("refresh-op-span", "", "read authenticated L1 submission evidence and publish the corresponding OP span observation")
	latestSpan := flags.String("refresh-latest-op-span", "", "collect the latest finalized OP blob submission using this beacon API and refresh the owner observation")
	if e := flags.Parse(os.Args[1:]); e != nil {
		return fmt.Errorf("INVALID_CLI")
	}
	if flags.NArg() != 0 || *path == "" {
		return fmt.Errorf("CONFIG_ARGUMENT_REQUIRED")
	}
	cfg, e := config.Load(*path, "chain-parser-go")
	if e != nil {
		return e
	}
	if *latestSpan != "" {
		if *check || *span != "" || *refresh != "" || *rotate != "" || *bootstrap != "" {
			return fmt.Errorf("INVALID_CLI")
		}
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
		defer cancel()
		report, err := evm.RefreshLatestOPSpan(ctx, cfg, *latestSpan)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(report)
	}
	if *span != "" {
		if *check || *refresh != "" || *rotate != "" || *bootstrap != "" {
			return fmt.Errorf("INVALID_CLI")
		}
		file, err := os.Open(*span)
		if err != nil {
			return err
		}
		defer file.Close()
		const max = 32 << 20
		raw, err := io.ReadAll(io.LimitReader(file, max+1))
		if err != nil {
			return err
		}
		if len(raw) > max {
			return fmt.Errorf("OP_BATCH_BYTE_LIMIT")
		}
		var submissions []evm.OPChannelSubmission
		if err = json.Unmarshal(raw, &submissions); err != nil {
			return err
		}
		ctx, cancel := context.WithTimeout(context.Background(), 2*time.Minute)
		defer cancel()
		report, err := evm.RefreshOwnerOPSpan(ctx, cfg, submissions)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(report)
	}
	if (*rotate == "") != (*bootstrap == "") || *rotate != "" && (*check || *refresh != "") {
		return fmt.Errorf("INVALID_CLI")
	}
	if *rotate != "" {
		successor, err := config.Load(*rotate, "chain-parser-go")
		if err != nil {
			return err
		}
		old, err := beaconRelease(cfg)
		if err != nil {
			return err
		}
		next, err := beaconRelease(successor)
		if err != nil {
			return err
		}
		ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
		defer cancel()
		result, err := provenance.PrepareBeaconRotation(ctx, old, next, *bootstrap)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(result)
	}
	if *refresh != "" {
		if *check {
			return fmt.Errorf("INVALID_CLI")
		}
		release, err := beaconRelease(cfg)
		if err != nil {
			return err
		}
		ctx, cancel := context.WithTimeout(context.Background(), time.Minute)
		defer cancel()
		result, err := provenance.RefreshBeaconProof(ctx, *refresh, release.Root, release.ProofFile, release.CheckpointAgeMS, release.FinalizedAgeMS)
		if err != nil {
			return err
		}
		return json.NewEncoder(os.Stdout).Encode(result)
	}
	if *check {
		fmt.Printf("{\"service\":\"chain-parser-go\",\"valid\":true,\"public_hash\":\"%s\",\"economic_hash\":\"%s\"}\n", cfg.PublicHash, cfg.EconomicHash)
		return nil
	}
	return app.Run(cfg, nil)
}

func main() {
	if e := run(); e != nil {
		_ = json.NewEncoder(os.Stderr).Encode(map[string]any{"timestamp_ms": time.Now().UnixMilli(), "service": "chain-parser-go", "role": "startup", "level": "error", "event": "service_start_failed", "config_release": "unavailable", "config_public_hash": "", "fields": map[string]any{"error_code": e.Error()}})
		os.Exit(1)
	}
}
