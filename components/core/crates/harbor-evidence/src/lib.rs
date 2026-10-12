//! Shared artifact identities and acceptance receipts. Callers own policy.

mod artifact;
pub mod cli;
pub mod execution;
mod receipt;

pub use artifact::{Artifact, FileIdentity, Manifest, accept, snapshot, validate_report, verify_http, verify_local};
pub use receipt::{read_json, write_json_atomic};
