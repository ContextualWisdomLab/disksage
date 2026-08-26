use disksage_lib::container_orphan_reclaim::{
    probe_container_orphans, ContainerRuntimeKind, ContainerRuntimeTarget, OrphanCategory,
};
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(unix)]
use std::os::unix::ffi::OsStringExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

#[cfg(unix)]
fn fake_runtime(script_body: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let runtime = temp.path().join("docker");
    std::fs::write(&runtime, format!("#!/bin/sh\nset -eu\n{script_body}\n"))
        .expect("write fake runtime");
    let mut permissions = std::fs::metadata(&runtime)
        .expect("fake runtime metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&runtime, permissions).expect("make fake runtime executable");
    (temp, runtime)
}

#[cfg(unix)]
fn docker_target(runtime: &Path) -> ContainerRuntimeTarget {
    ContainerRuntimeTarget::new(
        ContainerRuntimeKind::DockerNative,
        runtime.to_path_buf(),
        None,
    )
    .expect("valid Docker target")
}

#[cfg(unix)]
#[test]
fn healthy_empty_docker_lists_are_complete_and_binary_is_not_repeated() {
    let (_temp, runtime) = fake_runtime(
        r#"
case "${1:-}" in
  info) exit 0 ;;
  container)
    [ "${2:-}" = "ps" ] || exit 91
    exit 0
    ;;
  images) exit 0 ;;
  volume)
    [ "${2:-}" = "ls" ] || exit 92
    exit 0
    ;;
  network)
    [ "${2:-}" = "ls" ] || exit 93
    exit 0
    ;;
  *)
    echo "unexpected command" >&2
    exit 94
    ;;
esac
"#,
    );

    let plan = probe_container_orphans(&docker_target(&runtime));
    assert!(plan.runtime.healthy, "runtime info must receive info as argv[1]");
    assert!(plan.evidence_complete, "zero-record Docker listings are complete evidence");
    assert_eq!(plan.categories.len(), 4);
    for category in &plan.categories {
        assert!(category.evidence_complete, "{:?}: {:?}", category.category, category.issue);
        let evidence = category.evidence.as_ref().expect("complete category evidence");
        assert_eq!(evidence.total_records, 0);
        assert_eq!(evidence.candidate_records, 0);
        assert!(category.approval_phrase.is_none());
    }
}

#[cfg(unix)]
#[test]
fn option_shaped_network_name_is_rejected_before_network_inspect() {
    let (_temp, runtime) = fake_runtime(
        r#"
case "${1:-}" in
  info|container|images|volume) exit 0 ;;
  network)
    if [ "${2:-}" = "ls" ]; then
      printf '%s\n' '[{"Driver":"bridge","ID":"net-1","Name":"-danger"}]'
      exit 0
    fi
    echo "network inspect must not receive an option-shaped runtime name" >&2
    exit 95
    ;;
  *) exit 96 ;;
esac
"#,
    );

    let plan = probe_container_orphans(&docker_target(&runtime));
    let network = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Network)
        .expect("network category");
    assert!(!network.evidence_complete);
    assert_eq!(network.issue.as_deref(), Some("network-invalid-name"));
}

#[cfg(unix)]
#[test]
fn non_utf8_cli_argument_prints_real_usage_not_a_literal_placeholder() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let opaque = std::ffi::OsString::from_vec(vec![b'-', b'-', b'o', b'p', b'a', b'q', b'u', b'e', 0xff]);
    let output = Command::new(binary)
        .arg(opaque)
        .output()
        .expect("run shipped container orphan plan CLI");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 stderr");
    assert!(stderr.contains("Usage: disksage-container-orphan-plan"));
    assert!(!stderr.contains("{USAGE}"));
    assert!(!stderr.contains("opaque"));
}
