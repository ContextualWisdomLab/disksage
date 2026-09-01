//! Platform-truthful cache-measurement contract.
//!
//! The macOS implementation uses bounded parallel directory measurement. Windows and Linux use
//! the sequential handle-bound implementation, so they must not claim the stronger parallel
//! ontology evidence.

pub use crate::rules_impl::{
    cache_targets, clean_targets, is_catalog_path, BaseDirs, CacheCandidate, CacheTarget,
};

/// Return cache candidates with the measurement ontology class matching the implementation that
/// actually ran on this platform.
pub fn cache_candidates(bases: &BaseDirs) -> Vec<CacheCandidate> {
    let mut candidates = crate::rules_impl::cache_candidates(bases);
    let measurement_class = if cfg!(target_os = "macos") {
        "https://disksage.app/ontology#ParallelDirectoryMeasurement"
    } else {
        "https://disksage.app/ontology#StorageMeasurement"
    };
    for candidate in &mut candidates {
        candidate.measurement_ontology_class = measurement_class.into();
    }
    candidates
}
