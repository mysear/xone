use revm::primitives::{Address, B256, U256};
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageError {
    CodeHashMismatch { expected: B256, actual: B256 },
    InlineCode { address: Address },
    NonCanonicalCodeHash { address: Address },
    StorageWithoutAccount { address: Address },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DbError {
    UnknownAccount(Address),
    UnknownCode(B256),
    UnknownStorage { address: Address, slot: U256 },
    UnknownBlockHash(u64),
    ReadLimit,
    ByteLimit,
    CounterOverflow,
    Cancelled,
    Deadline,
    InstructionLimit,
    MemoryLimit,
}
impl DbError {
    pub fn reason(&self) -> &'static str {
        match self {
            Self::UnknownAccount(_) => "unknown_account",
            Self::UnknownCode(_) => "unknown_code",
            Self::UnknownStorage { .. } => "unknown_storage",
            Self::UnknownBlockHash(_) => "unknown_block_hash",
            Self::ReadLimit => "read_limit",
            Self::ByteLimit => "byte_limit",
            Self::CounterOverflow => "counter_overflow",
            Self::Cancelled => "cancelled",
            Self::Deadline => "deadline",
            Self::InstructionLimit => "instruction_limit",
            Self::MemoryLimit => "memory_limit",
        }
    }
}
impl std::fmt::Display for DbError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {:?}", self.reason(), self)
    }
}
impl std::error::Error for DbError {}
impl std::fmt::Display for ImageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid_image: {self:?}")
    }
}
impl std::error::Error for ImageError {}
