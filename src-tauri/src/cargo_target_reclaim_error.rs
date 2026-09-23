//! Typed owner failures for Cargo target cleanup.
//!
//! Partial cleanup is irreversible evidence, not a human-readable string. The owner
//! keeps the structured receipt typed until the CLI presentation boundary serializes it.

use std::fmt;

/// Versioned evidence emitted when cleanup mutated the reviewed target and then failed.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct CargoTargetPartialCleanupReceipt {
    schema_version: u32,
    code: &'static str,
    completion: &'static str,
    entries_removed: u64,
    target_view_allocated_bytes_before: u64,
    target_view_allocated_bytes_after: Option<u64>,
    observed_target_view_reduction_bytes: u64,
    ledger_reclaim_bytes: u64,
    cause: String,
}

impl CargoTargetPartialCleanupReceipt {
    const SCHEMA_VERSION: u32 = 1;
    const CODE: &'static str = "cargo-target-partial-clean-failed";
    const COMPLETION: &'static str = "partial";

    #[cfg(unix)]
    pub(crate) fn from_partial_cleanup(
        entries_removed: u64,
        cause: String,
        target_view_allocated_bytes_before: u64,
        target_view_allocated_bytes_after: Option<u64>,
    ) -> Self {
        let observed_target_view_reduction_bytes = target_view_allocated_bytes_after
            .map(|after| target_view_allocated_bytes_before.saturating_sub(after))
            .unwrap_or(0);
        Self {
            schema_version: Self::SCHEMA_VERSION,
            code: Self::CODE,
            completion: Self::COMPLETION,
            entries_removed,
            target_view_allocated_bytes_before,
            target_view_allocated_bytes_after,
            observed_target_view_reduction_bytes,
            // Target-view reduction does not prove allocator/filesystem block release.
            ledger_reclaim_bytes: 0,
            cause,
        }
    }

    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    pub fn code(&self) -> &str {
        self.code
    }

    pub fn completion(&self) -> &str {
        self.completion
    }

    pub fn entries_removed(&self) -> u64 {
        self.entries_removed
    }

    pub fn target_view_allocated_bytes_before(&self) -> u64 {
        self.target_view_allocated_bytes_before
    }

    pub fn target_view_allocated_bytes_after(&self) -> Option<u64> {
        self.target_view_allocated_bytes_after
    }

    pub fn observed_target_view_reduction_bytes(&self) -> u64 {
        self.observed_target_view_reduction_bytes
    }

    pub fn ledger_reclaim_bytes(&self) -> u64 {
        self.ledger_reclaim_bytes
    }

    pub fn cause(&self) -> &str {
        &self.cause
    }
}

/// Owner-level cleanup failure. Irreversible partial mutation remains typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoTargetReclaimError {
    Message(String),
    PartialCleanup(CargoTargetPartialCleanupReceipt),
}

impl CargoTargetReclaimError {
    pub fn contains(&self, needle: &str) -> bool {
        self.to_string().contains(needle)
    }
}

impl fmt::Display for CargoTargetReclaimError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Message(message) => formatter.write_str(message),
            Self::PartialCleanup(receipt) => write!(
                formatter,
                "{}:entries_removed={}:{}",
                receipt.code(),
                receipt.entries_removed(),
                receipt.cause()
            ),
        }
    }
}

impl std::error::Error for CargoTargetReclaimError {}

impl From<String> for CargoTargetReclaimError {
    fn from(message: String) -> Self {
        Self::Message(message)
    }
}

impl From<&str> for CargoTargetReclaimError {
    fn from(message: &str) -> Self {
        Self::Message(message.to_owned())
    }
}

impl PartialEq<&str> for CargoTargetReclaimError {
    fn eq(&self, other: &&str) -> bool {
        matches!(self, Self::Message(message) if message == other)
    }
}

#[cfg(unix)]
impl From<crate::unix_capability_cleanup::CleanupFailure> for CargoTargetReclaimError {
    fn from(failure: crate::unix_capability_cleanup::CleanupFailure) -> Self {
        match failure {
            crate::unix_capability_cleanup::CleanupFailure::NoMutation(cause) => {
                Self::Message(cause)
            }
            crate::unix_capability_cleanup::CleanupFailure::Partial {
                entries_removed,
                cause,
                target_view_allocated_bytes_before,
                target_view_allocated_bytes_after,
            } => Self::PartialCleanup(CargoTargetPartialCleanupReceipt::from_partial_cleanup(
                entries_removed,
                cause,
                target_view_allocated_bytes_before,
                target_view_allocated_bytes_after,
            )),
        }
    }
}
