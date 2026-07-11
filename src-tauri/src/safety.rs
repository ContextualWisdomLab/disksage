use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum SafetyError {
    Protected(PathBuf),
    Trash(String),
    Journal(String),
}

impl std::fmt::Display for SafetyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SafetyError::Protected(p) => write!(f, "보호된 경로: {}", p.display()),
            SafetyError::Trash(e) => write!(f, "휴지통 이동 실패: {e}"),
            SafetyError::Journal(e) => write!(f, "저널 기록 실패: {e}"),
        }
    }
}

/// 시스템·루트 경로 하드 거부 목록 (스펙 §7-3).
/// 안전 계층의 최후 방어선 — 호출자가 무엇을 넘기든 여기서 걸러진다.
pub fn is_protected(path: &Path) -> bool {
    // 드라이브/파일시스템 루트 자체
    if path.parent().is_none() {
        return true;
    }
    // 사용자 홈 루트 자체 (하위는 허용). 데스크톱 앱은 항상 사용자 세션에서 실행되므로
    // USERPROFILE/HOME 부재는 상정하지 않는다 — 없으면 이 계층만 생략되고
    // 루트/시스템 프리픽스 검사는 그대로 적용된다.
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    if let Some(h) = home {
        if path == Path::new(&h) {
            return true;
        }
    }
    #[cfg(windows)]
    {
        // 컴포넌트 단위 비교: '/'와 '\\' 모두 구분자로 파싱되고(C:/Windows 우회 차단),
        // 경계가 정확해 C:\WindowsBackup 같은 형제 폴더를 오차단하지 않는다
        fn lower_components(p: &Path) -> Vec<String> {
            p.components()
                .map(|c| c.as_os_str().to_string_lossy().to_lowercase())
                .collect()
        }
        let denied_roots = ["C:\\Windows", "C:\\Program Files", "C:\\Program Files (x86)"];
        let pc = lower_components(path);
        for d in denied_roots {
            let dc = lower_components(Path::new(d));
            if pc.len() >= dc.len() && pc[..dc.len()] == dc[..] {
                return true;
            }
        }
    }
    #[cfg(unix)]
    {
        let denied_prefixes = ["/usr", "/etc", "/bin", "/sbin", "/lib", "/boot", "/proc", "/sys", "/dev"];
        let s = path.to_string_lossy();
        if denied_prefixes
            .iter()
            .any(|d| s == *d || s.starts_with(&format!("{d}/")))
        {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct JournalEntry {
    pub ts_ms: u64,
    pub op: String,
    pub path: String,
    pub bytes: u64,
    pub outcome: String,
}

/// 파괴적 작업 저널 — 실행 전 "pending"으로 먼저 기록되고 결과로 덧붙는다 (스펙 §7-4)
pub fn journal_append(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let line = serde_json::to_string(entry).map_err(|e| SafetyError::Journal(e.to_string()))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(journal_path)
        .map_err(|e| SafetyError::Journal(e.to_string()))?;
    // 크래시로 개행 없이 끊긴 꼬리가 있으면 개행을 먼저 넣어 다음 엔트리와의 병합을 막는다 (자가 치유)
    let mut healing = String::new();
    let len = f.seek(SeekFrom::End(0)).map_err(|e| SafetyError::Journal(e.to_string()))?;
    if len > 0 {
        f.seek(SeekFrom::End(-1)).map_err(|e| SafetyError::Journal(e.to_string()))?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last).map_err(|e| SafetyError::Journal(e.to_string()))?;
        if last[0] != b'\n' {
            healing.push('\n');
        }
    }
    // 본문+개행을 한 번의 write로 — 두 syscall 사이 크래시로 인한 torn line 방지
    f.write_all(format!("{healing}{line}\n").as_bytes())
        .map_err(|e| SafetyError::Journal(e.to_string()))
}

pub fn journal_recent(journal_path: &Path, limit: usize) -> Vec<JournalEntry> {
    let Ok(content) = std::fs::read_to_string(journal_path) else { return Vec::new() };
    let mut entries: Vec<JournalEntry> = content
        .lines()
        .filter_map(|l| serde_json::from_str(l).ok())
        .collect();
    entries.reverse();
    entries.truncate(limit);
    entries
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn protects_system_and_root_paths() {
        #[cfg(windows)]
        {
            assert!(is_protected(Path::new("C:\\")));
            assert!(is_protected(Path::new("C:\\Windows")));
            assert!(is_protected(Path::new("C:\\Windows\\System32")));
            assert!(is_protected(Path::new("C:\\Program Files")));
            assert!(is_protected(Path::new("C:\\Program Files (x86)\\App")));
        }
        #[cfg(unix)]
        {
            assert!(is_protected(Path::new("/")));
            assert!(is_protected(Path::new("/usr")));
            assert!(is_protected(Path::new("/usr/bin/ls")));
            assert!(is_protected(Path::new("/etc")));
            assert!(is_protected(Path::new("/bin")));
            assert!(is_protected(Path::new("/lib")));
        }
    }

    #[test]
    fn protects_home_root_but_not_home_children() {
        let home = if cfg!(windows) {
            std::env::var("USERPROFILE").unwrap()
        } else {
            std::env::var("HOME").unwrap()
        };
        assert!(is_protected(Path::new(&home)));
        assert!(!is_protected(&Path::new(&home).join("some-cache-dir")));
    }

    #[test]
    fn allows_ordinary_deep_paths() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_protected(&tmp.path().join("node_modules")));
    }

    #[cfg(windows)]
    #[test]
    fn windows_guard_is_separator_agnostic_and_boundary_exact() {
        assert!(is_protected(Path::new("C:/Windows/System32")));
        assert!(is_protected(Path::new("c:/program files/SomeApp")));
        assert!(is_protected(Path::new("C:\\Program Files (x86)\\App")));
        assert!(!is_protected(Path::new("C:\\WindowsBackup")));
        assert!(!is_protected(Path::new("C:\\Windows.old"))); // 정당한 정리 대상
    }

    #[test]
    fn journal_roundtrip_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("journal.jsonl");
        for i in 0..3u64 {
            journal_append(
                &jp,
                &JournalEntry {
                    ts_ms: 1000 + i,
                    op: "trash_delete".into(),
                    path: format!("/x/{i}"),
                    bytes: i * 10,
                    outcome: "ok".into(),
                },
            )
            .unwrap();
        }
        let recent = journal_recent(&jp, 2);
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].path, "/x/2"); // 최신이 먼저
        assert_eq!(recent[1].path, "/x/1");
    }

    #[test]
    fn journal_recent_missing_file_is_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(journal_recent(&tmp.path().join("none.jsonl"), 5).is_empty());
    }

    #[test]
    fn journal_append_reports_io_error() {
        let tmp = tempfile::tempdir().unwrap();
        // 디렉토리를 저널 경로로 주면 열기 실패
        let err = journal_append(
            tmp.path(),
            &JournalEntry {
                ts_ms: 1,
                op: "trash_delete".into(),
                path: "/x".into(),
                bytes: 0,
                outcome: "ok".into(),
            },
        );
        assert!(matches!(err, Err(SafetyError::Journal(_))));
    }

    #[test]
    fn journal_append_heals_torn_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        // 개행 없이 끊긴 꼬리를 시뮬레이션
        std::fs::write(&jp, "{\"torn\":").unwrap();
        journal_append(
            &jp,
            &JournalEntry {
                ts_ms: 1,
                op: "trash_delete".into(),
                path: "/x".into(),
                bytes: 0,
                outcome: "ok".into(),
            },
        )
        .unwrap();
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 1, "치유된 새 엔트리는 온전히 읽혀야 함");
        assert_eq!(recent[0].path, "/x");
    }
}
