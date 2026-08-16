//! Verify every public checkpoint-admission discriminator fails closed.
//!
//! The checkpoint converter is intentionally pure: these regressions mutate bounded in-memory
//! evidence only and never touch provider APIs, credentials, or the filesystem.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_inventory::{
    hard_timeout_inventory, hard_timeout_inventory_from_checkpoint,
    CloudLocalAllocationInventory, CloudLocalInventoryOptions,
};

fn root() -> CloudRoot {
    CloudRoot {
        id: "icloud:checkpoint-contract".into(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "iCloud checkpoint contract".into(),
        path: "/Cloud".into(),
        readable: true,
        access_issue: None,
    }
}

fn checkpoint(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
) -> CloudLocalAllocationInventory {
    let mut checkpoint = hard_timeout_inventory(root, options, 17).unwrap();
    checkpoint.stop_reasons.clear();
    checkpoint.notices.retain(|notice| notice != "worker-hard-timeout");
    checkpoint.notices.retain(|notice| notice != "inventory-incomplete");
    checkpoint
        .notices
        .push("inventory-checkpoint-not-terminal".into());
    checkpoint
}

fn assert_invalid(
    root: &CloudRoot,
    options: CloudLocalInventoryOptions,
    checkpoint: CloudLocalAllocationInventory,
) {
    assert_eq!(
        hard_timeout_inventory_from_checkpoint(root, options, checkpoint).unwrap_err(),
        "cloud-local-inventory-checkpoint-invalid"
    );
}

#[test]
fn rejects_every_checkpoint_identity_and_state_mismatch() {
    let root = root();
    let options = CloudLocalInventoryOptions::default();

    let mut wrong_version = checkpoint(&root, options);
    wrong_version.version = 1;
    assert_invalid(&root, options, wrong_version);

    let mut wrong_id = checkpoint(&root, options);
    wrong_id.cloud_root_id = "icloud:other".into();
    assert_invalid(&root, options, wrong_id);

    let mut wrong_provider = checkpoint(&root, options);
    wrong_provider.provider = CloudProvider::Onedrive;
    assert_invalid(&root, options, wrong_provider);

    let mut wrong_scope = checkpoint(&root, options);
    wrong_scope.account_scope = CloudAccountScope::Organization;
    assert_invalid(&root, options, wrong_scope);

    let mut wrong_path = checkpoint(&root, options);
    wrong_path.cloud_root = "/OtherCloud".into();
    assert_invalid(&root, options, wrong_path);

    let mut wrong_options = checkpoint(&root, options);
    wrong_options.options.max_results -= 1;
    assert_invalid(&root, options, wrong_options);

    let mut terminal = checkpoint(&root, options);
    terminal.evidence_complete = true;
    assert_invalid(&root, options, terminal);

    let mut missing_checkpoint_notice = checkpoint(&root, options);
    missing_checkpoint_notice
        .notices
        .retain(|notice| notice != "inventory-checkpoint-not-terminal");
    assert_invalid(&root, options, missing_checkpoint_notice);
}

#[test]
fn valid_checkpoint_is_promoted_to_fail_closed_timeout_once() {
    let root = root();
    let options = CloudLocalInventoryOptions::default();
    let mut checkpoint = checkpoint(&root, options);
    checkpoint.stop_reasons.push("entry-errors".into());
    checkpoint.notices.push("inventory-incomplete".into());

    let recovered = hard_timeout_inventory_from_checkpoint(&root, options, checkpoint).unwrap();
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
            "{notice} must be present exactly once"
        );
    }
    assert!(recovered
        .notices
        .iter()
        .all(|notice| notice != "inventory-checkpoint-not-terminal"));
}
