use std::collections::{BTreeMap, BTreeSet};
use xone_domain::{err, Result};
#[derive(Clone, Debug)]
pub struct Update {
    pub provider: String,
    pub session: String,
    pub slot: u64,
    pub bank: String,
    pub pubkey: String,
    pub write_version: u64,
    pub data: Vec<u8>,
}
/// A provider write_version is comparable only within a declared provider/session.
/// Receiving all subscribed accounts is distinct from an explicit account-completion barrier.
pub struct Assembler {
    max_updates: usize,
    slot: u64,
    bank: String,
    session: Option<(String, String)>,
    required: BTreeSet<String>,
    updates: BTreeMap<String, Update>,
    complete: bool,
    uncertain: bool,
}
impl Assembler {
    pub fn new(max_updates: usize) -> Self {
        Self {
            max_updates,
            slot: 0,
            bank: String::new(),
            session: None,
            required: BTreeSet::new(),
            updates: BTreeMap::new(),
            complete: false,
            uncertain: true,
        }
    }
    pub fn begin(&mut self, slot: u64, bank: &str, required: Vec<String>) -> Result<()> {
        if required.is_empty() || required.len() > self.max_updates {
            return Err(err("ACCOUNT_SCOPE_LIMIT"));
        }
        self.slot = slot;
        self.bank = bank.into();
        self.required = required.into_iter().collect();
        self.updates.clear();
        self.session = None;
        self.complete = false;
        self.uncertain = false;
        Ok(())
    }
    pub fn update(&mut self, u: Update) -> Result<bool> {
        if self.uncertain {
            return Err(err("REANCHOR_REQUIRED"));
        }
        if u.slot != self.slot || u.bank != self.bank {
            return Err(err("ACCOUNT_BANK_MISMATCH"));
        }
        let session = (u.provider.clone(), u.session.clone());
        if self.session.as_ref().is_some_and(|s| *s != session) {
            self.disconnect();
            return Err(err("SOURCE_SESSION_CHANGED"));
        }
        if !self.required.contains(&u.pubkey) {
            return Err(err("UNDECLARED_ACCOUNT_DEPENDENCY"));
        }
        if let Some(old) = self.updates.get(&u.pubkey) {
            if u.write_version < old.write_version {
                return Err(err("STALE_ACCOUNT_UPDATE"));
            }
            if u.write_version == old.write_version {
                return if u.data == old.data {
                    Ok(false)
                } else {
                    Err(err("WRITE_VERSION_CONFLICT"))
                };
            }
        }
        self.session = Some(session);
        self.updates.insert(u.pubkey.clone(), u);
        self.complete = false;
        Ok(true)
    }
    pub fn complete_accounts(&mut self, slot: u64, bank: &str) -> Result<()> {
        if self.uncertain
            || slot != self.slot
            || bank != self.bank
            || self.updates.len() != self.required.len()
        {
            return Err(err("ACCOUNT_SCOPE_INCOMPLETE"));
        }
        self.complete = true;
        Ok(())
    }
    pub fn disconnect(&mut self) {
        self.uncertain = true;
        self.complete = false;
    }
    pub fn ready(&self) -> bool {
        !self.uncertain && self.complete && self.updates.len() == self.required.len()
    }
}
