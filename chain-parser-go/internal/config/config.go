package config

import (
	"bytes"
	"encoding/hex"
	"encoding/json"
	"errors"
	"io"
	"math/big"
	"net/url"
	"os"
	"path/filepath"
	"strings"

	"gopkg.in/yaml.v3"
	"xone/chain-parser-go/internal/domain"
)

const maxFile = 4 << 20

type Service struct {
	Name string `json:"name"`
	Role string `json:"role"`
	Mode string `json:"mode"`
}
type Startup struct {
	Mode          string `json:"mode"`
	Resume        bool   `json:"resume_previous_online_cursor"`
	HistoryBlocks bool   `json:"history_catchup_blocks_online_readiness"`
	MaxAge        uint64 `json:"max_baseline_age_ms"`
	MaxTail       uint64 `json:"max_tail_blocks"`
}
type History struct {
	Enabled     bool    `json:"enabled"`
	Namespace   string  `json:"namespace"`
	OnlineWrite bool    `json:"online_write_permission"`
	MaxShare    float64 `json:"max_worker_share"`
}
type Logging struct {
	Level    string `json:"level"`
	Format   string `json:"format"`
	Queue    int    `json:"queue_capacity"`
	MaxBytes int    `json:"max_event_bytes"`
}
type Control struct {
	Backend        string `json:"backend"`
	DSNFile        string `json:"dsn_file"`
	InstanceID     string `json:"instance_id"`
	LeaseTTLMS     uint64 `json:"lease_ttl_ms"`
	RenewalMS      uint64 `json:"renewal_ms"`
	MinCommitMS    uint64 `json:"min_commit_remaining_ms"`
	AllowPlaintext bool   `json:"allow_plaintext_development"`
}
type Kafka struct {
	Brokers              []string          `json:"brokers"`
	Topics               map[string]string `json:"topics"`
	ReplicaID            string            `json:"replica_id"`
	ChunkBytes           int               `json:"chunk_bytes"`
	TransactionTimeoutMS uint64            `json:"transaction_timeout_ms"`
	MaxTailRecords       int               `json:"max_tail_records"`
	SecurityProtocol     string            `json:"security_protocol"`
	AllowPlaintext       bool              `json:"allow_plaintext_development"`
}
type RawEvidenceConfig struct {
	MaxBytes       int64 `json:"max_bytes"`
	MaxRecords     int   `json:"max_records"`
	MaxRecordBytes int64 `json:"max_record_bytes"`
}
type Journal struct {
	RawMaintenance   *RawMaintenanceConfig `json:"raw_maintenance,omitempty"`
	RawEvidence      *RawEvidenceConfig    `json:"raw_evidence,omitempty"`
	SourceAuthTokens map[string]string     `json:"source_auth_tokens,omitempty"`
	Kafka            Kafka                 `json:"kafka"`
	Transport        string                `json:"transport"`
	Sources          []string              `json:"source_urls"`
	PollMS           uint64                `json:"poll_interval_ms"`
	TimeoutMS        uint64                `json:"request_timeout_ms"`
	MaxBytes         int                   `json:"max_payload_bytes"`
}
type State struct {
	Engine string `json:"engine"`
	Path   string `json:"path"`
}
type API struct {
	Listen           string `json:"listen"`
	AuthToken        string `json:"auth_token,omitempty"`
	MaxBody          int    `json:"max_body_bytes"`
	RPM              uint64 `json:"requests_per_minute"`
	QuoteTimeoutMS   uint64 `json:"quote_timeout_ms"`
	QuoteConcurrency int    `json:"quote_concurrency"`
}
type Routing struct {
	MaxHops        int           `json:"max_hops"`
	MaxCandidates  int           `json:"max_candidates"`
	MaxExpansions  int           `json:"max_expansions"`
	SplitParts     int           `json:"split_parts"`
	MaxSplits      int           `json:"max_splits"`
	ActivationCost domain.Amount `json:"route_activation_cost"`
	MaxAge         uint64        `json:"max_state_age_ms"`
	TTL            uint64        `json:"quote_ttl_ms"`
}
type Simulation struct {
	Backend         string   `json:"backend"`
	RPCURL          string   `json:"rpc_url"`
	MaxGas          uint64   `json:"max_gas"`
	MaxReads        uint64   `json:"max_state_reads"`
	TimeoutMS       uint64   `json:"timeout_ms"`
	AllowedTargets  []string `json:"allowed_targets"`
	StateFile       string   `json:"state_file,omitempty"`
	MaxStateBytes   uint64   `json:"max_state_bytes,omitempty"`
	MaxInstructions uint64   `json:"max_instructions,omitempty"`
	MaxMemoryBytes  uint64   `json:"max_memory_bytes,omitempty"`
	Concurrency     uint64   `json:"concurrency,omitempty"`
}
type V3PoolConfig struct {
	ExpectedImplementation           string `json:"expected_implementation,omitempty"`
	ExpectedImplementationCodeSHA256 string `json:"expected_implementation_code_sha256,omitempty"`
	ExpectedFactoryCodeSHA256        string `json:"expected_factory_code_sha256,omitempty"`
	ExpectedFactory                  string `json:"expected_factory"`
	ExpectedCodeSHA256               string `json:"expected_code_sha256"`
	TickWordRadius                   uint32 `json:"tick_word_radius"`
	MaxInitializedTicks              uint32 `json:"max_initialized_ticks"`
	MaxSteps                         uint32 `json:"max_steps"`
}
type V2PoolConfig struct {
	ExpectedFactory    string `json:"expected_factory"`
	ExpectedCodeSHA256 string `json:"expected_code_sha256"`
}

