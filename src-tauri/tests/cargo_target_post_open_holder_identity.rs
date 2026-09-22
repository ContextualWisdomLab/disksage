#[cfg(target_os = "linux")]
mod linux_post_open_holder_identity {
    use std::fs::{self, File};
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::fs::MetadataExt;
    use std::process::{Command, Stdio};

    fn assert_owner_requires_capability_bound_post_open_holder_authorization() {
        let owner_source = include_str!("../src/cargo_target_reclaim.rs");
        assert!(
            !owner_source.contains("active_use(&detached_target.clean_path)?;"),
            "P1: post-open mutation authorization still delegates to a pathname probe on the detached path; a holder of the reviewed object can be missed after pathname replacement"
        );
        assert!(
            owner_source.contains("ensure_opened_target_has_no_active_holders"),
            "P1: Unix owner path must authorize post-open holders from the reviewed filesystem capability/identity, not from a mutable pathname"
        );
    }

    #[test]
    fn holder_of_reviewed_object_survives_path_replacement_and_must_still_block_cleanup() {
        let root = tempfile::tempdir().expect("temp root");
        let target = root.path().join("target");
        let reviewed_stash = root.path().join("reviewed-object");
        let artifact = target.join("artifact");
        fs::create_dir(&target).expect("target");
        fs::write(&artifact, b"reviewed payload").expect("artifact");

        let reviewed_root = File::open(&target).expect("open reviewed root");
        let reviewed_root_identity = reviewed_root.metadata().expect("reviewed root metadata");
        let reviewed_artifact_identity = fs::metadata(&artifact).expect("artifact metadata");

        let mut holder = Command::new("/bin/sh")
            .arg("-c")
            .arg("exec 3<\"$1\"; printf 'READY\\n'; IFS= read -r _")
            .arg("disksage-holder")
            .arg(&artifact)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn holder");

        let mut ready = String::new();
        BufReader::new(holder.stdout.take().expect("holder stdout"))
            .read_line(&mut ready)
            .expect("holder ready line");
        assert_eq!(ready, "READY\n", "holder must acquire the artifact before substitution");

        let held_fd = fs::metadata(format!("/proc/{}/fd/3", holder.id()))
            .expect("inspect holder fd identity");
        assert_eq!(held_fd.dev(), reviewed_artifact_identity.dev());
        assert_eq!(held_fd.ino(), reviewed_artifact_identity.ino());

        fs::rename(&target, &reviewed_stash).expect("move reviewed pathname aside");
        fs::create_dir(&target).expect("replacement target");
        fs::write(target.join("REPLACEMENT_SENTINEL"), b"must-survive")
            .expect("replacement sentinel");

        let still_reviewed = reviewed_root.metadata().expect("reviewed handle after substitution");
        assert_eq!(still_reviewed.dev(), reviewed_root_identity.dev());
        assert_eq!(still_reviewed.ino(), reviewed_root_identity.ino());
        let held_after_swap = fs::metadata(format!("/proc/{}/fd/3", holder.id()))
            .expect("holder fd after substitution");
        assert_eq!(held_after_swap.dev(), reviewed_artifact_identity.dev());
        assert_eq!(held_after_swap.ino(), reviewed_artifact_identity.ino());

        let replacement = fs::metadata(&target).expect("replacement metadata");
        assert!(
            replacement.dev() != reviewed_root_identity.dev()
                || replacement.ino() != reviewed_root_identity.ino(),
            "replacement pathname must identify a different directory object"
        );
        assert_eq!(
            fs::read(target.join("REPLACEMENT_SENTINEL")).expect("replacement sentinel survives"),
            b"must-survive"
        );

        // A post-open authorization that starts again from `target` now inspects object B,
        // while the live holder remains attached to object A through its open descriptor.
        // The production owner must therefore match holder identities against the retained
        // reviewed capability rather than authorize deletion from the current pathname.
        assert_owner_requires_capability_bound_post_open_holder_authorization();

        holder
            .stdin
            .take()
            .expect("holder stdin")
            .write_all(b"release\n")
            .expect("release holder");
        assert!(holder.wait().expect("wait holder").success());
    }
}
