#[cfg(unix)]
mod unix_regressions {
    use disksage_lib::allocation_map::measure_root;
    use std::io;
    use std::os::unix::process::CommandExt;
    use std::process::Command;
    use std::time::Duration;

    #[test]
    fn nested_generated_roots_keep_generated_classification() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("target").join("nested");
        std::fs::create_dir_all(&root).unwrap();

        let report = measure_root(&root, 1, Duration::from_secs(1)).unwrap();

        assert_eq!(report.classification, "generated");
        assert!(report.evidence_complete);
    }

    #[test]
    fn wide_directory_scan_does_not_hold_one_descriptor_per_sibling() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        const SIBLINGS: u64 = 96;
        for index in 0..SIBLINGS {
            std::fs::create_dir(root.join(format!("child-{index:03}"))).unwrap();
        }

        let mut command = Command::new(env!("CARGO_BIN_EXE_disksage-allocation-map"));
        command
            .arg((SIBLINGS + 1).to_string())
            .arg("10000")
            .arg(&root);
        unsafe {
            command.pre_exec(|| {
                let limit = libc::rlimit {
                    rlim_cur: 32,
                    rlim_max: 32,
                };
                if libc::setrlimit(libc::RLIMIT_NOFILE, &limit) != 0 {
                    return Err(io::Error::last_os_error());
                }
                Ok(())
            });
        }

        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "allocation map failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reports: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(reports[0]["evidence_complete"], true);
        assert_eq!(reports[0]["visited_entries"], SIBLINGS + 1);
        assert!(reports[0]["stop_reason"].is_null());
    }

    #[test]
    fn retained_ancestor_names_consume_the_same_entry_budget() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("root");
        std::fs::create_dir(&root).unwrap();
        let first = root.join("a");
        std::fs::create_dir(&first).unwrap();
        std::fs::write(first.join("inner.bin"), b"inner").unwrap();
        for name in ["b", "c", "d", "e", "f", "g"] {
            std::fs::create_dir(root.join(name)).unwrap();
        }

        let report = measure_root(&root, 8, Duration::from_secs(1)).unwrap();

        assert_eq!(report.stop_reason, Some("entry-limit-reached"));
        assert!(!report.evidence_complete);
        assert_eq!(
            report.visited_entries, 2,
            "queued sibling names must consume the same global entry budget before descending"
        );
    }

    #[test]
    fn cli_entry_budget_is_shared_across_all_requested_roots() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir(&first).unwrap();
        std::fs::create_dir(&second).unwrap();
        std::fs::write(first.join("one.bin"), b"one").unwrap();
        std::fs::write(second.join("two.bin"), b"two").unwrap();

        let output = Command::new(env!("CARGO_BIN_EXE_disksage-allocation-map"))
            .arg("2")
            .arg("10000")
            .arg(&first)
            .arg(&second)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "allocation map failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let reports: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
        let reports = reports.as_array().expect("allocation map report array");
        assert_eq!(reports.len(), 2);
        let visited = reports
            .iter()
            .map(|report| report["visited_entries"].as_u64().unwrap())
            .sum::<u64>();
        assert!(visited <= 2, "command-wide budget was exceeded: {visited}");
        assert_eq!(reports[0]["evidence_complete"], true);
        assert_eq!(reports[1]["evidence_complete"], false);
        assert_eq!(reports[1]["stop_reason"], "entry-limit-reached");
    }
}