// WeightedPoolConfig is a versioned immutable WeightedPool allowlist. The
// policy is an operator admission assertion, not a token behavior proof.
type WeightedPoolConfig struct {
	ExpectedCodeSHA256       string `json:"expected_code_sha256"`
	ExpectedVault            string `json:"expected_vault"`
	ExpectedVaultCodeSHA256  string `json:"expected_vault_code_sha256"`
	PoolID                   string `json:"pool_id"`
	ExpectedToken0CodeSHA256 string `json:"expected_token0_code_sha256"`
	ExpectedToken1CodeSHA256 string `json:"expected_token1_code_sha256"`
	TokenPolicy              string `json:"token_policy"`
}
type StablePoolConfig struct {
	ExpectedCodeSHA256               string `json:"expected_code_sha256"`
	ExpectedImplementation           string `json:"expected_implementation,omitempty"`
	ExpectedImplementationCodeSHA256 string `json:"expected_implementation_code_sha256,omitempty"`
	ExpectedFactory                  string `json:"expected_factory,omitempty"`
	ExpectedFactoryCodeSHA256        string `json:"expected_factory_code_sha256,omitempty"`
	ExpectedVault                    string `json:"expected_vault,omitempty"`
	ExpectedVaultCodeSHA256          string `json:"expected_vault_code_sha256,omitempty"`
	PoolID                           string `json:"pool_id,omitempty"`
}
type SingletonCLPoolConfig struct {
	Manager                   string `json:"manager"`
	ExpectedManagerCodeSHA256 string `json:"expected_manager_code_sha256"`
	PoolID                    string `json:"pool_id"`
	LPFee                     uint32 `json:"lp_fee"`
	TickSpacing               int32  `json:"tick_spacing"`
	Parameters                string `json:"parameters"`
	TickWordRadius            uint32 `json:"tick_word_radius"`
	MaxInitializedTicks       uint32 `json:"max_initialized_ticks"`
	MaxSteps                  uint32 `json:"max_steps"`
}
type RaydiumCPMMConfig struct {
	ExpectedProgramdata            string `json:"expected_programdata"`
	ExpectedProgramCodeSHA256      string `json:"expected_program_code_sha256"`
	ExpectedDeployedSlot           uint64 `json:"expected_deployed_slot"`
	ExpectedTokenProgramCodeSHA256 string `json:"expected_token_program_code_sha256"`
}
type OrcaWhirlpoolConfig struct {
	ExpectedProgramdata            string  `json:"expected_programdata"`
	ExpectedProgramCodeSHA256      string  `json:"expected_program_code_sha256"`
	ExpectedDeployedSlot           uint64  `json:"expected_deployed_slot"`
	ExpectedTokenProgramCodeSHA256 string  `json:"expected_token_program_code_sha256"`
	TickArrayStarts                []int32 `json:"tick_array_starts"`
	MaxSteps                       uint32  `json:"max_steps"`
}
type RaydiumCLMMConfig struct {
	ExpectedProgramdata            string  `json:"expected_programdata"`
	ExpectedProgramCodeSHA256      string  `json:"expected_program_code_sha256"`
	ExpectedDeployedSlot           uint64  `json:"expected_deployed_slot"`
	ExpectedTokenProgramCodeSHA256 string  `json:"expected_token_program_code_sha256"`
	TickArrayStarts                []int32 `json:"tick_array_starts"`
	MaxSteps                       uint32  `json:"max_steps"`
}
type MultiTokenConfig struct {
	Token              string `json:"token"`
	ExpectedCodeSHA256 string `json:"expected_code_sha256"`
}
type MultiAssetConfig struct {
	ManagedV2                        *ManagedV2PoolConfig `json:"managed_v2,omitempty"`
	ExpectedLPToken                  string               `json:"expected_lp_token,omitempty"`
	ExpectedLPTokenCodeSHA256        string               `json:"expected_lp_token_code_sha256,omitempty"`
	ExpectedBasePoolCodeSHA256       string               `json:"expected_base_pool_code_sha256,omitempty"`
	ExpectedCodeSHA256               string               `json:"expected_code_sha256"`
	ExpectedImplementation           string               `json:"expected_implementation,omitempty"`
	ExpectedImplementationCodeSHA256 string               `json:"expected_implementation_code_sha256,omitempty"`
	ExpectedVault                    string               `json:"expected_vault,omitempty"`
	ExpectedVaultCodeSHA256          string               `json:"expected_vault_code_sha256,omitempty"`
	PoolID                           string               `json:"pool_id,omitempty"`
	Tokens                           []MultiTokenConfig   `json:"tokens"`
	TokenPolicy                      string               `json:"token_policy"`
}
type SingletonAdvancedPoolConfig struct {
	Protocol                  string `json:"protocol"`
	Manager                   string `json:"manager"`
	ExpectedManagerCodeSHA256 string `json:"expected_manager_code_sha256"`
	PoolID                    string `json:"pool_id"`
	Currency0                 string `json:"currency0"`
	Currency1                 string `json:"currency1"`
	KeyFee                    uint32 `json:"key_fee"`
	TickSpacing               int32  `json:"tick_spacing"`
	Hooks                     string `json:"hooks"`
	Parameters                string `json:"parameters"`
	TickWordRadius            uint32 `json:"tick_word_radius"`
	MaxInitializedTicks       uint32 `json:"max_initialized_ticks"`
	MaxSteps                  uint32 `json:"max_steps"`
}
type PoolConfig struct {
	PancakeBscLm      *PancakeBscLmConfig          `json:"pancake_bsc_lm,omitempty"`
	PancakeLm         *PancakeLmConfig             `json:"pancake_lm,omitempty"`
	SlipstreamContext *SlipstreamContextConfig     `json:"slipstream_context,omitempty"`
	Lfj               *LfjConfig                   `json:"lfj,omitempty"`
	InfinityBin       *InfinityBinConfig           `json:"infinity_bin,omitempty"`
	SingletonAdvanced *SingletonAdvancedPoolConfig `json:"singleton_advanced,omitempty"`
	MultiAsset        *MultiAssetConfig            `json:"multi_asset,omitempty"`
	Weighted          *WeightedPoolConfig          `json:"weighted,omitempty"`
	RaydiumCLMM       *RaydiumCLMMConfig           `json:"raydium_clmm,omitempty"`

	OrcaWhirlpool *OrcaWhirlpoolConfig `json:"orca_whirlpool,omitempty"`

	RaydiumCPMM *RaydiumCPMMConfig `json:"raydium_cpmm,omitempty"`

	SingletonCL *SingletonCLPoolConfig `json:"singleton_cl,omitempty"`

	Stable         *StablePoolConfig `json:"stable,omitempty"`
	V2             *V2PoolConfig     `json:"v2,omitempty"`
	V3             *V3PoolConfig     `json:"v3,omitempty"`
	ID             string            `json:"id"`
	Protocol       string            `json:"protocol"`
	Token0         string            `json:"token0"`
	Token1         string            `json:"token1"`
	FeeNumerator   uint64            `json:"fee_numerator"`
	FeeDenominator uint64            `json:"fee_denominator"`
	ProgramID      string            `json:"program_id"`
	Accounts       []string          `json:"accounts"`
}
type WebsocketConfig struct {
	URL                string `json:"url"`
	DialTimeoutMS      uint64 `json:"dial_timeout_ms"`
	IdleTimeoutMS      uint64 `json:"idle_timeout_ms"`
	SessionTimeoutMS   uint64 `json:"session_timeout_ms"`
	MaxMessageBytes    int64  `json:"max_message_bytes"`
	Queue              int    `json:"queue"`
	MaxMessages        uint64 `json:"max_messages"`
	MaxConnections     int    `json:"max_connections"`
	ReconnectBackoffMS uint64 `json:"reconnect_backoff_ms"`
	MaxBackoffMS       uint64 `json:"max_backoff_ms"`
}
type Chain struct {
	NativeProof        *NativeProofConfig    `json:"native_proof,omitempty"`
	ReceiptsMode       string                `json:"receipts_mode,omitempty"`
	ProviderProbe      *ProviderProbeConfig  `json:"provider_probe,omitempty"`
	DynamicCatalog     *DynamicCatalogConfig `json:"dynamic_catalog,omitempty"`
	BlockIntegrity     string                `json:"block_integrity,omitempty"`
	Websocket          *WebsocketConfig      `json:"websocket,omitempty"`
	Key                string                `json:"key"`
	Owner              string                `json:"owner_service"`
	Assignment         uint64                `json:"assignment_version"`
	Genesis            string                `json:"genesis_hash"`
	RPCURLs            []string              `json:"rpc_urls"`
	AnalysisIntervalMS uint64                `json:"analysis_interval_ms,omitempty"`
	Pools              []PoolConfig          `json:"pools"`
	Confirmation       string                `json:"confirmation_tag"`
	ReorgWindow        uint64                `json:"auto_reorg_window"`
}
type Config struct {
	Control    Control    `json:"control"`
	Service    Service    `json:"service"`
	Startup    Startup    `json:"startup"`
	History    History    `json:"history"`
	Logging    Logging    `json:"logging"`
	Journal    Journal    `json:"journal"`
	State      State      `json:"state"`
	API        API        `json:"api"`
	Chains     []Chain    `json:"chains"`
	Routing    Routing    `json:"routing"`
	Simulation Simulation `json:"simulation"`
}
type Snapshot struct {
	Config       Config
	Release      string
	PublicHash   string
	EconomicHash string
	Root         string
	ControlDSN   string
	Token        string
}
type ConfigProvider interface{ Load(string) ([]byte, error) }
type FileProvider struct{}

