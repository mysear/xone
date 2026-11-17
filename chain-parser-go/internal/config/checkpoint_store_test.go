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
	cfg.Config.Service.Name = "chain-parser-rust"
	if e = cfg.Config.Validate("chain-parser-rust"); e == nil || e.Error() != "CHECKPOINT_PUBLISH_OWNER" {
		t.Fatal(e)
	}
}
