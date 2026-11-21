//go:build integration

package service

import (
	"context"
	"flag"
	"fmt"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/twmb/franz-go/pkg/kgo"
	"github.com/twmb/franz-go/pkg/kmsg"
	"xone/chain-parser-go/internal/control"
	"xone/chain-parser-go/internal/domain"
	"xone/chain-parser-go/internal/journal"
)

// Reuse the existing integration suite's flag names with its isolated PG/Kafka.
var startupDSNFile = flag.String("control-test-dsn-file", "", "permission-restricted test DSN file")
var startupBroker = flag.String("kafka-test-broker", "", "isolated Kafka broker")

func startupIntegrationTopic(t *testing.T, ctx context.Context) string {
	t.Helper()
	client, err := kgo.NewClient(kgo.SeedBrokers(*startupBroker))
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(client.Close)
	topic := fmt.Sprintf("test.service-startup-%d", time.Now().UnixNano())
	request := kmsg.NewPtrCreateTopicsRequest()
	request.Topics = []kmsg.CreateTopicsRequestTopic{{Topic: topic, NumPartitions: 1, ReplicationFactor: 1}}
	response, err := request.RequestWith(ctx, client)
	if err != nil || len(response.Topics) != 1 || response.Topics[0].ErrorCode != 0 {
		t.Fatalf("cannot create isolated startup topic: %v %+v", err, response)
	}
	t.Cleanup(func() {
		cleanup, cancel := context.WithTimeout(context.Background(), 5*time.Second)
		defer cancel()
		request := kmsg.NewPtrDeleteTopicsRequest()
		request.TopicNames = []string{topic}
		request.Topics = []kmsg.DeleteTopicsRequestTopic{{Topic: &topic}}
		response, err := request.RequestWith(cleanup, client)
		if err != nil || len(response.Topics) != 1 || response.Topics[0].ErrorCode != 0 {
			t.Errorf("cannot delete isolated startup topic: %v %+v", err, response)
		}
	})
	return topic
}

// Real post-Acquire failures must release their lease even though openSession
// returns no writer for Start to close. Keep conflicting prepared evidence for
// normal reconciliation, and prevent the previously readable local baseline.
func TestStartupSessionFailureReleasesAcquiredLease(t *testing.T) {
	if *startupDSNFile == "" && *startupBroker == "" {
		t.Skip("dedicated PG/Kafka flags not configured")
	}
	if *startupDSNFile == "" || *startupBroker == "" {
		t.Fatal("both --control-test-dsn-file and --kafka-test-broker are required")
	}
	rawDSN, err := os.ReadFile(*startupDSNFile)
	if err != nil {
		t.Fatal("test DSN unavailable")
	}
	for _, fault := range []string{"owner-mismatch", "prepared-conflict"} {
		t.Run(fault, func(t *testing.T) {
			ctx, cancel := context.WithTimeout(context.Background(), 30*time.Second)
			defer cancel()
			topic := startupIntegrationTopic(t, ctx)
			cfg := startupIsolationConfig(t)
			cfg.Config.Control.LeaseTTLMS = 120000 // Longer than the whole test deadline.
			cfg.Config.Journal.Kafka.Brokers = []string{*startupBroker}
			chain := cfg.Config.Chains[0]
			cfg.Config.Journal.Kafka.Topics = map[string]string{chain.Key: topic}
			s, err := New(cfg)
			if err != nil {
				t.Fatal(err)
			}
			t.Cleanup(s.Close)
			s.control, err = control.Open(ctx, strings.TrimSpace(string(rawDSN)))
			if err != nil {
				t.Fatal(err)
			}
			baseline, err := testSnapshotSource(t)(ctx, chain)
			if err != nil {
				t.Fatal(err)
			}
			store := s.stores[chain.Key]
			if err = store.ApplyLocal(journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: baseline}); err != nil {
				t.Fatal(err)
			}
			committed, err := testSnapshotSource(t)(ctx, chain)
			if err != nil {
				t.Fatal(err)
			}
			var pendingHash string
			want := "JOURNAL_OWNER_MISMATCH"
			if fault == "owner-mismatch" {
				committed.Reference.OwnerService = "chain-parser-rust"
			} else {
				pending, err := testSnapshotSource(t)(ctx, chain)
				if err != nil {
					t.Fatal(err)
				}
				pending.Reference.OnlineGeneration++
				pending.Reference.WriterEpoch++
				pending.Reference.JournalSeq++
				unit := journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: pending}
				if err = store.Prepare(unit); err != nil {
					t.Fatal(err)
				}
				pendingHash, err = unit.CanonicalHash()
				if err != nil {
					t.Fatal(err)
				}
				committed.Reference = pending.Reference
				committed.Pools[0].Reserve0, err = domain.ParseAmount("1000001")
				if err != nil {
					t.Fatal(err)
				}
				want = "PREPARED_JOURNAL_CONFLICT"
			}
			seed, err := journal.OpenKafka(ctx, []string{*startupBroker}, topic, topic+"-seed", 1024, cfg.Config.Journal.MaxBytes, cfg.Config.Journal.Kafka.MaxTailRecords, 10*time.Second)
			if err != nil {
				t.Fatal(err)
			}
			_, err = seed.Publish(ctx, journal.Unit{Kind: "ONLINE_BASELINE_COMMIT", Snapshot: committed}, nil)
			seed.Close()
			if err != nil {
				t.Fatal(err)
			}
			writer, err := s.openSession(ctx, chain)
			if writer != nil {
				writer.close()
				t.Fatal("conflicting startup unexpectedly returned a writer")
			}
			if err == nil || err.Error() != want {
				t.Fatal("did not reach the intended post-Acquire failure", err, want)
			}
			if _, ready := store.Current(); ready {
				t.Fatal("failed writer left old baseline readable")
			}
			peer, err := s.control.Acquire(ctx, chain.Key, chain.Owner, chain.Assignment, "startup-release-peer", cfg.Config.Control.LeaseTTLMS)
			if err != nil {
				t.Fatal("failed writer leaked its unexpired lease", err)
			}
			if err = s.control.Release(ctx, peer); err != nil {
				t.Fatal(err)
			}
			pending, err := store.Prepared()
			if err != nil {
				t.Fatal(err)
			}
			if pendingHash == "" {
				if pending != nil {
					t.Fatal("owner mismatch fabricated prepared evidence")
				}
			} else if pending == nil {
				t.Fatal("failed session discarded conflicting prepared evidence")
			} else if actual, err := pending.CanonicalHash(); err != nil || actual != pendingHash {
				t.Fatal("failed session modified conflicting prepared evidence", err)
			}
		})
	}
}