func (FileProvider) Load(path string) ([]byte, error) {
	info, statErr := os.Stat(path)
	if statErr != nil || !info.Mode().IsRegular() || info.Size() > maxFile {
		return nil, errors.New("CONFIG_FILE_LIMIT")
	}
	f, e := os.Open(path)
	if e != nil {
		return nil, errors.New("CONFIG_FILE_UNREADABLE")
	}
	defer f.Close()
	b, e := io.ReadAll(io.LimitReader(f, maxFile+1))
	if e != nil || len(b) > maxFile {
		return nil, errors.New("CONFIG_FILE_LIMIT")
	}
	return b, nil
}
func decodeDocument(b []byte) (map[string]any, error) {
	if bytes.Contains(b, []byte("${")) {
		return nil, errors.New("ENV_INTERPOLATION_FORBIDDEN")
	}
	var node yaml.Node
	d := yaml.NewDecoder(bytes.NewReader(b))
	if e := d.Decode(&node); e != nil {
		return nil, errors.New("INVALID_YAML")
	}
	var extra any
	if d.Decode(&extra) != io.EOF {
		return nil, errors.New("MULTI_DOCUMENT_FORBIDDEN")
	}
	var walk func(*yaml.Node, int) error
	walk = func(n *yaml.Node, depth int) error {
		if depth > 32 || n.Kind == yaml.AliasNode || n.Anchor != "" {
			return errors.New("YAML_ALIAS_OR_DEPTH")
		}
		if n.Kind == yaml.MappingNode {
			seen := map[string]bool{}
			for i := 0; i < len(n.Content); i += 2 {
				k := n.Content[i].Value
				if seen[k] || k == "<<" {
					return errors.New("DUPLICATE_CONFIG_KEY")
				}
				seen[k] = true
			}
		}
		for _, v := range n.Content {
			if e := walk(v, depth+1); e != nil {
				return e
			}
		}
		return nil
	}
	if e := walk(&node, 0); e != nil {
		return nil, e
	}
	var m map[string]any
	if e := node.Decode(&m); e != nil || m == nil {
		return nil, errors.New("CONFIG_REQUIRES_MAP")
	}
	return m, nil
}
func strict(v any, out any) error {
	b, e := json.Marshal(v)
	if e != nil {
		return errors.New("CONFIG_ENCODING")
	}
	d := json.NewDecoder(bytes.NewReader(b))
	d.DisallowUnknownFields()
	if d.Decode(out) != nil {
		return errors.New("CONFIG_SCHEMA_MISMATCH")
	}
	return nil
}
func Load(path, expected string) (*Snapshot, error) {
	provider := FileProvider{}
	path, e := filepath.Abs(path)
	if e != nil {
		return nil, errors.New("CONFIG_PATH")
	}
	path, e = filepath.EvalSymlinks(path)
	if e != nil {
		return nil, errors.New("CONFIG_PATH")
	}
	root := filepath.Dir(path)
	b, e := provider.Load(path)
	if e != nil {
		return nil, e
	}
	doc, e := decodeDocument(b)
	if e != nil {
		return nil, e
	}
	merged := doc
	if e := requireConfigFields(merged); e != nil {
		return nil, e
	}
	var cfg Config
	if e = strict(merged, &cfg); e != nil {
		return nil, e
	}
	if e = cfg.Validate(expected); e != nil {
		return nil, e
	}
	token := cfg.API.AuthToken
	publicDoc := map[string]any{}
	for key, value := range merged {
		// encoding/json accepts case variants of struct fields. Rebuild credential
		// containers from decoded types so aliases cannot enter the public hash.
		if !strings.EqualFold(key, "api") && !strings.EqualFold(key, "journal") {
			publicDoc[key] = value
		}
	}
	api, _ := json.Marshal(cfg.API)
	var publicAPI map[string]any
	_ = json.Unmarshal(api, &publicAPI)
	delete(publicAPI, "auth_token")
	publicDoc["api"] = publicAPI
	publicJournal := cfg.Journal
	publicJournal.SourceAuthTokens = nil
	publicDoc["journal"] = publicJournal
	public, e := domain.Hash(publicDoc)
	if e != nil {
		return nil, e
	}
	economic, e := domain.Hash(map[string]any{"routing": merged["routing"], "simulation": merged["simulation"], "chains": merged["chains"]})
	if e != nil {
		return nil, e
	}
	var controlDSN string
	if cfg.Control.Backend == "postgres" {
		secretPath := cfg.Control.DSNFile
		if !filepath.IsAbs(secretPath) {
			secretPath = filepath.Join(root, secretPath)
		}
		info, e := os.Stat(secretPath)
		if e != nil || !info.Mode().IsRegular() || info.Mode().Perm()&0077 != 0 || info.Size() > 4096 {
			return nil, errors.New("CONTROL_SECRET_PERMISSIONS")
		}
		secret, e := provider.Load(secretPath)
		if e != nil {
			return nil, errors.New("CONTROL_SECRET_INVALID")
		}
		controlDSN = strings.TrimSpace(string(secret))
		if controlDSN == "" {
			return nil, errors.New("CONTROL_SECRET_INVALID")
		}
	}
	return &Snapshot{Config: cfg, Release: strings.TrimSuffix(filepath.Base(path), filepath.Ext(path)), PublicHash: public, EconomicHash: economic, Root: root, Token: token, ControlDSN: controlDSN}, nil
}
func (c Config) Validate(expected string) error {
	if err := c.Journal.RawMaintenance.Validate(c.Journal.RawEvidence); err != nil {
		return err
	}
	if c.Journal.RawMaintenance != nil && expected != "chain-parser-go" {
		return errors.New("RAW_MAINTENANCE_OWNER_INVALID")
	}
	invalid := func() error { return errors.New("CONFIG_CONSTRAINT_VIOLATION") }
	if c.Service.Name != expected || (expected != "chain-parser-go" && expected != "chain-parser-rust" && expected != "dex-router") {
		return invalid()
	}
	if raw := c.Journal.RawEvidence; raw != nil {
		if expected != "chain-parser-go" || c.Service.Role != "online" || raw.MaxRecords < 1 || raw.MaxRecords > 1000000 || raw.MaxBytes <= 0 || raw.MaxBytes > 1<<40 || raw.MaxRecordBytes < int64(c.Journal.MaxBytes)*4+16384 || raw.MaxRecordBytes > 128*1024*1024 || raw.MaxRecordBytes > raw.MaxBytes {
			return invalid()
		}
	}
	journalAnalytics := expected == "chain-parser-go" && c.Service.Role == "analytics" && c.Service.Mode == "journal"
	if journalAnalytics && (c.Journal.Transport != "kafka" || c.Control.Backend != "disabled") {
		return invalid()
	}
	if (c.Service.Role != "online" && c.Service.Role != "analytics") || (c.Service.Mode != "rpc" && !journalAnalytics) {
		return invalid()
	}
	if c.Startup.Mode != "HEAD_FIRST" || c.Startup.Resume || c.Startup.HistoryBlocks || c.Startup.MaxAge == 0 || c.Startup.MaxTail == 0 || c.History.OnlineWrite || c.History.Namespace != "history" || c.History.MaxShare < 0 || c.History.MaxShare > 0.25 {
		return invalid()
	}
	if c.Logging.Format != "json" || (c.Logging.Level != "debug" && c.Logging.Level != "info" && c.Logging.Level != "warn" && c.Logging.Level != "error") || c.Logging.Queue <= 0 || c.Logging.Queue > 65536 || c.Logging.MaxBytes < 512 || c.Logging.MaxBytes > 65536 {
		return invalid()
	}
	if (c.Journal.Transport != "http-development" && c.Journal.Transport != "kafka") || c.Journal.PollMS == 0 || c.Journal.TimeoutMS == 0 || c.Journal.MaxBytes < 1024 || c.Journal.MaxBytes > 67108864 {
		return invalid()
	}
	if c.API.Listen == "" || c.API.AuthToken != "" && len(c.API.AuthToken) < 24 || c.API.MaxBody < 1024 || c.API.MaxBody > 8388608 || c.API.RPM == 0 || len(c.Chains) == 0 {
		return invalid()
	}
	if expected == "dex-router" && (c.Routing.MaxHops < 1 || c.Routing.MaxHops > 4 || c.Routing.MaxCandidates < 1 || c.Routing.MaxCandidates > 256 || c.Routing.MaxExpansions < 1 || c.Routing.MaxExpansions > 100000 || c.Routing.SplitParts < 1 || c.Routing.SplitParts > 100 || c.Routing.MaxSplits < 1 || c.Routing.MaxSplits > 2 || c.Routing.MaxAge == 0 || c.Routing.TTL == 0) {
		return invalid()
	}
	if expected == "dex-router" && c.Simulation.Backend != "disabled" && c.Simulation.Backend != "evm-rpc" && c.Simulation.Backend != "revm-fixture" {
		return invalid()
	}
	if expected == "dex-router" && (c.Simulation.MaxGas == 0 || c.Simulation.MaxReads == 0 || c.Simulation.TimeoutMS == 0) {
		return invalid()
	}
	sim := c.Simulation
	if sim.Backend == "revm-fixture" {
		if expected != "dex-router" || sim.RPCURL != "" || sim.StateFile == "" || len(sim.StateFile) > 4096 || strings.ContainsRune(sim.StateFile, '\x00') || sim.MaxStateBytes < 1024 || sim.MaxStateBytes > 16777216 || sim.MaxInstructions == 0 || sim.MaxInstructions > 100000000 || sim.MaxMemoryBytes < 65536 || sim.MaxMemoryBytes > 67108864 || sim.Concurrency == 0 || sim.Concurrency > 8 || len(sim.AllowedTargets) == 0 || len(sim.AllowedTargets) > 64 {
			return invalid()
		}
		targets := map[string]bool{}
		for _, target := range sim.AllowedTargets {
			if domain.ValidateEVMAddress(target) != nil || target == "0x"+strings.Repeat("0", 40) || targets[target] {
				return invalid()
			}
			targets[target] = true
		}
	} else if sim.StateFile != "" || sim.MaxStateBytes != 0 || sim.MaxInstructions != 0 || sim.MaxMemoryBytes != 0 || sim.Concurrency != 0 {
		return invalid()
	}
	if expected == "chain-parser-go" && c.State.Engine != "pebble" || expected == "chain-parser-rust" && c.State.Engine != "rocksdb" || expected == "dex-router" && c.State.Engine != "memory" {
		return invalid()
	}
	seen := map[string]bool{}
	for _, ch := range c.Chains {
		if err := ch.ProviderProbe.Validate(ch.Key); err != nil {
			return err
		}
		if err := ch.DynamicCatalog.Validate(ch.Key); err != nil {
			return err
		}
		if ch.ProviderProbe != nil && expected != "chain-parser-go" {
			return invalid()
		}
		if ch.DynamicCatalog != nil && (ch.ProviderProbe == nil || c.Journal.RawEvidence == nil || expected != "chain-parser-go") {
			return invalid()
		}
		if ch.ReceiptsMode != "" && (expected != "chain-parser-go" || ch.ReceiptsMode != "per-tx" && ch.ReceiptsMode != "bulk-auto") {
			return invalid()
		}
		if ch.NativeProof != nil && ch.BlockIntegrity != "known-evm-native-commitments-v2" {
			return invalid()
		}
		if ch.BlockIntegrity != "" {
			if expected != "chain-parser-go" {
				return invalid()
			}
			switch ch.BlockIntegrity {
			case "ethereum-header-transactions-v1":
				if ch.Key != "evm:1" {
					return invalid()
				}
			case "known-evm-native-commitments-v2":
				if !ch.NativeProof.valid() {
					return invalid()
				}
				switch ch.Key {
				case "evm:1", "evm:10", "evm:8453", "evm:56", "evm:137", "evm:42161", "evm:43114":
				default:
					return invalid()
				}
			case "known-evm-header-raw-transactions-v1":
				switch ch.Key {
				case "evm:1", "evm:10", "evm:8453", "evm:56", "evm:137":
				default:
					return invalid()
				}
			default:
				return invalid()
			}
		}
		if ws := ch.Websocket; ws != nil {
			parsed, e := url.Parse(ws.URL)
			if e != nil || parsed.Hostname() == "" || parsed.User != nil || parsed.Fragment != "" || parsed.RawQuery != "" ||
				(parsed.Scheme != "wss" && !(parsed.Scheme == "ws" && c.Control.AllowPlaintext && (parsed.Hostname() == "localhost" || parsed.Hostname() == "127.0.0.1" || parsed.Hostname() == "::1"))) ||
				expected != "chain-parser-go" || !strings.HasPrefix(ch.Key, "evm:") ||
				ws.DialTimeoutMS == 0 || ws.DialTimeoutMS > 60000 || ws.IdleTimeoutMS == 0 || ws.IdleTimeoutMS > 60000 || ws.SessionTimeoutMS == 0 || ws.SessionTimeoutMS > 3600000 ||
				ws.MaxMessageBytes <= 0 || ws.MaxMessageBytes > int64(c.Journal.MaxBytes) || ws.Queue < 1 || ws.Queue > 1024 || ws.MaxMessages == 0 || ws.MaxMessages > 1000000 ||
				ws.MaxConnections < 1 || ws.MaxConnections > 64 || ws.ReconnectBackoffMS == 0 || ws.MaxBackoffMS < ws.ReconnectBackoffMS || ws.MaxBackoffMS > 60000 {
				return invalid()
			}
		}

		if domain.ValidateChain(ch.Key) != nil || seen[ch.Key] || ch.Assignment == 0 || ch.Genesis == "" || ch.ReorgWindow == 0 {
			return invalid()
		}
		for _, p := range ch.Pools {
			profile, known := domain.Protocol(p.Protocol)
			if p.Protocol == domain.PancakeBscLmProtocol {
				if !p.validatePancakeBscLm(ch.Key) {
					return invalid()
				}
				continue
			}
			if p.PancakeBscLm != nil {
				return invalid()
			}
			if p.Protocol == domain.PancakeLmProtocol {
				if !p.validatePancakeLm(ch.Key) {
					return invalid()
				}
				continue
			}
			if p.PancakeLm != nil {
				return invalid()
			}
			if p.Protocol == domain.SlipstreamContextProtocol {
				if !p.validateSlipstreamContext(ch.Key) {
					return invalid()
				}
				continue
			}
			if p.SlipstreamContext != nil {
				return invalid()
			}
			if p.Protocol == domain.LfjProtocol {
				if !p.validateLfj(ch.Key) {
					return invalid()
				}
				continue
			}
			if p.Lfj != nil {
				return invalid()
			}
			if p.MultiAsset != nil && p.MultiAsset.ManagedV2 != nil && !isManagedV2Protocol(p.Protocol) {
				return invalid()
			}
			if p.InfinityBin != nil && (!known || profile.Family != "infinity-bin") {
				return invalid()
			}
			if known && profile.Family == "infinity-bin" {
				if !p.validateInfinityBin(ch.Key) {
					return invalid()
				}
				continue
			}

			if p.SingletonAdvanced != nil && (!known || profile.Family != "singleton-advanced") {
				return invalid()
			}
			if known && profile.Family == "singleton-advanced" {
				v := p.SingletonAdvanced
				if v == nil || p.MultiAsset != nil || p.Weighted != nil || p.SingletonCL != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil || p.RaydiumCPMM != nil || !strings.HasPrefix(ch.Key, "evm:") || p.Protocol != v.Protocol+"-advanced" || p.ID != v.Manager+"/"+v.PoolID || p.Token0 != v.Currency0 || p.Token1 != v.Currency1 || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(v.KeyFee) || v.TickWordRadius > 16 || v.MaxInitializedTicks == 0 || v.MaxInitializedTicks > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 {
					return invalid()
				}
				state := domain.SingletonAdvancedState{Protocol: v.Protocol, Currency0: v.Currency0, Currency1: v.Currency1, KeyFee: v.KeyFee, SnapshotBlockHash: "0x" + strings.Repeat("1", 64), Snapshot: domain.SingletonCLState{Manager: v.Manager, ManagerCodeSHA256: v.ExpectedManagerCodeSHA256, PoolID: v.PoolID, LPFee: func() uint32 {
					if v.KeyFee == domain.SingletonDynamicFeeFlag {
						return 0
					}
					return v.KeyFee
				}(), Hooks: v.Hooks, Parameters: v.Parameters, Core: domain.ConcentratedState{TickSpacing: v.TickSpacing}}}
				if state.ValidateIdentity() != nil {
					return invalid()
				}
				continue
			}

			if p.MultiAsset != nil && (!known || profile.Family != "multi") {
				return invalid()
			}
			if known && profile.Family == "multi" {
				v := p.MultiAsset
				if v == nil || (v.TokenPolicy != "standard-no-bpt" && !(p.Protocol == domain.ManagedV2BptProtocol && v.TokenPolicy == "standard-bpt") && !(p.Protocol == domain.ManagedV2BreakerProtocol && v.TokenPolicy == "standard-breaker-bpt") && !(p.Protocol == "balancer-v2-composable-regular" && v.TokenPolicy == "standard-with-own-bpt") && !(p.Protocol == "curve-metausd-direct" && v.TokenPolicy == "standard-base-lp")) || p.Weighted != nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil || p.ProgramID != "" || len(p.Accounts) != 0 {
					return invalid()
				}
				if isManagedV2Protocol(p.Protocol) {
					if !p.validateManagedV2(ch.Key) {
						return invalid()
					}
					continue
				}
				if (p.Protocol != "curve-crypto2-full" && p.Protocol != "curve-crypto2-ramp") && (v.ExpectedLPToken != "" || v.ExpectedLPTokenCodeSHA256 != "") {
					return invalid()
				}
				if (p.Protocol == "curve-crypto2-full" || p.Protocol == "curve-crypto2-ramp") && (domain.ValidateEVMAddress(v.ExpectedLPToken) != nil || v.ExpectedLPTokenCodeSHA256 != domain.CurveCryptoLPCodeSHA256 || v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "") {
					return invalid()
				}
				if p.Protocol != "curve-metausd-direct" && v.ExpectedBasePoolCodeSHA256 != "" {
					return invalid()
				}
				if p.Protocol == "curve-metausd-direct" {
					if v.TokenPolicy != "standard-base-lp" || v.ExpectedImplementation == "" {
						return invalid()
					}
					for _, other := range ch.Pools {
						if other.ID == domain.CurveMetaBasePool {
							return invalid()
						}
					}
				}
				state := &domain.MultiAssetState{CodeSHA256: v.ExpectedCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Assets: []domain.MultiAsset{}, Weights: []domain.Amount{}, Amplification: "10000", AdminFee: 5000000000}
				for _, a := range v.Tokens {
					state.Assets = append(state.Assets, domain.MultiAsset{Token: a.Token, Balance: "0", Scaling: "1000000000000000000", CodeSHA256: a.ExpectedCodeSHA256})
				}
				if len(state.Assets) < 2 {
					return invalid()
				}
				if strings.HasPrefix(p.Protocol, "balancer-v2-") {
					state.Amplification = "0"
					state.AdminFee = 0
					if v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" {
						return invalid()
					}
					remaining := big.NewInt(1000000000000000000)
					n := len(state.Assets)
					for i := 0; i < n; i++ {
						weight := big.NewInt(10000000000000000)
						if i == n-1 {
							weight = remaining
						}
						state.Weights = append(state.Weights, domain.Amount(weight.String()))
						remaining.Sub(remaining, weight)
					}
					if p.Protocol == "balancer-v2-composable-regular" {
						state.Weights = []domain.Amount{}
						state.Amplification = "100000"
					}
				} else {
					if v.ExpectedImplementation != "" {
						if domain.ValidateEVMIdentity(v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256) != nil {
							return invalid()
						}
					} else if v.ExpectedImplementationCodeSHA256 != "" {
						return invalid()
					}
				}
				if p.Protocol == "balancer-v2-managed-fees" {
					return errors.New("MANAGED_FEE_RUNTIME_UNAVAILABLE")
				}
				if p.Protocol == "curve-crypto2-full" || p.Protocol == "curve-crypto2-ramp" {
					outFee := p.FeeNumerator
					if outFee < 500000 {
						outFee = 500000
					}
					state.CurveCrypto = &domain.CurveCryptoState{Gamma: "10000000000", OutFee: domain.Amount(new(big.Int).SetUint64(outFee).String()), FeeGamma: "230000000000000", AllowedExtraProfit: "1000000000000000000", AdjustmentStep: "146000000000000", MAHalfTime: 600, Invariant: "2000000000000000000000000", PriceScale: "1000000000000000000", PriceOracle: "1000000000000000000", LastPrices: "1000000000000000000", XCPProfit: "1000000000000000000", XCPProfitA: "1000000000000000000", VirtualPrice: "1000000000000000000", LPToken: v.ExpectedLPToken, LPTokenCodeSHA256: v.ExpectedLPTokenCodeSHA256, TotalSupply: "1000000000000000000000000", AdminReceiver: "0x" + strings.Repeat("0", 40), AdminReceiverBalance: "0", RawBalances: []domain.Amount{"0", "0"}, Timestamp: 1, BlockNumber: 1, BlockHash: "0x" + strings.Repeat("1", 64)}
				}
				if p.Protocol == "curve-crypto2-ramp" {
					state.CurveCrypto.Ramp = &domain.CurveCryptoRamp{InitialAmp: state.Amplification, InitialGamma: state.CurveCrypto.Gamma, InitialTime: 0}
				}
				if p.Protocol == "curve-metausd-direct" {
					state.CurveMeta = &domain.CurveMetaState{Implementation: v.ExpectedImplementation, ImplementationCodeSHA256: v.ExpectedImplementationCodeSHA256, BasePool: domain.CurveMetaBasePool, BasePoolCodeSHA256: v.ExpectedBasePoolCodeSHA256, BaseVirtualPrice: "1000000000000000000", StoredBalances: []domain.Amount{"0", "0"}, AdminBalances: []domain.Amount{"0", "0"}, Timestamp: 1, BlockNumber: 1, BlockHash: "0x" + strings.Repeat("1", 64)}
				}
				if p.Protocol == "curve-stableswap-ng" {
					if v.ExpectedImplementation != "" || v.ExpectedImplementationCodeSHA256 != "" {
						return invalid()
					}
					n := len(state.Assets)
					stored := make([]domain.Amount, n)
					admin := make([]domain.Amount, n)
					prices := make([]domain.Amount, n-1)
					for i := range stored {
						stored[i] = "0"
						admin[i] = "0"
					}
					for i := range prices {
						prices[i] = "0"
					}
					state.CurveNG = &domain.CurveNGState{StoredBalances: stored, AdminBalances: admin, LastPricesPacked: prices, LastDPacked: "0", MALastTime: "0", MAExpTime: 1, DMaTime: 1, Timestamp: 1, BlockNumber: 1, BlockHash: "0x" + strings.Repeat("1", 64), OffpegFeeMultiplier: "10000000000"}
				}
				model := domain.Pool{MultiAsset: state, ChainKey: ch.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
				if model.Validate() != nil {
					return invalid()
				}
				continue
			}

			if known && profile.Family == "weighted" {
				v := p.Weighted
				if v == nil || p.Stable != nil || p.V2 != nil || p.V3 != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || p.RaydiumCLMM != nil || v.TokenPolicy != "standard-no-bpt" {
					return invalid()
				}
				state := &domain.WeightedState{CodeSHA256: v.ExpectedCodeSHA256, Vault: v.ExpectedVault, VaultCodeSHA256: v.ExpectedVaultCodeSHA256, PoolID: v.PoolID, Scaling0: "1000000000000000000", Scaling1: "1000000000000000000", Weight0: "500000000000000000", Weight1: "500000000000000000", Token0CodeSHA256: v.ExpectedToken0CodeSHA256, Token1CodeSHA256: v.ExpectedToken1CodeSHA256}
				model := domain.Pool{Weighted: state, ChainKey: ch.Key, PoolID: p.ID, Protocol: p.Protocol, Token0: p.Token0, Token1: p.Token1, Reserve0: "0", Reserve1: "0", FeeNumerator: p.FeeNumerator, FeeDenominator: p.FeeDenominator, AdapterVersion: profile.AdapterVersion, TokenBehavior: "standard"}
				if model.Validate() != nil || p.ProgramID != "" || len(p.Accounts) != 0 {
					return invalid()
				}
				continue
			} else if p.Weighted != nil {
				return invalid()
			}

			if known && profile.Family == "rayclmm" {
				v := p.RaydiumCLMM
				if v == nil || !strings.HasPrefix(ch.Key, "solana:") || ch.Confirmation != "finalized" || p.ProgramID != domain.RayCLMMProgram || v.ExpectedProgramdata != domain.RayCLMMProgramData || len(p.Accounts) != 7 || p.Accounts[0] != p.ID || p.Accounts[4] != p.Token0 || p.Accounts[5] != p.Token1 || p.FeeDenominator != 1000000 || p.FeeNumerator >= 1000000 || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || p.OrcaWhirlpool != nil || v.MaxSteps == 0 || v.MaxSteps > 8192 || len(v.TickArrayStarts) < 3 || len(v.TickArrayStarts) > 7 {
					return invalid()
				}
				span := int64(v.TickArrayStarts[1]) - int64(v.TickArrayStarts[0])
				if span <= 0 || span%60 != 0 || span/60 > 1000 {
					return invalid()
				}
				for i, n := range v.TickArrayStarts {
					start := int64(n)
					if start%span != 0 || (p.Protocol != "raydium-clmm-bitmap" && p.Protocol != "raydium-clmm-custom-seed" && p.Protocol != "raydium-clmm-dynamic" && (start/span < -512 || start/span > 511)) || start/span < -7680 || start/span > 7679 || start > 443636 || start+span <= -443636 || i > 0 && start != int64(v.TickArrayStarts[i-1])+span {
						return invalid()
					}
				}
				keys := map[string]bool{}
				for _, key := range append(append([]string{}, p.Accounts...), p.ProgramID, v.ExpectedProgramdata, domain.RaydiumTokenProgram, domain.SolanaClock) {
					if domain.ValidateSolanaPubkey(key) != nil || keys[key] {
						return invalid()
					}
					keys[key] = true
				}
				for _, h := range []string{v.ExpectedProgramCodeSHA256, v.ExpectedTokenProgramCodeSHA256} {
					b, e := hex.DecodeString(h)
					if e != nil || len(b) != 32 || h != strings.ToLower(h) {
						return invalid()
					}
				}
				if domain.SolanaPubkeyOrder(p.Token0, p.Token1) >= 0 {
					return invalid()
				}
				continue
			} else if p.RaydiumCLMM != nil {
				return invalid()
			}

			if known && profile.Family == "orca" {
				v := p.OrcaWhirlpool
				if v == nil || v.ExpectedProgramdata != domain.OrcaProgramData || !strings.HasPrefix(ch.Key, "solana:") || ch.Confirmation != "finalized" || p.ProgramID != domain.OrcaProgram || len(p.Accounts) != 6 || p.Accounts[0] != p.ID || p.Accounts[4] != p.Token0 || p.Accounts[5] != p.Token1 || p.FeeDenominator != 1000000 || p.FeeNumerator > 60000 || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil || p.RaydiumCPMM != nil || v.MaxSteps == 0 || v.MaxSteps > 8192 || len(v.TickArrayStarts) < 3 || len(v.TickArrayStarts) > 7 {
					return invalid()
				}
				if p.Protocol == "orca-whirlpools-adaptive" || p.Protocol == "orca-whirlpools-adaptive-rewards" || p.Protocol == "orca-whirlpools-adaptive-dynamic" {
					if len(v.TickArrayStarts) != 3 {
						return invalid()
					}
					span := int64(v.TickArrayStarts[1]) - int64(v.TickArrayStarts[0])
					if span <= 0 || span%88 != 0 || span/88 > 65535 || int64(v.TickArrayStarts[2])-int64(v.TickArrayStarts[1]) != span {
						return invalid()
					}
					minStart := int64(domain.OrcaMinTick) / span
					if int64(domain.OrcaMinTick)%span != 0 {
						minStart--
					}
					minStart *= span
					for _, start := range v.TickArrayStarts {
						if int64(start)%span != 0 || int64(start) < minStart || start > domain.OrcaMaxTick {
							return invalid()
						}
					}
				}
				for i, start := range v.TickArrayStarts {
					if i > 0 && start <= v.TickArrayStarts[i-1] {
						return invalid()
					}
				}
				keys := map[string]bool{}
				for _, key := range append(append([]string{}, p.Accounts...), p.ProgramID, v.ExpectedProgramdata, domain.RaydiumTokenProgram, domain.SolanaClock) {
					if domain.ValidateSolanaPubkey(key) != nil || keys[key] {
						return invalid()
					}
					keys[key] = true
				}
				for _, h := range []string{v.ExpectedProgramCodeSHA256, v.ExpectedTokenProgramCodeSHA256} {
					b, e := hex.DecodeString(h)
					if e != nil || len(b) != 32 || h != strings.ToLower(h) {
						return invalid()
					}
				}
				if domain.SolanaPubkeyOrder(p.Token0, p.Token1) >= 0 {
					return invalid()
				}
				continue
			} else if p.OrcaWhirlpool != nil {
				return invalid()
			}

			if known && profile.Family == "raydium" {
				v := p.RaydiumCPMM
				if v == nil || v.ExpectedProgramdata != domain.RaydiumProgramData || !strings.HasPrefix(ch.Key, "solana:") || ch.Confirmation != "finalized" || p.ProgramID != domain.RaydiumCPMMProgram || len(p.Accounts) != 6 || p.Accounts[0] != p.ID || p.Accounts[4] != p.Token0 || p.Accounts[5] != p.Token1 || p.FeeDenominator != 1000000 || p.FeeNumerator >= 1000000 || p.V2 != nil || p.V3 != nil || p.Stable != nil || p.SingletonCL != nil {
					return invalid()
				}
				keys := map[string]bool{}
				for _, key := range append(append([]string{}, p.Accounts...), p.ProgramID, v.ExpectedProgramdata, domain.RaydiumTokenProgram, domain.SolanaClock) {
					if domain.ValidateSolanaPubkey(key) != nil || keys[key] {
						return invalid()
					}
					keys[key] = true
				}
				for _, h := range []string{v.ExpectedProgramCodeSHA256, v.ExpectedTokenProgramCodeSHA256} {
					b, e := hex.DecodeString(h)
					if e != nil || len(b) != 32 || h != strings.ToLower(h) {
						return invalid()
					}
				}
				// Decoded byte order is Solana Pubkey order, unlike base58 lexical order.
				if domain.SolanaPubkeyOrder(p.Token0, p.Token1) >= 0 {
					return invalid()
				}
				continue
			} else if p.RaydiumCPMM != nil {
				return invalid()
			}

			if (!known || profile.Family != "singleton") && strings.HasPrefix(ch.Key, "evm:") && strings.HasPrefix(strings.ToLower(p.ID), "0x") {
				if e := domain.ValidateEVMAddress(p.ID); e != nil {
					return invalid()
				}
			}
			if known && !strings.HasPrefix(ch.Key, "evm:") {
				return invalid()
			}
			if known && profile.Family == "singleton" {
				v := p.SingletonCL
				if v == nil || p.V3 != nil || p.Stable != nil || p.V2 != nil || domain.ValidateEVMIdentity(v.Manager, v.ExpectedManagerCodeSHA256) != nil || v.TickWordRadius > 16 || v.MaxInitializedTicks == 0 || v.MaxInitializedTicks > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 || p.FeeDenominator != 1000000 || p.FeeNumerator != uint64(v.LPFee) {
					return invalid()
				}
				id, e := domain.SingletonPoolID(p.Protocol, v.Manager, p.Token0, p.Token1, v.LPFee, v.TickSpacing, v.Parameters)
				if e != nil || id != v.PoolID || p.ID != v.Manager+"/"+id {
					return invalid()
				}
			} else if p.SingletonCL != nil {
				return invalid()
			}
			if known && (profile.Family == "aero" || profile.Family == "curve" || profile.Family == "balancer") {
				v := p.Stable
				if v == nil || p.V2 != nil || p.V3 != nil || p.FeeDenominator != profile.FeeDenominator {
					return invalid()
				}
				if domain.ValidateEVMIdentity(p.ID, v.ExpectedCodeSHA256) != nil || domain.ValidateEVMAddress(p.Token0) != nil || domain.ValidateEVMAddress(p.Token1) != nil {
					return invalid()
				}
				if v.ExpectedImplementation != "" {
					if domain.ValidateEVMIdentity(v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256) != nil {
						return invalid()
					}
				} else if v.ExpectedImplementationCodeSHA256 != "" {
					return invalid()
				}
				if profile.Family == "aero" {
					if domain.ValidateEVMIdentity(v.ExpectedFactory, v.ExpectedFactoryCodeSHA256) != nil || v.ExpectedVault != "" || v.PoolID != "" {
						return invalid()
					}
				}
				if profile.Family == "curve" && (v.ExpectedFactory != "" || v.ExpectedVault != "" || v.PoolID != "") {
					return invalid()
				}
				if profile.Family == "balancer" {
					poolID, e := hex.DecodeString(strings.TrimPrefix(v.PoolID, "0x"))
					if domain.ValidateEVMIdentity(v.ExpectedVault, v.ExpectedVaultCodeSHA256) != nil || len(poolID) != 32 || e != nil || !strings.HasPrefix(v.PoolID, "0x") || v.ExpectedFactory != "" {
						return invalid()
					}
				}
			} else if p.Stable != nil {
				return invalid()
			}
			if p.V2 != nil {
				for _, a := range []string{p.ID, p.Token0, p.Token1} {
					raw, e := hex.DecodeString(strings.TrimPrefix(a, "0x"))
					if e != nil || len(raw) != 20 || !strings.HasPrefix(a, "0x") || a != strings.ToLower(a) {
						return invalid()
					}
				}
			}
			if !known && expected == "chain-parser-go" {
				return invalid()
			}
			if p.V2 != nil {
				if !known || profile.Family != "v2" || domain.ValidateEVMIdentity(p.V2.ExpectedFactory, p.V2.ExpectedCodeSHA256) != nil {
					return invalid()
				}
			}
			if known && profile.Family == "v2" {
				if profile.RequireIdentity && p.V2 == nil {
					return invalid()
				}
				if p.Protocol == domain.V2CompleteProtocol {
					if p.V2 == nil || p.V2.ExpectedCodeSHA256 != domain.V2CompletePairCodeSHA256 || p.Token0 >= p.Token1 || p.ProgramID != "" || len(p.Accounts) != 0 {
						return invalid()
					}
					for _, a := range []string{p.ID, p.Token0, p.Token1, p.V2.ExpectedFactory} {
						if domain.ValidateEVMAddress(a) != nil || strings.Trim(strings.TrimPrefix(a, "0x"), "0") == "" {
							return invalid()
						}
					}
				}
				if profile.FeeDenominator != 0 && (p.FeeNumerator != profile.FeeNumerator || p.FeeDenominator != profile.FeeDenominator) {
					return invalid()
				}
			}
			if known && profile.Family == "v3" {
				v := p.V3
				if v != nil && v.ExpectedImplementation != "" {
					if domain.ValidateEVMIdentity(v.ExpectedImplementation, v.ExpectedImplementationCodeSHA256) != nil {
						return invalid()
					}
				} else if v != nil && v.ExpectedImplementationCodeSHA256 != "" {
					return invalid()
				}
				if (p.Protocol == "aerodrome-slipstream" || v != nil && v.ExpectedFactoryCodeSHA256 != "") && (v == nil || domain.ValidateEVMIdentity(v.ExpectedFactory, v.ExpectedFactoryCodeSHA256) != nil) {
					return invalid()
				}
				if v == nil || v.TickWordRadius > 16 || v.MaxInitializedTicks == 0 || v.MaxInitializedTicks > 4096 || v.MaxSteps == 0 || v.MaxSteps > 8192 || p.FeeDenominator != 1000000 || p.FeeNumerator >= 1000000 {
					return invalid()
				}
				for _, a := range []string{p.ID, p.Token0, p.Token1, v.ExpectedFactory} {
					raw, e := hex.DecodeString(strings.TrimPrefix(a, "0x"))
					if e != nil || len(raw) != 20 || !strings.HasPrefix(a, "0x") || a != strings.ToLower(a) {
						return invalid()
					}
				}
				code, e := hex.DecodeString(v.ExpectedCodeSHA256)
				if e != nil || len(code) != 32 || v.ExpectedCodeSHA256 != strings.ToLower(v.ExpectedCodeSHA256) {
					return invalid()
				}
			} else if p.V3 != nil {
				return invalid()
			}
		}
		seen[ch.Key] = true
		if expected != "dex-router" && ch.Owner != expected {
			return invalid()
		}
		if strings.HasPrefix(ch.Key, "evm:") && ch.Owner != "chain-parser-go" || strings.HasPrefix(ch.Key, "solana:") && ch.Owner != "chain-parser-rust" {
			return invalid()
		}
		if expected != "dex-router" && c.Service.Mode == "rpc" && len(ch.RPCURLs) == 0 {
			return invalid()
		}
	}
	if c.Startup.MaxAge > 315360000000 || c.Startup.MaxTail > 100000 || c.Journal.PollMS > 60000 || c.Journal.TimeoutMS > 60000 || c.API.QuoteTimeoutMS > 60000 || c.API.RPM > 1000000 || c.Routing.MaxAge > 315360000000 || c.Routing.TTL > 600000 || c.Control.LeaseTTLMS > 3600000 || c.Control.MinCommitMS == 0 || c.Journal.Kafka.TransactionTimeoutMS > 60000 || c.Simulation.MaxGas > 1000000000 || c.Simulation.MaxReads > 1000000 || c.Simulation.TimeoutMS > 60000 {
		return invalid()
	}
	if c.Control.InstanceID == "" || c.Control.LeaseTTLMS < 1000 || c.Control.RenewalMS == 0 || c.Control.RenewalMS >= c.Control.LeaseTTLMS/3 || c.Control.MinCommitMS >= c.Control.LeaseTTLMS {
		return invalid()
	}
	if c.Control.Backend != "disabled" && c.Control.Backend != "postgres" {
		return invalid()
	}
	if c.Control.Backend == "postgres" && (c.Control.DSNFile == "" || !c.Control.AllowPlaintext) {
		return invalid()
	}
	k := c.Journal.Kafka
	if k.ReplicaID == "" || k.ChunkBytes < 1024 || k.ChunkBytes > 1048576 || k.TransactionTimeoutMS == 0 || k.MaxTailRecords < 3 || k.MaxTailRecords > 100000 {
		return invalid()
	}
	if c.Journal.Transport == "kafka" {
		if len(k.Brokers) == 0 || k.SecurityProtocol != "PLAINTEXT" || !k.AllowPlaintext || c.Control.Backend != "postgres" && expected != "dex-router" && !journalAnalytics || c.Control.MinCommitMS <= k.TransactionTimeoutMS {
			return invalid()
		}
		seenTopics := map[string]bool{}
		for _, chain := range c.Chains {
			topic := k.Topics[chain.Key]
			if topic == "" || seenTopics[topic] {
				return invalid()
			}
			seenTopics[topic] = true
		}
	}
	return nil
}
func (s *Snapshot) Resolve(p string) string {
	if filepath.IsAbs(p) {
		return p
	}
	return filepath.Join(s.Root, p)
}

