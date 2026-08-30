#![cfg(unix)]

use disksage_lib::podman_reclaim::{execute_podman_storage_repair, plan_podman_storage_repair};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, OnceLock};

fn fixture_guard() -> MutexGuard<'static, ()> {
    static GUARD: OnceLock<Mutex<()>> = OnceLock::new();
    GUARD
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn write_fake_podman(script: &str) -> (tempfile::TempDir, PathBuf) {
    let temp = tempfile::tempdir().expect("temporary runtime directory");
    let fake = temp.path().join("podman");
    fs::write(&fake, script).expect("write fake Podman");
    let mut permissions = fs::metadata(&fake).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&fake, permissions).unwrap();
    (temp, fake)
}

fn fake_podman() -> (tempfile::TempDir, PathBuf) {
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_fake_podman(&format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    echo "repair command did not provide a trustworthy result" >&2
    exit 1
    ;;
  *" system check --quick "*)
    count=0
    if [ -f "$root/check-count" ]; then
      count=$(cat "$root/check-count")
    fi
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -le 2 ]; then
      echo "Damaged layer {layer_id}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    echo "postcheck unavailable" >&2
    exit 1
    ;;
esac
echo "unexpected fake Podman invocation: $*" >&2
exit 2
"#
    ))
}

fn fake_container_referenced_damage_podman() -> (tempfile::TempDir, PathBuf) {
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_fake_podman(&format!(
        r#"#!/bin/sh
case " $* " in
  *" system check --quick --repair "*)
    echo "Error: layer {layer_id} is in use by container 111111111111" >&2
    exit 125
    ;;
  *" system check --quick "*)
    echo "Damaged layer {layer_id}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn fake_scope_drift_podman() -> (tempfile::TempDir, PathBuf) {
    let first = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let second = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    write_fake_podman(&format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    touch "$root/repair-ran"
    exit 0
    ;;
  *" system check --quick "*)
    count=0
    [ ! -f "$root/check-count" ] || count=$(cat "$root/check-count")
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -eq 1 ]; then
      echo "Damaged layer {first}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    if [ "$count" -eq 2 ]; then
      echo "Damaged layer {second}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    exit 0
    ;;
esac
exit 2
"#
    ))
}

fn fake_postcheck_parse_failure() -> (tempfile::TempDir, PathBuf) {
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_fake_podman(&format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    touch "$root/repair-ran"
    exit 0
    ;;
  *" system check --quick "*)
    count=0
    [ ! -f "$root/check-count" ] || count=$(cat "$root/check-count")
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -le 2 ]; then
      echo "Damaged layer {layer_id}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    echo "Damaged layer not-a-valid-layer-id:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn fake_repair_output_failure() -> (tempfile::TempDir, PathBuf) {
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_fake_podman(&format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    touch "$root/repair-ran"
    head -c 1048577 /dev/zero
    exit 1
    ;;
  *" system check --quick "*)
    echo "Damaged layer {layer_id}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn fake_replaced_damage_podman() -> (tempfile::TempDir, PathBuf) {
    let first = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let replacement = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    write_fake_podman(&format!(
        r#"#!/bin/sh
root="$(dirname "$0")"
case " $* " in
  *" system check --quick --repair "*)
    exit 0
    ;;
  *" system check --quick "*)
    count=0
    [ ! -f "$root/check-count" ] || count=$(cat "$root/check-count")
    count=$((count + 1))
    printf '%s' "$count" > "$root/check-count"
    if [ "$count" -le 2 ]; then
      echo "Damaged layer {first}:"
      echo "Error: damage detected in local storage"
      exit 1
    fi
    echo "Damaged layer {replacement}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn fake_layer_in_use_podman() -> (tempfile::TempDir, PathBuf) {
    let layer_id = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    write_fake_podman(&format!(
        r#"#!/bin/sh
case " $* " in
  *" system check --quick --repair "*)
    echo "Error: layer is in use" >&2
    exit 1
    ;;
  *" system check --quick "*)
    echo "Damaged layer {layer_id}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn single_damage_fake(layer_id: &str) -> (tempfile::TempDir, PathBuf) {
    write_fake_podman(&format!(
        r#"#!/bin/sh
case " $* " in
  *" system check --quick "*)
    echo "Damaged layer {layer_id}:"
    echo "Error: damage detected in local storage"
    exit 1
    ;;
esac
exit 2
"#
    ))
}

fn approval_for(fake: &Path) -> String {
    plan_podman_storage_repair(fake, "podman-machine-default")
        .expect("initial damaged-layer evidence")
        .exact_approval_phrase
        .expect("repair approval phrase")
}

#[test]
fn incomplete_postcheck_never_serializes_unverified_repair_counts() {
    let _guard = fixture_guard();
    let (_temp, fake) = fake_podman();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "test incomplete postcheck",
        1,
    )
    .expect("the receipt must preserve that the command ran even when verification is incomplete");
    let json = serde_json::to_value(receipt).expect("serialize receipt");

    assert_eq!(json["postcheck_complete"], false);
    assert!(
        json["repaired_layer_records"].is_null(),
        "an incomplete postcheck cannot turn the precheck count into a verified repair count"
    );
    assert!(
        json["remaining_damaged_layer_records"].is_null(),
        "remaining damaged layers are unknown when the postcheck is incomplete"
    );
    assert_eq!(json["executed"], false);
}

#[test]
fn provider_refusal_identifies_a_container_referenced_damaged_layer() {
    let (_temp, fake) = fake_container_referenced_damage_podman();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "retain the damaged container for an evidence-guided remediation",
        6,
    )
    .expect("a provider refusal remains an auditable non-executed attempt");

    assert!(!receipt.executed);
    assert_eq!(receipt.status_code, 125);
    assert_eq!(
        receipt.execution_issue.as_deref(),
        Some("podman-storage-repair-provider-unable-to-detach-damaged-container")
    );
    assert_eq!(receipt.remaining_damaged_layer_records, Some(1));
}

