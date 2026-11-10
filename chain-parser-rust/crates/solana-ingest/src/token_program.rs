//! Explicit loader/version binding for the classic SPL Token dependency.
use crate::{
    raydium::{digest, get, key_at, pda, pubkey, tag_at, u64_at, RpcAccount},
    rpc::TOKEN_PROGRAM,
};
use std::collections::BTreeMap;
use xone_config::TokenProgramUpgradeConfig;
use xone_domain::{err, Result};
const UPGRADEABLE: &str = "BPFLoaderUpgradeab1e11111111111111111111111";
const IMMUTABLE: &str = "BPFLoader2111111111111111111111111111111111";

pub(crate) fn append_dependency(
    ids: &mut Vec<String>,
    upgrade: Option<&TokenProgramUpgradeConfig>,
) -> Result<()> {
    if let Some(upgrade) = upgrade {
        let expected = pda(&[&pubkey(TOKEN_PROGRAM)?], &pubkey(UPGRADEABLE)?)?.0;
        if upgrade.deployed_slot == 0 || upgrade.programdata != expected {
            return Err(err("TOKEN_PROGRAM_VERSION_UNSUPPORTED"));
        }
        ids.push(upgrade.programdata.clone());
    }
    Ok(())
}

pub(crate) fn verify(
    accounts: &BTreeMap<String, RpcAccount>,
    expected_code: &str,
    upgrade: Option<&TokenProgramUpgradeConfig>,
    bank_slot: u64,
) -> Result<()> {
    let fail = || err("TOKEN_PROGRAM_VERSION_UNSUPPORTED");
    let program = get(accounts, TOKEN_PROGRAM)?;
    if !program.executable {
        return Err(fail());
    }
    let Some(upgrade) = upgrade else {
        if program.owner != IMMUTABLE
            || program.data.is_empty()
            || digest(&program.data) != expected_code
        {
            return Err(fail());
        }
        return Ok(());
    };
    append_dependency(&mut Vec::new(), Some(upgrade))?;
    if program.owner != UPGRADEABLE
        || program.data.len() != 36
        || tag_at(&program.data, 0) != 2
        || key_at(&program.data, 4) != upgrade.programdata
    {
        return Err(fail());
    }
    let data = get(accounts, &upgrade.programdata)?;
    if data.owner != UPGRADEABLE
        || data.executable
        || data.data.len() <= 45
        || tag_at(&data.data, 0) != 3
        || data.data[12] > 1
        || u64_at(&data.data, 4) != upgrade.deployed_slot
        || upgrade.deployed_slot > bank_slot
        || digest(&data.data[45..]) != expected_code
        || !data.data[45..].starts_with(b"\x7fELF")
    {
        return Err(fail());
    }
    Ok(())
}
