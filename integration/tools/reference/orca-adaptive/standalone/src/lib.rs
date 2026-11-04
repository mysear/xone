//! Test-only boundary adapter for running the assigned test file while the main
//! domain crate is being edited concurrently. Uses the existing Q64 source file.
//! This is not the native oracle and is not linked into production.
#[path = "../../../../../../dex-router/crates/domain/src/orca_math.rs"]
pub mod orca_math;
pub type Result<T> = std::result::Result<T, Error>;
#[derive(Debug, PartialEq, Eq)]
pub struct Error(pub String);
pub fn err(code: &str) -> Error {
    Error(code.to_owned())
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::error::Error for Error {}
