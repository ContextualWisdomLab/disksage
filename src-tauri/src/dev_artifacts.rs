use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::scanner;

// A development tree can contain millions of generated entries. The inventory remains
// fail-closed for cleanup when this bounded metadata manifest cannot finish; it must never turn
// a partial observation into permission to move a recreated directory to the trash.
const ARTIFACT_MANIFEST_BUDGET: Duration = Duration::from_secs(3);
const ARTIFACT_MANIFEST_MAX_RECORDS: usize = 250_000;
const VSCODE_OBSOLETE_METADATA_MAX_BYTES: u64 = 1024 * 1024;
// Reversible Trash cleanup backs an interactive path, so an incomplete active-use probe must fail
// closed without inheriting the longer latency budget reserved for irreversible deletion.
const ARTIFACT_REVERSIBLE_ACTIVE_USE_TIMEOUT_MS: u64 = crate::reclaim::ACTIVE_USE_PROBE_TIMEOUT_MS;
// Recursive lsof must enumerate the artifact tree. Real Python environments exceeded the generic
// 2-second probe while completing in roughly 3 seconds, so the irreversible boundary owns a
// longer operational timeout instead of silently weakening the active-use gate.
const ARTIFACT_PERMANENT_ACTIVE_USE_TIMEOUT_MS: u64 = 30_000;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DevArtifact {
    pub path: String,
    pub kind: String,
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
    (".next", &["package.json"]),
    ("dist-electron", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    (".venv314", &["pyproject.toml", "requirements.txt", "setup.py", ".git"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]), // 마커 불필요 — 이름 자체가 파이썬 캐시
    (".mypy_cache", &[]),
    (".pytest_cache", &[]),
    (".ruff_cache", &[]),
    (".tox", &["pyproject.toml", "tox.ini", "setup.cfg"]),
    (".nox", &["pyproject.toml", "noxfile.py"]),
    (".codegraph", &[]), // 재생성 가능한 CodeGraph 인덱스
];

fn marker_exists(parent: &Path, artifact_name: &str, marker: &str) -> bool {
    let path = parent.join(marker);
    if artifact_name != ".tox" || marker != "setup.cfg" {
        return path.exists();
    }
    std::fs::metadata(&path).is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 1_048_576)
        && std::fs::read_to_string(path).is_ok_and(|text| {
            text.lines()
                .any(|line| line.trim().eq_ignore_ascii_case("[tox:tox]"))
        })
}

fn is_python_314_environment(path: &Path) -> bool {
    let config = path.join("pyvenv.cfg");
    std::fs::metadata(&config)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 65_536)
        && std::fs::read_to_string(config).is_ok_and(|text| {
            text.lines().any(|line| {
                line.split_once('=').is_some_and(|(key, value)| {
                    let key = key.trim();
                    (key.eq_ignore_ascii_case("version")
                        || key.eq_ignore_ascii_case("version_info"))
                        && value
                            .trim()
                            .strip_prefix("3.14")
                            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
                })
            })
        })
}

fn artifact_kind(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    ARTIFACT_KINDS.iter().find(|(k, _)| *k == name)
}

fn cargo_target_cache(path: &Path) -> bool {
    std::fs::read_to_string(path.join("CACHEDIR.TAG")).is_ok_and(|tag| {
        tag.starts_with("Signature: 8a477f597d28d172789f06886806bc55\n")
            && tag.contains("cache directory tag created by cargo")
    }) && path.join(".rustc_info.json").is_file()
        && path.join("debug").is_dir()
}

fn detected_artifact_kind(path: &Path, name: &str) -> Option<(&'static str, &'static [&'static str])> {
    artifact_kind(name)
        .map(|(kind, markers)| (*kind, *markers))
        .or_else(|| cargo_target_cache(path).then_some(("cargo-target-cache", &[])))
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

/// Build a bounded, deterministic metadata-only manifest for one generated directory.
///
/// Paths, kinds, sizes, mtimes, and symlink targets are enough to detect a stale selection while
/// avoiding sensitive content reads. A time/record bound makes the cleanup gate fail closed on
/// unusually large trees instead of blocking the UI indefinitely.
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
            manifest
                .records
                .push(format!("D\0{relative}\0{identity}\0{modified}"));
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
        manifest
            .records
            .push("!incomplete\0bounded-artifact-manifest".into());
    }
    manifest.records.sort_unstable();
    manifest.fingerprint = metadata_fingerprint(&manifest.records);
    manifest
}

