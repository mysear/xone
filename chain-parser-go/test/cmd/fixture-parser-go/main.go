// fixture-parser-go is an explicitly selected offline debugging executable.
package main

import (
	"context"
	"encoding/json"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"xone/chain-parser-go/internal/analysis"
	"xone/chain-parser-go/internal/app"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
)

func run() error {
	path := flag.String("config", "", "environment configuration file")
	fixtures := flag.String("fixtures", "", "directory of chain sample files")
	replay := flag.String("replay", "", "analytics event file")
	output := flag.String("output", "", "analytics output file")
	flag.Parse()
	if *replay != "" {
		f, e := os.Open(*replay)
		if e != nil {
			return fmt.Errorf("REPLAY_INPUT_UNREADABLE")
		}
		defer f.Close()
		result, e := analysis.Replay(context.Background(), f, 8<<20)
		if e != nil {
			return e
		}
		if *output == "" {
			return fmt.Errorf("REPLAY_OUTPUT_REQUIRED")
		}
		return analysis.Publish(*output, result)
	}
	if *path == "" || *fixtures == "" || flag.NArg() != 0 {
		return fmt.Errorf("USAGE: --config <file> --fixtures <directory>")
	}
	cfg, e := config.Load(*path, "chain-parser-go")
	if e != nil {
		return e
	}
	return app.Run(cfg, func(_ context.Context, ch config.Chain) (domain.Snapshot, error) {
		p := filepath.Join(*fixtures, strings.ReplaceAll(ch.Key, ":", "-")+".json")
		if _, e := os.Stat(p); os.IsNotExist(e) {
			p = filepath.Join(*fixtures, strings.SplitN(ch.Key, ":", 2)[0], "baseline.json")
		}
		raw, e := os.ReadFile(p)
		if e != nil {
			return domain.Snapshot{}, fmt.Errorf("DEBUG_SAMPLE_UNREADABLE")
		}
		if len(raw) > cfg.Config.Journal.MaxBytes {
			return domain.Snapshot{}, fmt.Errorf("DEBUG_SAMPLE_SIZE")
		}
		var s domain.Snapshot
		e = json.Unmarshal(raw, &s)
		return s, e
	})
}
func main() {
	if e := run(); e != nil {
		fmt.Fprintln(os.Stderr, e)
		os.Exit(1)
	}
}