func requireConfigFields(doc map[string]any) error {
	required := map[string][]string{
		"control": {"backend", "dsn_file", "instance_id", "lease_ttl_ms", "renewal_ms", "min_commit_remaining_ms", "allow_plaintext_development"},
		"service": {"name", "role", "mode"}, "startup": {"mode", "resume_previous_online_cursor", "history_catchup_blocks_online_readiness", "max_baseline_age_ms", "max_tail_blocks"},
		"history": {"enabled", "namespace", "online_write_permission", "max_worker_share"}, "logging": {"level", "format", "queue_capacity", "max_event_bytes"},
		"journal": {"transport", "source_urls", "poll_interval_ms", "request_timeout_ms", "max_payload_bytes", "kafka"}, "state": {"engine", "path"},
		"api": {"listen", "max_body_bytes", "requests_per_minute"},
	}
	for section, keys := range required {
		m, ok := doc[section].(map[string]any)
		if !ok {
			return errors.New("CONFIG_REQUIRED_SECTION")
		}
		for _, key := range keys {
			if _, ok := m[key]; !ok {
				return errors.New("CONFIG_REQUIRED_FIELD")
			}
		}
	}
	chains, ok := doc["chains"].([]any)
	if !ok {
		return errors.New("CONFIG_CHAINS_TYPE")
	}
	for _, entry := range chains {
		m, ok := entry.(map[string]any)
		if !ok {
			return errors.New("CONFIG_CHAIN_TYPE")
		}
		for _, key := range []string{"key", "owner_service", "assignment_version", "genesis_hash", "rpc_urls", "pools", "confirmation_tag", "auto_reorg_window"} {
			if _, ok := m[key]; !ok {
				return errors.New("CONFIG_REQUIRED_FIELD")
			}
		}
	}
	return nil
}
