use disksage_lib::cloud::{
    cloud_root_path_matches, compare_pre_copy_evidence, require_pre_copy_evidence_cohort,
    validate_cloud_root_readable, validate_source_root_readable, CloudAccountScope, CloudProvider,
    CloudRoot, PreCopyEvidenceObservation, PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION,
    PRE_COPY_EVIDENCE_MAX_SKEW_MS,
};
use std::path::{Path, PathBuf};

fn observation(stream: &str, observed_at_ms: u64, fingerprint_byte: char) -> PreCopyEvidenceObservation {
    PreCopyEvidenceObservation {
        stream: stream.to_string(),
        observed_at_ms,
        evidence_complete: true,
        fingerprint: std::iter::repeat_n(fingerprint_byte, 64).collect(),
    }
}

fn complete_observations() -> Vec<PreCopyEvidenceObservation> {
    vec![
        observation("icloud-sync-health-evidence", 1_000, 'a'),
        observation("provider-client-runtime-evidence", 1_001, 'b'),
        observation("volume-pressure-evidence", 1_002, 'c'),
    ]
}

#[test]
fn complete_pre_copy_cohort_is_integrity_bound_and_required() {
    let cohort = compare_pre_copy_evidence(complete_observations());

    assert_eq!(cohort.schema_version, PRE_COPY_EVIDENCE_COHORT_SCHEMA_VERSION);
    assert_eq!(cohort.observed_at_ms, 1_002);
    assert!(cohort.complete);
    assert!(cohort.blockers.is_empty());
    assert_eq!(cohort.cohort_fingerprint.len(), 64);
    assert!(require_pre_copy_evidence_cohort(Some(&cohort)).is_ok());

    let mut tampered = cohort.clone();
    tampered.observed_at_ms += 1;
    assert_eq!(
        require_pre_copy_evidence_cohort(Some(&tampered)),
        Err("pre-copy-evidence-cohort-integrity-invalid".to_string())
    );

    let mut unsupported = cohort;
    unsupported.schema_version += 1;
    assert_eq!(
        require_pre_copy_evidence_cohort(Some(&unsupported)),
        Err("pre-copy-evidence-cohort-schema-unsupported".to_string())
    );
    assert_eq!(
        require_pre_copy_evidence_cohort(None),
        Err("pre-copy-evidence-cohort-unavailable".to_string())
    );
}

#[test]
fn malformed_or_incomplete_pre_copy_evidence_fails_closed() {
    let malformed = vec![
        PreCopyEvidenceObservation {
            stream: "Bad_Stream".to_string(),
            observed_at_ms: 0,
            evidence_complete: false,
            fingerprint: "not-a-fingerprint".to_string(),
        },
        PreCopyEvidenceObservation {
            stream: "Bad_Stream".to_string(),
            observed_at_ms: 1,
            evidence_complete: true,
            fingerprint: "d".repeat(64),
        },
    ];
    let cohort = compare_pre_copy_evidence(malformed);

    assert!(!cohort.complete);
    for expected in [
        "pre-copy-evidence-stream-name-invalid",
        "pre-copy-evidence-stream-duplicate",
        "pre-copy-evidence-observation-time-invalid",
        "pre-copy-evidence-fingerprint-invalid",
        "pre-copy-evidence-stream-incomplete-Bad_Stream",
        "pre-copy-evidence-stream-unexpected",
        "pre-copy-evidence-stream-missing-icloud-sync-health-evidence",
        "pre-copy-evidence-stream-missing-provider-client-runtime-evidence",
        "pre-copy-evidence-stream-missing-volume-pressure-evidence",
    ] {
        assert!(cohort.blockers.iter().any(|blocker| blocker == expected), "missing blocker {expected:?}: {:?}", cohort.blockers);
    }
    assert_eq!(
        require_pre_copy_evidence_cohort(Some(&cohort)),
        Err("pre-copy-evidence-cohort-blocked".to_string())
    );

    let empty = compare_pre_copy_evidence(Vec::new());
    assert!(empty
        .blockers
        .iter()
        .any(|blocker| blocker == "pre-copy-evidence-cohort-empty"));
}

#[test]
fn observation_skew_beyond_the_bounded_window_blocks_copy_authority() {
    let mut observations = complete_observations();
    observations[2].observed_at_ms = 1_000 + PRE_COPY_EVIDENCE_MAX_SKEW_MS + 1;

    let cohort = compare_pre_copy_evidence(observations);

    assert!(!cohort.complete);
    assert!(cohort
        .blockers
        .iter()
        .any(|blocker| blocker == "pre-copy-evidence-observation-time-skew"));
    assert_eq!(
        require_pre_copy_evidence_cohort(Some(&cohort)),
        Err("pre-copy-evidence-cohort-blocked".to_string())
    );
}

#[test]
fn cloud_root_readability_revalidates_real_filesystem_state() {
    let temp = tempfile::tempdir().unwrap();
    let source = temp.path().join("source");
    std::fs::create_dir(&source).unwrap();
    assert!(validate_source_root_readable(&source).is_ok());

    let file = temp.path().join("not-a-directory");
    std::fs::write(&file, b"bounded").unwrap();
    assert!(validate_source_root_readable(&file)
        .unwrap_err()
        .starts_with("source-root-not-directory:"));

    let root = CloudRoot {
        id: "local-test-root".to_string(),
        provider: CloudProvider::Icloud,
        account_scope: CloudAccountScope::Personal,
        label: "Local test root".to_string(),
        path: source.to_string_lossy().into_owned(),
        readable: true,
        access_issue: None,
    };
    assert!(validate_cloud_root_readable(&root).is_ok());

    let mut discovery_blocked = root.clone();
    discovery_blocked.readable = false;
    discovery_blocked.access_issue = Some("permission-denied".to_string());
    assert!(validate_cloud_root_readable(&discovery_blocked)
        .unwrap_err()
        .contains("permission-denied"));

    let mut disappeared = root;
    disappeared.path = temp.path().join("disappeared").to_string_lossy().into_owned();
    assert!(validate_cloud_root_readable(&disappeared)
        .unwrap_err()
        .starts_with("cloud-root-unreadable:"));
}

#[test]
fn cloud_root_matching_prefers_identity_and_has_bounded_unicode_fallback() {
    let temp = tempfile::tempdir().unwrap();
    let real = temp.path().join("real");
    std::fs::create_dir(&real).unwrap();
    assert!(cloud_root_path_matches(&real, &real));

    #[cfg(unix)]
    {
        let alias = temp.path().join("alias");
        std::os::unix::fs::symlink(&real, &alias).unwrap();
        assert!(cloud_root_path_matches(&real, &alias));
    }

    let composed = PathBuf::from("/nonexistent/caf\u{e9}");
    let decomposed = PathBuf::from("/nonexistent/cafe\u{301}");
    assert!(cloud_root_path_matches(&composed, &decomposed));
    assert!(!cloud_root_path_matches(
        Path::new("/nonexistent/alpha"),
        Path::new("/nonexistent/beta")
    ));
}
