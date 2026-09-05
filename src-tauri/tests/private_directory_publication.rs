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
fn missing_private_ancestors_are_created_descriptor_relative_with_exact_modes() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
    let target = root.join("receipts/provider-cache/receipt.json");

    write_private_bytes_create_new_with_parents(&target, b"receipt", 0o400, 0o700).unwrap();

    assert_eq!(fs::read(&target).unwrap(), b"receipt");
    assert_eq!(fs::metadata(root.join("receipts")).unwrap().permissions().mode() & 0o777, 0o700);
    assert_eq!(
        fs::metadata(root.join("receipts/provider-cache"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert_eq!(fs::metadata(&target).unwrap().permissions().mode() & 0o777, 0o400);
}

#[test]
fn anchor_replacement_after_parent_provision_never_receives_the_record() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("private-root");
    let admitted = temp.path().join("admitted-root");
    fs::create_dir(&root).unwrap();
    set_mode(&root, 0o700);
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
