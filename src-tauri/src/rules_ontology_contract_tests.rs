#![cfg(not(target_os = "macos"))]

use crate::rules::{cache_candidates, BaseDirs};

#[test]
fn sequential_cache_measurement_does_not_claim_parallel_ontology() {
    let root = tempfile::tempdir().expect("temporary cache root");
    let bases = BaseDirs {
        temp: root.path().join("tmp"),
        local_data: root.path().join("local"),
        home: root.path().join("home"),
    };

    let candidates = cache_candidates(&bases);
    assert!(!candidates.is_empty());
    assert!(candidates.iter().all(|candidate| {
        candidate.measurement_ontology_class
            == "https://disksage.app/ontology#StorageMeasurement"
    }));
}
