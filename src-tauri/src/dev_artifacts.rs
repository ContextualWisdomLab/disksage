use std::collections::HashMap;
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
    /// Filesystem blocks currently allocated for regular files in this generated root and actually
    /// reclaimable when every hard-link name for each counted object is inside the root.
    pub allocated_bytes: u64,
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

const JAVASCRIPT_LOCKFILES: &[&str] = &[
    "package-lock.json",
    "pnpm-lock.yaml",
    "yarn.lock",
    "bun.lock",
    "bun.lockb",
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

fn project_rebuild_authority(parent: &Path, kind: &str) -> bool {
    match kind {
        "cargo-target-cache" => true,
        "target" => parent.join("Cargo.toml").is_file() && parent.join("Cargo.lock").is_file(),
        "node_modules" | ".next" | "dist-electron" => {
            parent.join("package.json").is_file()
                && JAVASCRIPT_LOCKFILES
                    .iter()
                    .any(|lockfile| parent.join(lockfile).is_file())
        }
        _ => true,
    }
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
    let tag_path = path.join("CACHEDIR.TAG");
    let tagged = std::fs::metadata(&tag_path)
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() <= 65_536)
        && std::fs::read_to_string(tag_path).is_ok_and(|tag| {
            tag.starts_with("Signature: 8a477f597d28d172789f06886806bc55\n")
                && tag.contains("cache directory tag created by cargo")
        })
        && path.join(".rustc_info.json").is_file();
    tagged && path.join("debug").is_dir()
}

fn detected_artifact_kind(path: &Path, name: &str) -> Option<(&'static str, &'static [&'static str])> {
    if cargo_target_cache(path) {
        return Some(("cargo-target-cache", &[]));
    }
    artifact_kind(name).map(|(kind, markers)| (*kind, *markers))
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
    allocated_bytes: u64,
    files: u64,
    skipped: u64,
    scan_complete: bool,
    records: Vec<String>,
    fingerprint: String,
    object_id: String,
}

#[derive(Clone, Copy)]
struct LinkAllocationEvidence {
    allocated_bytes: u64,
    total_links: u64,
    observed_links: u64,
}

#[cfg(unix)]
fn allocated_bytes(_path: &Path, metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.blocks().saturating_mul(512))
}

#[cfg(unix)]
fn hard_link_count(_path: &Path, metadata: &std::fs::Metadata) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    Some(metadata.nlink())
}

#[cfg(windows)]
fn windows_api_path(path: &Path) -> Option<Vec<u16>> {
    use std::os::windows::ffi::OsStrExt;

    const BACKSLASH: u16 = b'\\' as u16;
    const FORWARD_SLASH: u16 = b'/' as u16;
    const QUESTION: u16 = b'?' as u16;
    const DOT: u16 = b'.' as u16;
    let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return None;
    }
    let verbatim_prefix = [BACKSLASH, BACKSLASH, QUESTION, BACKSLASH];
    if wide.starts_with(&verbatim_prefix) {
        wide.push(0);
        return Some(wide);
    }
    let device_prefix = [BACKSLASH, BACKSLASH, DOT, BACKSLASH];
    if !path.is_absolute() || wide.starts_with(&device_prefix) {
        return None;
    }

    let normalized = std::path::absolute(path).ok()?;
    wide = normalized.as_os_str().encode_wide().collect();
    if wide.contains(&0) {
        return None;
    }
    for unit in &mut wide {
        if *unit == FORWARD_SLASH {
            *unit = BACKSLASH;
        }
    }
    let mut extended: Vec<u16> = if wide.starts_with(&[BACKSLASH, BACKSLASH]) {
        r"\\?\UNC\".encode_utf16().collect()
    } else {
        r"\\?\".encode_utf16().collect()
    };
    if wide.starts_with(&[BACKSLASH, BACKSLASH]) {
        extended.extend_from_slice(&wide[2..]);
    } else {
        extended.extend_from_slice(&wide);
    }
    extended.push(0);
    Some(extended)
}