fn modified_stamp(metadata: &std::fs::Metadata) -> Option<String> {
    let duration = metadata
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?;
    Some(format!(
        "{}:{}",
        duration.as_secs(),
        duration.subsec_nanos()
    ))
}

fn metadata_fingerprint(records: &[String]) -> String {
    let mut hasher = blake3::Hasher::new();
    for record in records {
        hasher.update(&(record.len() as u64).to_le_bytes());
        hasher.update(record.as_bytes());
    }
    hasher.finalize().to_hex().to_string()
}

fn editor_product(root_name: &str) -> Option<&'static str> {
    match root_name {
        ".vscode" => Some("Visual Studio Code"),
        ".vscode-insiders" => Some("Visual Studio Code Insiders"),
        ".vscode-server" => Some("Visual Studio Code Server"),
        ".cursor" => Some("Cursor"),
        _ => None,
    }
}

fn editor_product_for_extensions_dir(extensions: &Path) -> Option<&'static str> {
    if extensions.file_name().and_then(|name| name.to_str()) != Some("extensions") {
        return None;
    }
    let parent = extensions.parent()?;
    let editor_root = if parent.file_name().and_then(|name| name.to_str()) == Some("data") {
        parent.parent()?
    } else {
        parent
    };
    editor_root
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(editor_product)
}

fn is_editor_extension_directory(path: &Path) -> bool {
    path.parent()
        .and_then(editor_product_for_extensions_dir)
        .is_some()
}

