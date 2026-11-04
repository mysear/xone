package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"time"
	"xone/chain-parser-go/internal/app"
	"xone/chain-parser-go/internal/config"
)

func run() error {
	flags := flag.NewFlagSet("chain-parser-go", flag.ContinueOnError)
	path := flags.String("config", "", "environment configuration file")
	check := flags.Bool("check-config", false, "validate configuration and exit")
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
