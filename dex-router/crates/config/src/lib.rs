mod lfj;
mod meteora;
mod pancake_bsc_lm;
mod pancake_lm;
mod pump;
pub use pancake_bsc_lm::PancakeBscLmConfig;
mod slipstream_context;
pub use pancake_lm::PancakeLmConfig;
pub use slipstream_context::{SlipstreamContextConfig, SlipstreamModuleConfig};
mod managed_v2;
pub use managed_v2::{ManagedV2BptConfig, ManagedV2PoolConfig};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
use xone_domain::{err, hash, validate_chain, Amount, Result};
macro_rules! config_struct {($name:ident {$($f:ident:$ty:ty),* $(,)?})=>{#[derive(Clone,Debug,Serialize,Deserialize)] #[serde(deny_unknown_fields)] pub struct $name {$(pub $f:$ty),*}};}
config_struct!(Service {
    name: String,
    role: String,
    mode: String
});
config_struct!(Startup {
    mode: String,
    resume_previous_online_cursor: bool,
    history_catchup_blocks_online_readiness: bool,
    max_baseline_age_ms: u64,
    max_tail_blocks: u64
});
config_struct!(History {
    enabled: bool,
    namespace: String,
    online_write_permission: bool,
    max_worker_share: f64
});
config_struct!(Logging {
    level: String,
    format: String,
    queue_capacity: usize,
    max_event_bytes: usize
});
config_struct!(Control {
    backend: String,
    dsn_file: String,
    instance_id: String,
    lease_ttl_ms: u64,
    renewal_ms: u64,
    min_commit_remaining_ms: u64,
    allow_plaintext_development: bool
});
config_struct!(Kafka {brokers:Vec<String>,topics:BTreeMap<String,String>,replica_id:String,chunk_bytes:usize,transaction_timeout_ms:u64,max_tail_records:usize,security_protocol:String,allow_plaintext_development:bool});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub kafka: Kafka,
    pub transport: String,
    pub source_urls: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub source_auth_tokens: BTreeMap<String, String>,
    pub poll_interval_ms: u64,
    pub request_timeout_ms: u64,
    pub max_payload_bytes: usize,
}
config_struct!(State {
    engine: String,
    path: String
});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Api {
    pub listen: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub solana_runtime_manifest: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub cors_allowed_origins: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub auth_token: String,
    pub max_body_bytes: usize,
    pub requests_per_minute: u64,
    #[serde(default)]
    pub quote_timeout_ms: u64,
    #[serde(default)]
    pub quote_concurrency: usize,
}
config_struct!(Routing {
    max_hops: usize,
    max_candidates: usize,
    max_expansions: usize,
    split_parts: usize,
    max_splits: usize,
    route_activation_cost: Amount,
    max_state_age_ms: u64,
    quote_ttl_ms: u64
});
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Simulation {
    pub backend: String,
    pub rpc_url: String,
    pub max_gas: u64,
    pub max_state_reads: u64,
    pub timeout_ms: u64,
    pub allowed_targets: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state_file: String,
    #[serde(default, skip_serializing_if = "simulation_zero_u64")]
    pub max_state_bytes: u64,
    #[serde(default, skip_serializing_if = "simulation_zero_u64")]
    pub max_instructions: u64,
    #[serde(default, skip_serializing_if = "simulation_zero_u64")]
    pub max_memory_bytes: u64,
    #[serde(default, skip_serializing_if = "simulation_zero_usize")]
    pub concurrency: usize,
}
fn simulation_zero_u64(value: &u64) -> bool {
    *value == 0
}
fn simulation_zero_usize(value: &usize) -> bool {
    *value == 0
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct V3PoolConfig {
    #[serde(default)]
    pub expected_implementation: String,
    #[serde(default)]
    pub expected_implementation_code_sha256: String,
    #[serde(default)]
    pub expected_factory_code_sha256: String,
    pub expected_factory: String,
    pub expected_code_sha256: String,
    pub tick_word_radius: u32,
    pub max_initialized_ticks: u32,
    pub max_steps: u32,
}
config_struct!(V2PoolConfig {
    expected_factory: String,
    expected_code_sha256: String
});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StablePoolConfig {
    pub expected_code_sha256: String,
    #[serde(default)]
    pub expected_implementation: String,
    #[serde(default)]
    pub expected_implementation_code_sha256: String,
    #[serde(default)]
    pub expected_factory: String,
    #[serde(default)]
    pub expected_factory_code_sha256: String,
    #[serde(default)]
    pub expected_vault: String,
    #[serde(default)]
    pub expected_vault_code_sha256: String,
    #[serde(default)]
    pub pool_id: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SingletonCLPoolConfig {
    pub manager: String,
    pub expected_manager_code_sha256: String,
    pub pool_id: String,
    pub lp_fee: u32,
    pub tick_spacing: i32,
    pub parameters: String,
    pub tick_word_radius: u32,
    pub max_initialized_ticks: u32,
    pub max_steps: u32,
}
/// The canonical ProgramData PDA for classic SPL Token under Upgradeable Loader.
pub const CLASSIC_TOKEN_PROGRAMDATA: &str = "3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2";
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TokenProgramUpgradeConfig {
    pub programdata: String,
    pub deployed_slot: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCPMMConfig {
    pub expected_programdata: String,
    pub expected_program_code_sha256: String,
    pub expected_deployed_slot: u64,
    pub expected_token_program_code_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_program_upgrade: Option<TokenProgramUpgradeConfig>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrcaWhirlpoolConfig {
    pub expected_programdata: String,
    pub expected_program_code_sha256: String,
    pub expected_deployed_slot: u64,
    pub expected_token_program_code_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_program_upgrade: Option<TokenProgramUpgradeConfig>,
    pub tick_array_starts: Vec<i32>,
    pub max_steps: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RaydiumCLMMConfig {
    pub expected_programdata: String,
    pub expected_program_code_sha256: String,
    pub expected_deployed_slot: u64,
    pub expected_token_program_code_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_program_upgrade: Option<TokenProgramUpgradeConfig>,
    pub tick_array_starts: Vec<i32>,
    pub max_steps: u32,
}
config_struct!(WeightedPoolConfig {
    expected_code_sha256: String,
    expected_vault: String,
    expected_vault_code_sha256: String,
    pool_id: String,
    expected_token0_code_sha256: String,
    expected_token1_code_sha256: String,
    token_policy: String
});
config_struct!(MultiTokenConfig {
    token: String,
    expected_code_sha256: String
});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MultiAssetConfig {
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "xone_domain::managed_v2_state::deserialize_present"
    )]
    pub managed_v2: Option<ManagedV2PoolConfig>,
    #[serde(default)]
    pub expected_lp_token: String,
    #[serde(default)]
    pub expected_lp_token_code_sha256: String,
    #[serde(default)]
    pub expected_base_pool_code_sha256: String,
    pub expected_code_sha256: String,
    #[serde(default)]
    pub expected_implementation: String,
    #[serde(default)]
    pub expected_implementation_code_sha256: String,
    #[serde(default)]
    pub expected_vault: String,
    #[serde(default)]
    pub expected_vault_code_sha256: String,
    #[serde(default)]
    pub pool_id: String,
    pub tokens: Vec<MultiTokenConfig>,
    pub token_policy: String,
}
config_struct!(InfinityBinConfig {
    manager: String,
    expected_manager_code_sha256: String,
    vault: String,
    expected_vault_code_sha256: String,
    pool_id: String,
    bin_step: u32,
    lp_fee: u32,
    expected_token0_code_sha256: String,
    expected_token1_code_sha256: String,
    token_policy: String,
    max_bins: u32,
    max_tree_words: u32,
    max_steps: u32
});
config_struct!(SingletonAdvancedPoolConfig {
    protocol: String,
    manager: String,
    expected_manager_code_sha256: String,
    pool_id: String,
    currency0: String,
    currency1: String,
    key_fee: u32,
    tick_spacing: i32,
    hooks: String,
    parameters: String,
    tick_word_radius: u32,
    max_initialized_ticks: u32,
    max_steps: u32
});
config_struct!(LfjConfig {
    expected_code_sha256: String,
    expected_implementation: String,
    expected_implementation_code_sha256: String,
    expected_factory: String,
    expected_factory_code_sha256: String,
    expected_token0_code_sha256: String,
    expected_token1_code_sha256: String,
    bin_step: u32,
    token_policy: String,
    max_bins: u32,
    max_steps: u32
});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MeteoraConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token2022_upgrade: Option<TokenProgramUpgradeConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bitmap_extension: Option<String>,
    pub expected_programdata: String,
    pub expected_program_code_sha256: String,
    pub expected_deployed_slot: u64,
    pub expected_token_program_code_sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_program_upgrade: Option<TokenProgramUpgradeConfig>,
    pub bin_array_indices: Vec<i64>,
    pub max_steps: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PumpConfig {
    pub expected_programdata: String,
    pub expected_deployed_slot: u64,
    pub expected_program_code_sha256: String,
    pub expected_fee_programdata: String,
    pub expected_fee_deployed_slot: u64,
    pub expected_fee_program_code_sha256: String,
    pub expected_token_program_code_sha256: String,
    pub user: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_program_upgrade: Option<TokenProgramUpgradeConfig>,
}
config_struct!(PoolConfig {pump:Option<PumpConfig>,meteora:Option<MeteoraConfig>,pancake_bsc_lm:Option<PancakeBscLmConfig>,pancake_lm:Option<PancakeLmConfig>,slipstream_context:Option<SlipstreamContextConfig>,lfj:Option<LfjConfig>,infinity_bin:Option<InfinityBinConfig>,singleton_advanced:Option<SingletonAdvancedPoolConfig>,multi_asset:Option<MultiAssetConfig>,weighted:Option<WeightedPoolConfig>,raydium_clmm:Option<RaydiumCLMMConfig>,orca_whirlpool:Option<OrcaWhirlpoolConfig>,raydium_cpmm:Option<RaydiumCPMMConfig>,singleton_cl:Option<SingletonCLPoolConfig>,id:String,protocol:String,token0:String,token1:String,fee_numerator:u64,fee_denominator:u64,program_id:String,accounts:Vec<String>,v3:Option<V3PoolConfig>,v2:Option<V2PoolConfig>,stable:Option<StablePoolConfig>});
config_struct!(Chain {key:String,owner_service:String,assignment_version:u64,genesis_hash:String,rpc_urls:Vec<String>,pools:Vec<PoolConfig>,confirmation_tag:String,auto_reorg_window:u64});
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveRetention {
    pub max_segments: usize,
    pub max_bytes: u64,
}
/// Explicit per-chain budgets. A public/self-hosted Geyser needs no token.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct YellowstoneConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_retention: Option<ArchiveRetention>,
    pub endpoint: String,
    pub source_id: String,
    pub proto_commit: String,
    #[serde(default)]
    pub token_file: Option<String>,
    pub allow_loopback_plaintext: bool,
    pub connect_timeout_ms: u64,
    pub idle_timeout_ms: u64,
    pub wal_ack_timeout_ms: u64,
    pub reconnect_delay_ms: u64,
    pub max_frame_bytes: usize,
    pub max_wal_bytes: usize,
    pub max_wal_records: usize,
    pub max_replay_records: usize,
    pub max_replay_slots: u64,
    pub max_stream_bytes: usize,
    pub max_stream_slots: usize,
    pub max_stream_updates: usize,
    pub max_manifest_bytes: usize,
    pub max_catalog_pools: usize,
    pub manifest_directory: String,
    pub catalog_file: String,
}
impl YellowstoneConfig {
    pub fn validate(&self) -> Result<()> {
        let fail = || err("YELLOWSTONE_CONFIG_CONSTRAINT");
        if self.archive_retention.as_ref().is_some_and(|a| {
            !(1..=4096).contains(&a.max_segments)
                || !(1..=4 * 1024 * 1024 * 1024u64).contains(&a.max_bytes)
        }) {
            return Err(fail());
        }
        if self.proto_commit != "10f0af69d150637d8e3eaaf83b179d640988dd3c"
            || self.source_id.is_empty()
            || self.source_id.len() > 128
            || !self
                .source_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
            || self.endpoint.contains('@')
            || self.endpoint.contains('?')
            || self.endpoint.contains('#')
            || self.manifest_directory.is_empty()
            || self.catalog_file.is_empty()
            || self.token_file.as_ref().is_some_and(|p| p.is_empty())
            || [
                self.connect_timeout_ms,
                self.idle_timeout_ms,
                self.wal_ack_timeout_ms,
                self.reconnect_delay_ms,
            ]
            .iter()
            .any(|v| !(1..=300_000).contains(v))
            || !(1..=32 * 1024 * 1024).contains(&self.max_frame_bytes)
            || !(self.max_frame_bytes..=4 * 1024 * 1024 * 1024usize).contains(&self.max_wal_bytes)
            || !(1..=1_000_000).contains(&self.max_wal_records)
            || !(1..=self.max_wal_records).contains(&self.max_replay_records)
            || !(1..=4096).contains(&self.max_replay_slots)
            || !(self.max_frame_bytes..=1024 * 1024 * 1024).contains(&self.max_stream_bytes)
            || !(1..=4096).contains(&self.max_stream_slots)
            || !(1..=1_000_000).contains(&self.max_stream_updates)
            || !(1..=32 * 1024 * 1024).contains(&self.max_manifest_bytes)
            || !(1..=100_000).contains(&self.max_catalog_pools)
        {
            return Err(fail());
        }
        let authority = self
            .endpoint
            .strip_prefix("https://")
            .or_else(|| self.endpoint.strip_prefix("http://"))
            .ok_or_else(fail)?;
        let authority = authority.strip_suffix('/').unwrap_or(authority);
        if authority.is_empty()
            || authority.contains('/')
            || !authority.is_ascii()
            || authority
                .bytes()
                .any(|b| b.is_ascii_whitespace() || b.is_ascii_control())
        {
            return Err(fail());
        }
        let (name, port) = if authority.starts_with('[') {
            let end = authority.find(']').ok_or_else(fail)?;
            authority[1..end]
                .parse::<std::net::Ipv6Addr>()
                .map_err(|_| fail())?;
            let suffix = &authority[end + 1..];
            (
                &authority[..end + 1],
                if suffix.is_empty() {
                    None
                } else {
                    Some(suffix.strip_prefix(':').ok_or_else(fail)?)
                },
            )
        } else {
            match authority.split_once(':') {
                Some((host, port)) => (host, Some(port)),
                None => (authority, None),
            }
        };
        if name.is_empty()
            || (!name.starts_with('[')
                && !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-'))
            || port.is_some_and(|p| p.parse::<u16>().map_or(true, |n| n == 0))
        {
            return Err(fail());
        }
        let host = self.endpoint.strip_prefix("http://");
        if !(self.endpoint.starts_with("https://")
            || self.allow_loopback_plaintext
                && host.is_some_and(|h| {
                    h == "localhost"
                        || h.starts_with("localhost:")
                        || h == "127.0.0.1"
                        || h.starts_with("127.0.0.1:")
                        || h == "[::1]"
                        || h.starts_with("[::1]:")
                }))
        {
            return Err(fail());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub control: Control,
    pub service: Service,
    pub startup: Startup,
    pub history: History,
    pub logging: Logging,
    pub journal: Journal,
    pub state: State,
    pub api: Api,
    pub chains: Vec<Chain>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub yellowstone: BTreeMap<String, YellowstoneConfig>,
    #[serde(default)]
    pub routing: Routing,
    #[serde(default)]
    pub simulation: Simulation,
}
impl Default for Routing {
    fn default() -> Self {
        Self {
            max_hops: 0,
            max_candidates: 0,
            max_expansions: 0,
            split_parts: 0,
            max_splits: 0,
            route_activation_cost: "0".parse().expect("zero amount"),
            max_state_age_ms: 0,
            quote_ttl_ms: 0,
        }
    }
}
#[derive(Clone)]
pub struct ConfigSnapshot {
    pub config: Config,
    pub release: String,
    pub public_hash: String,
    pub economic_hash: String,
    pub root: PathBuf,
    pub token: String,
    pub control_dsn: String,
}
pub trait ConfigProvider {
    fn load(&self, path: &Path) -> Result<Vec<u8>>;
}
pub struct FileConfigProvider;
impl ConfigProvider for FileConfigProvider {
    fn load(&self, path: &Path) -> Result<Vec<u8>> {
        let meta = fs::metadata(path).map_err(|_| err("CONFIG_FILE_UNREADABLE"))?;
        if !meta.is_file() || meta.len() > 4 * 1024 * 1024 {
            return Err(err("CONFIG_FILE_LIMIT"));
        }
        let file = fs::File::open(path).map_err(|_| err("CONFIG_FILE_UNREADABLE"))?;
        let mut b = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut b)
            .map_err(|_| err("CONFIG_FILE_UNREADABLE"))?;
        if b.len() > 4 * 1024 * 1024 {
            return Err(err("CONFIG_FILE_LIMIT"));
        }
        Ok(b)
    }
}
pub fn decode_document(b: &[u8]) -> Result<serde_json::Value> {
    let text = std::str::from_utf8(b).map_err(|_| err("INVALID_YAML"))?;
    if text.contains("${") {
        return Err(err("ENV_INTERPOLATION_FORBIDDEN"));
    }
    // Reject aliases before deserializing. Anchors and tags outside quotes are deliberately unsupported.
    let mut quote = None;
    let mut escape = false;
    let mut comment = false;
    let mut prev = ' ';
    for c in text.chars() {
        if comment {
            if c == '\n' {
                comment = false;
            }
            prev = c;
            continue;
        }
        if let Some(q) = quote {
            if escape {
                escape = false;
            } else if c == '\\' && q == '"' {
                escape = true;
            } else if c == q {
                quote = None;
            }
        } else if c == '"' || c == '\'' {
            quote = Some(c);
        } else if c == '#' && prev.is_whitespace() {
            comment = true;
        } else if (c == '&' || c == '*' || c == '!')
            && (prev.is_whitespace() || [':', '[', ',', '{'].contains(&prev))
        {
            return Err(err("YAML_ALIAS_OR_TAG"));
        }
        prev = c;
    }
    // serde_yaml's Mapping rejects duplicate keys before conversion to JSON maps.
    let yaml: serde_yaml::Value =
        serde_yaml::from_slice(b).map_err(|_| err("INVALID_YAML_OR_DUPLICATE_KEY"))?;
    fn depth(v: &serde_yaml::Value, d: usize) -> Result<()> {
        if d > 32 {
            return Err(err("YAML_DEPTH"));
        }
        match v {
            serde_yaml::Value::Mapping(m) => {
                for (k, v) in m {
                    if k.as_str() == Some("<<") {
                        return Err(err("YAML_MERGE_FORBIDDEN"));
                    }
                    depth(v, d + 1)?
                }
            }
            serde_yaml::Value::Sequence(v) => {
                for x in v {
                    depth(x, d + 1)?
                }
            }
            serde_yaml::Value::Tagged(_) => return Err(err("YAML_TAG_FORBIDDEN")),
            _ => {}
        }
        Ok(())
    }
    depth(&yaml, 0)?;
    let v = serde_json::to_value(yaml).map_err(|_| err("INVALID_YAML"))?;
    if !v.is_object() {
        return Err(err("CONFIG_REQUIRES_MAP"));
    }
    Ok(v)
}
pub fn load(path: impl AsRef<Path>, expected: &str) -> Result<ConfigSnapshot> {
    let provider = FileConfigProvider;
    let path = fs::canonicalize(path).map_err(|_| err("CONFIG_PATH"))?;
    let root = path
        .parent()
        .ok_or_else(|| err("CONFIG_PATH"))?
        .to_path_buf();
    let value = decode_document(&provider.load(&path)?)?;
    let config: Config =
        serde_json::from_value(value.clone()).map_err(|_| err("CONFIG_SCHEMA_MISMATCH"))?;
    config.validate(expected)?;
    let token = config.api.auth_token.clone();
    let mut public_value = value;
    public_value["api"]
        .as_object_mut()
        .ok_or_else(|| err("CONFIG_SCHEMA_MISMATCH"))?
        .remove("auth_token");
    public_value["journal"]
        .as_object_mut()
        .ok_or_else(|| err("CONFIG_SCHEMA_MISMATCH"))?
        .remove("source_auth_tokens");
    let public_hash = hash(&public_value)?;
    let economic_hash = hash(
        &serde_json::json!({"routing":public_value.get("routing"),"simulation":public_value.get("simulation"),"chains":public_value.get("chains")}),
    )?;
    let control_dsn = if config.control.backend == "postgres" {
        let path = root.join(&config.control.dsn_file);
        let meta = fs::metadata(&path).map_err(|_| err("CONTROL_SECRET_PERMISSIONS"))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            if meta.permissions().mode() & 0o077 != 0 {
                return Err(err("CONTROL_SECRET_PERMISSIONS"));
            }
        }
        if !meta.is_file() || meta.len() > 4096 {
            return Err(err("CONTROL_SECRET_INVALID"));
        }
        String::from_utf8(provider.load(&path)?)
            .map_err(|_| err("CONTROL_SECRET_INVALID"))?
            .trim()
            .to_owned()
    } else {
        String::new()
    };
    Ok(ConfigSnapshot {
        control_dsn,
        config,
        release: path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("config")
            .to_owned(),
        public_hash,
        economic_hash,
        root,
        token,
    })
}
impl Config {
    pub fn validate(&self, expected: &str) -> Result<()> {
        let fail = || err("CONFIG_CONSTRAINT_VIOLATION");
        for (chain, stream) in &self.yellowstone {
            stream.validate()?;
            if !chain.starts_with("solana:")
                || !self.chains.iter().any(|c| {
                    c.key == *chain
                        && c.owner_service == "chain-parser-rust"
                        && c.confirmation_tag == "finalized"
                })
            {
                return Err(err("YELLOWSTONE_CHAIN_CONFIG"));
            }
        }
        if self.service.name != expected
            || !["chain-parser-go", "chain-parser-rust", "dex-router"].contains(&expected)
            || !["online", "analytics"].contains(&self.service.role.as_str())
            || !(["rpc"].contains(&self.service.mode.as_str())
                || expected == "chain-parser-go"
                    && self.service.role == "analytics"
                    && self.service.mode == "journal")
            || (self.service.mode == "journal"
                && (self.journal.transport != "kafka" || self.control.backend != "disabled"))
        {
            return Err(fail());
        }
        let s = &self.startup;
        let h = &self.history;
        if s.mode != "HEAD_FIRST"
            || s.resume_previous_online_cursor
            || s.history_catchup_blocks_online_readiness
            || s.max_baseline_age_ms == 0
            || s.max_tail_blocks == 0
            || h.online_write_permission
            || h.namespace != "history"
            || !(0.0..=0.25).contains(&h.max_worker_share)
        {
            return Err(fail());
        }
        let l = &self.logging;
        if l.format != "json"
            || !["debug", "info", "warn", "error"].contains(&l.level.as_str())
            || !(1..=65536).contains(&l.queue_capacity)
            || !(512..=65536).contains(&l.max_event_bytes)
        {
            return Err(fail());
        }
        let j = &self.journal;
        if !["http-development", "kafka"].contains(&j.transport.as_str())
            || j.poll_interval_ms == 0
            || j.request_timeout_ms == 0
            || !(1024..=67108864).contains(&j.max_payload_bytes)
        {
            return Err(fail());
        }
        if self.api.solana_runtime_manifest.len() > 4096
            || self.api.solana_runtime_manifest.contains('\0')
            || (expected != "dex-router" && !self.api.solana_runtime_manifest.is_empty())
        {
            return Err(fail());
        }
        if self.api.cors_allowed_origins.len() > 32
            || self.api.cors_allowed_origins.iter().any(|origin| {
                let host = origin
                    .strip_prefix("http://")
                    .or_else(|| origin.strip_prefix("https://"));
                origin.len() > 512
                    || host.is_none_or(|h| {
                        h.is_empty()
                            || h.chars()
                                .any(|c| c.is_whitespace() || c.is_control() || "/?#@*".contains(c))
                    })
            })
        {
            return Err(fail());
        }
        if self.api.listen.is_empty()
            || (!self.api.auth_token.is_empty() && self.api.auth_token.len() < 24)
            || !(1024..=8388608).contains(&self.api.max_body_bytes)
            || self.api.requests_per_minute == 0
            || self.chains.is_empty()
        {
            return Err(fail());
        }
        let r = &self.routing;
        let sim = &self.simulation;
        if expected == "dex-router" {
            if !(1..=4).contains(&r.max_hops)
                || !(1..=256).contains(&r.max_candidates)
                || !(1..=100000).contains(&r.max_expansions)
                || !(1..=100).contains(&r.split_parts)
                || !(1..=8).contains(&r.max_splits)
                || r.max_state_age_ms == 0
                || r.quote_ttl_ms == 0
            {
                return Err(fail());
            }
            if !["disabled", "evm-rpc", "revm-fixture"].contains(&sim.backend.as_str())
                || sim.max_gas == 0
                || sim.max_state_reads == 0
                || sim.timeout_ms == 0
            {
                return Err(fail());
            }
            if self.api.quote_timeout_ms == 0 || !(1..=256).contains(&self.api.quote_concurrency) {
                return Err(fail());
            }
        }
        if sim.backend == "revm-fixture" {
            if expected != "dex-router"
                || !sim.rpc_url.is_empty()
                || sim.state_file.is_empty()
                || sim.state_file.len() > 4096
                || sim.state_file.contains('\0')
                || !(1024..=16_777_216).contains(&sim.max_state_bytes)
                || !(1..=100_000_000).contains(&sim.max_instructions)
                || !(65_536..=67_108_864).contains(&sim.max_memory_bytes)
                || !(1..=8).contains(&sim.concurrency)
                || sim.allowed_targets.is_empty()
                || sim.allowed_targets.len() > 64
                || sim.allowed_targets.iter().any(|target| {
                    xone_domain::protocols::validate_address(target).is_err()
                        || target == &format!("0x{}", "0".repeat(40))
                })
                || sim
                    .allowed_targets
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != sim.allowed_targets.len()
            {
                return Err(fail());
            }
        } else if !sim.state_file.is_empty()
            || sim.max_state_bytes != 0
            || sim.max_instructions != 0
            || sim.max_memory_bytes != 0
            || sim.concurrency != 0
        {
            return Err(fail());
        }
        let engine = match expected {
            "chain-parser-go" => "pebble",
            "chain-parser-rust" => "rocksdb",
            _ => "memory",
        };
        if self.state.engine != engine {
            return Err(fail());
        }
        let mut seen = std::collections::BTreeSet::new();
        for c in &self.chains {
            for p in &c.pools {
                let profile = xone_domain::protocols::protocol(&p.protocol);
                if p.pump.is_some()
                    && !matches!(
                        p.protocol.as_str(),
                        xone_domain::pump::PROFILE
                            | xone_domain::pump::MIGRATION_PROFILE
                            | xone_domain::pump_amm::PROFILE
                            | xone_domain::pump_amm::BOOST_PROFILE
                    )
                {
                    return Err(err("PUMP_CONFIG_PROTOCOL"));
                }
                if matches!(
                    p.protocol.as_str(),
                    xone_domain::pump::PROFILE
                        | xone_domain::pump::MIGRATION_PROFILE
                        | xone_domain::pump_amm::PROFILE
                        | xone_domain::pump_amm::BOOST_PROFILE
                ) {
                    p.pump
                        .as_ref()
                        .ok_or_else(|| err("PUMP_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.meteora.is_some() && !xone_domain::meteora::supports_profile(&p.protocol) {
                    return Err(err("METEORA_CONFIG_PROTOCOL"));
                }
                if p.protocol == xone_domain::pancake_bsc_lm::PROFILE {
                    p.pancake_bsc_lm
                        .as_ref()
                        .ok_or_else(|| err("BSC4_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.pancake_bsc_lm.is_some() {
                    return Err(err("BSC4_CONFIG_PROTOCOL"));
                }
                if p.protocol == xone_domain::pancake_lm::PROFILE {
                    p.pancake_lm
                        .as_ref()
                        .ok_or_else(|| err("PANCAKE_LM_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.pancake_lm.is_some() {
                    return Err(err("PANCAKE_LM_CONFIG_PROTOCOL"));
                }
                if p.protocol == xone_domain::slipstream_context::PROFILE {
                    p.slipstream_context
                        .as_ref()
                        .ok_or_else(|| err("SLIPSTREAM_CONTEXT_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.slipstream_context.is_some() {
                    return Err(err("SLIPSTREAM_CONTEXT_CONFIG_PROTOCOL"));
                }
                if xone_domain::meteora::supports_profile(&p.protocol) {
                    p.meteora
                        .as_ref()
                        .ok_or_else(|| err("METEORA_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.meteora.is_some() {
                    return Err(err("METEORA_CONFIG_PROTOCOL"));
                }
                if p.protocol == xone_domain::lfj::PROFILE {
                    p.lfj
                        .as_ref()
                        .ok_or_else(|| err("LFJ_CONFIG_REQUIRED"))?
                        .validate(p, &c.key)?;
                    continue;
                }
                if p.lfj.is_some() {
                    return Err(err("LFJ_CONFIG_PROTOCOL"));
                }
                if p.infinity_bin.is_some() && p.protocol != xone_domain::infinity_bin::PROFILE {
                    return Err(fail());
                }
                if p.protocol == xone_domain::infinity_bin::PROFILE {
                    let v = p.infinity_bin.as_ref().ok_or_else(fail)?;
                    if v.token_policy != "standard"
                        || !p.program_id.is_empty()
                        || !p.accounts.is_empty()
                        || p.singleton_advanced.is_some()
                        || p.multi_asset.is_some()
                        || p.weighted.is_some()
                        || p.raydium_clmm.is_some()
                        || p.orca_whirlpool.is_some()
                        || p.raydium_cpmm.is_some()
                        || p.singleton_cl.is_some()
                        || p.stable.is_some()
                        || p.v2.is_some()
                        || p.v3.is_some()
                    {
                        return Err(fail());
                    }
                    use xone_domain::infinity_bin::{
                        InfinityBinEntry, InfinityBinState, InfinityBinTreeWord,
                    };
                    let root: Amount = "340282366920938463463374607431768211456".parse()?;
                    let state = InfinityBinState {
                        detached_active: None,
                        manager: v.manager.clone(),
                        manager_code_sha256: v.expected_manager_code_sha256.clone(),
                        vault: v.vault.clone(),
                        vault_code_sha256: v.expected_vault_code_sha256.clone(),
                        pool_id: v.pool_id.clone(),
                        hooks: xone_domain::singleton::ZERO_ADDRESS.into(),
                        parameters: format!("0x{:064x}", u64::from(v.bin_step) << 16),
                        token0_code_sha256: v.expected_token0_code_sha256.clone(),
                        token1_code_sha256: v.expected_token1_code_sha256.clone(),
                        bin_step: v.bin_step,
                        active_id: 8388608,
                        lp_fee: v.lp_fee,
                        protocol_fee0: 0,
                        protocol_fee1: 0,
                        max_bins: v.max_bins,
                        max_tree_words: v.max_tree_words,
                        max_steps: v.max_steps,
                        tree_level0: root,
                        tree_level1: vec![InfinityBinTreeWord {
                            index: 128,
                            value: "1".parse()?,
                        }],
                        tree_level2: vec![InfinityBinTreeWord {
                            index: 32768,
                            value: "1".parse()?,
                        }],
                        bins: vec![InfinityBinEntry {
                            id: 8388608,
                            reserve0: Amount::zero(),
                            reserve1: Amount::zero(),
                            total_shares: "1".parse()?,
                        }],
                        app_reserve0: Amount::zero(),
                        app_reserve1: Amount::zero(),
                        protocol_fees0: Amount::zero(),
                        protocol_fees1: Amount::zero(),
                        block_number: 1,
                        timestamp: 1,
                        block_hash: format!("0x{}", "1".repeat(64)),
                    };
                    let mut model = xone_domain::Pool {
                        pancake_bsc_lm: None,
                        pancake_lm: None,
                        slipstream_context: None,
                        lfj: None,
                        meteora: None,
                        pump: None,
                        infinity_bin: None,
                        singleton_advanced: None,
                        multi_asset: None,
                        weighted: None,
                        raydium_clmm: None,
                        orca_whirlpool: None,
                        raydium_cpmm: None,
                        singleton_cl: None,
                        stable: None,
                        v2: None,
                        v3: None,
                        chain_key: c.key.clone(),
                        pool_id: p.id.clone(),
                        protocol: p.protocol.clone(),
                        token0: p.token0.clone(),
                        token1: p.token1.clone(),
                        reserve0: Amount::zero(),
                        reserve1: Amount::zero(),
                        fee_numerator: p.fee_numerator,
                        fee_denominator: p.fee_denominator,
                        adapter_version: xone_domain::infinity_bin::ADAPTER.into(),
                        token_behavior: "standard".into(),
                    };
                    model.infinity_bin = Some(state);
                    model.validate()?;
                    continue;
                }

                if p.singleton_advanced.is_some()
                    && profile.is_none_or(|v| v.family != "singleton-advanced")
                {
                    return Err(fail());
                }
                if profile.is_some_and(|v| v.family == "singleton-advanced") {
                    let v = p.singleton_advanced.as_ref().ok_or_else(fail)?;
                    if p.multi_asset.is_some()
                        || p.weighted.is_some()
                        || p.singleton_cl.is_some()
                        || p.stable.is_some()
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.orca_whirlpool.is_some()
                        || p.raydium_clmm.is_some()
                        || p.raydium_cpmm.is_some()
                        || !c.key.starts_with("evm:")
                        || p.protocol != format!("{}-advanced", v.protocol)
                        || p.id != format!("{}/{}", v.manager, v.pool_id)
                        || p.token0 != v.currency0
                        || p.token1 != v.currency1
                        || p.fee_denominator != 1000000
                        || p.fee_numerator != u64::from(v.key_fee)
                        || v.tick_word_radius > 16
                        || !(1..=4096).contains(&v.max_initialized_ticks)
                        || !(1..=8192).contains(&v.max_steps)
                    {
                        return Err(fail());
                    }
                    xone_domain::protocols::validate_identity(
                        &v.manager,
                        &v.expected_manager_code_sha256,
                    )?;
                    let state = xone_domain::singleton::SingletonAdvancedState {
                        protocol: v.protocol.clone(),
                        currency0: v.currency0.clone(),
                        currency1: v.currency1.clone(),
                        key_fee: v.key_fee,
                        snapshot_block_hash: format!("0x{}", "1".repeat(64)),
                        snapshot: xone_domain::singleton::SingletonCLState {
                            manager: v.manager.clone(),
                            manager_code_sha256: v.expected_manager_code_sha256.clone(),
                            pool_id: v.pool_id.clone(),
                            lp_fee: if v.key_fee == xone_domain::singleton::DYNAMIC_FEE_FLAG {
                                0
                            } else {
                                v.key_fee
                            },
                            protocol_fee0: 0,
                            protocol_fee1: 0,
                            hooks: v.hooks.clone(),
                            parameters: v.parameters.clone(),
                            core: xone_domain::singleton::ConcentratedState {
                                sqrt_price_x96: "79228162514264337593543950336".parse()?,
                                tick: 0,
                                liquidity: "1".parse()?,
                                tick_spacing: v.tick_spacing,
                                max_steps: v.max_steps,
                                tick_words: vec![],
                                ticks: vec![],
                            },
                        },
                    };
                    state.validate_identity()?;
                    continue;
                }

                if p.multi_asset
                    .as_ref()
                    .is_some_and(|v| v.managed_v2.is_some())
                    && !managed_v2::is_managed_v2_protocol(&p.protocol)
                {
                    return Err(fail());
                }
                if p.multi_asset.is_some() && profile.is_none_or(|v| v.family != "multi") {
                    return Err(fail());
                }
                if let Some(profile) = profile.filter(|v| v.family == "multi") {
                    let v = p.multi_asset.as_ref().ok_or_else(fail)?;
                    if (v.token_policy != "standard-no-bpt"
                        && !(p.protocol == xone_domain::managed_v2_bpt_state::PROTOCOL
                            && v.token_policy == "standard-bpt")
                        && !(p.protocol == xone_domain::managed_v2_circuit_state::PROTOCOL
                            && v.token_policy == "standard-breaker-bpt")
                        && !(p.protocol == "balancer-v2-composable-regular"
                            && v.token_policy == "standard-with-own-bpt")
                        && !(p.protocol == "curve-metausd-direct"
                            && v.token_policy == "standard-base-lp"))
                        || p.weighted.is_some()
                        || p.stable.is_some()
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.singleton_cl.is_some()
                        || p.raydium_cpmm.is_some()
                        || p.orca_whirlpool.is_some()
                        || p.raydium_clmm.is_some()
                        || !p.program_id.is_empty()
                        || !p.accounts.is_empty()
                    {
                        return Err(fail());
                    }
                    if managed_v2::is_managed_v2_protocol(&p.protocol) {
                        p.validate_managed_v2(&c.key)?;
                        continue;
                    }
                    if !matches!(
                        p.protocol.as_str(),
                        "curve-crypto2-full" | "curve-crypto2-ramp"
                    ) && (!v.expected_lp_token.is_empty()
                        || !v.expected_lp_token_code_sha256.is_empty())
                    {
                        return Err(fail());
                    }
                    if matches!(
                        p.protocol.as_str(),
                        "curve-crypto2-full" | "curve-crypto2-ramp"
                    ) && (xone_domain::protocols::validate_address(&v.expected_lp_token)
                        .is_err()
                        || v.expected_lp_token_code_sha256
                            != xone_domain::curve_crypto_state::LP_RUNTIME_SHA256
                        || !v.expected_implementation.is_empty()
                        || !v.expected_implementation_code_sha256.is_empty())
                    {
                        return Err(fail());
                    }
                    if p.protocol != "curve-metausd-direct"
                        && !v.expected_base_pool_code_sha256.is_empty()
                    {
                        return Err(fail());
                    }
                    if p.protocol == "curve-metausd-direct"
                        && (v.token_policy != "standard-base-lp"
                            || v.expected_implementation.is_empty()
                            || c.pools
                                .iter()
                                .any(|other| other.id == xone_domain::curve_meta_state::BASE_POOL))
                    {
                        return Err(fail());
                    }
                    let n = v.tokens.len();
                    if !(2..=38).contains(&n) {
                        return Err(fail());
                    }
                    let weighted = p.protocol.starts_with("balancer-v2-");
                    let composable = p.protocol == "balancer-v2-composable-regular";
                    if weighted {
                        if !v.expected_implementation.is_empty()
                            || !v.expected_implementation_code_sha256.is_empty()
                        {
                            return Err(fail());
                        }
                    } else if !v.expected_implementation.is_empty() {
                        xone_domain::protocols::validate_identity(
                            &v.expected_implementation,
                            &v.expected_implementation_code_sha256,
                        )?;
                    } else if !v.expected_implementation_code_sha256.is_empty() {
                        return Err(fail());
                    }
                    let weights = if weighted && !composable {
                        (0..n)
                            .map(|i| {
                                if i + 1 == n {
                                    (1_000_000_000_000_000_000u64
                                        - 10_000_000_000_000_000u64 * (n as u64 - 1))
                                        .to_string()
                                        .parse()
                                } else {
                                    "10000000000000000".parse()
                                }
                            })
                            .collect::<Result<Vec<_>>>()?
                    } else {
                        Vec::new()
                    };
                    let assets = v
                        .tokens
                        .iter()
                        .map(|a| {
                            Ok(xone_domain::multi_asset::MultiAsset {
                                token: a.token.clone(),
                                balance: xone_domain::Amount::zero(),
                                scaling: "1000000000000000000".parse()?,
                                code_sha256: a.expected_code_sha256.clone(),
                            })
                        })
                        .collect::<Result<Vec<_>>>()?;
                    let ng = if p.protocol == "curve-stableswap-ng" {
                        if !v.expected_implementation.is_empty()
                            || !v.expected_implementation_code_sha256.is_empty()
                        {
                            return Err(fail());
                        }
                        Some(xone_domain::curve_ng_state::CurveNGState {
                            stored_balances: vec![Amount::zero(); n],
                            admin_balances: vec![Amount::zero(); n],
                            last_prices_packed: vec![Amount::zero(); n - 1],
                            last_d_packed: Amount::zero(),
                            ma_last_time: Amount::zero(),
                            ma_exp_time: 1,
                            d_ma_time: 1,
                            timestamp: 1,
                            block_number: 1,
                            block_hash: format!("0x{}", "1".repeat(64)),
                            offpeg_fee_multiplier: "10000000000".parse()?,
                        })
                    } else {
                        None
                    };
                    if p.protocol == "balancer-v2-managed-fees" {
                        return Err(err("MANAGED_FEE_RUNTIME_UNAVAILABLE"));
                    }
                    let managed_fees = None;
                    let amplification: Amount = if composable {
                        "100000"
                    } else if weighted {
                        "0"
                    } else {
                        "10000"
                    }
                    .parse()?;
                    let crypto_gamma: Amount = "10000000000".parse()?;
                    let state = xone_domain::multi_asset::MultiAssetState {
                        managed_v2: None,
                        curve_crypto: if matches!(
                            p.protocol.as_str(),
                            "curve-crypto2-full" | "curve-crypto2-ramp"
                        ) {
                            Some(xone_domain::curve_crypto_state::CurveCryptoState {
                                ramp: if p.protocol == "curve-crypto2-ramp" {
                                    Some(xone_domain::curve_crypto_state::CurveCryptoRamp {
                                        initial_amp: amplification.clone(),
                                        initial_gamma: crypto_gamma.clone(),
                                        initial_time: 0,
                                    })
                                } else {
                                    None
                                },
                                gamma: crypto_gamma,
                                out_fee: p.fee_numerator.max(500000).to_string().parse()?,
                                fee_gamma: "230000000000000".parse()?,
                                allowed_extra_profit: "1000000000000000000".parse()?,
                                adjustment_step: "146000000000000".parse()?,
                                ma_half_time: 600,
                                invariant: "2000000000000000000000000".parse()?,
                                price_scale: "1000000000000000000".parse()?,
                                price_oracle: "1000000000000000000".parse()?,
                                last_prices: "1000000000000000000".parse()?,
                                last_prices_timestamp: 0,
                                xcp_profit: "1000000000000000000".parse()?,
                                xcp_profit_a: "1000000000000000000".parse()?,
                                virtual_price: "1000000000000000000".parse()?,
                                not_adjusted: false,
                                future_a_gamma_time: 0,
                                lp_token: v.expected_lp_token.clone(),
                                lp_token_code_sha256: v.expected_lp_token_code_sha256.clone(),
                                total_supply: "1000000000000000000000000".parse()?,
                                admin_receiver: format!("0x{}", "0".repeat(40)),
                                admin_receiver_balance: Amount::zero(),
                                raw_balances: vec![Amount::zero(); 2],
                                timestamp: 1,
                                block_number: 1,
                                block_hash: format!("0x{}", "1".repeat(64)),
                            })
                        } else {
                            None
                        },
                        curve_meta: if p.protocol == "curve-metausd-direct" {
                            Some(xone_domain::curve_meta_state::CurveMetaState {
                                implementation: v.expected_implementation.clone(),
                                implementation_code_sha256: v
                                    .expected_implementation_code_sha256
                                    .clone(),
                                base_pool: xone_domain::curve_meta_state::BASE_POOL.into(),
                                base_pool_code_sha256: v.expected_base_pool_code_sha256.clone(),
                                base_virtual_price: "1000000000000000000".parse()?,
                                stored_balances: vec![Amount::zero(); 2],
                                admin_balances: vec![Amount::zero(); 2],
                                timestamp: 1,
                                block_number: 1,
                                block_hash: format!("0x{}", "1".repeat(64)),
                            })
                        } else {
                            None
                        },
                        managed_fees,
                        curve_ng: ng,
                        code_sha256: v.expected_code_sha256.clone(),
                        vault: v.expected_vault.clone(),
                        vault_code_sha256: v.expected_vault_code_sha256.clone(),
                        pool_id: v.pool_id.clone(),
                        assets,
                        weights,
                        amplification,
                        admin_fee: if weighted { 0 } else { 5_000_000_000 },
                    };
                    let pool = xone_domain::Pool {
                        pancake_bsc_lm: None,
                        pancake_lm: None,
                        slipstream_context: None,
                        lfj: None,
                        meteora: None,
                        pump: None,
                        infinity_bin: None,
                        multi_asset: Some(state),
                        singleton_advanced: None,
                        weighted: None,
                        stable: None,
                        v2: None,
                        v3: None,
                        singleton_cl: None,
                        raydium_cpmm: None,
                        orca_whirlpool: None,
                        raydium_clmm: None,
                        chain_key: c.key.clone(),
                        pool_id: p.id.clone(),
                        protocol: p.protocol.clone(),
                        token0: p.token0.clone(),
                        token1: p.token1.clone(),
                        reserve0: xone_domain::Amount::zero(),
                        reserve1: xone_domain::Amount::zero(),
                        fee_numerator: p.fee_numerator,
                        fee_denominator: p.fee_denominator,
                        adapter_version: profile.adapter.into(),
                        token_behavior: "standard".into(),
                    };
                    pool.validate()?;
                    continue;
                }

                if p.weighted.is_some() && profile.is_none_or(|v| v.family != "weighted") {
                    return Err(fail());
                }
                if profile.is_some_and(|x| x.family == "rayclmm") {
                    let v = p.raydium_clmm.as_ref().ok_or_else(fail)?;
                    if v.expected_programdata != xone_domain::raydium_clmm::PROGRAM_DATA
                        || !c.key.starts_with("solana:")
                        || c.confirmation_tag != "finalized"
                        || p.program_id != xone_domain::raydium_clmm::PROGRAM_ID
                        || p.accounts.len() != 7
                        || p.accounts[0] != p.id
                        || p.accounts[4] != p.token0
                        || p.accounts[5] != p.token1
                        || p.fee_denominator != 1000000
                        || p.fee_numerator >= 1000000
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.stable.is_some()
                        || p.singleton_cl.is_some()
                        || p.raydium_cpmm.is_some()
                        || p.orca_whirlpool.is_some()
                        || !(1..=8192).contains(&v.max_steps)
                        || !(3..=7).contains(&v.tick_array_starts.len())
                    {
                        return Err(fail());
                    }
                    let span =
                        i64::from(v.tick_array_starts[1]) - i64::from(v.tick_array_starts[0]);
                    if span <= 0 || span % 60 != 0 || span / 60 > 1000 {
                        return Err(fail());
                    }
                    for (i, start) in v.tick_array_starts.iter().enumerate() {
                        let start = i64::from(*start);
                        if start % span != 0
                            || (p.protocol != "raydium-clmm-bitmap"
                                && p.protocol != "raydium-clmm-custom-seed"
                                && p.protocol != "raydium-clmm-dynamic"
                                && !(-512..=511).contains(&(start / span)))
                            || !(-7680..=7679).contains(&(start / span))
                            || start > 443636
                            || start + span <= -443636
                            || i > 0 && start != i64::from(v.tick_array_starts[i - 1]) + span
                        {
                            return Err(fail());
                        }
                    }
                    let mut keys = std::collections::BTreeSet::new();
                    for key in p.accounts.iter().map(String::as_str).chain([
                        p.program_id.as_str(),
                        v.expected_programdata.as_str(),
                        xone_domain::raydium::TOKEN_PROGRAM,
                        xone_domain::raydium::CLOCK,
                    ]) {
                        xone_domain::raydium::validate_pubkey(key)?;
                        if !keys.insert(key) {
                            return Err(fail());
                        }
                    }
                    if let Some(token) = &v.token_program_upgrade {
                        if token.programdata != CLASSIC_TOKEN_PROGRAMDATA
                            || token.deployed_slot == 0
                            || !keys.insert(token.programdata.as_str())
                        {
                            return Err(fail());
                        }
                    }
                    for h in [
                        &v.expected_program_code_sha256,
                        &v.expected_token_program_code_sha256,
                    ] {
                        if h.len() != 64 || *h != h.to_ascii_lowercase() || hex::decode(h).is_err()
                        {
                            return Err(fail());
                        }
                    }
                    if xone_domain::raydium::validate_pubkey(&p.token0)?
                        >= xone_domain::raydium::validate_pubkey(&p.token1)?
                    {
                        return Err(fail());
                    }
                    continue;
                } else if p.raydium_clmm.is_some() {
                    return Err(fail());
                }

                if profile.is_some_and(|x| x.family == "orca") {
                    let v = p.orca_whirlpool.as_ref().ok_or_else(fail)?;
                    if v.expected_programdata != xone_domain::orca::PROGRAM_DATA
                        || !c.key.starts_with("solana:")
                        || c.confirmation_tag != "finalized"
                        || p.program_id != xone_domain::orca::PROGRAM_ID
                        || p.accounts.len() != 6
                        || p.accounts[0] != p.id
                        || p.accounts[4] != p.token0
                        || p.accounts[5] != p.token1
                        || p.fee_denominator != 1000000
                        || p.fee_numerator > 60000
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.stable.is_some()
                        || p.singleton_cl.is_some()
                        || p.raydium_cpmm.is_some()
                        || !(1..=8192).contains(&v.max_steps)
                        || !(3..=7).contains(&v.tick_array_starts.len())
                        || matches!(
                            p.protocol.as_str(),
                            "orca-whirlpools-adaptive"
                                | "orca-whirlpools-adaptive-rewards"
                                | "orca-whirlpools-adaptive-dynamic"
                        ) && v.tick_array_starts.len() != 3
                        || v.tick_array_starts.windows(2).any(|x| x[1] <= x[0])
                    {
                        return Err(fail());
                    }
                    if matches!(
                        p.protocol.as_str(),
                        "orca-whirlpools-adaptive-rewards" | "orca-whirlpools-adaptive-dynamic"
                    ) {
                        let starts = &v.tick_array_starts;
                        let span = i64::from(starts[1]) - i64::from(starts[0]);
                        if span <= 0
                            || span % 88 != 0
                            || span / 88 > u16::MAX.into()
                            || i64::from(starts[2]) - i64::from(starts[1]) != span
                            || starts.iter().any(|start| {
                                let start = i64::from(*start);
                                start % span != 0
                                    || start
                                        < i64::from(xone_domain::orca::MIN_TICK).div_euclid(span)
                                            * span
                                    || start > i64::from(xone_domain::orca::MAX_TICK)
                            })
                        {
                            return Err(fail());
                        }
                    }
                    let mut keys = std::collections::BTreeSet::new();
                    for key in p.accounts.iter().map(String::as_str).chain([
                        p.program_id.as_str(),
                        v.expected_programdata.as_str(),
                        xone_domain::raydium::TOKEN_PROGRAM,
                        xone_domain::raydium::CLOCK,
                    ]) {
                        xone_domain::raydium::validate_pubkey(key)?;
                        if !keys.insert(key) {
                            return Err(fail());
                        }
                    }
                    if let Some(token) = &v.token_program_upgrade {
                        if token.programdata != CLASSIC_TOKEN_PROGRAMDATA
                            || token.deployed_slot == 0
                            || !keys.insert(token.programdata.as_str())
                        {
                            return Err(fail());
                        }
                    }
                    for code in [
                        &v.expected_program_code_sha256,
                        &v.expected_token_program_code_sha256,
                    ] {
                        if code.len() != 64
                            || *code != code.to_ascii_lowercase()
                            || hex::decode(code).is_err()
                        {
                            return Err(fail());
                        }
                    }
                    if xone_domain::raydium::validate_pubkey(&p.token0)?
                        >= xone_domain::raydium::validate_pubkey(&p.token1)?
                    {
                        return Err(fail());
                    }
                    continue;
                } else if p.orca_whirlpool.is_some() {
                    return Err(fail());
                }

                if profile.is_some_and(|x| x.family == "raydium") {
                    let v = p.raydium_cpmm.as_ref().ok_or_else(fail)?;
                    if v.expected_programdata != xone_domain::raydium::PROGRAM_DATA
                        || !c.key.starts_with("solana:")
                        || c.confirmation_tag != "finalized"
                        || p.program_id != xone_domain::raydium::PROGRAM_ID
                        || p.accounts.len() != 6
                        || p.accounts[0] != p.id
                        || p.accounts[4] != p.token0
                        || p.accounts[5] != p.token1
                        || p.fee_denominator != 1000000
                        || p.fee_numerator >= 1000000
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.stable.is_some()
                        || p.singleton_cl.is_some()
                    {
                        return Err(fail());
                    }
                    let mut keys = std::collections::BTreeSet::new();
                    for key in p.accounts.iter().map(String::as_str).chain([
                        p.program_id.as_str(),
                        v.expected_programdata.as_str(),
                        xone_domain::raydium::TOKEN_PROGRAM,
                        xone_domain::raydium::CLOCK,
                    ]) {
                        xone_domain::raydium::validate_pubkey(key)?;
                        if !keys.insert(key) {
                            return Err(fail());
                        }
                    }
                    if let Some(token) = &v.token_program_upgrade {
                        if token.programdata != CLASSIC_TOKEN_PROGRAMDATA
                            || token.deployed_slot == 0
                            || !keys.insert(token.programdata.as_str())
                        {
                            return Err(fail());
                        }
                    }
                    for h in [
                        &v.expected_program_code_sha256,
                        &v.expected_token_program_code_sha256,
                    ] {
                        if h.len() != 64 || *h != h.to_ascii_lowercase() || hex::decode(h).is_err()
                        {
                            return Err(fail());
                        }
                    }
                    if xone_domain::raydium::validate_pubkey(&p.token0)?
                        >= xone_domain::raydium::validate_pubkey(&p.token1)?
                    {
                        return Err(fail());
                    }
                    continue;
                } else if p.raydium_cpmm.is_some() {
                    return Err(fail());
                }

                if profile.is_none_or(|x| x.family != "singleton")
                    && c.key.starts_with("evm:")
                    && p.id.to_ascii_lowercase().starts_with("0x")
                {
                    xone_domain::protocols::validate_address(&p.id)?;
                }
                if profile.is_some() && !c.key.starts_with("evm:") {
                    return Err(fail());
                }
                if p.v2.is_some() {
                    for a in [&p.id, &p.token0, &p.token1] {
                        if !a.starts_with("0x")
                            || a.len() != 42
                            || *a != a.to_ascii_lowercase()
                            || hex::decode(&a[2..]).is_err()
                        {
                            return Err(fail());
                        }
                    }
                }
                if profile.is_none() && expected == "chain-parser-go" {
                    return Err(fail());
                }
                if let Some(profile) = profile.filter(|p| p.family == "weighted") {
                    let v = p.weighted.as_ref().ok_or_else(fail)?;
                    if p.stable.is_some()
                        || p.v2.is_some()
                        || p.v3.is_some()
                        || p.singleton_cl.is_some()
                        || p.raydium_cpmm.is_some()
                        || p.orca_whirlpool.is_some()
                        || p.raydium_clmm.is_some()
                        || v.token_policy != "standard-no-bpt"
                        || !p.program_id.is_empty()
                        || !p.accounts.is_empty()
                    {
                        return Err(fail());
                    }
                    let model = xone_domain::Pool {
                        pancake_bsc_lm: None,
                        pancake_lm: None,
                        slipstream_context: None,
                        lfj: None,
                        meteora: None,
                        pump: None,
                        infinity_bin: None,
                        multi_asset: None,
                        singleton_advanced: None,
                        weighted: Some(xone_domain::weighted::WeightedState {
                            code_sha256: v.expected_code_sha256.clone(),
                            vault: v.expected_vault.clone(),
                            vault_code_sha256: v.expected_vault_code_sha256.clone(),
                            pool_id: v.pool_id.clone(),
                            scaling0: "1000000000000000000".parse()?,
                            scaling1: "1000000000000000000".parse()?,
                            weight0: "500000000000000000".parse()?,
                            weight1: "500000000000000000".parse()?,
                            token0_code_sha256: v.expected_token0_code_sha256.clone(),
                            token1_code_sha256: v.expected_token1_code_sha256.clone(),
                        }),
                        stable: None,
                        v2: None,
                        v3: None,
                        singleton_cl: None,
                        raydium_cpmm: None,
                        orca_whirlpool: None,
                        raydium_clmm: None,
                        chain_key: c.key.clone(),
                        pool_id: p.id.clone(),
                        protocol: p.protocol.clone(),
                        token0: p.token0.clone(),
                        token1: p.token1.clone(),
                        reserve0: xone_domain::Amount::zero(),
                        reserve1: xone_domain::Amount::zero(),
                        fee_numerator: p.fee_numerator,
                        fee_denominator: p.fee_denominator,
                        adapter_version: profile.adapter.into(),
                        token_behavior: "standard".into(),
                    };
                    model.validate()?;
                    continue;
                } else if p.weighted.is_some() {
                    return Err(fail());
                }
                if profile.is_some_and(|p| p.family == "singleton") {
                    let v = p.singleton_cl.as_ref().ok_or_else(fail)?;
                    xone_domain::protocols::validate_identity(
                        &v.manager,
                        &v.expected_manager_code_sha256,
                    )?;
                    if p.v2.is_some()
                        || p.v3.is_some()
                        || p.stable.is_some()
                        || v.tick_word_radius > 16
                        || !(1..=4096).contains(&v.max_initialized_ticks)
                        || !(1..=8192).contains(&v.max_steps)
                        || p.fee_denominator != 1000000
                        || p.fee_numerator != u64::from(v.lp_fee)
                    {
                        return Err(fail());
                    }
                    let id = xone_domain::singleton::pool_id(
                        &p.protocol,
                        &v.manager,
                        &p.token0,
                        &p.token1,
                        v.lp_fee,
                        v.tick_spacing,
                        &v.parameters,
                    )?;
                    if id != v.pool_id || p.id != format!("{}/{}", v.manager, id) {
                        return Err(fail());
                    }
                } else if p.singleton_cl.is_some() {
                    return Err(fail());
                }
                if let Some(profile) =
                    profile.filter(|p| ["aero", "curve", "balancer"].contains(&p.family))
                {
                    let v = p.stable.as_ref().ok_or_else(fail)?;
                    if p.v2.is_some()
                        || p.v3.is_some()
                        || p.fee_denominator != profile.fee_denominator
                    {
                        return Err(fail());
                    }
                    xone_domain::protocols::validate_identity(&p.id, &v.expected_code_sha256)?;
                    for a in [&p.token0, &p.token1] {
                        xone_domain::protocols::validate_address(a)?;
                    }
                    if !v.expected_implementation.is_empty() {
                        xone_domain::protocols::validate_identity(
                            &v.expected_implementation,
                            &v.expected_implementation_code_sha256,
                        )?;
                    } else if !v.expected_implementation_code_sha256.is_empty() {
                        return Err(fail());
                    }
                    match profile.family {
                        "aero" => {
                            xone_domain::protocols::validate_identity(
                                &v.expected_factory,
                                &v.expected_factory_code_sha256,
                            )?;
                            if !v.expected_vault.is_empty() || !v.pool_id.is_empty() {
                                return Err(fail());
                            }
                        }
                        "curve" => {
                            if !v.expected_factory.is_empty()
                                || !v.expected_vault.is_empty()
                                || !v.pool_id.is_empty()
                            {
                                return Err(fail());
                            }
                        }
                        "balancer" => {
                            xone_domain::protocols::validate_identity(
                                &v.expected_vault,
                                &v.expected_vault_code_sha256,
                            )?;
                            if v.pool_id.len() != 66
                                || !v.pool_id.starts_with("0x")
                                || hex::decode(&v.pool_id[2..]).is_err()
                                || !v.expected_factory.is_empty()
                            {
                                return Err(fail());
                            }
                        }
                        _ => return Err(fail()),
                    }
                } else if p.stable.is_some() {
                    return Err(fail());
                }
                if let Some(v) = &p.v2 {
                    if profile.is_none_or(|p| p.family != "v2") {
                        return Err(fail());
                    }
                    xone_domain::protocols::validate_identity(
                        &v.expected_factory,
                        &v.expected_code_sha256,
                    )?;
                }
                if let Some(profile) = profile.filter(|p| p.family == "v2") {
                    if profile.require_identity && p.v2.is_none() {
                        return Err(fail());
                    }
                    if p.protocol == xone_domain::v2_swap::PROFILE {
                        let v = p.v2.as_ref().ok_or_else(fail)?;
                        if v.expected_code_sha256 != xone_domain::v2_swap::PAIR_CODE_SHA256
                            || p.token0 >= p.token1
                            || !p.program_id.is_empty()
                            || !p.accounts.is_empty()
                        {
                            return Err(fail());
                        }
                        for a in [&p.id, &p.token0, &p.token1, &v.expected_factory] {
                            if xone_domain::protocols::validate_address(a).is_err()
                                || a[2..].bytes().all(|b| b == b'0')
                            {
                                return Err(fail());
                            }
                        }
                    }
                    if profile.fee_denominator != 0
                        && (p.fee_numerator != profile.fee_numerator
                            || p.fee_denominator != profile.fee_denominator)
                    {
                        return Err(fail());
                    }
                }
                if profile.is_some_and(|p| p.family == "v3") {
                    let v = p.v3.as_ref().ok_or_else(fail)?;
                    if !v.expected_implementation.is_empty() {
                        xone_domain::protocols::validate_identity(
                            &v.expected_implementation,
                            &v.expected_implementation_code_sha256,
                        )?;
                    } else if !v.expected_implementation_code_sha256.is_empty() {
                        return Err(fail());
                    }
                    if p.protocol == "aerodrome-slipstream"
                        || !v.expected_factory_code_sha256.is_empty()
                    {
                        xone_domain::protocols::validate_identity(
                            &v.expected_factory,
                            &v.expected_factory_code_sha256,
                        )?;
                    }
                    if v.tick_word_radius > 16
                        || !(1..=4096).contains(&v.max_initialized_ticks)
                        || !(1..=8192).contains(&v.max_steps)
                        || p.fee_denominator != 1000000
                        || p.fee_numerator >= 1000000
                    {
                        return Err(fail());
                    }
                    for a in [&p.id, &p.token0, &p.token1, &v.expected_factory] {
                        if !a.starts_with("0x")
                            || a.len() != 42
                            || *a != a.to_ascii_lowercase()
                            || hex::decode(&a[2..]).is_err()
                        {
                            return Err(fail());
                        }
                    }
                    if v.expected_code_sha256.len() != 64
                        || v.expected_code_sha256 != v.expected_code_sha256.to_ascii_lowercase()
                        || hex::decode(&v.expected_code_sha256).is_err()
                    {
                        return Err(fail());
                    }
                } else if p.v3.is_some() {
                    return Err(fail());
                }
            }
            validate_chain(&c.key)?;
            if !seen.insert(&c.key)
                || c.assignment_version == 0
                || c.genesis_hash.is_empty()
                || c.auto_reorg_window == 0
                || expected != "dex-router" && c.owner_service != expected
            {
                return Err(fail());
            }
            if c.key.starts_with("evm:") && c.owner_service != "chain-parser-go"
                || c.key.starts_with("solana:") && c.owner_service != "chain-parser-rust"
                || expected != "dex-router"
                    && self.service.mode == "rpc"
                    && c.rpc_urls.is_empty()
                    && !self.yellowstone.contains_key(&c.key)
            {
                return Err(fail());
            }
        }
        if s.max_baseline_age_ms > 315360000000
            || s.max_tail_blocks > 100000
            || j.poll_interval_ms > 60000
            || j.request_timeout_ms > 60000
            || self.api.quote_timeout_ms > 60000
            || self.api.requests_per_minute > 1000000
            || r.max_state_age_ms > 315360000000
            || r.quote_ttl_ms > 600000
            || self.control.lease_ttl_ms > 3600000
            || self.control.min_commit_remaining_ms == 0
            || j.kafka.transaction_timeout_ms > 60000
            || sim.max_gas > 1000000000
            || sim.max_state_reads > 1000000
            || sim.timeout_ms > 60000
        {
            return Err(fail());
        }
        let c = &self.control;
        let k = &self.journal.kafka;
        if c.instance_id.is_empty()
            || c.lease_ttl_ms < 1000
            || c.renewal_ms == 0
            || c.renewal_ms >= c.lease_ttl_ms / 3
            || c.min_commit_remaining_ms >= c.lease_ttl_ms
            || !["disabled", "postgres"].contains(&c.backend.as_str())
        {
            return Err(fail());
        }
        if c.backend == "postgres" && (c.dsn_file.is_empty() || !c.allow_plaintext_development) {
            return Err(fail());
        }
        if k.replica_id.is_empty()
            || !(1024..=1048576).contains(&k.chunk_bytes)
            || k.transaction_timeout_ms == 0
            || !(3..=100000).contains(&k.max_tail_records)
        {
            return Err(fail());
        }
        if j.transport == "kafka" {
            if k.brokers.is_empty()
                || k.security_protocol != "PLAINTEXT"
                || !k.allow_plaintext_development
                || expected != "dex-router" && c.backend != "postgres"
                || c.min_commit_remaining_ms <= k.transaction_timeout_ms
            {
                return Err(fail());
            }
            let mut topics = std::collections::BTreeSet::new();
            for chain in &self.chains {
                let topic = k.topics.get(&chain.key).ok_or_else(fail)?;
                if topic.is_empty() || !topics.insert(topic) {
                    return Err(fail());
                }
            }
        }
        Ok(())
    }
}
impl ConfigSnapshot {
    pub fn resolve(&self, p: &str) -> PathBuf {
        self.root.join(p)
    }
}
pub fn cli_args() -> Result<(String, bool)> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut path = None;
    let mut check = false;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" if path.is_none() && i + 1 < args.len() => {
                i += 1;
                path = Some(args[i].clone());
            }
            "--check-config" if !check => check = true,
            _ => {
                return Err(err(
                    "USAGE: --config config/config-dev.yaml [--check-config]",
                ))
            }
        }
        i += 1;
    }
    Ok((path.ok_or_else(|| err("CONFIG_ARGUMENT_REQUIRED"))?, check))
}
