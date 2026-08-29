use disksage_lib::container_orphan_reclaim::{
    execute_container_orphan_prune, probe_container_orphans, ContainerRuntimeKind,
    ContainerRuntimeTarget, OrphanCategory,
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
fn docker_audit_requests_full_ids_and_uses_dangling_image_evidence() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (_temp, runtime) = fake_runtime(&format!(
        r#"
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    [ "${{2:-}}" = "ps" ] || exit 91
    case " $* " in *" --no-trunc "*) ;; *) echo "missing container --no-trunc" >&2; exit 92 ;; esac
    case " $* " in
      *" --filter ancestor={FULL_ID} "*) exit 0 ;;
      *) printf '%s\n' '{{"ID":"{FULL_ID}","State":"running","Names":[]}}' ;;
    esac
    ;;
  images)
    case " $* " in *" --no-trunc "*) ;; *) echo "missing image --no-trunc" >&2; exit 93 ;; esac
    case " $* " in *" --filter dangling=true "*) ;; *) echo "missing dangling filter" >&2; exit 94 ;; esac
    printf '%s\n' '{{"Containers":"N/A","ID":"{FULL_ID}","Repository":"<none>","Size":"72.9MB","Tag":"<none>"}}'
    ;;
  image)
    [ "${{2:-}}" = "inspect" ] || exit 96
    case " $* " in *" --format {{{{json .}}}} "*) ;; *) echo "missing image inspect format" >&2; exit 97 ;; esac
    printf '%s\n' '{{"Id":"sha256:{FULL_ID}","Size":72900000}}'
    ;;
  volume|network) exit 0 ;;
  *) exit 98 ;;
esac
"#
    ));

    let plan = probe_container_orphans(&docker_target(&runtime));
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");
    assert!(container.evidence_complete, "{:?}", container.issue);
    assert_eq!(container.evidence.as_ref().unwrap().candidate_records, 0);

    let image = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Image)
        .expect("image category");
    assert!(image.evidence_complete, "{:?}", image.issue);
    let evidence = image.evidence.as_ref().expect("image evidence");
    assert_eq!(evidence.total_records, 1);
    assert_eq!(evidence.candidate_records, 1);
    assert_eq!(evidence.candidate_size_sum_bytes, Some(72_900_000));
    assert!(image.approval_phrase.is_some());
}

#[cfg(unix)]
#[test]
fn docker_image_size_identity_mismatch_blocks_only_image_category() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const OTHER_ID: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    let (_temp, runtime) = fake_runtime(&format!(
        r#"
case "${{1:-}}" in
  info|container|volume|network) exit 0 ;;
  images)
    printf '%s\n' '{{"Containers":"N/A","ID":"{FULL_ID}","Repository":"<none>","Size":"72.9MB","Tag":"<none>"}}'
    ;;
  image)
    [ "${{2:-}}" = "inspect" ] || exit 96
    printf '%s\n' '{{"Id":"sha256:{OTHER_ID}","Size":72900000}}'
    ;;
  *) exit 98 ;;
esac
"#
    ));
    let plan = probe_container_orphans(&docker_target(&runtime));
    let image = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Image)
        .expect("image category");
    assert!(!image.evidence_complete);
    assert_eq!(
        image.issue.as_deref(),
        Some("docker-image-size-identity-mismatch")
    );
}

#[cfg(unix)]
#[test]
fn approved_container_execution_targets_only_the_fingerprinted_candidate() {
    const FULL_ID: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let (_temp, runtime) = fake_runtime(&format!(
        r#"
case "${{1:-}}" in
  info) exit 0 ;;
  container)
    if [ "${{2:-}}" = "ps" ]; then
      printf '%s\n' '{{"ID":"{FULL_ID}","State":"exited","Names":[]}}'
      exit 0
    fi
    if [ "${{2:-}}" = "rm" ] && [ "${{3:-}}" = "{FULL_ID}" ] && [ "${{4:-}}" = "" ]; then
      printf '%s\n' '{FULL_ID}'
      exit 0
    fi
    if [ "${{2:-}}" = "prune" ]; then
      echo "category-wide prune is not candidate-bound" >&2
      exit 97
    fi
    exit 98
    ;;
  images|volume|network) exit 0 ;;
  *) exit 99 ;;
esac
"#
    ));
    let target = docker_target(&runtime);
    let plan = probe_container_orphans(&target);
    let container = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Container)
        .expect("container category");
    let phrase = container
        .approval_phrase
        .as_deref()
        .expect("candidate-bound approval phrase");

    let execution = execute_container_orphan_prune(
        &target,
        OrphanCategory::Container,
        phrase,
        "Remove the exact stopped-container candidate verified by DiskSage.",
        1,
    )
    .expect("exact candidate removal must succeed");

    assert!(execution.executed);
    assert_eq!(execution.status_code, 0);
    assert!(execution.stdout.contains(FULL_ID));
    assert!(!execution.command.iter().any(|part| part == "prune"));
    assert!(!execution.command.iter().any(|part| part == FULL_ID));
    assert_eq!(execution.command.last().map(String::as_str), Some("<candidate-set>"));
}