#[cfg(windows)]
fn allocated_bytes(path: &Path, _metadata: &std::fs::Metadata) -> Option<u64> {
    const INVALID_FILE_SIZE: u32 = u32::MAX;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCompressedFileSizeW(file_name: *const u16, high: *mut u32) -> u32;
        fn GetLastError() -> u32;
        fn SetLastError(error: u32);
    }
    let wide = windows_api_path(path)?;
    let mut high = 0_u32;
    // SAFETY: `wide` is a live NUL-terminated UTF-16 absolute path and `high` is writable.
    let low = unsafe {
        SetLastError(0);
        GetCompressedFileSizeW(wide.as_ptr(), &mut high)
    };
    // INVALID_FILE_SIZE can also be a valid low word; GetLastError disambiguates it.
    if low == INVALID_FILE_SIZE && unsafe { GetLastError() } != 0 {
        return None;
    }
    Some((u64::from(high) << 32) | u64::from(low))
}

#[cfg(windows)]
fn hard_link_count(path: &Path, _metadata: &std::fs::Metadata) -> Option<u64> {
    let handle = winapi_util::Handle::from_path_any(path).ok()?;
    let info = winapi_util::file::information(&handle).ok()?;
    Some(info.number_of_links())
}

#[cfg(not(any(unix, windows)))]
fn allocated_bytes(_path: &Path, metadata: &std::fs::Metadata) -> Option<u64> {
    Some(metadata.len())
}

#[cfg(not(any(unix, windows)))]
fn hard_link_count(_path: &Path, _metadata: &std::fs::Metadata) -> Option<u64> {
    None
}

/// Canonicalize each discovered provider root independently so one unavailable provider cannot
/// erase otherwise valid provider boundaries.
fn canonicalize_provider_roots<I>(roots: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = PathBuf>,
{
    roots
        .into_iter()
        .map(|path| std::fs::canonicalize(&path).unwrap_or(path))
        .collect()
}

/// Establish fail-closed user-home authority, then discover the provider roots that development
/// artifact inventory must never cross. Home identity is checked both before and after
/// canonicalization so a replaced or symlinked home cannot widen deletion authority.
fn discovered_provider_roots() -> Option<Vec<PathBuf>> {
    #[cfg(windows)]
    let candidates = [
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        crate::home_resolution::windows_home_drive_path(),
    ];
    #[cfg(not(windows))]
    let candidates = [std::env::var_os("HOME").map(PathBuf::from), None];
    let home = crate::home_resolution::select_absolute_home(candidates).ok()?;
    if home
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
    {
        return None;
    }
    let metadata = std::fs::symlink_metadata(&home).ok()?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return None;
    }
    let expected_identity = crate::safety::filesystem_object_id(&home).ok()?;
    let home = std::fs::canonicalize(home).ok()?;
    if crate::safety::filesystem_object_id(&home).ok()? != expected_identity {
        return None;
    }
    Some(canonicalize_provider_roots(
        crate::cloud::discover_cloud_roots(&home)
            .into_iter()
            .map(|cloud_root| PathBuf::from(cloud_root.path)),
    ))
}

/// Reject both platform-native File Provider ancestry and any descendant of a provider root that
/// DiskSage discovered from the identity-checked home directory. Canonicalization failure is unsafe
/// here because cleanup inventory must not gain authority from an unresolved path.
fn provider_managed_ancestry(path: &Path, provider_roots: &[PathBuf]) -> bool {
    let component_blocked = path.components().any(|component| {
        matches!(component, std::path::Component::Normal(value) if value == "CloudStorage" || value == "Mobile Documents")
    });
    if component_blocked {
        return true;
    }
    let Ok(canonical_path) = std::fs::canonicalize(path) else {
        return true;
    };
    provider_roots
        .iter()
        .any(|cloud_path| canonical_path.starts_with(cloud_path))
}

