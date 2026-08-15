//! Integration coverage for public cloud-local-inventory watchdog and checkpoint boundaries.
//!
//! The fixtures use an empty temporary directory and in-memory checkpoints only. They never open
//! file contents, contact a provider, or authorize eviction.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint,
    inventory_cloud_local_allocations_with_checkpoints, CloudLocalAllocationInventory,
    CloudLocalInventoryOptions,
};
use std::path::Path;

fn cloud_root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:checkpoint-test".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "checkpoint test".into(),
        path: path.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    }
}

fn bounded_options() -> CloudLocalInventoryOptions {
    CloudLocalInventoryOptions {
        min_allocated_bytes: 1,
        max_entries: 100,
        max_results: 10,
        max_depth: 4,
        max_duration_ms: 10_000,
        max_issues: 10,
    }
}

fn checkpoint_fixture() -> (CloudRoot, CloudLocalInventoryOptions, CloudLocalAllocationInventory) {
    let directory = tempfile::tempdir().expect("temporary cloud inventory root");
    let root = cloud_root(directory.path());
    let options = bounded_options();
    let mut checkpoints = Vec::new();
    let terminal = inventory_cloud_local_allocations_with_checkpoints(
        &root,
        options,
        123,
        |checkpoint| {
            checkpoints.push(checkpoint.clone());
            Ok(())
        },
    )
    .expect("empty metadata-only inventory");
    assert!(terminal.evidence_complete);
    let checkpoint = checkpoints
        .into_iter()
        .next()
        .expect("initial non-terminal checkpoint");
    assert!(!checkpoint.evidence_complete);
    assert!(checkpoint
        .notices
        .contains(&"inventory-checkpoint-not-terminal".to_string()));
    (root, options, checkpoint)
}

fn expect_checkpoint_rejection(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    checkpoint: &CloudLocalAllocationInventory,
    mutate: impl FnOnce(&mut CloudLocalAllocationInventory),
) {
    let mut invalid = checkpoint.clone();
    mutate(&mut invalid);
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(root, options, invalid).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );
}

#[test]
fn checkpoint_recovery_rejects_independent_identity_and_terminal_drift() {
    let (root, options, checkpoint) = checkpoint_fixture();

    expect_checkpoint_rejection(&root, options, &checkpoint, |value| value.version = 3);
    expect_checkpoint_rejection(&root, options, &checkpoint, |value| {
        value.cloud_root_id = "icloud:other".into()
    });
    expect_checkpoint_rejection(&root, options, &checkpoint, |value| {
        value.cloud_root = "/different-root".into()
    });
    expect_checkpoint_rejection(&root, options, &checkpoint, |value| {
        value.options.max_depth += 1
    });
    expect_checkpoint_rejection(&root, options, &checkpoint, |value| {
        value.evidence_complete = true
    });
    expect_checkpoint_rejection(&root, options, &checkpoint, |value| {
        value
            .notices
            .retain(|notice| notice != "inventory-checkpoint-not-terminal")
    });
}

#[test]
fn checkpoint_recovery_is_idempotently_fail_closed_for_timeout_markers() {
    let (root, options, checkpoint) = checkpoint_fixture();
    let mut checkpoint = checkpoint;
    checkpoint.stop_reasons.push("hard-timeout-reached".into());
    checkpoint.notices.push("inventory-incomplete".into());
    checkpoint.notices.push("worker-hard-timeout".into());

    let recovered = hard_timeout_inventory_from_checkpoint(&root, options, checkpoint)
        .expect("valid checkpoint recovery");
    assert!(!recovered.evidence_complete);
    assert_eq!(
        recovered
            .stop_reasons
            .iter()
            .filter(|reason| reason.as_str() == "hard-timeout-reached")
            .count(),
        1
    );
    for notice in [
        "inventory-incomplete",
        "worker-hard-timeout",
        "partial-inventory-recovered-from-worker-checkpoint",
    ] {
        assert_eq!(
            recovered
                .notices
                .iter()
                .filter(|value| value.as_str() == notice)
                .count(),
            1,
            "recovery marker must be unique: {notice}"
        );
    }
    assert!(!recovered
        .notices
        .contains(&"inventory-checkpoint-not-terminal".to_string()));
}

#[test]
fn hard_timeout_rejects_each_out_of_range_public_option() {
    let root = cloud_root(Path::new("/Cloud"));
    let base = bounded_options();

    for invalid in [
        CloudLocalInventoryOptions {
            max_entries: 0,
            ..base
        },
        CloudLocalInventoryOptions {
            max_results: 0,
            ..base
        },
        CloudLocalInventoryOptions {
            max_depth: 65,
            ..base
        },
        CloudLocalInventoryOptions {
            max_duration_ms: 0,
            ..base
        },
        CloudLocalInventoryOptions {
            max_issues: 0,
            ..base
        },
    ] {
        assert!(hard_timeout_inventory(&root, invalid, 1).is_err());
    }
}
