use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_domain::Pool;
use xone_solana_ingest::{orca, raydium::RpcAccount, raydium_clmm};
fn corpus(orca: bool) -> Value {
    serde_json::from_str(if orca {
        include_str!("../../../test/contracts/solana-advanced/orca-rewards.json")
    } else {
        include_str!("../../../test/contracts/solana-advanced/raydium-clmm-rewards.json")
    })
    .unwrap()
}
fn unhex(s: &str) -> Vec<u8> {
    s.as_bytes()
        .chunks_exact(2)
        .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
        .collect()
}
struct Fixture {
    orca: bool,
    ch: Chain,
    cfg: PoolConfig,
    accounts: BTreeMap<String, RpcAccount>,
    slot: u64,
}
impl Fixture {
    fn new(orca: bool) -> Self {
        Self::with_layout(orca, false)
    }
    fn with_layout(orca: bool, dynamic: bool) -> Self {
        let c = corpus(orca);
        let f = &c[if dynamic {
            "dynamic_fixture"
        } else {
            "fixture"
        }];
        let sk = if orca {
            "orca_whirlpool"
        } else {
            "raydium_clmm"
        };
        let accounts = f["rpc_accounts"]
            .as_object()
            .unwrap()
            .iter()
            .map(|(id, v)| {
                let a = if orca {
                    RpcAccount {
                        owner: v["owner"].as_str().unwrap().into(),
                        data: unhex(v["data_hex"].as_str().unwrap()),
                        executable: v["executable"].as_bool().unwrap(),
                        lamports: v["lamports"].as_u64().unwrap(),
                    }
                } else {
                    RpcAccount::decode(v).unwrap()
                };
                (id.clone(), a)
            })
            .collect();
        Self{orca,ch:serde_json::from_value(json!({"key":c["base_pool"]["chain_key"],"owner_service":"reward-test","assignment_version":1,"genesis_hash":"fixture","rpc_urls":[],"pools":[],"confirmation_tag":"finalized","auto_reorg_window":32})).unwrap(),
            cfg:serde_json::from_value(f["pool_config"].clone()).unwrap(),accounts,slot:c["base_pool"][sk]["bank_slot"].as_u64().unwrap()}
    }
    fn decode(&self) -> xone_domain::Result<Pool> {
        if self.orca {
            orca::decode_orca_whirlpool(&self.ch, &self.cfg, &self.accounts, self.slot)
        } else {
            raydium_clmm::decode_raydium_clmm(&self.ch, &self.cfg, &self.accounts, self.slot)
        }
    }
}
#[test]
fn original_native_packed_accounts_decode_to_complete_reward_and_tick_state() {
    for (is_orca, dynamic) in [(true, false), (true, true), (false, false)] {
        let f = Fixture::with_layout(is_orca, dynamic);
        let p = f.decode().unwrap();
        let c = corpus(is_orca);
        let expected: Pool = serde_json::from_value(
            c[if dynamic {
                "dynamic_fixture"
            } else {
                "fixture"
            }]["pool"]
                .clone(),
        )
        .unwrap();
        assert_eq!(p, expected);
    }
}
#[test]
fn initialized_rewards_remain_rejected_by_legacy_profiles() {
    for is_orca in [true, false] {
        let mut f = Fixture::new(is_orca);
        f.cfg.protocol = if is_orca {
            "orca-whirlpools-static"
        } else {
            "raydium-clmm-static"
        }
        .into();
        assert!(f.decode().is_err());
    }
}
#[test]
fn packed_reward_control_padding_and_uninitialized_growth_fail_closed() {
    for is_orca in [true, false] {
        for kind in ["control", "uninitialized", "tick-growth"] {
            let mut f = Fixture::new(is_orca);
            let state = f.accounts.get_mut(&f.cfg.id).unwrap();
            if kind == "control" {
                state.data[if is_orca { 269 + 128 + 64 } else { 397 }] = 4;
            } else if kind == "uninitialized" {
                state.data[if is_orca {
                    269 + 2 * 128 + 96
                } else {
                    397 + 2 * 169 + 25
                }] = 1;
            } else {
                let ids = if is_orca {
                    orca::required_accounts(&f.cfg).unwrap()
                } else {
                    raydium_clmm::required_accounts(&f.cfg).unwrap()
                };
                let array = f.accounts.get_mut(&ids[11]).unwrap();
                // First array's first tick is initialized in both native fixtures.
                array.data[if is_orca { 12 + 65 + 32 } else { 44 + 68 + 32 }] = 1;
            }
            assert!(
                f.decode().is_err(),
                "unsupported packed state accepted: {is_orca} {kind}"
            );
        }
    }
}
#[test]
fn token2022_and_initialized_adaptive_oracles_are_still_unsupported() {
    for is_orca in [true, false] {
        let mut f = Fixture::new(is_orca);
        let mint = &f.cfg.token0;
        let a = f.accounts.get_mut(mint).unwrap();
        a.owner = "TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb".into();
        a.data.extend_from_slice(&[0; 128]);
        assert!(f.decode().is_err());
        if is_orca {
            let mut f = Fixture::new(true);
            let oracle = orca::oracle_address(&f.cfg).unwrap();
            let a = f.accounts.get_mut(&oracle).unwrap();
            a.owner = orca::PROGRAM_ID.into();
            a.data = vec![0; 254];
            assert!(f.decode().is_err());
        }
    }
}
#[test]
fn native_fixture_spl_accounts_have_no_hidden_extension_bytes() {
    let c = corpus(false);
    let bytes = STANDARD
        .decode(
            c["fixture"]["rpc_accounts"][c["base_pool"]["token0"].as_str().unwrap()]["data"][0]
                .as_str()
                .unwrap(),
        )
        .unwrap();
    assert_eq!(bytes.len(), 82);
}
