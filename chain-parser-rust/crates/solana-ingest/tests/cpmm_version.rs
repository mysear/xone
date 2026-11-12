use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_solana_ingest::rpc::{decode_cpmm, decode_cpmm_at, Account, CPMM_PROGRAM, TOKEN_PROGRAM};

struct Fixture {
    chain: Chain,
    pool: PoolConfig,
    accounts: BTreeMap<String, Account>,
}

fn key(byte: u8) -> String {
    bs58::encode([byte; 32]).into_string()
}

fn put_key(data: &mut [u8], offset: usize, address: &str) {
    let bytes = bs58::decode(address).into_vec().unwrap();
    data[offset..offset + 32].copy_from_slice(&bytes);
}

fn put_u64(data: &mut [u8], offset: usize, value: u64) {
    data[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
}

impl Fixture {
    fn archived() -> Self {
        let ids: Vec<_> = (1..=6).map(key).collect();
        let pool = PoolConfig {
            pancake_bsc_lm: None,
            pancake_lm: None,
            slipstream_context: None,
            lfj: None,
            meteora: None,
            pump: None,
            infinity_bin: None,
            stable: None,
            weighted: None,
            multi_asset: None,
            singleton_advanced: None,
            singleton_cl: None,
            raydium_cpmm: None,
            orca_whirlpool: None,
            raydium_clmm: None,
            id: ids[0].clone(),
            protocol: "raydium-cpmm".into(),
            token0: ids[4].clone(),
            token1: ids[5].clone(),
            fee_numerator: 2500,
            fee_denominator: 1_000_000,
            program_id: CPMM_PROGRAM.into(),
            accounts: ids.clone(),
            v2: None,
            v3: None,
        };
        let chain = Chain {
            key: "solana:mainnet".into(),
            owner_service: "test-parser".into(),
            assignment_version: 1,
            genesis_hash: key(20),
            rpc_urls: vec![],
            pools: vec![pool.clone()],
            confirmation_tag: "finalized".into(),
            auto_reorg_window: 32,
        };
        let mut state = vec![0; 637];
        state[..8].copy_from_slice(&[247, 237, 227, 245, 215, 195, 222, 70]);
        for (offset, address) in [
            (8, &ids[3]),
            (40, &key(9)),
            (72, &ids[1]),
            (104, &ids[2]),
            (136, &key(10)),
            (168, &ids[4]),
            (200, &ids[5]),
            (232, &TOKEN_PROGRAM.to_owned()),
            (264, &TOKEN_PROGRAM.to_owned()),
            (296, &key(8)),
        ] {
            put_key(&mut state, offset, address);
        }
        state[328] = 255;
        state[330..333].copy_from_slice(&[9, 6, 9]);
        for (offset, value) in [
            (333, 5000),
            (341, 50),
            (349, 60),
            (357, 7),
            (365, 11),
            (381, 123),
        ] {
            put_u64(&mut state, offset, value);
        }
        let mut config = vec![0; 236];
        config[..8].copy_from_slice(&[218, 244, 33, 104, 203, 203, 43, 111]);
        for (offset, value) in [(12, 2500), (20, 100_000), (28, 50_000), (36, 1000)] {
            put_u64(&mut config, offset, value);
        }
        put_key(&mut config, 44, &key(11));
        put_key(&mut config, 76, &key(12));
        let mut accounts = BTreeMap::new();
        accounts.insert(
            ids[0].clone(),
            Account {
                owner: CPMM_PROGRAM.into(),
                data: state,
            },
        );
        accounts.insert(
            ids[3].clone(),
            Account {
                owner: CPMM_PROGRAM.into(),
                data: config,
            },
        );
        for (vault_index, mint_index, amount, decimals) in [(1, 4, 10_000, 6), (2, 5, 20_000, 9)] {
            let mut vault = vec![0; 165];
            put_key(&mut vault, 0, &ids[mint_index]);
            put_key(&mut vault, 32, &key(7));
            put_u64(&mut vault, 64, amount);
            vault[108] = 1;
            let mut mint = vec![0; 82];
            put_u64(&mut mint, 36, 1_000_000);
            mint[44] = decimals;
            mint[45] = 1;
            accounts.insert(
                ids[vault_index].clone(),
                Account {
                    owner: TOKEN_PROGRAM.into(),
                    data: vault,
                },
            );
            accounts.insert(
                ids[mint_index].clone(),
                Account {
                    owner: TOKEN_PROGRAM.into(),
                    data: mint,
                },
            );
        }
        Self {
            chain,
            pool,
            accounts,
        }
    }

    fn data(&mut self, account_index: usize) -> &mut [u8] {
        &mut self
            .accounts
            .get_mut(&self.pool.accounts[account_index])
            .unwrap()
            .data
    }

    fn assert_rejected(&self, expected: &str) {
        let error = decode_cpmm(&self.chain, &self.pool, &self.accounts).unwrap_err();
        assert_eq!(error.0, expected);
        let error = decode_cpmm_at(&self.chain, &self.pool, &self.accounts, 1000).unwrap_err();
        assert_eq!(error.0, expected);
    }
}

#[test]
fn zero_reserved_archive_layout_preserves_reserves_and_adapter() {
    let fixture = Fixture::archived();
    let decoded = decode_cpmm(&fixture.chain, &fixture.pool, &fixture.accounts).unwrap();
    assert_eq!(decoded.reserve0.to_string(), "9943");
    assert_eq!(decoded.reserve1.to_string(), "19929");
    assert_eq!(decoded.fee_numerator, 2500);
    assert_eq!(decoded.fee_denominator, 1_000_000);
    assert_eq!(decoded.adapter_version, "raydium-cpmm/archive-v1");
}

#[test]
fn rejects_creator_modes_including_unknown_mode() {
    for mode in [1, 2, 3, 255] {
        let mut fixture = Fixture::archived();
        fixture.data(0)[389] = mode;
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
}

#[test]
fn rejects_creator_enable_even_when_config_rate_is_zero() {
    let mut fixture = Fixture::archived();
    fixture.data(0)[390] = 1;
    fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
}

#[test]
fn rejects_creator_rates_and_share_even_when_pool_fee_is_disabled() {
    for offset in [108, 116] {
        let mut fixture = Fixture::archived();
        put_u64(fixture.data(3), offset, 1);
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
}

#[test]
fn rejects_each_accrued_creator_balance_even_when_creator_fee_is_disabled() {
    for offset in [397, 405] {
        let mut fixture = Fixture::archived();
        put_u64(fixture.data(0), offset, 1);
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
}

#[test]
fn rejects_unknown_pool_and_config_reserved_fields() {
    for (account, offset) in [(0, 391), (0, 413), (0, 636), (3, 124), (3, 235)] {
        let mut fixture = Fixture::archived();
        fixture.data(account)[offset] = 1;
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
}

#[test]
fn wrapper_rejects_nonzero_open_time_without_pinned_bank_time() {
    let mut fixture = Fixture::archived();
    put_u64(fixture.data(0), 373, 1);
    let error = decode_cpmm(&fixture.chain, &fixture.pool, &fixture.accounts).unwrap_err();
    assert_eq!(error.0, "POOL_NOT_OPEN");
}

#[test]
fn preserves_program_and_account_identity_checks() {
    for account in [0, 3] {
        let mut fixture = Fixture::archived();
        fixture
            .accounts
            .get_mut(&fixture.pool.accounts[account])
            .unwrap()
            .owner = key(99);
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
    let mut fixture = Fixture::archived();
    put_key(fixture.data(0), 72, &key(99));
    fixture.assert_rejected("POOL_ACCOUNT_IDENTITY");
    for account in [1, 2, 4, 5] {
        let mut fixture = Fixture::archived();
        fixture
            .accounts
            .get_mut(&fixture.pool.accounts[account])
            .unwrap()
            .owner = key(99);
        fixture.assert_rejected("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED");
    }
    let mut fixture = Fixture::archived();
    put_key(fixture.data(1), 0, &key(99));
    fixture.assert_rejected("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED");
}

#[test]
fn bank_timestamp_rejects_future_opening_even_when_wall_clock_has_passed() {
    for open_time in [1001, u64::MAX] {
        let mut fixture = Fixture::archived();
        put_u64(fixture.data(0), 373, open_time);
        let error =
            decode_cpmm_at(&fixture.chain, &fixture.pool, &fixture.accounts, 1000).unwrap_err();
        assert_eq!(error.0, "POOL_NOT_OPEN");
    }
}

#[test]
fn bank_timestamp_accepts_opening_boundary_and_past_opening() {
    for (open_time, bank_timestamp) in [(0, 0), (999, 1000), (1000, 1000), (u64::MAX, u64::MAX)] {
        let mut fixture = Fixture::archived();
        put_u64(fixture.data(0), 373, open_time);
        let decoded = decode_cpmm_at(
            &fixture.chain,
            &fixture.pool,
            &fixture.accounts,
            bank_timestamp,
        )
        .unwrap();
        assert_eq!(decoded.reserve0.to_string(), "9943");
        assert_eq!(decoded.reserve1.to_string(), "19929");
        assert_eq!(decoded.adapter_version, "raydium-cpmm/archive-v1");
        assert!(decoded.stable.is_none());
    }
}

#[test]
fn rejects_wrong_lengths_without_reading_reserved_offsets() {
    for (account, size) in [(0, 0), (0, 636), (0, 638), (3, 0), (3, 235), (3, 237)] {
        let mut fixture = Fixture::archived();
        fixture
            .accounts
            .get_mut(&fixture.pool.accounts[account])
            .unwrap()
            .data
            .resize(size, 0);
        fixture.assert_rejected("PROGRAM_VERSION_UNSUPPORTED");
    }
}

#[test]
fn preserves_swap_status_and_mint_program_checks() {
    let mut fixture = Fixture::archived();
    fixture.data(0)[329] = 4;
    fixture.assert_rejected("POOL_SWAP_DISABLED");
    for offset in [168, 200] {
        let mut fixture = Fixture::archived();
        put_key(fixture.data(0), offset, &key(99));
        fixture.assert_rejected("POOL_ACCOUNT_IDENTITY");
    }
    for offset in [232, 264] {
        let mut fixture = Fixture::archived();
        put_key(fixture.data(0), offset, &key(99));
        fixture.assert_rejected("TOKEN_EXTENSION_OR_LAYOUT_UNSUPPORTED");
    }
}
