use super::{BoundReadLimits, DbError, ExecutionControl, ReadUsage, StateImage};
use revm::{
    primitives::{AccountInfo, Address, Bytecode, B256, U256},
    Database,
};
use std::{
    sync::{atomic::Ordering, Arc},
    time::Instant,
};

/// Strict, immutable image access. Does not implement DatabaseCommit.
/// All attempts admitted before a sticky failure count, including misses and the limit-crossing attempt.
pub struct StrictDB {
    image: Arc<StateImage>,
    limits: BoundReadLimits,
    control: ExecutionControl,
    usage: ReadUsage,
    failure: Option<DbError>,
}
#[derive(Clone, Copy)]
enum ReadKind {
    Basic,
    Code,
    Storage,
    BlockHash,
}
impl StrictDB {
    pub fn new(image: Arc<StateImage>, limits: BoundReadLimits, control: ExecutionControl) -> Self {
        Self {
            image,
            limits,
            control,
            usage: ReadUsage::default(),
            failure: None,
        }
    }
    pub fn usage(&self) -> ReadUsage {
        self.usage
    }
    pub(crate) fn fail(&mut self, error: DbError) -> DbError {
        self.failure.get_or_insert(error).clone()
    }
    pub(crate) fn check_control(&mut self) -> Result<(), DbError> {
        if let Some(error) = &self.failure {
            return Err(error.clone());
        }
        if self.control.cancellation.load(Ordering::Acquire) {
            return Err(self.fail(DbError::Cancelled));
        }
        if self
            .control
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(self.fail(DbError::Deadline));
        }
        Ok(())
    }
    fn charge(&mut self, kind: ReadKind, bytes: u64) -> Result<(), DbError> {
        self.check_control()?;
        let next_reads = self
            .usage
            .reads
            .checked_add(1)
            .ok_or_else(|| self.fail(DbError::CounterOverflow))?;
        let next_bytes = self
            .usage
            .bytes
            .checked_add(bytes)
            .ok_or_else(|| self.fail(DbError::CounterOverflow))?;
        self.usage.reads = next_reads;
        self.usage.bytes = next_bytes;
        // Per-kind counters cannot exceed the checked aggregate.
        match kind {
            ReadKind::Basic => self.usage.basic += 1,
            ReadKind::Code => self.usage.code += 1,
            ReadKind::Storage => self.usage.storage += 1,
            ReadKind::BlockHash => self.usage.block_hash += 1,
        }
        if next_reads > self.limits.max_reads {
            return Err(self.fail(DbError::ReadLimit));
        }
        if next_bytes > self.limits.max_bytes {
            return Err(self.fail(DbError::ByteLimit));
        }
        Ok(())
    }
}
impl Database for StrictDB {
    type Error = DbError;
    fn basic(&mut self, address: Address) -> Result<Option<AccountInfo>, DbError> {
        let bytes = match self.image.data.accounts.get(&address) {
            Some(Some(_)) => 93,
            Some(None) => 21,
            None => 20,
        };
        self.charge(ReadKind::Basic, bytes)?;
        self.image
            .data
            .accounts
            .get(&address)
            .cloned()
            .ok_or(DbError::UnknownAccount(address))
    }
    fn code_by_hash(&mut self, hash: B256) -> Result<Bytecode, DbError> {
        let len = self
            .image
            .data
            .bytecodes
            .get(&hash)
            .map_or(0, |bytes| bytes.len());
        let bytes = u64::try_from(len)
            .ok()
            .and_then(|len| len.checked_add(32))
            .ok_or_else(|| self.fail(DbError::CounterOverflow))?;
        self.charge(ReadKind::Code, bytes)?;
        self.image
            .data
            .bytecodes
            .get(&hash)
            .cloned()
            .map(Bytecode::new_legacy)
            .ok_or(DbError::UnknownCode(hash))
    }
    fn storage(&mut self, address: Address, slot: U256) -> Result<U256, DbError> {
        let exists = self.image.data.storage.contains_key(&(address, slot));
        self.charge(ReadKind::Storage, if exists { 84 } else { 52 })?;
        self.image
            .data
            .storage
            .get(&(address, slot))
            .copied()
            .ok_or(DbError::UnknownStorage { address, slot })
    }
    fn block_hash(&mut self, number: u64) -> Result<B256, DbError> {
        let exists = self.image.data.block_hashes.contains_key(&number);
        self.charge(ReadKind::BlockHash, if exists { 40 } else { 8 })?;
        self.image
            .data
            .block_hashes
            .get(&number)
            .copied()
            .ok_or(DbError::UnknownBlockHash(number))
    }
}
