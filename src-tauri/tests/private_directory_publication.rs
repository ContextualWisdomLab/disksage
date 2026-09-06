#![cfg(unix)]

#[path = "../src/private_directory_publication.rs"]
mod private_directory_publication;

use private_directory_publication::{
    write_private_bytes_create_new_with_parents,
    write_private_bytes_create_new_with_parents_with_hooks,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;

fn set_mode(path: &std::path::Path, mode: u32) {
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[test]
fn missing_private_ancestors_fail_closed_until_creation_returns_object_authority() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    let target = root.join("receipts/provider-cache/receipt.json");

    let error = write_private_bytes_create_new_with_parents(&target, b"receipt", 0o400, 0o700)
        .expect_err("pathname-only mkdirat provisioning must not grant publication authority");

    assert_eq!(
        error,
        "private-directory-publication-parent-provisioning-unavailable"
    );
    assert!(
        !root.join("receipts").exists(),
        "fail-closed provisioning must not leave a pathname-created ancestor"
    );
    assert!(!target.exists());
}

#[test]
fn existing_leaf_parent_must_already_be_owner_private_and_is_not_chmodded() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    let receipt_dir = root.join("receipts");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    fs::create_dir(&receipt_dir).unwrap();
    set_mode(&receipt_dir, 0o755);
    let target = receipt_dir.join("receipt.json");

    let error = write_private_bytes_create_new_with_parents(&target, b"receipt", 0o400, 0o700)
        .unwrap_err();

    assert_eq!(
        error,
        "private-directory-publication-directory-mode-drift"
    );
    assert_eq!(fs::metadata(&receipt_dir).unwrap().permissions().mode() & 0o777, 0o755);
    assert!(!target.exists());
}

#[test]
fn anchor_replacement_after_parent_admission_never_receives_the_record() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    let admitted = temp.path().join("admitted-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    fs::create_dir(root.join("receipts")).unwrap();
    set_mode(&root.join("receipts"), 0o700);
    let target = root.join("receipts/receipt.json");
    let replacement_receipt = root.join("receipts/receipt.json");
    let old_receipt = admitted.join("receipts/receipt.json");
    let hook_root = root.clone();
    let hook_admitted = admitted.clone();

    let error = write_private_bytes_create_new_with_parents_with_hooks(
        &target,
        b"authorized",
        0o400,
        0o700,
        move || {
            fs::rename(&hook_root, &hook_admitted).unwrap();
            fs::create_dir(&hook_root).unwrap();
            set_mode(&hook_root, 0o700);
            fs::create_dir(hook_root.join("receipts")).unwrap();
            set_mode(&hook_root.join("receipts"), 0o700);
        },
        || {},
    )
    .unwrap_err();

    assert_eq!(error, "private-directory-publication-anchor-identity-drift");
    assert!(!replacement_receipt.exists());
    assert!(!old_receipt.exists());
}

#[test]
fn post_write_anchor_replacement_invalidates_only_the_admitted_record() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    let admitted = temp.path().join("admitted-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    fs::create_dir(root.join("receipts")).unwrap();
    set_mode(&root.join("receipts"), 0o700);
    let target = root.join("receipts/receipt.json");
    let replacement_receipt = root.join("receipts/receipt.json");
    let admitted_receipt = admitted.join("receipts/receipt.json");
    let hook_root = root.clone();
    let hook_admitted = admitted.clone();

    let error = write_private_bytes_create_new_with_parents_with_hooks(
        &target,
        b"authorized",
        0o400,
        0o700,
        || {},
        move || {
            fs::rename(&hook_root, &hook_admitted).unwrap();
            fs::create_dir(&hook_root).unwrap();
            set_mode(&hook_root, 0o700);
            fs::create_dir(hook_root.join("receipts")).unwrap();
            set_mode(&hook_root.join("receipts"), 0o700);
            fs::write(hook_root.join("receipts/receipt.json"), b"replacement").unwrap();
        },
    )
    .unwrap_err();

    assert_eq!(error, "private-directory-publication-anchor-identity-drift");
    assert_eq!(fs::read(&replacement_receipt).unwrap(), b"replacement");
    assert_eq!(fs::metadata(&admitted_receipt).unwrap().len(), 0);
}

#[test]
fn post_write_mode_widening_fails_closed_and_invalidates_the_exact_record() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    fs::create_dir(root.join("receipts")).unwrap();
    set_mode(&root.join("receipts"), 0o700);
    let target = root.join("receipts/receipt.json");
    let hook_target = target.clone();

    let error = write_private_bytes_create_new_with_parents_with_hooks(
        &target,
        b"authorized",
        0o400,
        0o700,
        || {},
        move || {
            set_mode(&hook_target, 0o644);
        },
    )
    .unwrap_err();

    assert_eq!(error, "private-directory-publication-file-mode-drift");
    assert_eq!(fs::metadata(&target).unwrap().len(), 0);
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o7777,
        0o400
    );
}

#[test]
fn post_write_existing_leaf_parent_mode_widening_fails_closed() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    let target = root.join("receipt.json");
    let hook_root = root.clone();

    let error = write_private_bytes_create_new_with_parents_with_hooks(
        &target,
        b"authorized",
        0o400,
        0o700,
        || {},
        move || {
            set_mode(&hook_root, 0o755);
        },
    )
    .unwrap_err();

    assert_eq!(
        error,
        "private-directory-publication-directory-mode-drift"
    );
    assert_eq!(fs::metadata(&root).unwrap().permissions().mode() & 0o777, 0o755);
    assert_eq!(fs::metadata(&target).unwrap().len(), 0);
}