/// Build a bounded, deterministic metadata-only manifest for one generated directory.
///
/// Paths, kinds, logical sizes, allocated sizes, mtimes, hard-link topology, and filesystem
/// identities detect stale or only-partly-reclaimable selections without reading file contents.
/// A time/record bound makes the cleanup gate fail closed on unusually large trees instead of
/// blocking the UI indefinitely.
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
    let mut link_allocations: HashMap<String, LinkAllocationEvidence> = HashMap::new();
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
            let Ok(identity) = crate::safety::filesystem_object_id(&entry_path) else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let modified = modified_stamp(&metadata).unwrap_or_else(|| {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                "<unknown>".into()
            });
            manifest.bytes = manifest.bytes.saturating_add(metadata.len());
            let Some(allocated) = allocated_bytes(entry_path, &metadata) else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            let Some(total_links) = hard_link_count(entry_path, &metadata) else {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            };
            if total_links == 0 {
                manifest.skipped = manifest.skipped.saturating_add(1);
                manifest.scan_complete = false;
                continue;
            }
            match link_allocations.entry(identity.clone()) {
                std::collections::hash_map::Entry::Vacant(vacant) => {
                    vacant.insert(LinkAllocationEvidence {
                        allocated_bytes: allocated,
                        total_links,
                        observed_links: 1,
                    });
                }
                std::collections::hash_map::Entry::Occupied(mut occupied) => {
                    let evidence = occupied.get_mut();
                    if evidence.allocated_bytes != allocated || evidence.total_links != total_links {
                        manifest.scan_complete = false;
                    }
                    evidence.observed_links = evidence.observed_links.saturating_add(1);
                }
            }
            manifest.files = manifest.files.saturating_add(1);
            manifest.records.push(format!(
                "F\0{relative}\0{identity}\0{}\0{allocated}\0{total_links}\0{modified}",
                metadata.len()
            ));
            if crate::cloud::metadata_is_dataless(&metadata) {
                manifest.scan_complete = false;
            }
        }
    }

    for evidence in link_allocations.values() {
        if evidence.observed_links > evidence.total_links {
            manifest.scan_complete = false;
            continue;
        }
        if evidence.observed_links == evidence.total_links {
            manifest.allocated_bytes = manifest
                .allocated_bytes
                .saturating_add(evidence.allocated_bytes);
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

/// 마커 인접 아티팩트 디렉토리를 찾아 mtime 나이를 증거로 보존하고 크기 내림차순으로 반환.
///
/// WalkDir의 부모 우선 순회를 이용해 검증된 아티팩트 아래는 즉시 건너뛴다. 생성물
/// 내부의 중첩 `node_modules`까지 다시 훑지 않으므로 큰 개발 트리에서도 같은 바이트를
/// 탐색 단계와 manifest 단계에서 두 번 읽지 않는다.
pub fn find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact> {
    // Age is report/approval evidence, not inventory or cleanup authority. Mutation remains bound
    // to a fresh manifest, filesystem identity, active-use evidence, and the requested selection.
    let _ = min_age_days;
    let Some(provider_roots) = discovered_provider_roots() else {
        return Vec::new();
    };
    let Ok(root_metadata) = std::fs::symlink_metadata(root) else {
        return Vec::new();
    };
    if !root.is_absolute()
        || root_metadata.file_type().is_symlink()
        || !root_metadata.is_dir()
        || provider_managed_ancestry(root, &provider_roots)
    {
        return Vec::new();
    }
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
        if e.file_type().is_dir() && provider_managed_ancestry(e.path(), &provider_roots) {
            walker.skip_current_dir();
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
        let Some((kind, markers)) = detected_artifact_kind(path, &name) else {
            continue;
        };
        let parent = path.parent().unwrap_or(root);
        let marker_ok = (markers.is_empty()
            || markers
                .iter()
                .any(|marker| marker_exists(parent, &name, marker)))
            && project_rebuild_authority(parent, kind);
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
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (kind, _) = detected_artifact_kind(path, &name)?;
            let parent = path.parent().unwrap_or(root);
            let manifest = artifact_manifest(path);
            if manifest.allocated_bytes == 0 {
                return None;
            }
            Some(DevArtifact {
                path: path.to_string_lossy().into_owned(),
                kind: kind.to_string(),
                project: parent
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                bytes: manifest.bytes,
                allocated_bytes: manifest.allocated_bytes,
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
                let manifest = artifact_manifest(&path);
                Some(DevArtifact {
                    path: path.to_string_lossy().into_owned(),
                    kind: "vscode-obsolete-extension".into(),
                    project: product.into(),
                    bytes: manifest.bytes,
                    allocated_bytes: manifest.allocated_bytes,
                    files: manifest.files,
                    skipped: manifest.skipped,
                    scan_complete: manifest.scan_complete,
                    fingerprint: manifest.fingerprint,
                    object_id: manifest.object_id,
                    age_days: if age == u64::MAX { 0 } else { age },
                })
            }),
    );

    found.sort_by(|a, b| {
        b.allocated_bytes
            .cmp(&a.allocated_bytes)
            .then_with(|| b.bytes.cmp(&a.bytes))
            .then_with(|| a.path.cmp(&b.path))
    });
    found
}

/// Locate the nearest enclosing Git worktree root for a generated artifact.
pub fn enclosing_worktree_root(artifact: &Path) -> Option<PathBuf> {
    artifact
        .ancestors()
        .find(|ancestor| ancestor.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Assess deletion blockers for one rebuild-authorized artifact without weakening inventory
/// authority. Protection criteria are additive to rebuild, provider, allocation, identity and
/// active-use evidence; they never admit a path that inventory rejected.
pub fn assess_dev_artifact_protection(
    artifact: &DevArtifact,
    context: &crate::reclaim_protection::ProtectionContext,
) -> crate::reclaim_protection::ProtectionAssessment {
    let path = Path::new(&artifact.path);
    let mut reasons = Vec::new();

    if crate::reclaim_protection::is_protected_data_dir_name(&artifact.kind) {
        match artifact.kind.as_str() {
            "local" => reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_LOCAL.into()),
            "results" => {
                reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_RESULTS.into())
            }
            _ => {}
        }
    }

    for component in path.components() {
        let std::path::Component::Normal(name) = component else {
            continue;
        };
        let name = name.to_string_lossy();
        if crate::reclaim_protection::is_protected_data_dir_name(&name) {
            match name.as_ref() {
                "local" => reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_LOCAL.into()),
                "results" => {
                    reasons.push(crate::reclaim_protection::REASON_PROTECTED_DATA_RESULTS.into())
                }
                _ => {}
            }
        }
        if crate::reclaim_protection::is_orchestration_lead_name(&name) {
            reasons.push(crate::reclaim_protection::REASON_ORCHESTRATION_LEAD.into());
        }
    }

    if let Some(worktree) = enclosing_worktree_root(path) {
        let assessment = crate::reclaim_protection::assess_worktree_protections(
            &worktree,
            None,
            None,
            context,
            false,
            false,
            None,
            false,
            None,
            true,
        );
        reasons.extend(crate::reclaim_protection::artifact_blocking_reason_codes(
            &assessment,
        ));
    }

    if let Some(window_secs) = context.recent_write_window_secs {
        let now_unix_secs = context
            .now_unix_secs
            .unwrap_or_else(crate::reclaim_protection::now_unix_secs);
        if let Some(reason) =
            crate::reclaim_protection::recent_write_reason(path, window_secs, now_unix_secs)
        {
            reasons.push(reason.to_string());
        }
    }

    crate::reclaim_protection::ProtectionAssessment::with_reasons(reasons)
}

fn artifact_protection_blocking_reasons(
    assessment: &crate::reclaim_protection::ProtectionAssessment,
) -> Vec<String> {
    let mut reasons = crate::reclaim_protection::artifact_blocking_reason_codes(assessment);
    reasons.extend(
        assessment
            .reason_codes
            .iter()
            .filter(|code| {
                code.starts_with("protected-data-path:")
                    || code.as_str() == crate::reclaim_protection::REASON_PROTECTED_CREDENTIALS
            })
            .cloned(),
    );
    reasons.sort();
    reasons.dedup();
    reasons
}

/// Partition already rebuild-authorized inventory into reclaimable and protected sets. The
/// assessment is explanatory evidence; cleanup re-evaluates the same authority immediately before
/// mutation so a stale UI partition cannot authorize deletion.
pub fn partition_artifacts_by_protection(
    artifacts: &[DevArtifact],
    context: &crate::reclaim_protection::ProtectionContext,
) -> (
    Vec<DevArtifact>,
    Vec<(DevArtifact, crate::reclaim_protection::ProtectionAssessment)>,
) {
    let mut reclaimable = Vec::new();
    let mut protected = Vec::new();
    for artifact in artifacts {
        let assessment = assess_dev_artifact_protection(artifact, context);
        if artifact_protection_blocking_reasons(&assessment).is_empty() {
            reclaimable.push(artifact.clone());
        } else {
            protected.push((artifact.clone(), assessment));
        }
    }
    (reclaimable, protected)
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
    clean_artifacts_with_protection(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        &crate::reclaim_protection::ProtectionContext::default(),
    )
}

/// Reversible cleanup with caller-supplied protection evidence. The context can add live Orca,
/// lead-queue, open-PR or explicit recent-write evidence; it cannot relax static protection rules.
pub fn clean_artifacts_with_protection(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    protection: &crate::reclaim_protection::ProtectionContext,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_disposition(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        false,
        protection,
    )
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
    permanently_delete_artifacts_with_protection(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        &crate::reclaim_protection::ProtectionContext::default(),
    )
}

/// Permanent cleanup with the same protection authority used by reversible cleanup.
pub fn permanently_delete_artifacts_with_protection(
    requests: &[DevArtifact],
    root: &Path,
    min_age_days: u64,
    journal_path: &Path,
    now_ms: u64,
    protection: &crate::reclaim_protection::ProtectionContext,
) -> Vec<DevArtifactCleanResult> {
    clean_artifacts_with_disposition(
        requests,
        root,
        min_age_days,
        journal_path,
        now_ms,
        true,
        protection,
    )
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
    protection: &crate::reclaim_protection::ProtectionContext,
) -> Vec<DevArtifactCleanResult> {
    let current = find_artifacts(root, min_age_days, now_ms);
    requests
        .iter()
        .map(|request| {
            let protection_assessment = assess_dev_artifact_protection(request, protection);
            let protection_blockers = artifact_protection_blocking_reasons(&protection_assessment);
            if !protection_blockers.is_empty() {
                return DevArtifactCleanResult {
                    path: request.path.clone(),
                    ok: false,
                    error: format!(
                        "development artifact protected: {}",
                        protection_blockers.join(",")
                    ),
                };
            }

            let matches = current.iter().find(|candidate| {
                candidate.path == request.path
                    && candidate.kind == request.kind
                    && candidate.project == request.project
                    && candidate.bytes == request.bytes
                    && candidate.allocated_bytes == request.allocated_bytes
                    && (request.kind == "vscode-obsolete-extension" || request.allocated_bytes > 0)
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
        match marker {
            "package.json" => fs::write(p.join("package-lock.json"), b"{}").unwrap(),
            "Cargo.toml" => fs::write(p.join("Cargo.lock"), b"version = 4\n").unwrap(),
            _ => {}
        }
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
        assert!(nm.allocated_bytes > 0);
        assert_eq!(nm.age_days, 0, "sentinel now_ms는 age_days 0으로 보고");
    }

    #[cfg(unix)]
    #[test]
    fn hard_links_inside_artifact_count_physical_allocation_once() {
        let tmp = tempfile::tempdir().unwrap();
        let target = project(tmp.path(), "cargo-app", "Cargo.toml", "target");
        let payload = target.join("payload.bin");
        fs::hard_link(&payload, target.join("payload-copy.bin")).unwrap();
        let metadata = fs::metadata(&payload).unwrap();
        let expected = allocated_bytes(&payload, &metadata).unwrap();
        assert!(expected > 0, "fixture must consume physical allocation");

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        assert_eq!(found.len(), 1);
        assert_eq!(
            found[0].allocated_bytes, expected,
            "one filesystem object must not be counted once per in-root hard-link name"
        );
    }

    #[cfg(unix)]
    #[test]
    fn external_hard_link_does_not_claim_retained_blocks_reclaimable() {
        let tmp = tempfile::tempdir().unwrap();
        let target = project(tmp.path(), "cargo-app", "Cargo.toml", "target");
        let payload = target.join("payload.bin");
        fs::hard_link(&payload, tmp.path().join("cargo-app/payload-retained.bin")).unwrap();

        let found = find_artifacts(tmp.path(), 0, u64::MAX);

        assert!(
            found.is_empty(),
            "blocks retained by a hard link outside the generated root are not reclaimable"
        );
    }

    #[test]
    fn canonicalizes_available_provider_roots_without_discarding_unavailable_ones() {
        let tmp = tempfile::tempdir().unwrap();
        let existing = tmp.path().join("provider-existing");
        let unavailable = tmp.path().join("provider-unavailable");
        fs::create_dir_all(&existing).unwrap();

        let roots = canonicalize_provider_roots([existing.clone(), unavailable.clone()]);

        assert_eq!(roots.len(), 2);
        assert_eq!(roots[0], fs::canonicalize(existing).unwrap());
        assert_eq!(roots[1], unavailable);
    }

    #[test]
    fn provider_ancestry_rejects_named_and_canonical_provider_descendants() {
        let tmp = tempfile::tempdir().unwrap();
        let named = tmp.path().join("Library/CloudStorage/provider/workspace");
        fs::create_dir_all(&named).unwrap();
        assert!(provider_managed_ancestry(&named, &[]));

        let provider = tmp.path().join("onedrive-root");
        let descendant = provider.join("workspace");
        fs::create_dir_all(&descendant).unwrap();
        let roots = canonicalize_provider_roots([provider]);
        assert!(provider_managed_ancestry(&descendant, &roots));
    }

    #[test]
    fn broad_selected_root_prunes_nested_provider_build_roots() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("Library/CloudStorage/provider/repository");
        fs::create_dir_all(project.join("target")).unwrap();
        fs::write(
            project.join("Cargo.toml"),
            b"[package]\nname='fixture'\nversion='0.1.0'",
        )
        .unwrap();
        fs::write(project.join("Cargo.lock"), b"version = 4").unwrap();
        fs::write(project.join("target/output.bin"), b"generated").unwrap();

        assert!(
            find_artifacts(tmp.path(), 0, u64::MAX).is_empty(),
            "nested provider-managed roots must be pruned before candidate planning"
        );
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
        let empty_obsolete = found
            .iter()
            .find(|item| item.path == server_obsolete.to_string_lossy())
            .expect("native .obsolete lifecycle must retain an empty extension directory");
        assert_eq!(empty_obsolete.allocated_bytes, 0);
        assert_eq!(editor_product(".cursor"), Some("Cursor"));
        assert_eq!(editor_product(".unknown-editor"), None);
    }

    #[test]
    fn age_is_evidence_not_admission_authority() {
        let tmp = tempfile::tempdir().unwrap();
        project(tmp.path(), "fresh", "package.json", "node_modules");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let found = find_artifacts(tmp.path(), 30, now_ms);

        assert_eq!(found.len(), 1);
        assert_eq!(found[0].age_days, 0);
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
    fn discovers_named_target_cache_without_project_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
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
    fn ignores_named_target_layout_without_cargo_authority() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("target");
        for child in ["deps", "build", "incremental"] {
            fs::create_dir_all(target.join("debug").join(child)).unwrap();
        }
        fs::write(target.join("customer-owned.sqlite"), b"business data").unwrap();
        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());
    }

    #[test]
    fn ignores_oversized_standalone_cargo_cache_tag() {
        let tmp = tempfile::tempdir().unwrap();
        let cache = tmp.path().join("standalone-cache");
        fs::create_dir_all(cache.join("debug")).unwrap();
        fs::write(cache.join(".rustc_info.json"), "{}").unwrap();
        let mut tag = "Signature: 8a477f597d28d172789f06886806bc55\n# This file is a cache directory tag created by cargo.\n".to_owned();
        tag.push_str(&"x".repeat(65_536));
        fs::write(cache.join("CACHEDIR.TAG"), tag).unwrap();

        assert!(find_artifacts(tmp.path(), 0, u64::MAX).is_empty());
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
