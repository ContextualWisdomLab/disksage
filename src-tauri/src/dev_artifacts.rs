use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::scanner;

// A development tree can contain millions of generated entries. The inventory remains
// fail-closed for cleanup when this bounded metadata manifest cannot finish; it must never turn
// a partial observation into permission to move a recreated directory to the trash.
const ARTIFACT_MANIFEST_BUDGET: Duration = Duration::from_secs(3);
const ARTIFACT_MANIFEST_MAX_RECORDS: usize = 250_000;
const ARTIFACT_ACTIVE_USE_TIMEOUT_MS: u64 = 10_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevArtifact {
    pub path: String,
    pub kind: String,
    pub ontology_class: String,
    pub project: String,
    pub bytes: u64,
    pub files: u64,
    pub skipped: u64,
    pub scan_complete: bool,
    /// Deterministic metadata manifest; file contents are never read.
    pub fingerprint: String,
    /// Platform filesystem identity of the candidate root; unlike a path it cannot be reused by
    /// a recreated directory on Unix/Windows.
    pub object_id: String,
    pub age_days: u64,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevArtifactCleanResult {
    pub path: String,
    pub ok: bool,
    pub error: String,
}

/// (아티팩트 디렉토리명, 같은 부모에 있어야 하는 프로젝트 마커들)
const ARTIFACT_KINDS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]),
    (".codegraph", &[]),
];

fn artifact_kind(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    ARTIFACT_KINDS.iter().find(|(k, _)| *k == name)
}

fn ontology_class(kind: &str) -> &'static str {
    match kind {
        "target" => "https://disksage.app/ontology#RustBuildArtifact",
        "node_modules" | ".venv" | "venv" | "__pycache__" => {
            "https://disksage.app/ontology#BuildArtifact"
        }
        ".codegraph" => "https://disksage.app/ontology#CodeIndexArtifact",
        _ => "https://disksage.app/ontology#RegenerableArtifact",
    }
}

fn active_use_blocker(
    evidence: &crate::git_worktree::GitWorktreeActiveUseEvidence,
) -> Option<&'static str> {
    if !evidence.assessed || !evidence.evidence_complete {
        Some("development-artifact-active-use-evidence-incomplete")
    } else if evidence.active {
        Some("development-artifact-active-use-detected")
    } else {
        None
    }
}

fn age_days(path: &Path, now_ms: u64) -> u64 {
    let Ok(md) = path.metadata() else { return 0 };
    let Ok(mtime) = md.modified() else { return 0 };
    let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) else {
        return 0;
    };
    let mtime_ms = dur.as_millis() as u64;
    now_ms.saturating_sub(mtime_ms) / 86_400_000
}

#[derive(Default)]
struct ArtifactManifest {
    bytes: u64,
    files: u64,
    skipped: u64,
    scan_complete: bool,
    records: Vec<String>,
    fingerprint: String,
    object_id: String,
}

