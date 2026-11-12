package config

import (
	"encoding/json"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"xone/chain-parser-go/internal/domain"
)

func TestValidSingleFileAndInvalidYAML(t *testing.T) {
	s, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	if s.Config.Startup.Mode != "HEAD_FIRST" || s.PublicHash == "" {
		t.Fatal(s)
	}
	for _, raw := range []string{"a: 1\na: 2\n", "a: &x [1]\nb: *x\n", "a: ${HOME}\n"} {
		if _, e = decodeDocument([]byte(raw)); e == nil {
			t.Fatalf("accepted %s", raw)
		}
	}
}
func TestInvalidConfigDoesNotLeakSecrets(t *testing.T) {
	dir := t.TempDir()
	p := filepath.Join(dir, "bad.yaml")
	os.WriteFile(p, []byte("secret: do-not-print-this\n"), 0600)
	_, e := Load(p, "chain-parser-go")
	if e == nil || strings.Contains(e.Error(), "do-not-print-this") {
		t.Fatal(e)
	}
}
func TestConfigUnderSymlinkDirectory(t *testing.T) {
	root, e := filepath.Abs("../../config")
	if e != nil {
		t.Fatal(e)
	}
	link := filepath.Join(t.TempDir(), "bundle")
	if e = os.Symlink(root, link); e != nil {
		t.Fatal(e)
	}
	if _, e = Load(filepath.Join(link, "config-test.yaml"), "chain-parser-go"); e != nil {
		t.Fatal(e)
	}
}
func TestOversizedDurationRejectedBeforeOpeningDependencies(t *testing.T) {
	s, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	s.Config.Control.LeaseTTLMS = ^uint64(0)
	if s.Config.Validate("chain-parser-go") == nil {
		t.Fatal("oversized lease accepted")
	}
}

func TestJournalAnalyticsModeIsExplicitAndCannotOwnLease(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	cfg.Config.Service.Mode = "journal"
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("online service accepted journal-only mode")
	}
	cfg.Config.Service.Role = "analytics"
	cfg.Config.Journal.Transport = "kafka"
	cfg.Config.Journal.Kafka.Brokers = []string{"127.0.0.1:9092"}
	cfg.Config.Journal.Kafka.Topics = map[string]string{"evm:31337": "canonical.evm"}
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Control.Backend = "postgres"
	cfg.Config.Control.DSNFile = "secrets/dsn"
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("analytics journal job allowed control lease backend")
	}
}

func TestV3RequiresExplicitCodeAndFiniteTickBudget(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	p := PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: "uniswap-v3", Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: 3000, FeeDenominator: 1000000}
	cfg.Config.Chains[0].Pools = []PoolConfig{p}
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("V3 without code/proof budget accepted")
	}
	p.V3 = &V3PoolConfig{ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedCodeSHA256: strings.Repeat("a", 64), TickWordRadius: 1, MaxInitializedTicks: 1024, MaxSteps: 512}
	cfg.Config.Chains[0].Pools[0] = p
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
	cfg.Config.Chains[0].Pools[0].V3.TickWordRadius = 17
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("unbounded tick radius accepted")
	}
}

func TestSingleFileWithoutDevelopmentCredentials(t *testing.T) {
	raw, err := os.ReadFile("../../config/config-test.yaml")
	if err != nil {
		t.Fatal(err)
	}
	doc, err := decodeDocument(raw)
	if err != nil {
		t.Fatal(err)
	}
	doc["service"].(map[string]any)["mode"] = "rpc"
	api := doc["api"].(map[string]any)
	delete(api, "auth_token")
	delete(api, "quote_timeout_ms")
	delete(api, "quote_concurrency")
	delete(doc, "routing")
	delete(doc, "simulation")
	for _, value := range doc["chains"].([]any) {
		ch := value.(map[string]any)
		delete(ch, "fixture_path")
		ch["rpc_urls"] = []string{"http://127.0.0.1:8545"}
	}
	path := filepath.Join(t.TempDir(), "config-dev.yaml")
	write := func() {
		raw, err := json.Marshal(doc)
		if err != nil {
			t.Fatal(err)
		}
		if err = os.WriteFile(path, raw, 0600); err != nil {
			t.Fatal(err)
		}
	}
	write()
	cfg, err := Load(path, "chain-parser-go")
	if err != nil {
		t.Fatal(err)
	}
	if cfg.Token != "" {
		t.Fatal("unexpected development credential")
	}
	doc["service"].(map[string]any)["mode"] = "fixture"
	write()
	if _, err = Load(path, "chain-parser-go"); err == nil {
		t.Fatal("production entry accepted fixture mode")
	}
}

