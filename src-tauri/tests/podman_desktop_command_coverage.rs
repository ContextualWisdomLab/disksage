use disksage_lib::podman_desktop::{inspect_podman_reclaim, PODMAN_DESKTOP_SCHEMA_KIND};

/// Exercise the production desktop command boundary with the host's read-only Podman probe.
///
/// The assertions intentionally cover only invariants that hold whether Podman is absent,
/// installed without a machine, or connected to a running machine. This keeps the regression
/// deterministic while proving that the actual command wrapper executes instead of relying only
/// on source-text contracts or the lower-level projection helper.
#[test]
fn desktop_command_executes_the_read_only_probe_boundary() {
    let evidence = inspect_podman_reclaim();

    assert_eq!(evidence.schema_kind, PODMAN_DESKTOP_SCHEMA_KIND);
    assert_eq!(evidence.schema_version, 1);
    assert_eq!(evidence.physically_reclaimable_bytes, None);
    assert_eq!(evidence.assessment_status, "unverified");
    assert!(evidence.notices.iter().any(|notice| {
        notice.contains("no prune, remove, machine lifecycle, TRIM, or raw-image mutation")
    }));
}
