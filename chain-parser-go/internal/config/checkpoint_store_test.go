package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"testing"
)

func TestCheckpointStoreURLScopeAndSecretFingerprint(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, url := range []string{"https://objects.example/bucket", "https://objects.example:443/bucket-1", "http://127.0.0.1:8092/bucket"} {
		cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: url, AllowPlaintext: true}
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(url, e)
		}
	}
	for _, url := range []string{"http://objects.example/bucket", "https://objects.example/", "https://objects.example/a/../b", "https://objects.example/a%2fb", "https://user:secret@objects.example", "https://objects.example?token=secret", "https://objects.example/#x", "https://objects.example:0/b", "https://objects.example:0443/b", "https://Objects.example/b", "https://objects.example/a//b", "http://127.0.0.10:8092"} {
		cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: url, AllowPlaintext: true}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal(url)
		}
	}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "http://127.0.0.1:8092/bucket"}
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("implicit plaintext")
	}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket"}
	path := filepath.Join(t.TempDir(), "config.json")
	original, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	var doc map[string]any
	if e = json.Unmarshal(original, &doc); e != nil {
		t.Fatal(e)
	}
	doc["journal"].(map[string]any)["checkpoint_store"] = cfg.Config.Journal.CheckpointStore
	save := func() {
		raw, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(path, raw, 0600); e != nil {
			t.Fatal(e)
		}
	}
	save()
	before, e := Load(path, "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Journal.SourceAuthTokens = map[string]string{cfg.Config.Journal.CheckpointStore.BaseURL: "secret-object-store-token-123456"}
	doc["journal"].(map[string]any)["source_auth_tokens"] = cfg.Config.Journal.SourceAuthTokens
	save()
	after, e := Load(path, "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	if before.PublicHash != after.PublicHash || before.EconomicHash != after.EconomicHash {
		t.Fatal("object credential entered public identity")
	}
}

func TestCheckpointPeriodicPublishingIsExplicitBoundedAndGoOnlineOnly(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket"}
	for _, interval := range []uint64{0, 100, 1000, 3600000} {
		cfg.Config.Journal.CheckpointStore.PublishIntervalMS = interval
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(interval, e)
		}
	}
	for _, interval := range []uint64{1, 99, 3600001, ^uint64(0)} {
		cfg.Config.Journal.CheckpointStore.PublishIntervalMS = interval
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal(interval)
		}
	}
	cfg.Config.Journal.CheckpointStore.PublishIntervalMS = 100
	cfg.Config.Service.Role = "analytics"
	if e = cfg.Config.Validate("chain-parser-go"); e == nil || e.Error() != "CHECKPOINT_PUBLISH_OWNER" {
		t.Fatal(e)
	}
	cfg.Config.Service.Role = "online"
	// Keep the original Go regression selector; stage110 separately enables Rust owner.
	cfg.Config.Service.Name = "dex-router"
	if e = cfg.Config.Validate("dex-router"); e == nil || e.Error() != "CHECKPOINT_PUBLISH_OWNER" {
		t.Fatal(e)
	}
}

func TestCheckpointPeriodicConfigurationAlsoAcceptsExplicitRustOnlineOwner(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Service.Name = "chain-parser-rust"
	cfg.Config.State.Engine = "rocksdb"
	cfg.Config.Chains = []Chain{{Key: "solana:localnet", Owner: "chain-parser-rust", Assignment: 1, Genesis: "11111111111111111111111111111111", RPCURLs: []string{"http://127.0.0.1:8899"}, Pools: []PoolConfig{}, Confirmation: "finalized", ReorgWindow: 128}}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket", PublishIntervalMS: 100}
	if e = cfg.Config.Validate("chain-parser-rust"); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Service.Role = "analytics"
	if e = cfg.Config.Validate("chain-parser-rust"); e == nil || e.Error() != "CHECKPOINT_PUBLISH_OWNER" {
		t.Fatal(e)
	}
}