func TestRemovedManifestAndFixtureFieldsAreRejected(t *testing.T) {
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	var base map[string]any
	if e = json.Unmarshal(raw, &base); e != nil {
		t.Fatal(e)
	}
	path := filepath.Join(t.TempDir(), "config-test.yaml")
	cases := []map[string]any{{"schema_version": 1, "release_id": "old", "files": []any{}}}
	var withFixture map[string]any
	_ = json.Unmarshal(raw, &withFixture)
	withFixture["chains"].([]any)[0].(map[string]any)["fixture_path"] = "sample.json"
	cases = append(cases, withFixture)
	for _, doc := range cases {
		b, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(path, b, 0600); e != nil {
			t.Fatal(e)
		}
		if _, e = Load(path, "chain-parser-go"); e == nil {
			t.Fatal("removed config format was accepted")
		}
	}
}

func TestAPIAuthDoesNotChangePublicBusinessFingerprint(t *testing.T) {
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	var doc map[string]any
	_ = json.Unmarshal(raw, &doc)
	path := filepath.Join(t.TempDir(), "config-test.yaml")
	write := func() {
		b, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(path, b, 0600); e != nil {
			t.Fatal(e)
		}
	}
	write()
	without, e := Load(path, "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	doc["api"].(map[string]any)["auth_token"] = "explicit-business-api-token-123456"
	doc["journal"].(map[string]any)["source_auth_tokens"] = map[string]string{"http://127.0.0.1:8091": "upstream-only-token-1234567890"}
	write()
	with, e := Load(path, "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	if without.PublicHash != with.PublicHash || without.EconomicHash != with.EconomicHash {
		t.Fatal("authentication leaked into a public business fingerprint")
	}
	if with.Token != "explicit-business-api-token-123456" {
		t.Fatal("explicit auth token was not loaded")
	}
}

func TestCredentialKeyCaseCannotAffectPublicFingerprint(t *testing.T) {
	raw, e := os.ReadFile("../../config/config-test.yaml")
	if e != nil {
		t.Fatal(e)
	}
	var doc map[string]any
	_ = json.Unmarshal(raw, &doc)
	path := filepath.Join(t.TempDir(), "config-test.yaml")
	write := func() {
		b, e := json.Marshal(doc)
		if e != nil {
			t.Fatal(e)
		}
		if e = os.WriteFile(path, b, 0600); e != nil {
			t.Fatal(e)
		}
	}
	write()
	baseline, e := Load(path, "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, token := range []string{"upstream-only-token-1234567890", "rotated-upstream-token-1234567"} {
		doc["journal"].(map[string]any)["SOURCE_AUTH_TOKENS"] = map[string]string{"http://127.0.0.1:8091": token}
		doc["API"] = map[string]any{"AUTH_TOKEN": token}
		write()
		cfg, e := Load(path, "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		if cfg.PublicHash != baseline.PublicHash {
			t.Fatal("case variant of an authentication field affected the public fingerprint")
		}
	}
}

func TestNamedDexVersionsRequireCorrectIdentityAndFee(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, name := range []string{"pancake-v1", "pancake-v2", "sushiswap-v2"} {
		profile, _ := domain.Protocol(name)
		p := PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: name, Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: profile.FeeNumerator, FeeDenominator: profile.FeeDenominator}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("named DEX without identity accepted")
		}
		p.V2 = &V2PoolConfig{ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedCodeSHA256: strings.Repeat("a", 64)}
		cfg.Config.Chains[0].Pools[0] = p
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(name, e)
		}
		cfg.Config.Chains[0].Pools[0].FeeNumerator++
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("wrong fee accepted")
		}
	}
}

