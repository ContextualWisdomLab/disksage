#![cfg(unix)]

use disksage_lib::podman_reclaim::{prune_dangling_images, DEFAULT_PODMAN_MACHINE};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn hash_frame(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

fn approval_phrase(image_id: &str, size_bytes: u64) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"disksage.podman-unused-images.v1");
    hash_frame(&mut hasher, image_id.as_bytes());
    hash_frame(&mut hasher, &size_bytes.to_be_bytes());
    hash_frame(&mut hasher, &0u64.to_be_bytes());
    let digest = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    format!("DiskSage Podman dangling image prune 승인 {digest}")
}

fn write_fake_podman(path: &Path, counter: &Path, mutation_log: &Path, drift: bool) {
    let first_id = "a".repeat(64);
    let second_id = "b".repeat(64);
    let second_listing = if drift {
        format!(
            r#"[{{"Id":"{first_id}","RepoTags":[],"Containers":0,"Size":100}},{{"Id":"{second_id}","RepoTags":[],"Containers":0,"Size":200}}]"#
        )
    } else {
        format!(r#"[{{"Id":"{first_id}","RepoTags":[],"Containers":0,"Size":100}}]"#)
    };
    let script = format!(
        r#"#!/bin/sh
set -eu
if [ "$1" = "machine" ] && [ "$2" = "inspect" ]; then
  printf '%s\n' '[{{"ConfigDir":{{"Path":"/tmp"}},"Name":"podman-machine-default","State":"running","Resources":{{"DiskSize":100}}}}]'
  exit 0
fi
if [ "$1" = "--connection" ] && [ "$3" = "images" ]; then
  if [ ! -e '{counter}' ]; then
    printf '1' > '{counter}'
    printf '%s\n' '[{{"Id":"{first_id}","RepoTags":[],"Containers":0,"Size":100}}]'
  else
    printf '%s\n' '{second_listing}'
  fi
  exit 0
fi
if [ "$1" = "--connection" ] && [ "$3" = "image" ] && [ "$4" = "prune" ]; then
  printf '%s\n' 'broad-prune-invoked' >> '{mutation_log}'
  exit 0
fi
if [ "$1" = "--connection" ] && [ "$3" = "image" ] && [ "$4" = "rm" ]; then
  printf '%s\n' "$*" >> '{mutation_log}'
  exit 0
fi
printf '%s\n' 'unexpected fake podman invocation' >&2
exit 97
"#,
        counter = counter.display(),
        mutation_log = mutation_log.display(),
    );
    fs::write(path, script).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

#[test]
fn prune_revalidates_exact_candidate_set_before_any_destructive_command() {
    let fixture = tempfile::tempdir().unwrap();
    let podman = fixture.path().join("podman");
    let counter = fixture.path().join("images-counter");
    let mutation_log = fixture.path().join("mutation-log");
    write_fake_podman(&podman, &counter, &mutation_log, true);

    let first_id = "a".repeat(64);
    let confirmation = approval_phrase(&first_id, 100);
    let error = prune_dangling_images(
        &podman,
        DEFAULT_PODMAN_MACHINE,
        &confirmation,
        "Reviewed exact dangling-image candidate set",
        1,
    )
    .unwrap_err();

    assert_eq!(error, "podman-prune-candidate-set-changed");
    assert!(
        !mutation_log.exists(),
        "candidate drift must fail before broad or exact image deletion"
    );
}

#[test]
fn prune_uses_only_exact_approved_ids_without_force_or_recursive_parent_prune() {
    let fixture = tempfile::tempdir().unwrap();
    let podman = fixture.path().join("podman");
    let counter = fixture.path().join("images-counter");
    let mutation_log = fixture.path().join("mutation-log");
    write_fake_podman(&podman, &counter, &mutation_log, false);

    let first_id = "a".repeat(64);
    let confirmation = approval_phrase(&first_id, 100);
    let result = prune_dangling_images(
        &podman,
        DEFAULT_PODMAN_MACHINE,
        &confirmation,
        "Reviewed exact dangling-image candidate set",
        1,
    )
    .unwrap();

    let mutation = fs::read_to_string(&mutation_log).unwrap();
    assert!(!mutation.contains("image prune"));
    assert!(!mutation.contains("--force"));
    assert!(mutation.contains("image rm --no-prune"));
    assert!(mutation.contains(&first_id));
    assert_eq!(result.candidate_set_sha256.len(), 64);
    assert!(result.executed);
    assert_eq!(
        result.command,
        vec![
            "podman",
            "--connection",
            DEFAULT_PODMAN_MACHINE,
            "image",
            "rm",
            "--no-prune",
            "<approved-image-ids>",
        ]
    );
}