#[cfg(unix)]
#[test]
fn volume_execution_requires_explicit_ownership_and_preserves_compose_volumes() {
    let (_temp, runtime) = fake_runtime(
        r#"
case "${1:-}" in
  info|container|images|network) exit 0 ;;
  volume)
    case "${2:-}" in
      ls)
        printf '%s\n' '[{"Name":"owned-cache"},{"Name":"compose-data"}]'
        ;;
      inspect)
        if [ "${3:-}" = "owned-cache" ]; then
          printf '%s\n' '[{"Name":"owned-cache","Driver":"local","CreatedAt":"2026-08-30T00:00:00Z","Labels":{"io.contextualwisdomlab.disksage.owner":"disksage","io.contextualwisdomlab.disksage.reclaimable":"true"}}]'
        else
          printf '%s\n' '[{"Name":"compose-data","Driver":"local","CreatedAt":"2026-08-30T00:00:00Z","Labels":{"com.docker.compose.project":"customer-app"}}]'
        fi
        ;;
      rm)
        [ "${3:-}" = "owned-cache" ] && [ "${4:-}" = "" ] || exit 97
        printf '%s\n' 'owned-cache'
        ;;
      *) exit 98 ;;
    esac
    ;;
  *) exit 99 ;;
esac
"#,
    );
    let target = docker_target(&runtime);
    let plan = probe_container_orphans(&target);
    let volume = plan
        .categories
        .iter()
        .find(|category| category.category == OrphanCategory::Volume)
        .expect("volume category");
    let evidence = volume.evidence.as_ref().expect("volume evidence");
    assert_eq!(evidence.total_records, 2);
    assert_eq!(evidence.candidate_records, 1);

    let execution = execute_container_orphan_prune(
        &target,
        OrphanCategory::Volume,
        volume.approval_phrase.as_deref().unwrap(),
        "Remove only the explicitly owned cache volume after fresh reinspection.",
        1,
    )
    .expect("owned volume removal must succeed");
    assert!(execution.executed);
    assert!(execution.stdout.contains("owned-cache"));
    assert!(!execution.stdout.contains("compose-data"));
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

#[test]
fn cli_help_must_be_a_terminal_solo_request() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let output = Command::new(binary)
        .args(["--runtime", "docker-native", "--help"])
        .output()
        .expect("run shipped container orphan plan CLI");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 stderr");
    assert!(stderr.contains("help must be used alone"));
    assert!(stderr.contains("Usage: disksage-container-orphan-plan"));
}

#[test]
fn unsupported_runtime_kind_is_not_reflected_in_diagnostics() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let untrusted = "customer-secret-runtime-name";
    let output = Command::new(binary)
        .args(["--runtime", untrusted])
        .output()
        .expect("run shipped container orphan plan CLI");

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 stderr");
    assert!(stderr.contains("unsupported runtime kind"));
    assert!(!stderr.contains(untrusted));
}

#[test]
fn singleton_cli_options_reject_duplicates_before_domain_work() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let cases = [
        (
            vec!["--runtime", "docker-native", "--runtime", "docker-native"],
            "--runtime may be supplied once",
        ),
        (
            vec!["--runtime", "docker-colima-context", "--scope", "one", "--scope", "two"],
            "--scope may be supplied once",
        ),
        (
            vec!["--runtime", "docker-native", "--bin", "first", "--bin", "second"],
            "--bin may be supplied once",
        ),
        (
            vec!["--runtime", "docker-native", "--pretty", "--pretty"],
            "--pretty may be supplied once",
        ),
    ];

    for (args, expected) in cases {
        let output = Command::new(binary)
            .args(args)
            .output()
            .expect("run shipped container orphan plan CLI");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 stderr");
        assert!(stderr.contains(expected), "stderr={stderr}");
    }
}

#[test]
fn runtime_scope_relationship_is_validated_before_domain_work() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let cases = [
        (
            vec!["--runtime", "docker-native", "--scope", "ignored"],
            "--scope is not valid for docker-native",
        ),
        (
            vec!["--runtime", "docker-colima-context"],
            "--scope is required for docker-colima-context",
        ),
        (
            vec!["--runtime", "podman-machine"],
            "--scope is required for podman-machine",
        ),
    ];

    for (args, expected) in cases {
        let output = Command::new(binary)
            .args(args)
            .output()
            .expect("run shipped container orphan plan CLI");
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        let stderr = String::from_utf8(output.stderr).expect("bounded UTF-8 stderr");
        assert!(stderr.contains(expected), "stderr={stderr}");
    }
}

#[cfg(unix)]
#[test]
fn podman_machine_cli_defaults_to_the_podman_binary() {
    let binary = env!("CARGO_BIN_EXE_disksage-container-orphan-plan");
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let podman = temp.path().join("podman");
    std::fs::write(
        &podman,
        r#"#!/bin/sh
set -eu
[ "${1:-}" = "--connection" ] || exit 91
[ "${2:-}" = "machine-a" ] || exit 92
shift 2
case "${1:-}" in
  info|container|images|volume|network) exit 0 ;;
  *) exit 93 ;;
esac
"#,
    )
    .expect("write fake podman runtime");
    let mut permissions = std::fs::metadata(&podman)
        .expect("fake podman metadata")
        .permissions();
    permissions.set_mode(0o700);
    std::fs::set_permissions(&podman, permissions).expect("make fake podman executable");

    let output = Command::new(binary)
        .env("PATH", temp.path())
        .args(["--runtime", "podman-machine", "--scope", "machine-a"])
        .output()
        .expect("run shipped container orphan plan CLI");

    assert_eq!(output.status.code(), Some(0));
    assert!(output.stderr.is_empty());
    let document: serde_json::Value = serde_json::from_slice(&output.stdout)
        .expect("machine-readable container orphan evidence");
    assert_eq!(document["runtime"]["healthy"], true);
    assert_eq!(document["evidence_complete"], true);
}
