use serde::{Deserialize, Serialize};
use xone_domain::{err, Result};
/// Conditional-PUT HTTP object API. Tokens use the exact base URL key in source_auth_tokens.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointStore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub local_budget: Option<CheckpointBudget>,
    #[serde(default, skip_serializing_if = "zero")]
    pub publish_interval_ms: u64,
    pub base_url: String,
    #[serde(default)]
    pub allow_plaintext_development: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointBudget {
    pub max_bytes: u64,
    pub max_files: u64,
    pub max_entries: u64,
}
impl CheckpointBudget {
    pub fn validate(&self) -> Result<()> {
        if self.max_bytes == 0
            || self.max_bytes > 1 << 40
            || self.max_files == 0
            || self.max_files > 100000
            || self.max_entries < self.max_files
            || self.max_entries > 1000000
        {
            return Err(err("CHECKPOINT_LOCAL_BUDGET_CONFIG"));
        }
        Ok(())
    }
}
fn zero(n: &u64) -> bool {
    *n == 0
}
impl CheckpointStore {
    pub fn validate(&self) -> Result<()> {
        if let Some(budget) = &self.local_budget {
            budget.validate()?;
        }
        let fail = || err("CHECKPOINT_STORE_CONFIG_INVALID");
        if self.base_url.len() > 2048
            || self.publish_interval_ms > 3600000
            || (self.publish_interval_ms > 0 && self.publish_interval_ms < 100)
        {
            return Err(fail());
        }
        let (rest, secure) = if let Some(rest) = self.base_url.strip_prefix("https://") {
            (rest, true)
        } else if let Some(rest) = self.base_url.strip_prefix("http://") {
            (rest, false)
        } else {
            return Err(fail());
        };
        if !secure && !self.allow_plaintext_development {
            return Err(fail());
        }
        let mut parts = rest.split('/');
        let authority = parts
            .next()
            .ok_or_else(fail)?
            .split(':')
            .collect::<Vec<_>>();
        let host = authority[0];
        if authority.len() > 2 || host.is_empty() || host.len() > 253 {
            return Err(fail());
        }
        for label in host.split('.') {
            if label.is_empty()
                || label.len() > 63
                || label.starts_with('-')
                || label.ends_with('-')
                || !label
                    .bytes()
                    .all(|v| v.is_ascii_lowercase() || v.is_ascii_digit() || v == b'-')
            {
                return Err(fail());
            }
        }
        if authority.len() == 2 {
            let port = authority[1].parse::<u16>().map_err(|_| fail())?;
            if port == 0 || port.to_string() != authority[1] {
                return Err(fail());
            }
        }
        if !secure && host != "127.0.0.1" {
            return Err(fail());
        }
        for part in parts {
            if part.is_empty()
                || [".", ".."].contains(&part)
                || !part
                    .bytes()
                    .all(|v| v.is_ascii_alphanumeric() || b"._-".contains(&v))
            {
                return Err(fail());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checkpoint_store_url_scope_is_explicit_and_shared() {
        let mut cfg = crate::load("../../config/config-test.yaml", "dex-router")
            .or_else(|_| crate::load("../../config/config-test.yaml", "chain-parser-rust"))
            .unwrap()
            .config;
        let expected = cfg.service.name.clone();
        for url in [
            "https://objects.example/bucket",
            "https://objects.example:443/bucket-1",
            "http://127.0.0.1:8092/bucket",
        ] {
            cfg.journal.checkpoint_store = Some(CheckpointStore {
                local_budget: None,
                publish_interval_ms: 0,
                base_url: url.into(),
                allow_plaintext_development: true,
            });
            cfg.validate(&expected).unwrap();
        }
        for url in [
            "http://objects.example/bucket",
            "https://objects.example/",
            "https://objects.example/a/../b",
            "https://objects.example/a%2fb",
            "https://user:secret@objects.example",
            "https://objects.example?token=secret",
            "https://objects.example/#x",
            "https://objects.example:0/b",
            "https://objects.example:0443/b",
            "https://Objects.example/b",
            "https://objects.example/a//b",
            "http://127.0.0.10:8092",
        ] {
            cfg.journal.checkpoint_store = Some(CheckpointStore {
                local_budget: None,
                publish_interval_ms: 0,
                base_url: url.into(),
                allow_plaintext_development: true,
            });
            assert!(cfg.validate(&expected).is_err(), "{url}");
        }
        cfg.journal.checkpoint_store = Some(CheckpointStore {
            local_budget: None,
            publish_interval_ms: 0,
            base_url: "http://127.0.0.1:8092/bucket".into(),
            allow_plaintext_development: false,
        });
        assert!(cfg.validate(&expected).is_err());
    }
    #[test]
    fn checkpoint_periodic_publishing_is_bounded_and_never_enabled_for_rust_roles() {
        let mut cfg = crate::load("../../config/config-test.yaml", "dex-router")
            .or_else(|_| crate::load("../../config/config-test.yaml", "chain-parser-rust"))
            .unwrap()
            .config;
        let expected = cfg.service.name.clone();
        let mut store: CheckpointStore =
            serde_json::from_str(r#"{"base_url":"https://objects.example/bucket"}"#).unwrap();
        assert_eq!(store.publish_interval_ms, 0);
        store.validate().unwrap();
        for interval in [100, 1000, 3600000] {
            store.publish_interval_ms = interval;
            store.validate().unwrap();
            cfg.journal.checkpoint_store = Some(store.clone());
            // Historical selector is retained; stage110 enables Rust online owner.
            if expected == "chain-parser-rust" {
                cfg.validate(&expected).unwrap();
            } else {
                assert_eq!(
                    cfg.validate(&expected).unwrap_err().0,
                    "CHECKPOINT_PUBLISH_OWNER"
                );
            }
        }
        for interval in [1, 99, 3600001, u64::MAX] {
            store.publish_interval_ms = interval;
            assert!(store.validate().is_err());
        }
    }
    #[test]
    fn periodic_checkpoint_config_accepts_online_parser_roles_and_rejects_quote_roles() {
        let mut cfg = crate::load("../../config/config-test.yaml", "dex-router")
            .or_else(|_| crate::load("../../config/config-test.yaml", "chain-parser-rust"))
            .unwrap()
            .config;
        cfg.service.name = "chain-parser-go".into();
        cfg.service.role = "online".into();
        cfg.state.engine = "pebble".into();
        cfg.yellowstone.clear();
        let mut chain = cfg.chains[0].clone();
        chain.key = "evm:31337".into();
        chain.owner_service = "chain-parser-go".into();
        chain.genesis_hash = format!("0x{}", "0".repeat(64));
        chain.confirmation_tag = "latest".into();
        chain.rpc_urls = vec!["http://127.0.0.1:8545".into()];
        chain.pools.clear();
        cfg.chains = vec![chain];
        cfg.journal.checkpoint_store = Some(CheckpointStore {
            local_budget: None,
            base_url: "https://objects.example/bucket".into(),
            allow_plaintext_development: false,
            publish_interval_ms: 100,
        });
        cfg.validate("chain-parser-go").unwrap();
        cfg.service.name = "chain-parser-rust".into();
        cfg.state.engine = "rocksdb".into();
        cfg.chains[0].key = "solana:localnet".into();
        cfg.chains[0].owner_service = "chain-parser-rust".into();
        cfg.chains[0].genesis_hash = "11111111111111111111111111111111".into();
        cfg.chains[0].confirmation_tag = "finalized".into();
        cfg.chains[0].rpc_urls = vec!["http://127.0.0.1:8899".into()];
        cfg.validate("chain-parser-rust").unwrap();
        cfg.service.name = "dex-router".into();
        cfg.state.engine = "memory".into();
        assert_eq!(
            cfg.validate("dex-router").unwrap_err().0,
            "CHECKPOINT_PUBLISH_OWNER"
        );
    }
    #[test]
    fn checkpoint_local_budget_parameters_fail_before_runtime() {
        for budget in [
            CheckpointBudget {
                max_bytes: 0,
                max_files: 1,
                max_entries: 1,
            },
            CheckpointBudget {
                max_bytes: (1u64 << 40) + 1,
                max_files: 1,
                max_entries: 1,
            },
            CheckpointBudget {
                max_bytes: 1,
                max_files: 0,
                max_entries: 1,
            },
            CheckpointBudget {
                max_bytes: 1,
                max_files: 100001,
                max_entries: 100001,
            },
            CheckpointBudget {
                max_bytes: 1,
                max_files: 5,
                max_entries: 4,
            },
            CheckpointBudget {
                max_bytes: 1,
                max_files: 1,
                max_entries: 1000001,
            },
        ] {
            assert!(budget.validate().is_err());
        }
        CheckpointBudget {
            max_bytes: 1,
            max_files: 1,
            max_entries: 1,
        }
        .validate()
        .unwrap();
    }
}
