use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

pub const MAVEN_CACHE_AUDIT_SCHEMA_KIND: &str = "disksage.maven-cache-audit/v1";
pub const MAVEN_CACHE_ONTOLOGY_CLASS: &str = "https://disksage.app/ontology#PackageCacheArtifact";

#[derive(Debug, Clone, Copy)]
pub struct MavenCacheAuditOptions {
    pub max_entries: u64,
    pub max_candidates: usize,
    pub max_issues: usize,
}

impl Default for MavenCacheAuditOptions {
    fn default() -> Self {
        Self {
            max_entries: 2_000_000,
            max_candidates: 500,
            max_issues: 200,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MavenCacheCandidate {
    pub ontology_class: String,
    pub relative_path: String,
    pub bytes: u64,
    pub artifact_files: u64,
    pub repository_ids: Vec<String>,
    pub observed_oldest_modified_ms: u64,
    pub observed_latest_modified_ms: u64,
    pub candidate_fingerprint: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MavenCacheAuditIssue {
    pub relative_path: String,
    pub reason: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MavenCacheAuditReport {
    pub schema_kind: String,
    pub ontology_class: String,
    pub repository_root: String,
    pub generated_at_ms: u64,
    pub scanned_entries: u64,
    pub marker_directories: u64,
    pub remote_recoverable_directories: u64,
    pub remote_recoverable_bytes: u64,
    pub held_directories: u64,
    pub held_bytes: u64,
    pub held_reason_counts: BTreeMap<String, u64>,
    pub candidate_set_fingerprint: String,
    pub candidates: Vec<MavenCacheCandidate>,
    pub issues: Vec<MavenCacheAuditIssue>,
    pub scan_truncated: bool,
    pub candidate_output_truncated: bool,
    pub truncated: bool,
    pub provider_write_executed: bool,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MavenCachePruneReport {
    pub schema_kind: String,
    pub repository_root: String,
    pub generated_at_ms: u64,
    pub expected_candidate_set_fingerprint: String,
    pub observed_candidate_set_fingerprint: String,
    pub candidate_directories: u64,
    pub candidate_bytes: u64,
    pub removed_directories: u64,
    pub removed_bytes: u64,
    pub skipped_directories: u64,
    pub skip_reason_counts: BTreeMap<String, u64>,
    pub apply_requested: bool,
    pub filesystem_mutation_executed: bool,
    pub complete: bool,
}

#[derive(Debug)]
struct MarkerDiscovery {
    marker_directories: Vec<PathBuf>,
    scanned_entries: u64,
    truncated: bool,
}

#[derive(Debug)]
struct FileObservation {
    name: String,
    bytes: u64,
    modified_ms: u64,
}

#[derive(Debug)]
struct DirectoryAudit {
    bytes: u64,
    candidate: Option<MavenCacheCandidate>,
    held_reason: Option<String>,
    issue_reason: Option<String>,
}

fn discover_marker_directories(root: &Path, max_entries: u64) -> Result<MarkerDiscovery, String> {
    let mut stack = vec![root.to_path_buf()];
    let mut markers = Vec::new();
    let mut scanned_entries = 0u64;
    let mut truncated = false;

    while let Some(directory) = stack.pop() {
        let mut entries: Vec<_> = fs::read_dir(&directory)
            .map_err(|_| "maven-cache-directory-unreadable".to_string())?
            .collect::<Result<_, _>>()
            .map_err(|_| "maven-cache-entry-unreadable".to_string())?;
        entries.sort_by_key(|entry| entry.file_name());

        let mut child_directories = Vec::new();
        for entry in entries {
            if scanned_entries >= max_entries {
                truncated = true;
                break;
            }
            scanned_entries += 1;
            let metadata = fs::symlink_metadata(entry.path())
                .map_err(|_| "maven-cache-entry-metadata-unavailable".to_string())?;
            if metadata.file_type().is_symlink() {
                continue;
            }
            if metadata.is_dir() {
                child_directories.push(entry.path());
            } else if metadata.is_file()
                && entry.file_name().to_string_lossy() == "_remote.repositories"
            {
                if let Some(parent) = entry.path().parent() {
                    markers.push(parent.to_path_buf());
                }
            }
        }
        if truncated {
            break;
        }
        child_directories.sort();
        stack.extend(child_directories.into_iter().rev());
    }
    markers.sort();
    markers.dedup();
    Ok(MarkerDiscovery {
        marker_directories: markers,
        scanned_entries,
        truncated,
    })
}

fn relative_path(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "maven-cache-path-outside-root".to_string())?;
    let value = relative
        .to_str()
        .ok_or_else(|| "maven-cache-path-not-utf8".to_string())?;
    if relative.components().next().is_none()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err("maven-cache-relative-path-invalid".into());
    }
    Ok(value.replace('\\', "/"))
}

fn is_support_file(name: &str) -> bool {
    name == "_remote.repositories"
        || name == "resolver-status.properties"
        || name.ends_with(".lastUpdated")
        || name.ends_with(".sha1")
        || name.ends_with(".md5")
        || name.ends_with(".sha256")
        || name.ends_with(".sha512")
        || (name.starts_with("maven-metadata-") && name.ends_with(".xml"))
}

fn modified_ms(metadata: &fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .and_then(|value| u64::try_from(value.as_millis()).ok())
        .unwrap_or(0)
}

fn parse_remote_marker(
    marker: &Path,
) -> Result<(BTreeMap<String, String>, BTreeSet<String>), String> {
    let metadata = fs::symlink_metadata(marker)
        .map_err(|_| "remote-marker-metadata-unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("remote-marker-not-regular-file".into());
    }
    if metadata.len() > 1024 * 1024 {
        return Err("remote-marker-too-large".into());
    }
    let encoded = fs::read_to_string(marker).map_err(|_| "remote-marker-not-utf8".to_string())?;
    let mut attributions = BTreeMap::new();
    let mut repository_ids = BTreeSet::new();
    for raw_line in encoded.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (filename, attribution) = line
            .split_once('>')
            .ok_or_else(|| "remote-marker-line-invalid".to_string())?;
        let repository_id = attribution
            .strip_suffix('=')
            .ok_or_else(|| "remote-marker-line-invalid".to_string())?;
        let component = Path::new(filename);
        if component.components().count() != 1
            || !matches!(component.components().next(), Some(Component::Normal(_)))
            || filename.contains(['/', '\\'])
        {
            return Err("remote-marker-filename-invalid".into());
        }
        if let Some(existing) = attributions.insert(filename.to_string(), repository_id.into()) {
            if existing != repository_id {
                return Err("remote-marker-attribution-conflict".into());
            }
        }
        if !repository_id.is_empty() {
            repository_ids.insert(repository_id.to_string());
        }
    }
    if attributions.is_empty() {
        return Err("remote-marker-empty".into());
    }
    Ok((attributions, repository_ids))
}

fn candidate_fingerprint(
    relative_path: &str,
    observations: &[FileObservation],
    attributions: &BTreeMap<String, String>,
) -> String {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.maven-cache-audit\0v1\0");
    hasher.update(&(relative_path.len() as u64).to_le_bytes());
    hasher.update(relative_path.as_bytes());
    for observation in observations {
        hasher.update(&(observation.name.len() as u64).to_le_bytes());
        hasher.update(observation.name.as_bytes());
        hasher.update(&observation.bytes.to_le_bytes());
        hasher.update(&observation.modified_ms.to_le_bytes());
        let repository_id = attributions
            .get(&observation.name)
            .map(String::as_str)
            .unwrap_or("");
        hasher.update(&(repository_id.len() as u64).to_le_bytes());
        hasher.update(repository_id.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn candidate_set_fingerprint(root: &str, candidates: &[MavenCacheCandidate]) -> String {
    let mut ordered: Vec<_> = candidates.iter().collect();
    ordered.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage.maven-cache-candidate-set\0v1\0");
    hasher.update(&(root.len() as u64).to_le_bytes());
    hasher.update(root.as_bytes());
    for candidate in ordered {
        hasher.update(&(candidate.relative_path.len() as u64).to_le_bytes());
        hasher.update(candidate.relative_path.as_bytes());
        hasher.update(&candidate.bytes.to_le_bytes());
        hasher.update(candidate.candidate_fingerprint.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn audit_marker_directory(root: &Path, directory: &Path) -> DirectoryAudit {
    let relative = match relative_path(root, directory) {
        Ok(value) => value,
        Err(reason) => {
            return DirectoryAudit {
                bytes: 0,
                candidate: None,
                held_reason: Some("unsafe-relative-path".into()),
                issue_reason: Some(reason),
            };
        }
    };
    let marker = directory.join("_remote.repositories");
    let mut entries: Vec<_> = match fs::read_dir(directory) {
        Ok(entries) => match entries.collect::<Result<Vec<_>, _>>() {
            Ok(entries) => entries,
            Err(_) => {
                return DirectoryAudit {
                    bytes: 0,
                    candidate: None,
                    held_reason: Some("unreadable-version-directory".into()),
                    issue_reason: Some("maven-version-entry-unreadable".into()),
                };
            }
        },
        Err(_) => {
            return DirectoryAudit {
                bytes: 0,
                candidate: None,
                held_reason: Some("unreadable-version-directory".into()),
                issue_reason: Some("maven-version-directory-unreadable".into()),
            };
        }
    };
    entries.sort_by_key(|entry| entry.file_name());

    let mut observations = Vec::new();
    let mut regular_names = BTreeSet::new();
    let mut payload_names = Vec::new();
    let mut bytes = 0u64;
    let mut has_symlink = false;
    let mut has_nested_directory = false;
    let mut has_local_metadata = false;
    for entry in entries {
        let name = match entry.file_name().into_string() {
            Ok(value) => value,
            Err(_) => {
                return DirectoryAudit {
                    bytes,
                    candidate: None,
                    held_reason: Some("non-utf8-entry".into()),
                    issue_reason: Some("maven-version-entry-not-utf8".into()),
                };
            }
        };
        let metadata = match fs::symlink_metadata(entry.path()) {
            Ok(value) => value,
            Err(_) => {
                return DirectoryAudit {
                    bytes,
                    candidate: None,
                    held_reason: Some("unreadable-version-directory".into()),
                    issue_reason: Some("maven-version-entry-metadata-unavailable".into()),
                };
            }
        };
        if metadata.file_type().is_symlink() {
            has_symlink = true;
            continue;
        }
        if metadata.is_dir() {
            has_nested_directory = true;
            continue;
        }
        if !metadata.is_file() {
            continue;
        }
        bytes = bytes.saturating_add(metadata.len());
        regular_names.insert(name.clone());
        observations.push(FileObservation {
            name: name.clone(),
            bytes: metadata.len(),
            modified_ms: modified_ms(&metadata),
        });
        if name == "maven-metadata-local.xml" {
            has_local_metadata = true;
        }
        if !is_support_file(&name) {
            payload_names.push(name);
        }
    }

    let (attributions, repository_ids) = match parse_remote_marker(&marker) {
        Ok(value) => value,
        Err(reason) => {
            return DirectoryAudit {
                bytes,
                candidate: None,
                held_reason: Some("invalid-remote-marker".into()),
                issue_reason: Some(reason),
            };
        }
    };
    let version_name = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_uppercase();
    let held_reason = if version_name.ends_with("-SNAPSHOT") {
        Some("snapshot-version")
    } else if attributions.values().any(String::is_empty) {
        Some("local-artifact")
    } else if has_local_metadata {
        Some("local-metadata")
    } else if has_symlink {
        Some("symlink-entry")
    } else if has_nested_directory {
        Some("nested-directory")
    } else if payload_names.is_empty() {
        Some("no-artifact-payload")
    } else if payload_names
        .iter()
        .any(|name| !attributions.contains_key(name))
    {
        Some("untracked-payload")
    } else if attributions
        .keys()
        .any(|name| !regular_names.contains(name))
    {
        Some("marker-reference-missing")
    } else if repository_ids.is_empty() {
        Some("no-remote-repository")
    } else {
        None
    };

    if let Some(reason) = held_reason {
        return DirectoryAudit {
            bytes,
            candidate: None,
            held_reason: Some(reason.into()),
            issue_reason: None,
        };
    }

    observations.sort_by(|left, right| left.name.cmp(&right.name));
    let observed_oldest_modified_ms = observations
        .iter()
        .map(|observation| observation.modified_ms)
        .min()
        .unwrap_or(0);
    let observed_latest_modified_ms = observations
        .iter()
        .map(|observation| observation.modified_ms)
        .max()
        .unwrap_or(0);
    let candidate = MavenCacheCandidate {
        ontology_class: MAVEN_CACHE_ONTOLOGY_CLASS.into(),
        relative_path: relative.clone(),
        bytes,
        artifact_files: payload_names.len() as u64,
        repository_ids: repository_ids.into_iter().collect(),
        observed_oldest_modified_ms,
        observed_latest_modified_ms,
        candidate_fingerprint: candidate_fingerprint(&relative, &observations, &attributions),
    };
    DirectoryAudit {
        bytes,
        candidate: Some(candidate),
        held_reason: None,
        issue_reason: None,
    }
}

pub fn audit_maven_repository(
    root: &Path,
    options: MavenCacheAuditOptions,
    generated_at_ms: u64,
) -> Result<MavenCacheAuditReport, String> {
    let root_metadata =
        fs::symlink_metadata(root).map_err(|_| "maven-cache-root-unavailable".to_string())?;
    if root_metadata.file_type().is_symlink() || !root_metadata.is_dir() {
        return Err("maven-cache-root-not-real-directory".into());
    }
    let canonical_root =
        fs::canonicalize(root).map_err(|_| "maven-cache-root-unavailable".to_string())?;
    let root_text = canonical_root
        .to_str()
        .ok_or_else(|| "maven-cache-root-not-utf8".to_string())?
        .to_string();
    let discovery = discover_marker_directories(&canonical_root, options.max_entries)?;
    let marker_directories = discovery.marker_directories.len() as u64;
    let mut held_reason_counts = BTreeMap::new();
    let mut candidates = Vec::new();
    let mut issues = Vec::new();
    let mut remote_recoverable_directories = 0u64;
    let mut remote_recoverable_bytes = 0u64;
    let mut held_directories = 0u64;
    let mut held_bytes = 0u64;

    if discovery.truncated {
        if marker_directories > 0 {
            held_reason_counts.insert("scan-truncated".into(), marker_directories);
            held_directories = marker_directories;
        }
    } else {
        for directory in &discovery.marker_directories {
            let audit = audit_marker_directory(&canonical_root, directory);
            if let Some(candidate) = audit.candidate {
                remote_recoverable_directories = remote_recoverable_directories.saturating_add(1);
                remote_recoverable_bytes = remote_recoverable_bytes.saturating_add(candidate.bytes);
                candidates.push(candidate);
            } else {
                held_directories = held_directories.saturating_add(1);
                held_bytes = held_bytes.saturating_add(audit.bytes);
                let reason = audit
                    .held_reason
                    .unwrap_or_else(|| "unclassified".to_string());
                *held_reason_counts.entry(reason).or_insert(0) += 1;
            }
            if let Some(reason) = audit.issue_reason {
                if issues.len() < options.max_issues {
                    issues.push(MavenCacheAuditIssue {
                        relative_path: relative_path(&canonical_root, directory)
                            .unwrap_or_else(|_| "<unavailable>".into()),
                        reason,
                    });
                }
            }
        }
    }
    candidates.sort_by(|left, right| {
        right
            .bytes
            .cmp(&left.bytes)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    let candidate_set_fingerprint = candidate_set_fingerprint(&root_text, &candidates);
    let candidate_output_truncated = candidates.len() > options.max_candidates;
    candidates.truncate(options.max_candidates);

    Ok(MavenCacheAuditReport {
        schema_kind: MAVEN_CACHE_AUDIT_SCHEMA_KIND.into(),
        ontology_class: MAVEN_CACHE_ONTOLOGY_CLASS.into(),
        repository_root: root_text,
        generated_at_ms,
        scanned_entries: discovery.scanned_entries,
        marker_directories,
        remote_recoverable_directories,
        remote_recoverable_bytes,
        held_directories,
        held_bytes,
        held_reason_counts,
        candidate_set_fingerprint,
        candidates,
        issues,
        scan_truncated: discovery.truncated,
        candidate_output_truncated,
        truncated: discovery.truncated || candidate_output_truncated,
        provider_write_executed: false,
    })
}

fn valid_candidate_set_fingerprint(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

pub fn prune_maven_repository(
    root: &Path,
    expected_candidate_set_fingerprint: &str,
    apply: bool,
    max_entries: u64,
    generated_at_ms: u64,
) -> Result<MavenCachePruneReport, String> {
    if !valid_candidate_set_fingerprint(expected_candidate_set_fingerprint) {
        return Err("maven-cache-prune-expected-fingerprint-invalid".into());
    }
    if max_entries == 0 {
        return Err("maven-cache-prune-max-entries-invalid".into());
    }

    let audit = audit_maven_repository(
        root,
        MavenCacheAuditOptions {
            max_entries,
            max_candidates: usize::MAX,
            max_issues: 200,
        },
        generated_at_ms,
    )?;
    if audit.scan_truncated || audit.candidate_output_truncated || audit.truncated {
        return Err("maven-cache-prune-audit-truncated".into());
    }
    if audit.candidate_set_fingerprint != expected_candidate_set_fingerprint {
        return Err("maven-cache-prune-candidate-set-mismatch".into());
    }
    if apply {
        return Err("maven-cache-prune-identity-bound-recycle-unavailable".into());
    }

    Ok(MavenCachePruneReport {
        schema_kind: "disksage.maven-cache-prune/v1".into(),
        repository_root: audit.repository_root,
        generated_at_ms,
        expected_candidate_set_fingerprint: expected_candidate_set_fingerprint.into(),
        observed_candidate_set_fingerprint: audit.candidate_set_fingerprint,
        candidate_directories: audit.remote_recoverable_directories,
        candidate_bytes: audit.remote_recoverable_bytes,
        removed_directories: 0,
        removed_bytes: 0,
        skipped_directories: 0,
        skip_reason_counts: BTreeMap::new(),
        apply_requested: false,
        filesystem_mutation_executed: false,
        complete: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn version_dir(root: &Path, relative: &str) -> std::path::PathBuf {
        let path = root.join(relative);
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn remote_artifact(path: &Path, stem: &str, repository_id: &str, bytes: usize) {
        fs::write(path.join(format!("{stem}.jar")), vec![1u8; bytes]).unwrap();
        fs::write(path.join(format!("{stem}.pom")), b"<project/>").unwrap();
        fs::write(
            path.join("_remote.repositories"),
            format!("{stem}.jar>{repository_id}=\n{stem}.pom>{repository_id}=\n"),
        )
        .unwrap();
    }

    #[test]
    fn marks_only_fully_remote_attributed_version_directories_recoverable() {
        let tmp = tempfile::tempdir().unwrap();
        let remote = version_dir(tmp.path(), "org/example/demo/1.0.0");
        remote_artifact(&remote, "demo-1.0.0", "central", 1024);

        let report =
            audit_maven_repository(tmp.path(), MavenCacheAuditOptions::default(), 123).unwrap();

        assert_eq!(report.schema_kind, MAVEN_CACHE_AUDIT_SCHEMA_KIND);
        assert_eq!(report.marker_directories, 1);
        assert_eq!(report.remote_recoverable_directories, 1);
        assert_eq!(report.held_directories, 0);
        assert_eq!(report.candidates.len(), 1);
        assert_eq!(report.candidates[0].relative_path, "org/example/demo/1.0.0");
        assert_eq!(report.candidates[0].repository_ids, vec!["central"]);
        assert_eq!(report.candidates[0].artifact_files, 2);
        assert!(report.candidates[0].observed_oldest_modified_ms > 0);
        assert!(
            report.candidates[0].observed_latest_modified_ms
                >= report.candidates[0].observed_oldest_modified_ms
        );
        assert_eq!(report.candidates[0].candidate_fingerprint.len(), 64);
        assert_eq!(report.candidate_set_fingerprint.len(), 64);
        assert!(!report.provider_write_executed);
    }

    #[test]
    fn holds_local_installs_untracked_payloads_and_snapshots() {
        let tmp = tempfile::tempdir().unwrap();

        let local = version_dir(tmp.path(), "org/example/local/1.0.0");
        fs::write(local.join("local-1.0.0.jar"), b"local").unwrap();
        fs::write(local.join("_remote.repositories"), "local-1.0.0.jar>=\n").unwrap();

        let untracked = version_dir(tmp.path(), "org/example/mixed/1.0.0");
        remote_artifact(&untracked, "mixed-1.0.0", "central", 64);
        fs::write(untracked.join("private-classifier.jar"), b"private").unwrap();

        let snapshot = version_dir(tmp.path(), "org/example/demo/2.0-SNAPSHOT");
        remote_artifact(&snapshot, "demo-2.0-20260729.010203-1", "snapshots", 64);

        let report =
            audit_maven_repository(tmp.path(), MavenCacheAuditOptions::default(), 456).unwrap();

        assert_eq!(report.remote_recoverable_directories, 0);
        assert_eq!(report.held_directories, 3);
        assert_eq!(report.held_reason_counts["local-artifact"], 1);
        assert_eq!(report.held_reason_counts["untracked-payload"], 1);
        assert_eq!(report.held_reason_counts["snapshot-version"], 1);
        assert!(report.candidates.is_empty());
    }

    #[test]
    fn counts_regular_file_bytes_when_remote_marker_is_invalid() {
        let tmp = tempfile::tempdir().unwrap();
        let invalid = version_dir(tmp.path(), "org/example/invalid/1.0.0");
        fs::write(invalid.join("invalid-1.0.0.jar"), vec![1u8; 4096]).unwrap();
        fs::write(invalid.join("_remote.repositories"), "malformed").unwrap();
        let expected_bytes = fs::metadata(invalid.join("invalid-1.0.0.jar"))
            .unwrap()
            .len()
            + fs::metadata(invalid.join("_remote.repositories"))
                .unwrap()
                .len();

        let report =
            audit_maven_repository(tmp.path(), MavenCacheAuditOptions::default(), 456).unwrap();

        assert_eq!(report.remote_recoverable_directories, 0);
        assert_eq!(report.held_directories, 1);
        assert_eq!(report.held_bytes, expected_bytes);
        assert_eq!(report.held_reason_counts["invalid-remote-marker"], 1);
        assert_eq!(report.issues[0].reason, "remote-marker-line-invalid");
    }

    #[test]
    fn candidate_output_is_largest_first_and_bounded_without_changing_totals() {
        let tmp = tempfile::tempdir().unwrap();
        for (name, bytes) in [("small", 10usize), ("large", 100usize)] {
            let path = version_dir(tmp.path(), &format!("org/example/{name}/1.0.0"));
            remote_artifact(&path, &format!("{name}-1.0.0"), "central", bytes);
        }
        let full =
            audit_maven_repository(tmp.path(), MavenCacheAuditOptions::default(), 789).unwrap();
        let report = audit_maven_repository(
            tmp.path(),
            MavenCacheAuditOptions {
                max_candidates: 1,
                ..MavenCacheAuditOptions::default()
            },
            789,
        )
        .unwrap();

        assert_eq!(report.remote_recoverable_directories, 2);
        assert_eq!(
            report.candidate_set_fingerprint,
            full.candidate_set_fingerprint
        );
        assert_eq!(report.candidates.len(), 1);
        assert!(report.candidates[0].relative_path.contains("large"));
        assert!(!report.scan_truncated);
        assert!(report.candidate_output_truncated);
        assert!(report.truncated);
    }

    #[test]
    fn rejects_non_directory_roots_and_entry_limit_is_fail_closed() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("not-a-directory");
        fs::write(&file, b"x").unwrap();
        assert!(audit_maven_repository(&file, MavenCacheAuditOptions::default(), 0).is_err());

        let version = version_dir(tmp.path(), "org/example/demo/1.0.0");
        remote_artifact(&version, "demo-1.0.0", "central", 64);
        let report = audit_maven_repository(
            tmp.path(),
            MavenCacheAuditOptions {
                max_entries: 1,
                ..MavenCacheAuditOptions::default()
            },
            0,
        )
        .unwrap();
        assert!(report.scan_truncated);
        assert!(!report.candidate_output_truncated);
        assert!(report.truncated);
        assert_eq!(report.remote_recoverable_directories, 0);
    }

    #[test]
    fn prune_dry_run_requires_exact_fingerprint_and_does_not_mutate() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repository");
        let version = version_dir(&root, "org/example/demo/1.0.0");
        remote_artifact(&version, "demo-1.0.0", "central", 64);
        let audit = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 123).unwrap();

        let report =
            prune_maven_repository(&root, &audit.candidate_set_fingerprint, false, 10_000, 456)
                .unwrap();

        assert_eq!(report.candidate_directories, 1);
        assert_eq!(report.removed_directories, 0);
        assert!(!report.filesystem_mutation_executed);
        assert!(report.complete);
        assert!(version.exists());
        assert!(prune_maven_repository(&root, "not-a-fingerprint", false, 10_000, 456).is_err());
        assert!(prune_maven_repository(&root, &"0".repeat(64), false, 10_000, 456).is_err());
    }

    #[test]
    fn prune_apply_fails_closed_without_identity_bound_recycle() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("repository");
        let remote = version_dir(&root, "org/example/remote/1.0.0");
        remote_artifact(&remote, "remote-1.0.0", "central", 64);
        let local = version_dir(&root, "org/example/local/1.0.0");
        fs::write(local.join("local-1.0.0.jar"), b"local").unwrap();
        fs::write(local.join("_remote.repositories"), "local-1.0.0.jar>=\n").unwrap();
        let audit = audit_maven_repository(&root, MavenCacheAuditOptions::default(), 123).unwrap();

        let error =
            prune_maven_repository(&root, &audit.candidate_set_fingerprint, true, 10_000, 456)
                .unwrap_err();

        assert_eq!(
            error,
            "maven-cache-prune-identity-bound-recycle-unavailable"
        );
        assert!(remote.exists());
        assert!(local.exists());
    }
}
