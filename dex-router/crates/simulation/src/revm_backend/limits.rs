use std::{
    sync::{
        atomic::{AtomicBool, AtomicU64},
        Arc,
    },
    time::Instant,
};
#[derive(Clone, Copy, Debug)]
pub struct BoundReadLimits {
    pub max_reads: u64,
    pub max_bytes: u64,
}
#[derive(Clone, Debug)]
pub struct ExecutionControl {
    pub cancellation: Arc<AtomicBool>,
    pub deadline: Option<Instant>,
    pub progress: Option<Arc<AtomicU64>>,
}
impl Default for ExecutionControl {
    fn default() -> Self {
        Self {
            cancellation: Arc::new(AtomicBool::new(false)),
            deadline: None,
            progress: None,
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReadUsage {
    pub reads: u64,
    pub bytes: u64,
    pub basic: u64,
    pub code: u64,
    pub storage: u64,
    pub block_hash: u64,
}

/// All resource ceilings must be supplied by the caller; no implicit unbounded mode.
#[derive(Clone, Copy, Debug)]
pub struct ExecutionLimits {
    pub reads: BoundReadLimits,
    pub max_gas_limit: u64,
    pub max_instructions: u64,
    pub max_input_bytes: usize,
    pub max_memory_bytes: u64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Telemetry {
    pub reads: ReadUsage,
    pub instructions: u64,
    pub initializations: u64,
    pub calls: u64,
    pub creates: u64,
}
