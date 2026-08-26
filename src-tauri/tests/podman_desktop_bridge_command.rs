use disksage_lib::podman_desktop::PODMAN_DESKTOP_SCHEMA_KIND;
use disksage_lib::podman_desktop_bridge::inspect_podman_desktop_evidence;

/// Exercise the separately registered privacy-safe Podman command through its public Rust boundary.
///
/// The probe may report partial evidence when Podman is absent or unhealthy, but the bridge must
/// always preserve the schema and must never claim verified host physical reclaimability.
#[test]
fn privacy_safe_podman_bridge_executes_public_boundary() {
    let evidence = inspect_podman_desktop_evidence();

    assert_eq!(evidence.schema_kind, PODMAN_DESKTOP_SCHEMA_KIND);
    assert_eq!(evidence.schema_version, 1);
    assert_eq!(evidence.physically_reclaimable_bytes, None);
    assert_eq!(evidence.assessment_status, "unverified");
    assert_eq!(evidence.notices.len(), 2);
    assert!(evidence.notices.iter().any(|notice| {
        notice.contains("no prune, remove, machine lifecycle, TRIM, or raw-image mutation")
    }));
}
