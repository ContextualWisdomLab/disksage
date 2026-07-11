use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::scanner;

#[derive(Debug, Clone, serde::Serialize)]
pub struct DevArtifact {
    pub path: String,
    pub kind: String,
    pub project: String,
    pub bytes: u64,
    pub age_days: u64,
}

/// (아티팩트 디렉토리명, 같은 부모에 있어야 하는 프로젝트 마커들)
const ARTIFACT_KINDS: &[(&str, &[&str])] = &[
    ("node_modules", &["package.json"]),
    ("target", &["Cargo.toml"]),
    (".venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("venv", &["pyproject.toml", "requirements.txt", "setup.py"]),
    ("__pycache__", &[]), // 마커 불필요 — 이름 자체가 파이썬 캐시
];

fn artifact_kind(name: &str) -> Option<&'static (&'static str, &'static [&'static str])> {
    ARTIFACT_KINDS.iter().find(|(k, _)| *k == name)
}

fn age_days(path: &Path, now_ms: u64) -> u64 {
    let Ok(md) = path.metadata() else { return 0 };
    let Ok(mtime) = md.modified() else { return 0 };
    let Ok(dur) = mtime.duration_since(std::time::UNIX_EPOCH) else { return 0 };
    let mtime_ms = dur.as_millis() as u64;
    now_ms.saturating_sub(mtime_ms) / 86_400_000
}

/// 마커 인접 아티팩트 디렉토리를 찾아 mtime 나이로 걸러 크기 내림차순으로 반환.
///
/// 2패스로 나눈 이유: jwalk는 병렬로 디렉토리를 순회해 부모/자식 방문 순서를
/// 보장하지 않는다. 그래서 "이미 찾은 아티팩트의 하위는 건너뛴다" 식으로 순회
/// 도중 걸러내면, 중첩 node_modules의 자식이 부모보다 먼저 방문될 경우 둘 다
/// 별도 항목으로 남는다. 1패스에서는 마커 인접 검증까지만 마친 후보 경로를 전부
/// 모으고(순서 무관), 2패스에서 다른 후보의 하위 경로인 것을 제거한 뒤에야 크기를
/// 계산해 중첩분을 이중 계산하지 않는다.
pub fn find_artifacts(root: &Path, min_age_days: u64, now_ms: u64) -> Vec<DevArtifact> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(|_depth, _path, _state, children| {
            // 심링크/reparse point 제외 — scanner의 순회 전반 패턴과 동일
            children.retain(|r| r.as_ref().map(scanner::keep_entry).unwrap_or(true));
        });

    for entry in walker {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_dir() {
            continue;
        }
        let path = e.path();
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else { continue };
        let Some((_, markers)) = artifact_kind(&name) else { continue };
        let parent = path.parent().unwrap_or(root);
        let marker_ok = markers.is_empty() || markers.iter().any(|m| parent.join(m).exists());
        if marker_ok {
            candidates.push(path);
        }
    }

    // 다른 후보의 하위 경로(중첩 아티팩트)는 제거 — 방문 순서에 의존하지 않는 비교
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

    let mut found: Vec<DevArtifact> = top_level
        .into_iter()
        .filter_map(|path| {
            let age = age_days(path, now_ms);
            if age < min_age_days {
                return None;
            }
            let name = path.file_name()?.to_string_lossy().into_owned();
            let (kind, _) = artifact_kind(&name)?;
            let parent = path.parent().unwrap_or(root);
            let bytes = scanner::scan_dir(path, &AtomicBool::new(false), |_| {}).stats.bytes;
            Some(DevArtifact {
                path: path.to_string_lossy().into_owned(),
                kind: kind.to_string(),
                project: parent
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                bytes,
                age_days: age,
            })
        })
        .collect();

    found.sort_by(|a, b| b.bytes.cmp(&a.bytes));
    found
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
}