#[test]
fn native_repair_approval_matches_machine_scope_instead_of_a_stale_candidate_set() {
    let _guard = fixture_guard();
    let (temp, fake) = fake_scope_drift_podman();
    let first_plan = plan_podman_storage_repair(&fake, "podman-machine-default")
        .expect("initial damaged-layer evidence");
    let approval = first_plan
        .exact_approval_phrase
        .as_deref()
        .expect("machine-scoped repair approval");

    assert!(
        approval.starts_with("DiskSage Podman machine storage repair 승인 "),
        "the approval text must name the broad native repair scope"
    );
    assert!(
        !approval.contains(&first_plan.candidate_set_sha256),
        "a broad native repair cannot truthfully bind authority to only the preflight IDs"
    );

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        approval,
        "approve the selected machine's native repair scope",
        2,
    )
    .expect("candidate drift remains inside an explicitly machine-scoped approval");

    assert!(
        temp.path().join("repair-ran").exists(),
        "the explicitly approved machine-scoped repair must run"
    );
    assert_eq!(
        receipt.command,
        vec![
            "podman",
            "--connection",
            "podman-machine-default",
            "system",
            "check",
            "--quick",
            "--repair"
        ],
        "the receipt must record the exact selected connection that was mutated"
    );
}

#[test]
fn a_failed_postcheck_after_mutation_still_returns_an_auditable_receipt() {
    let _guard = fixture_guard();
    let (temp, fake) = fake_postcheck_parse_failure();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "preserve mutation evidence when verification fails",
        3,
    )
    .expect("postcheck parsing failure after mutation must not erase the execution receipt");
    let json = serde_json::to_value(receipt).expect("serialize receipt");

    assert!(temp.path().join("repair-ran").exists());
    assert_eq!(json["postcheck_complete"], false);
    assert!(json["repaired_layer_records"].is_null());
    assert!(json["remaining_damaged_layer_records"].is_null());
}

#[test]
fn a_post_spawn_capture_failure_still_returns_an_auditable_receipt() {
    let _guard = fixture_guard();
    let (temp, fake) = fake_repair_output_failure();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "preserve attempted mutation evidence after capture failure",
        5,
    )
    .expect("a post-spawn failure must remain an auditable repair attempt");

    assert!(temp.path().join("repair-ran").exists());
    assert!(receipt.command_attempted);
    assert_eq!(receipt.status_code, -126);
    assert_eq!(
        receipt.execution_issue.as_deref(),
        Some("podman-storage-repair-output-too-large")
    );
}

#[test]
fn repair_counts_compare_damage_identities_instead_of_only_aggregate_counts() {
    let _guard = fixture_guard();
    let (_temp, fake) = fake_replaced_damage_podman();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "measure repaired identities independently from new damage",
        4,
    )
    .expect("complete pre/post evidence");

    assert_eq!(receipt.repaired_layer_records, Some(1));
    assert_eq!(receipt.remaining_damaged_layer_records, Some(1));
}

#[test]
fn layer_in_use_failure_remains_auditable_and_never_claims_repair() {
    let _guard = fixture_guard();
    let (_temp, fake) = fake_layer_in_use_podman();
    let approval = approval_for(&fake);

    let receipt = execute_podman_storage_repair(
        &fake,
        "podman-machine-default",
        &approval,
        "retain a fail-closed receipt when a damaged layer is in use",
        6,
    )
    .expect("a spawned repair failure must remain auditable");

    assert!(receipt.command_attempted);
    assert_eq!(receipt.status_code, 1);
    assert_eq!(
        receipt.execution_issue.as_deref(),
        Some("podman-storage-repair-provider-exit-status-unexpected")
    );
    assert!(!receipt.executed);
    assert!(receipt.postcheck_complete);
    assert_eq!(receipt.repaired_layer_records, Some(0));
    assert_eq!(receipt.remaining_damaged_layer_records, Some(1));
}

#[test]
fn damaged_layer_hex_casing_does_not_change_the_precheck_fingerprint() {
    let _guard = fixture_guard();
    let lower = "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd";
    let upper = lower.to_ascii_uppercase();
    let (_lower_temp, lower_fake) = single_damage_fake(lower);
    let (_upper_temp, upper_fake) = single_damage_fake(&upper);

    let lower_plan = plan_podman_storage_repair(&lower_fake, "podman-machine-default").unwrap();
    let upper_plan = plan_podman_storage_repair(&upper_fake, "podman-machine-default").unwrap();

    assert_eq!(
        lower_plan.candidate_set_sha256,
        upper_plan.candidate_set_sha256
    );
    assert_eq!(
        lower_plan.exact_approval_phrase,
        upper_plan.exact_approval_phrase
    );
}

#[test]
fn storage_repair_rejects_unbounded_or_control_character_rationales_before_probe() {
    let _guard = fixture_guard();
    let (_temp, fake) = fake_podman();
    let overlong = "x".repeat(1_001);
    assert_eq!(
        execute_podman_storage_repair(&fake, "podman-machine-default", "unused", &overlong, 1,)
            .unwrap_err(),
        "podman-storage-repair-request-invalid"
    );
    assert_eq!(
        execute_podman_storage_repair(
            &fake,
            "podman-machine-default",
            "unused",
            "operator\ncontrol",
            1,
        )
        .unwrap_err(),
        "podman-storage-repair-request-invalid"
    );
    assert!(
        !tempfile::TempDir::path(&_temp).join("check-count").exists(),
        "invalid rationale must be rejected before invoking Podman"
    );
}