func TestStableProfilesRequireExplicitModelIdentity(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	p := PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: "aerodrome-v2-stable", Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeDenominator: 10000}
	cfg.Config.Chains[0].Pools = []PoolConfig{p}
	if cfg.Config.Validate("chain-parser-go") == nil {
		t.Fatal("unidentified stable model accepted")
	}
	p.Stable = &StablePoolConfig{ExpectedCodeSHA256: strings.Repeat("a", 64), ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedFactoryCodeSHA256: strings.Repeat("b", 64)}
	cfg.Config.Chains[0].Pools[0] = p
	if e = cfg.Config.Validate("chain-parser-go"); e != nil {
		t.Fatal(e)
	}
}

func TestV3OptionalFactoryDigestValidatedForEveryProfile(t *testing.T) {
	cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, name := range []string{"uniswap-v3", "pancake-v3"} {
		p := PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: name, Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeNumerator: 3000, FeeDenominator: 1000000, V3: &V3PoolConfig{ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedCodeSHA256: strings.Repeat("a", 64), TickWordRadius: 1, MaxInitializedTicks: 32, MaxSteps: 512}}
		for _, hash := range []string{"", strings.Repeat("b", 64), "aa", strings.Repeat("g", 64), strings.Repeat("B", 64)} {
			p.V3.ExpectedFactoryCodeSHA256 = hash
			cfg.Config.Chains[0].Pools = []PoolConfig{p}
			e = cfg.Config.Validate("chain-parser-go")
			if (hash == "" || hash == strings.Repeat("b", 64)) != (e == nil) {
				t.Fatalf("%s digest %q: %v", name, hash, e)
			}
		}
	}
}

func TestVelodromeProfilesRequirePinnedFactoryAndImplementationIdentity(t *testing.T) {
	for _, name := range []string{"velodrome-v2-stable", "velodrome-v2-volatile"} {
		cfg, e := Load("../../config/config-test.yaml", "chain-parser-go")
		if e != nil {
			t.Fatal(e)
		}
		p := PoolConfig{ID: "0x" + strings.Repeat("3", 40), Protocol: name, Token0: "0x" + strings.Repeat("1", 40), Token1: "0x" + strings.Repeat("2", 40), FeeDenominator: 10000}
		cfg.Config.Chains[0].Pools = []PoolConfig{p}
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("unidentified Velodrome model accepted")
		}
		p.Stable = &StablePoolConfig{ExpectedCodeSHA256: strings.Repeat("a", 64), ExpectedFactory: "0x" + strings.Repeat("4", 40), ExpectedFactoryCodeSHA256: strings.Repeat("b", 64)}
		cfg.Config.Chains[0].Pools[0] = p
		if e = cfg.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(e)
		}
		p.Stable.ExpectedFactoryCodeSHA256 = ""
		if cfg.Config.Validate("chain-parser-go") == nil {
			t.Fatal("unbound Velodrome factory accepted")
		}
	}
}

func TestCORSOriginsAreBoundedAndRejectCredentialsAndPaths(t *testing.T) {
	s, e := Load("../../config/config-test.yaml", "chain-parser-go")
	if e != nil {
		t.Fatal(e)
	}
	for _, origin := range []string{"http://127.0.0.1:5173", "https://quotes.example.com"} {
		s.Config.API.CORSAllowedOrigins = []string{origin}
		if e = s.Config.Validate("chain-parser-go"); e != nil {
			t.Fatal(e)
		}
	}
	for _, origin := range []string{"*", "null", "https://example.com/path", "http://user:pass@example.com", "https://example.com?token=a", "https://a\r\nb", "file://localhost"} {
		s.Config.API.CORSAllowedOrigins = []string{origin}
		if s.Config.Validate("chain-parser-go") == nil {
			t.Fatal("accepted", origin)
		}
	}
}
