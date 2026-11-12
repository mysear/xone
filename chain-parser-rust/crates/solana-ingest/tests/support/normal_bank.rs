use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_solana_ingest::rpc::{Account, CPMM_PROGRAM, TOKEN_PROGRAM};

struct Fixture {
    chain: Chain,
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
        Self { chain, accounts }
    }
}

pub fn archive() -> (Chain, BTreeMap<String, Account>) {
    let mut f = Fixture::archived();
    f.chain.key = "solana:localnet".into();
    f.chain.genesis_hash = "localnet".into();
    f.chain.owner_service = "chain-parser-rust".into();
    f.chain.rpc_urls = vec![];
    (f.chain, f.accounts)
}
