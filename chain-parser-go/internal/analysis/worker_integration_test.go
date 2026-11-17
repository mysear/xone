//go:build integration

package analysis

import (
	"context"
	"encoding/json"
	"flag"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"syscall"
	"testing"
	"time"
	"xone/chain-parser-go/internal/config"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

var workerBroker = flag.String("analytics-worker-broker", "", "dedicated Kafka broker")
var workerBinary = flag.String("analytics-worker-binary", "", "current Go service executable")

func TestContinuousAnalyticsActualServiceResumesWithoutFencingOrPoisonSkipping(t *testing.T) {
	if *workerBroker == "" || *workerBinary == "" {
		t.Fatal("dedicated broker and current service executable required")
	}
	ctx, cancel := context.WithTimeout(context.Background(), 90*time.Second)
	defer cancel()
	owner, err := journal.OpenKafka(ctx, []string{*workerBroker}, "test.analytics", "test.continuous-analytics-owner", 1024, 8<<20, 4096, 5*time.Second)
	if err != nil {
		t.Fatal(err)
	}
	defer owner.Close()
	base := snapshot(t)
	if _, err = owner.Publish(ctx, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: base}, nil); err != nil {
		t.Fatal(err)
	}
	cfg, err := config.Load("../../config/config-test.yaml", "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	root := t.TempDir()
	output := os.Getenv("XONE_ANALYTICS_WORKER_OUTPUT")
	if output == "" {
		output = root
	}
	if err = os.MkdirAll(output, 0700); err != nil {
		t.Fatal(err)
	}
	cfg.Config.Service.Role = "analytics"
	cfg.Config.Service.Mode = "journal-continuous"
	cfg.Config.State.Path = filepath.Join(root, "state")
	cfg.Config.Journal.Transport = "kafka"
	cfg.Config.Journal.Kafka.Brokers = []string{*workerBroker}
	cfg.Config.Journal.Kafka.Topics = map[string]string{base.Reference.Block.ChainKey: "test.analytics"}
	cfg.Config.Journal.PollMS = 50
	cfg.Config.Journal.TimeoutMS = 10000
	cfg.Config.Control.Backend = "disabled"
	cfg.Config.Control.DSNFile = ""
	cfg.Config.Chains = cfg.Config.Chains[:1]
	cfg.Config.Chains[0].AnalysisIntervalMS = 60000
	cfg.Config.Chains[0].RPCURLs = nil
	cfg.Config.Logging.Level = "info"
	if err = cfg.Config.Validate("chain-parser-go"); err != nil {
		t.Fatal(err)
	}
	// Preserve the original JSON's optional-field presence; zero-value Amount
	// fields in unused adapters must not be invented when writing a test config.
	source, err := os.ReadFile("../../config/config-test.yaml")
	if err != nil {
		t.Fatal(err)
	}
	var document map[string]any
	if err = json.Unmarshal(source, &document); err != nil {
		t.Fatal(err)
	}
	document["service"] = cfg.Config.Service
	document["state"] = cfg.Config.State
	journalConfig := document["journal"].(map[string]any)
	journalConfig["transport"] = "kafka"
	journalConfig["poll_interval_ms"] = cfg.Config.Journal.PollMS
	journalConfig["request_timeout_ms"] = cfg.Config.Journal.TimeoutMS
	kafkaConfig := journalConfig["kafka"].(map[string]any)
	kafkaConfig["brokers"] = cfg.Config.Journal.Kafka.Brokers
	kafkaConfig["topics"] = cfg.Config.Journal.Kafka.Topics
	document["control"] = cfg.Config.Control
	chain := document["chains"].([]any)[0].(map[string]any)
	chain["analysis_interval_ms"] = 60000
	chain["rpc_urls"] = []string{}
	document["chains"] = []any{chain}
	document["logging"].(map[string]any)["level"] = "info"
	raw, err := json.Marshal(document)
	if err != nil {
		t.Fatal(err)
	}
	configPath := filepath.Join(root, "config-test.yaml")
	if err = os.WriteFile(configPath, raw, 0600); err != nil {
		t.Fatal(err)
	}
	checkpoint := filepath.Join(cfg.Config.State.Path, "analytics", base.Reference.Block.ChainKey+".journal.json")
	type child struct {
		cmd      *exec.Cmd
		done     chan error
		finished chan struct{}
		log      *os.File
	}
	start := func(name string) *child {
		log, e := os.Create(filepath.Join(output, name+".log"))
		if e != nil {
			t.Fatal(e)
		}
		command := exec.Command(*workerBinary, "--config", configPath)
		command.Stdout = log
		command.Stderr = log
		if e = command.Start(); e != nil {
			log.Close()
			t.Fatal(e)
		}
		c := &child{cmd: command, done: make(chan error, 1), finished: make(chan struct{}), log: log}
		go func() { c.done <- command.Wait(); close(c.finished) }()
		t.Cleanup(func() {
			select {
			case <-c.finished:
			default:
				_ = command.Process.Kill()
				select {
				case <-c.finished:
				case <-time.After(5 * time.Second):
				}
			}
			_ = log.Close()
		})
		return c
	}
	waitState := func(seq uint64) *JournalState {
		until := time.Now().Add(15 * time.Second)
		for time.Now().Before(until) && ctx.Err() == nil {
			state, e := LoadJournalState(checkpoint, 8<<20, base.Reference.Block.ChainKey, base.Reference.OwnerService, base.Reference.OwnerAssignmentVersion, 60000)
			if e == nil && state.Reference != nil && state.Reference.JournalSeq == seq && state.Ready {
				return state
			}
			time.Sleep(20 * time.Millisecond)
		}
		t.Fatal("analytics checkpoint did not reach sequence", seq)
		return nil
	}
	save := func(name string) {
		bytes, e := os.ReadFile(checkpoint)
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(filepath.Join(output, name+".json"), bytes, 0600); e != nil {
			t.Fatal(e)
		}
	}
	publish := func(seq uint64, hash, price string, complex bool) domain.Snapshot {
		next := base
		next.Reference.JournalSeq = seq
		next.Reference.Block.Number += seq - 1
		next.Reference.Block.Hash = "0x" + strings.Repeat(hash, 64)
		next.Reference.Block.ParentHash = base.Reference.Block.Hash
		next.Reference.Block.TimestampMS += 1000 * (seq - 1)
		if seq > 2 {
			next.Reference.Block.ParentHash = "0x" + strings.Repeat(string(byte('0'+seq-1)), 64)
		}
		fact := swap(next, price)
		if complex {
			fact.Swaps[0].Amount1In = "1"
		}
		unit := journal.Unit{Kind: "BLOCK_COMMIT", Snapshot: next, NormalizedFacts: &domain.FactBatch{SchemaVersion: 1, DecoderVersion: "uniswap-v2/1", Blocks: []domain.FactBlock{fact}}}
		if _, e := owner.Publish(ctx, unit, nil); e != nil {
			t.Fatal("analytics worker fenced or blocked owner", e)
		}
		return next
	}
	first := start("worker-first")
	waitState(base.Reference.JournalSeq)
	save("baseline")
	publish(2, "2", "100", false)
	before := waitState(2)
	if len(before.Trades) != 1 || len(before.Release().Candles) != 1 {
		t.Fatal("new facts not consumed")
	}
	save("first-fact")
	if err = first.cmd.Process.Signal(syscall.SIGTERM); err != nil {
		t.Fatal(err)
	}
	select {
	case err = <-first.done:
		if err != nil {
			t.Fatal("worker did not stop cleanly", err)
		}
	case <-time.After(10 * time.Second):
		t.Fatal("worker cancellation did not stop process")
	}
	second := start("worker-second")
	// Wait for a newly completed pass, rather than accepting an old on-disk file as restart evidence.
	until := time.Now().Add(15 * time.Second)
	for time.Now().Before(until) {
		bytes, _ := os.ReadFile(filepath.Join(output, "worker-second.log"))
		if strings.Contains(string(bytes), "analytics_journal_checkpoint_published") {
			break
		}
		time.Sleep(20 * time.Millisecond)
	}
	bytes, _ := os.ReadFile(filepath.Join(output, "worker-second.log"))
	if !strings.Contains(string(bytes), "analytics_journal_checkpoint_published") {
		t.Fatal("restarted worker did not complete a cycle")
	}
	restarted := waitState(2)
	if len(restarted.Trades) != 1 || restarted.Release().Candles[0].Revision != before.Release().Candles[0].Revision {
		t.Fatal("restart duplicated facts or revised unchanged window")
	}
	publish(3, "3", "200", false)
	last := waitState(3)
	save("second-fact")
	if len(last.Trades) != 2 {
		t.Fatal("continuous worker missed new fact")
	}
	cursor := last.Cursor
	publish(4, "4", "300", true)
	select {
	case err = <-second.done:
		if err == nil {
			t.Fatal("poison fact did not stop worker")
		}
	case <-time.After(15 * time.Second):
		t.Fatal("poison fact did not stop process")
	}
	failed, e := LoadJournalState(checkpoint, 8<<20, base.Reference.Block.ChainKey, base.Reference.OwnerService, base.Reference.OwnerAssignmentVersion, 60000)
	if e != nil || failed.Cursor != cursor || failed.Ready || failed.Release().Publishable || failed.SuspendedReason != "ANALYTICS_COMPLEX_SWAP_UNSUPPORTED" {
		t.Fatal("poison fact changed durable cursor or retained release", e, failed)
	}
	save("poison-stopped")
	publish(5, "5", "400", false) // The independent writer remains usable after analytics failure.
	poisonRestart := start("worker-poison-restart")
	select {
	case err = <-poisonRestart.done:
		if err == nil {
			t.Fatal("restart skipped poison fact")
		}
	case <-time.After(15 * time.Second):
		t.Fatal("restart did not revalidate poison fact")
	}
	after, e := LoadJournalState(checkpoint, 8<<20, base.Reference.Block.ChainKey, base.Reference.OwnerService, base.Reference.OwnerAssignmentVersion, 60000)
	if e != nil || after.Cursor != cursor || after.Ready {
		t.Fatal("restart skipped poison cursor", e)
	}
	save("poison-restart")
	_ = filepath.WalkDir(cfg.Config.State.Path, func(path string, entry os.DirEntry, e error) error {
		if e != nil {
			t.Fatal(e)
		}
		if !entry.IsDir() && (entry.Name() == "LOCK" || entry.Name() == "CURRENT" || strings.HasSuffix(entry.Name(), ".sst")) {
			t.Fatal("analytics opened an online KV", path)
		}
		return nil
	})
	t.Log("actual service: continuous new facts, clean cancellation, restart idempotence, live writer retained, poison stopped and not skipped; no online KV")
}
