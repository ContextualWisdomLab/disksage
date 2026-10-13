#![cfg(unix)]

use disksage_lib::podman_reclaim::{probe_podman_reclaim, DEFAULT_PODMAN_MACHINE};
use serde_json::json;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

#[test]
fn stopped_machine_is_reported_and_skips_live_api_probes() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path();
    let raw_image_path = temp.path().join("podman-machine.raw");
    fs::write(&raw_image_path, b"read-only-observation-fixture").unwrap();
    fs::write(
        config_dir.join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({
            "ImagePath": {
                "Path": raw_image_path.to_string_lossy()
            }
        })
        .to_string(),
    )
    .unwrap();

    let inspect = json!([{
        "ConfigDir": { "Path": config_dir.to_string_lossy() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "stopped",
        "Resources": { "DiskSize": 100 }
    }])
    .to_string();
    let fake_podman = temp.path().join("podman");
    fs::write(
        &fake_podman,
        format!(
            "#!/bin/sh\n\
             SCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
             printf '%s\\n' \"$*\" >> \"$SCRIPT_DIR/invocations.log\"\n\
             if [ \"$*\" = \"machine inspect podman-machine-default\" ]; then\n\
               cat <<'DISKSAGE_INSPECT_JSON'\n\
             {inspect}\n\
             DISKSAGE_INSPECT_JSON\n\
               exit 0\n\
             fi\n\
             printf '%s\\n' 'unexpected live API probe for stopped machine' >&2\n\
             exit 91\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_podman, fs::Permissions::from_mode(0o700)).unwrap();

    let plan = probe_podman_reclaim(
        Path::new(&fake_podman),
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );

    assert!(!plan.evidence_complete);
    assert_eq!(plan.machine.as_ref().map(|machine| machine.state.as_str()), Some("stopped"));
    assert!(plan.raw_image.is_some());
    assert!(plan.guest_filesystem.is_none());
    assert!(plan.store.is_none());
    assert!(plan.system_df.is_none());
    assert!(plan.unused_images.is_none());
    assert_eq!(plan.issues, vec!["podman-machine-not-running"]);
    assert!(plan
        .assessment
        .reason_codes
        .iter()
        .any(|code| code == "partial-evidence"));

    let invocations = fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    assert_eq!(
        invocations.lines().collect::<Vec<_>>(),
        vec!["machine inspect podman-machine-default"]
    );
}

#[test]
fn running_state_is_case_insensitive_and_admits_read_only_live_probes() {
    let temp = tempfile::tempdir().unwrap();
    let config_dir = temp.path();
    let raw_image_path = temp.path().join("podman-machine.raw");
    fs::write(&raw_image_path, b"read-only-observation-fixture").unwrap();
    fs::write(
        config_dir.join(format!("{DEFAULT_PODMAN_MACHINE}.json")),
        json!({
            "ImagePath": {
                "Path": raw_image_path.to_string_lossy()
            }
        })
        .to_string(),
    )
    .unwrap();

    let inspect = json!([{
        "ConfigDir": { "Path": config_dir.to_string_lossy() },
        "Name": DEFAULT_PODMAN_MACHINE,
        "State": "RUNNING",
        "Resources": { "DiskSize": 100 }
    }])
    .to_string();
    let fake_podman = temp.path().join("podman");
    fs::write(
        &fake_podman,
        format!(
            "#!/bin/sh\n\
             SCRIPT_DIR=$(CDPATH= cd -- \"$(dirname -- \"$0\")\" && pwd)\n\
             printf '%s\\n' \"$*\" >> \"$SCRIPT_DIR/invocations.log\"\n\
             if [ \"$*\" = \"machine inspect podman-machine-default\" ]; then\n\
               cat <<'DISKSAGE_INSPECT_JSON'\n\
             {inspect}\n\
             DISKSAGE_INSPECT_JSON\n\
               exit 0\n\
             fi\n\
             printf '%s\\n' 'fixture-live-probe-failure' >&2\n\
             exit 91\n"
        ),
    )
    .unwrap();
    fs::set_permissions(&fake_podman, fs::Permissions::from_mode(0o700)).unwrap();

    let plan = probe_podman_reclaim(
        Path::new(&fake_podman),
        DEFAULT_PODMAN_MACHINE,
        Duration::from_secs(2),
    );

    assert!(!plan.evidence_complete);
    assert_eq!(plan.machine.as_ref().map(|machine| machine.state.as_str()), Some("RUNNING"));
    assert!(plan.raw_image.is_some());
    assert!(plan.guest_filesystem.is_none());
    assert!(plan.store.is_none());
    assert!(plan.system_df.is_none());
    assert!(plan.unused_images.is_none());
    assert!(
        !plan
            .issues
            .iter()
            .any(|issue| issue == "podman-machine-not-running"),
        "case-insensitive running state must not be classified as stopped"
    );

    let invocations = fs::read_to_string(temp.path().join("invocations.log")).unwrap();
    let invocations = invocations.lines().collect::<Vec<_>>();
    assert_eq!(invocations[0], "machine inspect podman-machine-default");
    assert!(
        invocations
            .iter()
            .any(|invocation| invocation.starts_with("machine ssh podman-machine-default -- df ")),
        "running-state evidence must attempt the guest filesystem probe"
    );
    assert!(
        invocations
            .iter()
            .any(|invocation| invocation.starts_with("--connection podman-machine-default info ")),
        "running-state evidence must attempt the Podman store probe"
    );
    assert!(
        invocations.iter().any(|invocation| {
            invocation.starts_with("--connection podman-machine-default system df ")
        }),
        "running-state evidence must attempt the system-df probe"
    );
    assert!(
        invocations
            .iter()
            .any(|invocation| invocation.starts_with("--connection podman-machine-default images ")),
        "running-state evidence must attempt the unused-image probe"
    );
}