fn artifact_manifest(root: &Path) -> ArtifactManifest {
    let mut manifest = ArtifactManifest {
        scan_complete: true,
        ..ArtifactManifest::default()
    };
    let root_object_id = crate::safety::filesystem_object_id(root).ok();
    if root_object_id.is_none() {
        manifest.scan_complete = false;
    }
    manifest.object_id = root_object_id.unwrap_or_default();
    let deadline = Instant::now() + ARTIFACT_MANIFEST_BUDGET;
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| entry.depth() == 0 || scanner::keep_entry(entry));

    for entry in walker {
        if Instant::now() >= deadline || manifest.records.len() >= ARTIFACT_MANIFEST_MAX_RECORDS {
            manifest.scan_complete = false;
            break;
        }
        let Ok(entry) = entry else {
            manifest.skipped = manifest.skipped.saturating_add(1);
            manifest.scan_complete = false;
            continue;
        };
        let entry_path = entry.path();
        let relative = entry_path
            .strip_prefix(root)
            .unwrap_or(entry_path)
            .to_string_lossy()
            .replace('\\', "/");
        let relative = if relative.is_empty() { "." } else { &relative };
        let file_type = entry.file_type();
        if file_type.is_dir() {
            let Ok(metadata) = entry.metadata() else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let identity = crate::safety::filesystem_object_id(&entry_path).unwrap_or_else(|_| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            manifest.records.push(format!("D\0{relative}\0{identity}\0{modified}"));
        } else if file_type.is_file() {
            let Ok(metadata) = entry.metadata() else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let identity = crate::safety::filesystem_object_id(&entry_path).unwrap_or_else(|_| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            manifest.bytes = manifest.bytes.saturating_add(metadata.len());
            manifest.files = manifest.files.saturating_add(1);
            manifest.records.push(format!(
                "F\0{relative}\0{identity}\0{}\0{modified}",
                metadata.len()
            ));
        }
    }

    if !manifest.scan_complete {
        manifest.records.push("!incomplete\0bounded-artifact-manifest".into());
    }
    manifest.records.sort_unstable();
    manifest.fingerprint = metadata_fingerprint(&manifest.records);
    manifest
}

fn modified_stamp(metadata: &std::fs::Metadata) -> Option<String> {
    let duration = metadata.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
    Some(format!("{}:{}", duration.as_secs(), duration.subsec_nanos()))
}

fn metadata_fingerprint(records: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for record in records {
        hasher.update(&(record.len() as u64).to_le_bytes());
        hasher.update(record.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn discover_root_candidate(root: &Path) -> Option<PathBuf> {
    let entry = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(0)
        .into_iter()
        .next()?
        .ok()?;
    if !entry.file_type().is_dir() || !scanner::keep_entry(&entry) {
        return None;
    }
    let path = entry.path();
    let name = path.file_name()?.to_string_lossy().into_owned();
    let (_, markers) = artifact_kind(&name)?;
    let parent = path.parent().unwrap_or(root);
    (markers.is_empty() || markers.iter().any(|marker| parent.join(marker).exists()))
        .then(|| path.to_path_buf())
}

fn discover_candidates(root: &Path) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut walker = walkdir::WalkDir::new(root).follow_links(false).into_iter();
    while let Some(entry) = walker.next() {
        let Ok(entry) = entry else { continue };
        if !scanner::keep_entry(&entry) {
            if entry.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if !entry.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        let Some(name) = path.file_name().map(|name| name.to_string_lossy().into_owned()) else {
            continue;
        };
        let Some((_, markers)) = artifact_kind(&name) else {
            continue;
        };
        let parent = path.parent().unwrap_or(root);
        if markers.is_empty() || markers.iter().any(|marker| parent.join(marker).exists()) {
            candidates.push(path.to_path_buf());
            walker.skip_current_dir();
        }
    }
    candidates
}

pub fn find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact> {
    let worker_count = std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .saturating_mul(4)
        .min(32);
    let root_candidate = discover_root_candidate(root);
    let children = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            (file_type.is_dir() && !file_type.is_symlink()).then(|| entry.path())
        })
        .collect();
    let mut candidates: Vec<PathBuf> =
        crate::stale_git_clone::bounded_parallel_map(children, worker_count, |path| {
            discover_candidates(&path)
        })
        .into_iter()
        .flatten()
        .collect();
    if let Some(root_candidate) = root_candidate {
        candidates.push(root_candidate);
    }

    let top_level: Vec<&Path> = candidates
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            !candidates
                .iter()
                .enumerate()
                .any(|(j, other)| *i != j && p.starts_with(other))
        })
        .map(|(_, p)| p.as_path())
        .collect();

    let paths = top_level.into_iter().map(Path::to_path_buf).collect();
    let root = root.to_path_buf();
    let mut found: Vec<DevArtifact> =
        crate::stale_git_clone::bounded_parallel_map(paths, worker_count, move |path| {
            let age = if now_ms == u64::MAX { u64::MAX } else { age_days(&path, now_ms) };
            if age < min_age_days {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (kind, _) = artifact_kind(&name)?;
            let parent = path.parent().unwrap_or(&root);
            let manifest = artifact_manifest(&path);
            Some(DevArtifact {
                path: path.to_string_lossy().into_owned(),
                kind: kind.to_string(),
                ontology_class: ontology_class(kind).into(),
                project: parent
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                bytes: manifest.bytes,
                files: manifest.files,
                skipped: manifest.skipped,
                scan_complete: manifest.scan_complete,
                fingerprint: manifest.fingerprint,
                object_id: manifest.object_id,
                age_days: if age == u64::MAX { 0 } else { age },
            })
        })
        .into_iter()
        .flatten()
        .collect();

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
}

pub fn clean_artifacts(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<DevArtifactCleanResult> {
    let mut seen_paths = std::collections::BTreeSet::new();
    if requests
        .iter()
        .any(|request| !seen_paths.insert(request.path.as_str()))
    {
        return requests
            .iter()
            .map(|request| DevArtifactCleanResult {
                path: request.path.clone(),
                ok: false,
                error: "duplicate development artifact cleanup request path; rescan before cleanup".into(),
            })
            .collect();
    }

    let current = find_artifacts(root, min_age_days, now_ms);
    let active_use = crate::stale_git_clone::bounded_parallel_map(
        requests.iter().map(|request| PathBuf::from(&request.path)).collect(),
        8,
        |path| {
            let evidence = crate::git_worktree::active_use_evidence(
                &path,
                ARTIFACT_ACTIVE_USE_TIMEOUT_MS,
                crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
                true,
            );
            (path, active_use_blocker(&evidence))
        },
    )
    .into_iter()
    .collect::<std::collections::BTreeMap<_, _>>();
    let current = std::sync::Arc::new(current);
    let active_use = std::sync::Arc::new(active_use);
    let requests = std::sync::Arc::new(
        requests
            .iter()
            .cloned()
            .map(|request| (PathBuf::from(&request.path), request))
            .collect::<std::collections::BTreeMap<_, _>>(),
    );
    let paths = requests.keys().cloned().collect();
    let journal_path = journal_path.to_path_buf();
    crate::stale_git_clone::bounded_parallel_map(paths, 8, move |path| {
        let current = std::sync::Arc::clone(&current);
        let active_use = std::sync::Arc::clone(&active_use);
        let request = requests
            .get(&path)
            .expect("cleanup request path must remain indexed")
            .clone();
        let matches = current.iter().find(|candidate| {
            candidate.path == request.path
                && candidate.kind == request.kind
                && candidate.ontology_class == request.ontology_class
                && candidate.project == request.project
                && candidate.bytes == request.bytes
                && candidate.files == request.files
                && candidate.skipped == request.skipped
                && candidate.scan_complete
                && request.scan_complete
                && request.skipped == 0
                && candidate.fingerprint == request.fingerprint
                && !request.object_id.is_empty()
                && candidate.object_id == request.object_id
                && candidate.age_days >= request.age_days
        });

        if matches.is_none() {
            return DevArtifactCleanResult {
                path: request.path,
                ok: false,
                error: "development artifact changed or its bounded manifest is incomplete; rescan before cleanup".into(),
            };
        }

        if let Some(blocker) = active_use.get(Path::new(&request.path)).copied().flatten() {
            return DevArtifactCleanResult {
                path: request.path,
                ok: false,
                error: blocker.into(),
            };
        }

        match crate::safety::trash_delete_if_identity(
            Path::new(&request.path),
            &request.object_id,
            request.bytes,
            &journal_path,
            now_ms,
        ) {
            Ok(()) => DevArtifactCleanResult {
                path: request.path.clone(),
                ok: true,
                error: String::new(),
            },
            Err(error) => DevArtifactCleanResult {
                path: request.path.clone(),
                ok: false,
                error: error.to_string(),
            },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(root: &std::path::Path, name: &str, marker: &str, artifact: &str) -> std::path::PathBuf {
        let p = root.join(name);
        fs::create_dir_all(&p).unwrap();
        fs::write(p.join(marker), b"{}").unwrap();
        let a = p.join(artifact);
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("payload.bin"), vec![0u8; 256]).unwrap();
        a
    }

    #[test]
    fn finds_marker_adjacent_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "webapp", "package.json", "node_modules");
        project(tmp.path(), "cli", "Cargo.toml", "target");
        let orphan = tmp.path().join("random").join("node_modules");
        fs::create_dir_all(&orphan).unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);
        let kinds: Vec<&str> = found.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"node_modules"));
        assert!(kinds.contains(&"target"));
        assert!(!found.iter().any(|a| a.path.contains("random")));
    }

    #[test]
    fn finds_regenerable_codegraph_indexes() {
        let tmp = tempfile::tempdir().unwrap();
        let index = tmp.path().join("repo/.codegraph");
        fs::create_dir_all(&index).unwrap();
        fs::write(index.join("db"), b"generated").unwrap();
        let found = find_artifacts(tmp.path(), 0, u64::MAX);
        assert!(found.iter().any(|artifact| artifact.kind == ".codegraph" && artifact.path == index.to_string_lossy()));
    }

    #[test]
    fn respects_min_age() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "fresh", "package.json", "node_modules");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(find_artifacts(tmp.path(), 30, now_ms).is_empty());
        assert_eq!(find_artifacts(tmp.path(), 0, now_ms).len(), 1);
    }

    #[test]
    fn artifacts_inside_artifacts_are_not_double_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = project(tmp.path(), "app", "package.json", "node_modules");
        let nested = nm.join("dep").join("node_modules");
        fs::create_dir_all(&nested).unwrap();
        fs::write(nm.join("dep").join("package.json"), b"{}").unwrap();
        assert_eq!(find_artifacts(tmp.path(), 0, u64::MAX).len(), 1);
    }

    #[test]
    fn cleanup_fails_closed_when_artifact_identity_changes() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "app", "package.json", "node_modules");
        let candidates = find_artifacts(tmp.path(), 0, u64::MAX);
        assert_eq!(candidates.len(), 1);
        let journal = tmp.path().join("journal.jsonl");
        let original = tmp.path().join("original-node-modules");
        let live = tmp.path().join("app/node_modules");
        std::fs::rename(&live, &original).unwrap();
        std::fs::create_dir(&live).unwrap();
        std::fs::write(live.join("replacement.bin"), b"replacement").unwrap();
        let results = clean_artifacts(&candidates, tmp.path(), 0, &journal, 1);
        assert_eq!(results.len(), 1);
        assert!(!results[0].ok);
        assert!(results[0].error.contains("changed"));
        assert!(live.exists());
        assert!(original.exists());
        assert!(!journal.exists());
    }

    #[test]
    fn active_or_incomplete_use_evidence_blocks_cleanup() {
        let mut evidence = crate::git_worktree::GitWorktreeActiveUseEvidence {
            method: "test".into(),
            assessed: true,
            evidence_complete: false,
            active: false,
            observed_pids: Vec::new(),
            results_truncated: false,
            error: None,
        };
        assert_eq!(active_use_blocker(&evidence), Some("development-artifact-active-use-evidence-incomplete"));
        evidence.evidence_complete = true;
        evidence.active = true;
        assert_eq!(active_use_blocker(&evidence), Some("development-artifact-active-use-detected"));
        evidence.active = false;
        assert_eq!(active_use_blocker(&evidence), None);
    }
}
