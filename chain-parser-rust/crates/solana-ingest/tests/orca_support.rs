//! Synthetic account bytes; these pins are not deployment evidence.
#![allow(dead_code)]
use curve25519_dalek::edwards::CompressedEdwardsY;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use xone_config::{Chain, PoolConfig};
use xone_solana_ingest::{orca, raydium::RpcAccount, rpc::TOKEN_PROGRAM};
pub const PROGRAM: &str = "whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc";
pub const PD: &str = "CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD";
pub const LOADER: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
pub const LOADER2: &str = "BPFLoader2111111111111111111111111111111111";
pub const SYSTEM: &str = "11111111111111111111111111111111";
pub const CLOCK: &str = "SysvarC1ock11111111111111111111111111111111";
pub fn key(n: u8) -> String {
    bs58::encode([n; 32]).into_string()
}
pub fn bytes(k: &str) -> Vec<u8> {
    bs58::decode(k).into_vec().unwrap()
}
pub fn put_key(d: &mut [u8], o: usize, k: &str) {
    d[o..o + 32].copy_from_slice(&bytes(k));
}
pub fn put64(d: &mut [u8], o: usize, n: u64) {
    d[o..o + 8].copy_from_slice(&n.to_le_bytes());
}
pub fn put128(d: &mut [u8], o: usize, n: u128) {
    d[o..o + 16].copy_from_slice(&n.to_le_bytes());
}
pub fn disc(name: &str) -> [u8; 8] {
    Sha256::digest(format!("account:{name}").as_bytes())[..8]
        .try_into()
        .unwrap()
}
pub fn pda(seeds: &[&[u8]]) -> (String, u8) {
    for bump in (0..=255u8).rev() {
        let mut h = Sha256::new();
        for s in seeds {
            h.update(s);
        }
        h.update([bump]);
        h.update(bytes(PROGRAM));
        h.update(b"ProgramDerivedAddress");
        let b: [u8; 32] = h.finalize().into();
        if CompressedEdwardsY(b).decompress().is_none() {
            return (bs58::encode(b).into_string(), bump);
        }
    }
    unreachable!()
}
pub fn account(owner: &str, data: Vec<u8>, executable: bool) -> RpcAccount {
    RpcAccount {
        owner: owner.into(),
        data,
        executable,
        lamports: 1_000_000_000,
    }
}
pub struct Fixture {
    pub chain: Chain,
    pub pool: PoolConfig,
    pub accounts: BTreeMap<String, RpcAccount>,
    pub oracle: String,
    pub arrays: Vec<String>,
}
impl Fixture {
    pub fn new(dynamic: bool) -> Self {
        Self::with_native(dynamic, false)
    }
    pub fn with_native(dynamic: bool, native: bool) -> Self {
        let cfg = key(4);
        let mint0 = key(5);
        let mint1 = if native {
            "So11111111111111111111111111111111111111112".into()
        } else {
            key(6)
        };
        let (pool, bump) = pda(&[
            b"whirlpool",
            &bytes(&cfg),
            &bytes(&mint0),
            &bytes(&mint1),
            &64u16.to_le_bytes(),
        ]);
        let oracle = pda(&[b"oracle", &bytes(&pool)]).0;
        let starts = [-5632i32, 0, 5632];
        let arrays: Vec<_> = starts
            .iter()
            .map(|s| pda(&[b"tick_array", &bytes(&pool), s.to_string().as_bytes()]).0)
            .collect();
        let code = b"orca synthetic pinned code";
        let token_code = b"classic token synthetic code";
        let config:PoolConfig=serde_json::from_value(json!({"id":pool,"protocol":"orca-whirlpools-static","program_id":PROGRAM,"token0":mint0,"token1":mint1,"accounts":[pool,key(2),key(3),cfg,mint0,mint1],"fee_numerator":999,"fee_denominator":1000000,"raydium_cpmm":null,"v2":null,"v3":null,"stable":null,"singleton_cl":null,"orca_whirlpool":{"expected_programdata":PD,"expected_program_code_sha256":format!("{:x}",Sha256::digest(code)),"expected_deployed_slot":50,"expected_token_program_code_sha256":format!("{:x}",Sha256::digest(token_code)),"tick_array_starts":starts,"max_steps":8192}})).unwrap();
        let chain:Chain=serde_json::from_value(json!({"key":"solana:test","owner_service":"solana-parser","assignment_version":1,"genesis_hash":"test-genesis","rpc_urls":[],"pools":[],"confirmation_tag":"finalized","auto_reorg_window":64})).unwrap();
        let mut accounts = BTreeMap::new();
        let mut state = vec![0; 653];
        state[..8].copy_from_slice(&disc("Whirlpool"));
        put_key(&mut state, 8, &cfg);
        state[40] = bump;
        state[41..43].copy_from_slice(&64u16.to_le_bytes());
        state[43..45].copy_from_slice(&64u16.to_le_bytes());
        state[45..47].copy_from_slice(&3000u16.to_le_bytes());
        state[47..49].copy_from_slice(&1000u16.to_le_bytes());
        put128(&mut state, 49, 1000000);
        put128(&mut state, 65, 1u128 << 64);
        put_key(&mut state, 101, &mint0);
        put_key(&mut state, 133, &key(2));
        put128(&mut state, 165, 17);
        put_key(&mut state, 181, &mint1);
        put_key(&mut state, 213, &key(3));
        put128(&mut state, 245, 19);
        put64(&mut state, 261, 900);
        put_key(&mut state, 333, &key(9));
        accounts.insert(pool.clone(), account(PROGRAM, state, false));
        let mut conf = vec![0; 108];
        conf[..8].copy_from_slice(&disc("WhirlpoolsConfig"));
        conf[104..106].copy_from_slice(&1000u16.to_le_bytes());
        accounts.insert(cfg, account(PROGRAM, conf, false));
        for (i, m) in [mint0, mint1].iter().enumerate() {
            let mut mint = vec![0; 82];
            mint[44] = if native && i == 1 { 9 } else { 6 };
            mint[45] = 1;
            accounts.insert(m.clone(), account(TOKEN_PROGRAM, mint, false));
            let mut v = vec![0; 165];
            put_key(&mut v, 0, m);
            put_key(&mut v, 32, &pool);
            put64(&mut v, 64, 10000000);
            v[108] = 1;
            if native && i == 1 {
                v[109..113].copy_from_slice(&1u32.to_le_bytes());
                put64(&mut v, 113, 1000);
            }
            accounts.insert(key(2 + i as u8), account(TOKEN_PROGRAM, v, false));
        }
        let mut program = vec![0; 36];
        program[..4].copy_from_slice(&2u32.to_le_bytes());
        put_key(&mut program, 4, PD);
        accounts.insert(PROGRAM.into(), account(LOADER, program, true));
        let mut pd = vec![0; 45];
        pd[..4].copy_from_slice(&3u32.to_le_bytes());
        put64(&mut pd, 4, 50);
        pd.extend_from_slice(code);
        accounts.insert(PD.into(), account(LOADER, pd, false));
        accounts.insert(
            TOKEN_PROGRAM.into(),
            account(LOADER2, token_code.to_vec(), true),
        );
        let mut clock = vec![0; 40];
        put64(&mut clock, 0, 100);
        put64(&mut clock, 16, 7);
        put64(&mut clock, 32, 1000);
        accounts.insert(
            CLOCK.into(),
            account("Sysvar1111111111111111111111111111111111111", clock, false),
        );
        accounts.insert(oracle.clone(), account(SYSTEM, vec![], false));
        for (start, address) in starts.into_iter().zip(&arrays) {
            let mut a = vec![0; if dynamic { 148 } else { 9988 }];
            a[..8].copy_from_slice(&disc(if dynamic {
                "DynamicTickArray"
            } else {
                "TickArray"
            }));
            a[8..12].copy_from_slice(&start.to_le_bytes());
            put_key(&mut a, if dynamic { 12 } else { 9956 }, &pool);
            accounts.insert(address.clone(), account(PROGRAM, a, false));
        }
        Self {
            chain,
            pool: config,
            accounts,
            oracle,
            arrays,
        }
    }
    pub fn decode(&self) -> xone_domain::Result<xone_domain::Pool> {
        orca::decode_orca_whirlpool(&self.chain, &self.pool, &self.accounts, 100)
    }
}
