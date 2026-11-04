//! Independent EVM execution over an explicit, strict Shanghai state image.
pub mod db;
pub mod error;
pub mod execute;
pub mod image;
pub mod limits;
pub use db::StrictDB;
pub use error::{DbError, ImageError};
pub(crate) use execute::observe;
pub use execute::{execute, ExecutionError, ExecutionFailure, ExecutionOutput, TransactionRequest};
pub use image::{ImageData, StateImage};
pub use limits::{BoundReadLimits, ExecutionControl, ExecutionLimits, ReadUsage, Telemetry};
