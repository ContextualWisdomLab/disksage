//! Public-boundary coverage for cloud-local inventory option and checkpoint validation.
//!
//! The production inventory remains metadata-only and read-only. These regressions exercise
//! fail-closed limits and checkpoint identity validation without changing provider or eviction
//! authority owned by other active PRs.

use disksage::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint,
    inventory_cloud_local_allocations_with_checkpoints, CloudLocalInventoryOptions,
};
use std::path::Path;

fn root(path: &Path) -> CloudRoot {
    CloudRoot {
        id: "icloud:coverage".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "coverage root".into(),
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

fn assert_invalid(
    options: CloudLocalInventoryOptions,
    expected: &str,
) {
    let error = hard_timeout_inventory(&root(Path::new("/Cloud")), options, 1).unwrap_err();
    assert_eq!(error, expected);
}

#[test]
fn public_timeout_report_rejects_every_unbounded_option_family() {
    let baseline = bounded_options();

    for max_entries in [0, 1_000_001] {
        assert_invalid(
            CloudLocalInventoryOptions {
                max_entries,
                ..baseline
            },
            "cloud-local-inventory-max-entries-invalid",
        );
    }
    for max_results in [0, 10_001] {
        assert_invalid(
            CloudLocalInventoryOptions {
                max_results,
                ..baseline
            },
            "cloud-local-inventory-max-results-invalid",
        );
    }
    assert_invalid(
        CloudLocalInventoryOptions {
            max_depth: 65,
            ..baseline
        },
        "cloud-local-inventory-max-depth-invalid",
    );
    for max_duration_ms in [0, 300_001] {
        assert_invalid(
            CloudLocalInventoryOptions {
                max_duration_ms,
                ..baseline
            },
            "cloud-local-inventory-max-duration-invalid",
        );
    }
    for max_issues in [0, 1_001] {
        assert_invalid(
            CloudLocalInventoryOptions {
                max_issues,
                ..baseline
            },
            "cloud-local-inventory-max-issues-invalid",
        );
    }
}

#[test]
fn public_timeout_checkpoint_rejects_identity_and_terminal_state_drift() {
    let temp = tempfile::tempdir().unwrap();
    let cloud_root = root(temp.path());
    let options = bounded_options();
    let mut checkpoints = Vec::new();

    let terminal = inventory_cloud_local_allocations_with_checkpoints(
        &cloud_root,
        options,
        44,
        |checkpoint| {
            checkpoints.push(checkpoint.clone());
            Ok(())
        },
    )
    .unwrap();
    assert!(terminal.evidence_complete);

    let checkpoint = checkpoints
        .into_iter()
        .next()
        .expect("the checkpoint API must emit an initial nonterminal snapshot");
    assert!(!checkpoint.evidence_complete);
    assert!(checkpoint
        .notices
        .iter()
        .any(|notice| notice == "inventory-checkpoint-not-terminal"));

    let recovered = hard_timeout_inventory_from_checkpoint(
        &cloud_root,
        options,
        checkpoint.clone(),
    )
    .unwrap();
    assert!(!recovered.evidence_complete);
    assert!(recovered
        .stop_reasons
        .iter()
        .any(|reason| reason == "hard-timeout-reached"));
    assert!(recovered
        .notices
        .iter()
        .any(|notice| notice == "partial-inventory-recovered-from-worker-checkpoint"));
    assert!(!recovered
        .notices
        .iter()
        .any(|notice| notice == "inventory-checkpoint-not-terminal"));

    let mut cases = Vec::new();

    let mut wrong_version = checkpoint.clone();
    wrong_version.version += 1;
    cases.push(wrong_version);

    let mut wrong_id = checkpoint.clone();
    wrong_id.cloud_root_id = "icloud:other".into();
    cases.push(wrong_id);

    let mut wrong_provider = checkpoint.clone();
    wrong_provider.provider = CloudProvider::Dropbox;
    cases.push(wrong_provider);

    let mut wrong_scope = checkpoint.clone();
    wrong_scope.account_scope = CloudAccountScope::Team;
    cases.push(wrong_scope);

    let mut wrong_path = checkpoint.clone();
    wrong_path.cloud_root = "/different-root".into();
    cases.push(wrong_path);

    let mut wrong_options = checkpoint.clone();
    wrong_options.options.max_results += 1;
    cases.push(wrong_options);

    let mut terminal_checkpoint = checkpoint.clone();
    terminal_checkpoint.evidence_complete = true;
    cases.push(terminal_checkpoint);

    let mut missing_marker = checkpoint;
    missing_marker
        .notices
        .retain(|notice| notice != "inventory-checkpoint-not-terminal");
    cases.push(missing_marker);

    for invalid in cases {
        assert_eq!(
            hard_timeout_inventory_from_checkpoint(&cloud_root, options, invalid).unwrap_err(),
            "cloud-local-inventory-checkpoint-invalid"
        );
    }
}
