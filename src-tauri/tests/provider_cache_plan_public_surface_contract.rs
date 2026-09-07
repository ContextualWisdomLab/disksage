use disksage_lib::provider_cache::ProviderCacheReclaimPlan;
use std::fs;
use std::path::PathBuf;

fn source(path: &str) -> String {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    fs::read_to_string(manifest_dir.join(path)).expect("provider-cache public surface must remain readable")
}

#[test]
fn commercial_rust_plan_does_not_name_irreversible_approval_authority() {
    let public_api = source("src/provider_cache.rs");

    let reexport_start = public_api
        .find("pub use crate::provider_cache_reclaim::{")
        .expect("provider-cache facade must explicitly choose internal DTOs it re-exports");
    let reexport_tail = &public_api[reexport_start..];
    let reexport_end = reexport_tail
        .find("};")
        .expect("provider-cache facade re-export block must terminate");
    let reexport_block = &reexport_tail[..reexport_end];
    assert!(
        !reexport_block.contains("ProviderCacheReclaimPlan"),
        "commercial Rust callers must not receive the historical plan type that names permanent approval"
    );

    let public_plan_start = public_api
        .find("pub struct ProviderCacheReclaimPlan")
        .expect("provider-cache facade must own the commercial plan DTO");
    let public_plan_tail = &public_api[public_plan_start..];
    let public_plan_end = public_plan_tail
        .find("}\n")
        .expect("commercial provider-cache plan must terminate");
    let public_plan = &public_plan_tail[..public_plan_end];
    assert!(
        public_plan.contains("trash_approval_phrase")
            && !public_plan.contains("exact_approval_phrase"),
        "commercial Rust plan must expose reversible Trash approval only"
    );

    assert!(
        public_api.contains("fn project_plan("),
        "the facade must explicitly project the historical plan into the commercial plan DTO"
    );
}

#[test]
fn commercial_plan_wire_schema_contains_only_reversible_approval() {
    let plan = ProviderCacheReclaimPlan {
        schema_version: 1,
        platform: "macos".into(),
        observed_at_ms: 1,
        installed_edge_version: None,
        podman_machine_present: false,
        podman_recreation_source: None,
        evidence_complete: false,
        candidates: Vec::new(),
        issues: vec!["provider-cache-evidence-incomplete".into()],
        plan_fingerprint: "a".repeat(64),
        trash_approval_phrase: Some("DiskSage Trash approval".into()),
    };
    let value = serde_json::to_value(plan).expect("commercial plan must remain serializable");
    let object = value
        .as_object()
        .expect("commercial provider-cache plan must serialize as an object");

    assert!(object.contains_key("trash_approval_phrase"));
    assert!(
        !object.contains_key("exact_approval_phrase"),
        "unsupported irreversible approval must be absent from the serialized public plan"
    );
}