/// Return extension directories that VS Code itself marked obsolete.
///
/// `.obsolete` is native lifecycle authority, so no version-age heuristic is needed. Only a real
/// metadata file at `.vscode/extensions/.obsolete` and single-component real child directories are
/// accepted.
fn vscode_obsolete_extension_paths(metadata_path: &Path) -> Vec<(PathBuf, &'static str)> {
    let mut paths = Vec::new();
    let Some(extensions) = metadata_path.parent() else {
        return paths;
    };
    let Some(product) = editor_product_for_extensions_dir(extensions) else {
        return paths;
    };
    let Ok(metadata) = std::fs::symlink_metadata(metadata_path) else {
        return paths;
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > VSCODE_OBSOLETE_METADATA_MAX_BYTES
    {
        return paths;
    }
    let Ok(bytes) = std::fs::read(metadata_path) else {
        return paths;
    };
    let Ok(document) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
        return paths;
    };
    let Some(names) = document.as_object() else {
        return paths;
    };
    for (name, obsolete) in names {
        if obsolete.as_bool() != Some(true) {
            continue;
        }
        let mut components = Path::new(name).components();
        let Some(std::path::Component::Normal(component)) = components.next() else {
            continue;
        };
        if components.next().is_some() || component.is_empty() {
            continue;
        }
        let candidate = extensions.join(component);
        let Ok(candidate_metadata) = std::fs::symlink_metadata(&candidate) else {
            continue;
        };
        if candidate_metadata.is_dir() && !candidate_metadata.file_type().is_symlink() {
            paths.push((candidate, product));
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// 마커 인접 아티팩트 디렉토리를 찾아 mtime 나이로 걸러 크기 내림차순으로 반환.
///
/// WalkDir의 부모 우선 순회를 이용해 검증된 아티팩트 아래는 즉시 건너뛴다. 생성물
/// 내부의 중첩 `node_modules`까지 다시 훑지 않으므로 큰 개발 트리에서도 같은 바이트를
/// 탐색 단계와 manifest 단계에서 두 번 읽지 않는다.
pub fn find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut obsolete_extensions = Vec::new();
    let mut walker = walkdir::WalkDir::new(root).follow_links(false).into_iter();

    while let Some(entry) = walker.next() {
        let Ok(e) = entry else { continue };
        if crate::safety::is_explicitly_protected(e.path()) {
            if e.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if e.depth() > 0 && !scanner::keep_entry(&e) {
            if e.file_type().is_dir() {
                walker.skip_current_dir();
            }
            continue;
        }
        if e.file_type().is_file() && e.file_name() == ".obsolete" {
            obsolete_extensions.extend(vscode_obsolete_extension_paths(e.path()));
            continue;
        }
        if !e.file_type().is_dir() {
            continue;
        }
        let path = e.path();
        if is_editor_extension_directory(path) {
            walker.skip_current_dir();
            continue;
        }
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        let Some((_, markers)) = detected_artifact_kind(path, &name) else {
            continue;
        };
        let parent = path.parent().unwrap_or(root);
        let marker_ok = markers.is_empty()
            || markers
                .iter()
                .any(|marker| marker_exists(parent, &name, marker));
        if name == ".venv314" && (!marker_ok || !is_python_314_environment(path)) {
            walker.skip_current_dir();
            continue;
        }
        if marker_ok {
            candidates.push(path.to_path_buf());
            walker.skip_current_dir();
        }
    }

    obsolete_extensions.sort();
    obsolete_extensions.dedup();
    let mut found: Vec<DevArtifact> = candidates
        .iter()
        .map(PathBuf::as_path)
        .filter(|path| {
            !obsolete_extensions
                .iter()
                .any(|(obsolete, _)| path.starts_with(obsolete))
        })
        .filter_map(|path| {
            let age = if now_ms == u64::MAX {
                u64::MAX
            } else {
                age_days(path, now_ms)
            };
            if age < min_age_days {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (kind, _) = detected_artifact_kind(path, &name)?;
            let parent = path.parent().unwrap_or(root);
            let manifest = artifact_manifest(path);
            Some(DevArtifact {
                path: path.to_string_lossy().into_owned(),
                kind: kind.to_string(),
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
        .collect();

    found.extend(
        obsolete_extensions
            .into_iter()
            .filter_map(|(path, product)| {
                let age = if now_ms == u64::MAX {
                    u64::MAX
                } else {
                    age_days(&path, now_ms)
                };
                if age < min_age_days {
                    return None;
                }
                let manifest = artifact_manifest(&path);
                Some(DevArtifact {
                    path: path.to_string_lossy().into_owned(),
                    kind: "vscode-obsolete-extension".into(),
                    project: product.into(),
                    bytes: manifest.bytes,
                    files: manifest.files,
                    skipped: manifest.skipped,
                    scan_complete: manifest.scan_complete,
                    fingerprint: manifest.fingerprint,
                    object_id: manifest.object_id,
                    age_days: if age == u64::MAX { 0 } else { age },
                })
            }),
    );

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
}

/// Re-scan and move only unchanged development artifacts to OS Trash.
///
/// The request manifest is deliberately compared against a fresh bounded scan. A path match is
/// not sufficient because a recreated `target` or `node_modules` directory could otherwise cause
/// an unrelated artifact to be removed.
pub fn clean_artifacts(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_disposition(requests, root, min_age_days, journal_path, now_ms, false)
}

/// Permanently delete only unchanged, inactive development artifacts after an explicit caller
/// approval. This provides physical reclaim without requiring a global Trash-empty operation.
pub fn permanently_delete_artifacts(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_disposition(requests, root, min_age_days, journal_path, now_ms, true)
}

fn artifact_active_use_timeout_ms(permanent: bool) -> u64 {
    if permanent {
        ARTIFACT_PERMANENT_ACTIVE_USE_TIMEOUT_MS
    } else {
        ARTIFACT_REVERSIBLE_ACTIVE_USE_TIMEOUT_MS
    }
}

fn clean_artifacts_with_disposition(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    permanent: bool,
) -> Vec<DevArtifactCleanResult> {
    let current = find_artifacts(root, min_age_days, now_ms);
    requests
        .iter()
        .map(|request| {
            let matches = current.iter().find(|candidate| {
                candidate.path == request.path
                    && candidate.kind == request.kind
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
                    path: request.path.clone(),
                    ok: false,
                    error: "development artifact changed or its bounded manifest is incomplete; rescan before cleanup".into(),
                };
            }

            let active_use = crate::git_worktree::active_use_evidence(
                Path::new(&request.path),
                artifact_active_use_timeout_ms(permanent),
                crate::reclaim::ACTIVE_USE_PROBE_MAX_PIDS,
                true,
            );
            if !active_use.assessed
                || !active_use.evidence_complete
                || active_use.error.is_some()
                || active_use.results_truncated
            {
                return DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: false,
                    error: "development artifact active-use evidence incomplete; rescan before cleanup".into(),
                };
            }
            if active_use.active {
                return DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: false,
                    error: "development artifact is active; close the using process before cleanup".into(),
                };
            }

            let mutation = if permanent {
                crate::safety::permanent_delete_dir_if_identity(
                    Path::new(&request.path),
                    &request.object_id,
                    request.bytes,
                    journal_path,
                    now_ms,
                )
            } else {
                crate::safety::trash_delete_if_identity(
                    Path::new(&request.path),
                    &request.object_id,
                    request.bytes,
                    journal_path,
                    now_ms,
                )
            };
            match mutation {
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
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project(
        root: &std::path::Path,
        name: &str,
        marker: &str,
        artifact: &str,
    ) -> std::path::PathBuf {
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
        // 마커 없는 가짜 — 탐지되면 안 됨
        let orphan = tmp.path().join("random").join("node_modules");
        fs::create_dir_all(&orphan).unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        let kinds: Vec<&str> = found.iter().map(|a| a.kind.as_str()).collect();
        assert!(kinds.contains(&"node_modules"));
        assert!(kinds.contains(&"target"));
        assert!(
            !found.iter().any(|a| a.path.contains("random")),
            "마커 없는 아티팩트는 제외"
        );
        let nm = found.iter().find(|a| a.kind == "node_modules").unwrap();
        assert_eq!(nm.project, "webapp");
        assert_eq!(nm.bytes, 256);
        assert_eq!(nm.age_days, 0, "sentinel now_ms는 age_days 0으로 보고");
    }

    #[test]
    fn finds_only_explicit_javascript_build_outputs() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [".next", "dist-electron"] {
            project(tmp.path(), name, "package.json", name);
        }
        let generic_project = tmp.path().join("generic");
        fs::create_dir_all(generic_project.join(".build")).unwrap();
        fs::write(generic_project.join("package.json"), b"{}").unwrap();
        fs::write(generic_project.join(".build/customer-data.bin"), b"owned").unwrap();
        fs::create_dir_all(tmp.path().join("unowned/.next")).unwrap();
        let found = find_artifacts(tmp.path(), 0, u64::MAX);
        for name in [".next", "dist-electron"] {
            assert!(found.iter().any(|artifact| artifact.kind == name));
        }
        assert!(!found.iter().any(|artifact| artifact.kind == ".build"));
        assert!(!found.iter().any(|artifact| artifact.path.contains("unowned")));
    }

    #[test]
    fn finds_regenerable_codegraph_indexes() {
        let tmp = tempfile::tempdir().unwrap();
        let index = tmp.path().join("repo/.codegraph");
        fs::create_dir_all(&index).unwrap();
        fs::write(index.join("db"), b"generated").unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        assert!(found.iter().any(|artifact| {
            artifact.kind == ".codegraph" && artifact.path == index.to_string_lossy()
        }));
    }

    #[cfg(unix)]
    #[test]
    fn finds_only_native_marked_real_vscode_extension_directories() {
        use std::os::unix::fs::symlink;

        let tmp = tempfile::tempdir().unwrap();
        let extensions = tmp.path().join(".vscode/extensions");
        let obsolete = extensions.join("publisher.tool-1.0.0");
        let retained = extensions.join("publisher.keep-1.0.0");
        let server_extensions = tmp.path().join(".vscode-server/data/extensions");
        let server_obsolete = server_extensions.join("publisher.server-1.0.0");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&obsolete).unwrap();
        fs::create_dir(&retained).unwrap();
        fs::create_dir_all(&server_obsolete).unwrap();
        fs::create_dir(&outside).unwrap();
        symlink(&outside, extensions.join("linked-1.0.0")).unwrap();
        fs::write(obsolete.join("package.json"), b"{}").unwrap();
        fs::write(
            extensions.join(".obsolete"),
            br#"{"publisher.tool-1.0.0":true,"publisher.keep-1.0.0":false,"../outside":true,"linked-1.0.0":true}"#,
        )
        .unwrap();
        fs::write(
            server_extensions.join(".obsolete"),
            br#"{"publisher.server-1.0.0":true}"#,
        )
        .unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        assert_eq!(found.len(), 2);
        assert!(found
            .iter()
            .all(|item| item.kind == "vscode-obsolete-extension"));
        assert!(found
            .iter()
            .any(|item| item.path == obsolete.to_string_lossy()));
        assert!(found
            .iter()
            .any(|item| item.path == server_obsolete.to_string_lossy()));
        assert_eq!(editor_product(".cursor"), Some("Cursor"));
        assert_eq!(editor_product(".unknown-editor"), None);
    }

    #[test]
    fn respects_min_age() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "fresh", "package.json", "node_modules");
        // 방금 만든 것: min_age_days=30이면 제외 (now = 실제 현재로는 나이가 0)
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;
        assert!(find_artifacts(tmp.path(), 30, now_ms).is_empty());
        // min_age_days=0이면 포함
        assert_eq!(find_artifacts(tmp.path(), 0, now_ms).len(), 1);
    }

    #[test]
    fn artifacts_inside_artifacts_are_not_double_counted() {
        let tmp = tempfile::tempdir().unwrap();
        let nm = project(tmp.path(), "app", "package.json", "node_modules");
        // node_modules 내부의 중첩 node_modules — 별도 항목이면 안 됨
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
        assert!(
            !journal.exists(),
            "stale identity must not create a journal"
        );
    }

    #[cfg(unix)]
    #[test]
    fn permanent_cleanup_physically_removes_an_unchanged_inactive_artifact() {
        let tmp = tempfile::tempdir().unwrap();
        let artifact = project(tmp.path(), "app", "package.json", "node_modules");
        let candidates = find_artifacts(tmp.path(), 0, u64::MAX);
        let journal = tmp.path().join("journal.jsonl");

        let results = permanently_delete_artifacts(&candidates, tmp.path(), 0, &journal, 1);

        assert_eq!(results.len(), 1);
        assert!(results[0].ok, "{}", results[0].error);
        assert!(!artifact.exists());
        assert_eq!(
            crate::safety::journal_recent(&journal, 1)[0].op,
            "permanent_generated_directory_delete"
        );
    }

    #[test]
    fn discovers_regenerable_python_tool_caches() {
        let tmp = tempfile::tempdir().unwrap();
        for name in [".mypy_cache", ".pytest_cache", ".ruff_cache"] {
            let path = tmp.path().join(name);
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("cache.bin"), b"cache").unwrap();
        }
        let mut kinds = find_artifacts(tmp.path(), 0, u64::MAX)
            .into_iter()
            .map(|artifact| artifact.kind)
            .collect::<Vec<_>>();
        kinds.sort();
        assert_eq!(kinds, [".mypy_cache", ".pytest_cache", ".ruff_cache"]);
    }

    #[test]
    fn discovers_marker_gated_python_tool_environments() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("setup.cfg"), "[tox:tox]").unwrap();
        fs::create_dir(tmp.path().join(".tox")).unwrap();
        fs::write(tmp.path().join("noxfile.py"), "").unwrap();
        fs::create_dir(tmp.path().join(".nox")).unwrap();

        let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

        assert!(artifacts.iter().any(|artifact| artifact.kind == ".tox"));
        assert!(artifacts.iter().any(|artifact| artifact.kind == ".nox"));
    }

    #[test]
    fn ignores_tox_directory_when_setup_cfg_has_no_tox_section() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("setup.cfg"), "[metadata]").unwrap();
        fs::create_dir(tmp.path().join(".tox")).unwrap();

        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());
    }

    #[test]
    fn discovers_python_314_project_environment() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".git"), "gitdir: /private/fixture").unwrap();
        fs::create_dir(tmp.path().join(".venv314")).unwrap();
        fs::write(tmp.path().join(".venv314/pyvenv.cfg"), "version = 3.14.0").unwrap();

        let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, ".venv314");
    }

    #[test]
    fn discovers_standalone_cargo_target_cache_by_native_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("wardnet-pr95-target");
        fs::create_dir_all(target.join("debug")).unwrap();
        fs::write(
            target.join("CACHEDIR.TAG"),
            "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by cargo.\n",
        )
        .unwrap();
        fs::write(target.join(".rustc_info.json"), "{}").unwrap();

        let artifacts = find_artifacts(tmp.path(), 0, u64::MAX);

        assert_eq!(artifacts.len(), 1);
        assert_eq!(artifacts[0].kind, "cargo-target-cache");
    }

    #[test]
    fn ignores_named_python_314_directory_without_matching_environment_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join(".git"), "gitdir: /private/fixture").unwrap();
        fs::create_dir(tmp.path().join(".venv314")).unwrap();
        fs::write(tmp.path().join(".venv314/pyvenv.cfg"), "version = 3.13.9").unwrap();

        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());

        fs::write(tmp.path().join(".venv314/pyvenv.cfg"), "version = 3.140.0").unwrap();
        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());

        fs::remove_file(tmp.path().join(".git")).unwrap();
        fs::write(tmp.path().join("pyproject.toml"), "[project]").unwrap();
        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());
    }
}
