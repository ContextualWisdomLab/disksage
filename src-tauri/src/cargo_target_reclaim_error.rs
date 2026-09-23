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
    observed_target_view_reduction_bytes: Option<u64>,
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
            .map(|after| target_view_allocated_bytes_before.saturating_sub(after));
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

    /// Receipt schema version consumed by durable recovery and presentation boundaries.
    pub fn schema_version(&self) -> u32 {
        self.schema_version
    }

    /// Stable machine-readable failure code for irreversible partial cleanup.
    pub fn code(&self) -> &str {
        self.code
    }

    /// Completion state. Version 1 uses `partial` for this receipt type.
    pub fn completion(&self) -> &str {
        self.completion
    }

    /// Number of descendant entries irreversibly removed before the failure.
    pub fn entries_removed(&self) -> u64 {
        self.entries_removed
    }

    /// Allocated bytes visible beneath the retained target capability before cleanup.
    pub fn target_view_allocated_bytes_before(&self) -> u64 {
        self.target_view_allocated_bytes_before
    }

    /// Best-effort allocated bytes still visible after failure, when measurement succeeded.
    pub fn target_view_allocated_bytes_after(&self) -> Option<u64> {
        self.target_view_allocated_bytes_after
    }

    /// Observed target-view reduction, or `None` when post-failure measurement was unavailable.
    pub fn observed_target_view_reduction_bytes(&self) -> Option<u64> {
        self.observed_target_view_reduction_bytes
    }

    /// Buyer reclaim credit; zero until a separate physical block-release contract exists.
    pub fn ledger_reclaim_bytes(&self) -> u64 {
        self.ledger_reclaim_bytes
    }

    /// Causal filesystem failure recorded after irreversible mutation began.
    pub fn cause(&self) -> &str {
        &self.cause
    }
}

/// Owner-level cleanup failure. Irreversible partial mutation remains typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoTargetReclaimError {
    /// Failure for which no versioned irreversible-partial receipt is required.
    Message(String),
    /// Irreversible partial cleanup carrying the canonical recovery receipt.
    PartialCleanup(CargoTargetPartialCleanupReceipt),
}

impl CargoTargetReclaimError {
    /// Compatibility predicate for existing callers that inspect stable textual reason codes.
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
            crate::unix_capability_cleanup::CleanupFailure::NoMutation { cause } => {
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn partial_receipt_serialization_preserves_unknown_post_failure_measurement() {
        let receipt = CargoTargetPartialCleanupReceipt::from_partial_cleanup(
            1,
            "cargo-target-capability-unlinkat-failed:permission".into(),
            4096,
            None,
        );
        let json = serde_json::to_value(&receipt).expect("serialize partial receipt");
        assert_eq!(json["schema_version"], 1);
        assert_eq!(json["completion"], "partial");
        assert_eq!(json["target_view_allocated_bytes_before"], 4096);
        assert!(json["target_view_allocated_bytes_after"].is_null());
        assert!(json["observed_target_view_reduction_bytes"].is_null());
        assert_eq!(json["ledger_reclaim_bytes"], 0);
    }
}