func TestCheckpointConfigExportsCanonicalDefaultsAndSecretFreeHashes(t *testing.T) {
	directory := os.Getenv("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
	if directory == "" {
		directory = t.TempDir()
	}
	if e := os.MkdirAll(directory, 0700); e != nil {
		t.Fatal(e)
	}
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	var vectors []map[string]any
	var hashes []string
	for i := 0; i < 5; i++ {
		var doc map[string]any
		if e = json.Unmarshal(raw, &doc); e != nil {
			t.Fatal(e)
		}
		journal := doc["journal"].(map[string]any)
		if i > 0 {
			store := map[string]any{"base_url": "https://objects.example/bucket"}
			if i >= 2 {
				store["allow_plaintext_development"] = false
				store["publish_interval_ms"] = 0
			}
			if i >= 3 {
				store["publish_interval_ms"] = 100
			}
			journal["checkpoint_store"] = store
		}
		if i == 4 {
			journal["source_auth_tokens"] = map[string]string{"https://objects.example/bucket": "test-only-checkpoint-credential-20261118"}
			doc["api"].(map[string]any)["auth_token"] = "test-only-api-credential-20261118"
		}
		encoded, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		name := "config-" + string(rune('0'+i)) + ".json"
		path := filepath.Join(directory, name)
		if e = os.WriteFile(path, encoded, 0600); e != nil {
			t.Fatal(e)
		}
		loaded, e := Load(path, "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		hashes = append(hashes, loaded.PublicHash)
		vectors = append(vectors, map[string]any{"file": name, "public_hash": loaded.PublicHash, "economic_hash": loaded.EconomicHash, "expected": "chain-parser-go"})
	}
	if hashes[1] != hashes[2] || hashes[3] != hashes[4] || hashes[0] == hashes[1] || hashes[2] == hashes[3] {
		t.Fatal("public identity does not distinguish semantics or excludes defaults/secrets incorrectly", hashes)
	}
	encoded, e := json.MarshalIndent(map[string]any{"vectors": vectors}, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(filepath.Join(directory, "vectors.json"), encoded, 0600); e != nil {
		t.Fatal(e)
	}
}

func TestCheckpointLocalBudgetConfigAndIndependentHashes(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket"}
	for _, budget := range []CheckpointBudget{{MaxBytes: 0, MaxFiles: 1, MaxEntries: 1}, {MaxBytes: 1<<40 + 1, MaxFiles: 1, MaxEntries: 1}, {MaxBytes: 1, MaxFiles: 0, MaxEntries: 1}, {MaxBytes: 1, MaxFiles: 100001, MaxEntries: 100001}, {MaxBytes: 1, MaxFiles: 5, MaxEntries: 4}, {MaxBytes: 1, MaxFiles: 1, MaxEntries: 1000001}} {
		cfg.Config.Journal.CheckpointStore.LocalBudget = &budget
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal(budget)
		}
	}
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	out := os.Getenv("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
	if out == "" {
		out = t.TempDir()
	}
	if e = os.MkdirAll(out, 0700); e != nil {
		t.Fatal(e)
	}
	var vectors []map[string]any
	for i, budget := range []*CheckpointBudget{nil, {MaxBytes: 1, MaxFiles: 1, MaxEntries: 1}, {MaxBytes: 1 << 30, MaxFiles: 1000, MaxEntries: 2048}} {
		var doc map[string]any
		if e = json.Unmarshal(raw, &doc); e != nil {
			t.Fatal(e)
		}
		store := &CheckpointStore{BaseURL: "https://objects.example/bucket", LocalBudget: budget}
		doc["journal"].(map[string]any)["checkpoint_store"] = store
		encoded, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		name := "budget-config-" + string(rune('0'+i)) + ".json"
		path := filepath.Join(out, name)
		if e = os.WriteFile(path, encoded, 0600); e != nil {
			t.Fatal(e)
		}
		loaded, e := Load(path, "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		vectors = append(vectors, map[string]any{"file": name, "public_hash": loaded.PublicHash, "economic_hash": loaded.EconomicHash})
	}
	encoded, e := json.MarshalIndent(map[string]any{"vectors": vectors}, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(filepath.Join(out, "budget-vectors.json"), encoded, 0600); e != nil {
		t.Fatal(e)
	}
	if vectors[0]["public_hash"] == vectors[1]["public_hash"] || vectors[1]["public_hash"] == vectors[2]["public_hash"] {
		t.Fatal("budgets omitted from public hash")
	}
}

func TestCheckpointRetentionPolicyIsBoundedAndGoOwnerOnly(t *testing.T) {
	valid := CheckpointRetention{KeepLatest: 2, MinAgeMS: 3600000, MaxRemove: 2, MaxObjects: 32, MaxEntries: 256, MaxReadBytes: 1 << 20}
	if e := valid.Validate(); e != nil {
		t.Fatal(e)
	}
	for mode := 0; mode < 6; mode++ {
		policy := valid
		switch mode {
		case 0:
			policy.KeepLatest = 1
		case 1:
			policy.MinAgeMS = 0
		case 2:
			policy.MaxRemove = 65
		case 3:
			policy.MaxObjects = 1
		case 4:
			policy.MaxEntries = 1
		case 5:
			policy.MaxReadBytes = 256<<20 + 1
		}
		if policy.Validate() == nil {
			t.Fatal(mode)
		}
	}
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket", LocalRetention: &valid}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	for _, expected := range []string{"dex-router"} {
		if e = cfg.Config.Validate(expected); e == nil || e.Error() != "CHECKPOINT_RETENTION_OWNER" {
			t.Fatal(expected, e)
		}
	}
	cfg.Config.Service.Role = "analytics"
	if e = cfg.Config.Validate("chain-parser-go"); e == nil || e.Error() != "CHECKPOINT_RETENTION_OWNER" {
		t.Fatal(e)
	}
}

func TestCheckpointRetentionExportsSharedPolicyHashes(t *testing.T) {
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	directory := os.Getenv("XONE_CHECKPOINT_CONFIG_VECTORS_OUTPUT")
	if directory == "" {
		directory = t.TempDir()
	}
	if e = os.MkdirAll(directory, 0700); e != nil {
		t.Fatal(e)
	}
	var vectors []map[string]any
	for i := 0; i < 2; i++ {
		var doc map[string]any
		if e = json.Unmarshal(raw, &doc); e != nil {
			t.Fatal(e)
		}
		policy := &CheckpointRetention{KeepLatest: 2 + uint64(i), MinAgeMS: 3600000, MaxRemove: 2, MaxObjects: 32, MaxEntries: 256, MaxReadBytes: 1 << 20}
		doc["journal"].(map[string]any)["checkpoint_store"] = &CheckpointStore{BaseURL: "https://objects.example/bucket", LocalRetention: policy}
		encoded, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		name := "retention-config-" + string(rune('0'+i)) + ".json"
		path := filepath.Join(directory, name)
		if e = os.WriteFile(path, encoded, 0600); e != nil {
			t.Fatal(e)
		}
		loaded, e := Load(path, "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		vectors = append(vectors, map[string]any{"file": name, "public_hash": loaded.PublicHash, "economic_hash": loaded.EconomicHash})
	}
	if vectors[0]["public_hash"] == vectors[1]["public_hash"] || vectors[0]["economic_hash"] != vectors[1]["economic_hash"] {
		t.Fatal("retention policy not separated from economic identity")
	}
	encoded, e := json.MarshalIndent(map[string]any{"vectors": vectors}, "", "  ")
	if e != nil {
		t.Fatal(e)
	}
	if e = os.WriteFile(filepath.Join(directory, "retention-vectors.json"), encoded, 0600); e != nil {
		t.Fatal(e)
	}
}

func TestCheckpointRetentionAllowsExplicitRustOnlineOwner(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Service.Name = "chain-parser-rust"
	cfg.Config.State.Engine = "rocksdb"
	cfg.Config.Chains = []Chain{{Key: "solana:localnet", Owner: "chain-parser-rust", Assignment: 1, Genesis: "11111111111111111111111111111111", RPCURLs: []string{"http://127.0.0.1:8899"}, Pools: []PoolConfig{}, Confirmation: "finalized", ReorgWindow: 128}}
	cfg.Config.Journal.CheckpointStore = &CheckpointStore{BaseURL: "https://objects.example/bucket", LocalRetention: &CheckpointRetention{KeepLatest: 2, MinAgeMS: 3600000, MaxRemove: 2, MaxObjects: 32, MaxEntries: 256, MaxReadBytes: 1 << 20}}
	if e = cfg.Config.Validate("chain-parser-rust"); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Service.Role = "analytics"
	if e = cfg.Config.Validate("chain-parser-rust"); e == nil || e.Error() != "CHECKPOINT_RETENTION_OWNER" {
		t.Fatal(e)
	}
}
