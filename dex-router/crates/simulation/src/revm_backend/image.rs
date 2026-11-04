use super::ImageError;
use revm::primitives::{AccountInfo, Address, Bytes, B256, U256};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ImageData {
    pub accounts: BTreeMap<Address, Option<AccountInfo>>,
    pub bytecodes: BTreeMap<B256, Bytes>,
    pub storage: BTreeMap<(Address, U256), U256>,
    pub block_hashes: BTreeMap<u64, B256>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StateImage {
    pub(crate) data: ImageData,
}
impl StateImage {
    pub fn new(data: ImageData) -> Result<Self, ImageError> {
        for (expected, bytes) in &data.bytecodes {
            let actual = revm::primitives::keccak256(bytes);
            if *expected != actual {
                return Err(ImageError::CodeHashMismatch {
                    expected: *expected,
                    actual,
                });
            }
        }
        for (address, info) in &data.accounts {
            if let Some(info) = info {
                if info.code.is_some() {
                    return Err(ImageError::InlineCode { address: *address });
                }
                if info.code_hash == B256::ZERO {
                    return Err(ImageError::NonCanonicalCodeHash { address: *address });
                }
            }
        }
        for (address, _) in data.storage.keys() {
            if !matches!(data.accounts.get(address), Some(Some(_))) {
                return Err(ImageError::StorageWithoutAccount { address: *address });
            }
        }
        Ok(Self { data })
    }
    /// Copy this image and apply a REVM delta from execution against this image.
    /// Only touched accounts commit. Shanghai selfdestruct and EIP-161 clearing remove accounts.
    /// Undeclared storage stays unknown, including slots not enumerated for newly created accounts.
    pub fn apply(&self, state: &revm::primitives::EvmState) -> Result<Self, ImageError> {
        let mut data = self.data.clone();
        for (address, account) in state {
            if !account.is_touched() {
                continue;
            }
            if account.is_selfdestructed() || account.info.is_empty() {
                data.accounts.insert(*address, None);
                data.storage.retain(|(owner, _), _| owner != address);
                continue;
            }
            if account.is_created() {
                data.storage.retain(|(owner, _), _| owner != address);
            }
            let mut info = account.info.clone();
            if let Some(code) = info.code.take() {
                let bytes = code.original_bytes();
                let actual = revm::primitives::keccak256(&bytes);
                if info.code_hash != actual {
                    return Err(ImageError::CodeHashMismatch {
                        expected: info.code_hash,
                        actual,
                    });
                }
                data.bytecodes.insert(actual, bytes);
            }
            data.accounts.insert(*address, Some(info));
            for (slot, value) in &account.storage {
                data.storage
                    .insert((*address, *slot), value.present_value());
            }
        }
        Self::new(data)
    }
    pub fn data(&self) -> &ImageData {
        &self.data
    }
}
