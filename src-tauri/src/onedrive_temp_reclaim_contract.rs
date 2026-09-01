//! Public OneDrive temporary-download reclaim contract.
//!
//! Read-only planning remains available, but destructive execution is withheld until DiskSage can
//! bind the final filesystem identity observation and removal to one atomic operation. The current
//! implementation revalidates a pathname and then deletes that pathname separately, leaving a
//! replacement race in which newly written customer data could inherit an older approval.

use std::path::Path;

pub use crate::onedrive_temp_reclaim_impl::{
    plan, OneDriveTempCandidate, OneDriveTempExecution, OneDriveTempPlan,
};

const ATOMIC_REMOVAL_UNAVAILABLE: &str = "onedrive-temp-atomic-removal-unavailable";

/// Refuse irreversible temporary-file deletion until identity validation and removal are atomic.
///
/// OneDrive's remote copy is retained, but that does not make an unrelated replacement object at
/// the same local pathname safe to delete. Returning before planning or provider observation keeps
/// the shipped mutation boundary fail-closed while preserving the read-only planning surface.
pub fn execute(
    _home: &Path,
    _expected_fingerprint: &str,
    _approval: &str,
    _executed_at_ms: u64,
) -> Result<OneDriveTempExecution, String> {
    Err(ATOMIC_REMOVAL_UNAVAILABLE.into())
}
