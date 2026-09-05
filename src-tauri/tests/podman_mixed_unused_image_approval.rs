#[cfg(unix)]
mod unix {
    use disksage_lib::podman_reclaim::probe_podman_reclaim;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::time::Duration;

    #[test]
    fn mixed_tagged_and_untagged_unused_images_offer_exact_prune_approval() {
        let root = std::env::temp_dir().join(format!(
            "disksage-podman-mixed-approval-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let raw_image = root.join("machine.raw");
        fs::write(&raw_image, vec![0_u8; 4096]).unwrap();
        fs::write(
            root.join("podman-machine-default.json"),
            format!(r#"{{"ImagePath":{{"Path":"{}"}}}}"#, raw_image.display()),
        )
        .unwrap();

        let script = root.join("podman");
        let script_body = format!(
            r#"#!/bin/sh
case "$*" in
  "machine inspect podman-machine-default")
    printf '%s\n' '[{{"ConfigDir":{{"Path":"{}"}},"Name":"podman-machine-default","State":"running","Resources":{{"DiskSize":100}}}}]'
    ;;
  "machine ssh podman-machine-default -- df -B1 --output=size,used,avail /")
    printf '%s\n' '1B-blocks Used Avail' '107374182400 32212254720 75161927680'
    ;;
  "--connection podman-machine-default info --format json")
    printf '%s\n' '{{"store":{{"graphRoot":"/var/home/core/.local/share/containers/storage","graphRootAllocated":107374182400,"graphRootUsed":32212254720,"imageStore":{{"number":2}},"containerStore":{{"number":0,"running":0,"stopped":0}}}}}}'
    ;;
  "--connection podman-machine-default system df --format json")
    printf '%s\n' '[{{"Type":"Images","Total":2,"Active":0,"RawSize":300,"RawReclaimable":300}},{{"Type":"Containers","Total":0,"Active":0,"RawSize":0,"RawReclaimable":0}},{{"Type":"Local Volumes","Total":0,"Active":0,"RawSize":0,"RawReclaimable":0}}]'
    ;;
  "--connection podman-machine-default images --all --format json")
    printf '%s\n' '[{{"Id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa","RepoTags":[],"Containers":0,"Size":100}},{{"Id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","RepoTags":["localhost/keep:latest"],"Containers":0,"Size":200}}]'
    ;;
  *)
    printf '%s\n' "unexpected invocation: $*" >&2
    exit 9
    ;;
esac
"#,
            root.display()
        );
        fs::write(&script, script_body).unwrap();
        let mut permissions = fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(&script, permissions).unwrap();

        let plan = probe_podman_reclaim(
            Path::new(&script),
            "podman-machine-default",
            Duration::from_secs(2),
        );

        let evidence = plan.unused_images.expect("unused image evidence should be complete");
        assert_eq!(evidence.unused_untagged_records, 1);
        assert_eq!(evidence.unused_tagged_records, 1);
        assert!(plan.dangling_prune_approval_phrase.is_some());
        fs::remove_dir_all(root).unwrap();
    }
}
