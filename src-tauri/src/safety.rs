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

/// HOME/USERPROFILE이 설정돼 있을 때만 정확히 그 경로와 일치하는지 (없으면 이 계층은 생략).
/// 실제 프로세스 환경변수를 건드리지 않고 부재 케이스를 테스트하기 위해 분리된 순수 함수.
fn is_home_root(path: &Path, home: Option<&str>) -> bool {
    match home {
        Some(h) => path == Path::new(h),
        None => false,
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
    if is_home_root(path, home.as_deref()) {
        return true;
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
        // 시스템 드라이브가 C:가 아닌 머신도 보호 — env에서 유도, 실패 시 C: 폴백
        let denied_roots: Vec<String> = {
            let mut roots = Vec::new();
            if let Ok(w) = std::env::var("SystemRoot") {
                roots.push(w); // 예: C:\Windows, D:\Windows
            } else {
                roots.push(r"C:\Windows".to_string());
            }
            match std::env::var("ProgramFiles") {
                Ok(p) => roots.push(p),
                Err(_) => roots.push(r"C:\Program Files".to_string()),
            }
            match std::env::var("ProgramFiles(x86)") {
                Ok(p) => roots.push(p),
                Err(_) => roots.push(r"C:\Program Files (x86)".to_string()),
            }
            roots
        };
        let pc = lower_components(path);
        for d in denied_roots {
            let dc = lower_components(Path::new(&d));
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

/// std::io 오류를 SafetyError::Journal로 감싸는 공용 매퍼.
/// journal_append의 여러 호출부가 동일한 클로저 리터럴을 각자 만들면 그중 실제 I/O 실패로만
/// 트리거되는 자리(디스크 풀/경합 등)는 단위 테스트로 재현하기 어려워 커버리지 사각이 생긴다.
/// 이름 있는 함수 하나로 모으면 이 함수 자체를 직접 호출해 한 번에 검증할 수 있다.
fn journal_io_err(e: std::io::Error) -> SafetyError {
    SafetyError::Journal(e.to_string())
}

fn journal_serde_err(e: serde_json::Error) -> SafetyError {
    SafetyError::Journal(e.to_string())
}

/// 파괴적 작업 저널 — 실행 전 "pending"으로 먼저 기록되고 결과로 덧붙는다 (스펙 §7-4)
pub fn journal_append(journal_path: &Path, entry: &JournalEntry) -> Result<(), SafetyError> {
    use std::io::{Read, Seek, SeekFrom, Write};
    let line = serde_json::to_string(entry).map_err(journal_serde_err)?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .append(true)
        .open(journal_path)
        .map_err(journal_io_err)?;
    // 크래시로 개행 없이 끊긴 꼬리가 있으면 개행을 먼저 넣어 다음 엔트리와의 병합을 막는다 (자가 치유)
    let mut healing = String::new();
    let len = f.seek(SeekFrom::End(0)).map_err(journal_io_err)?;
    if len > 0 {
        f.seek(SeekFrom::End(-1)).map_err(journal_io_err)?;
        let mut last = [0u8; 1];
        f.read_exact(&mut last).map_err(journal_io_err)?;
        if last[0] != b'\n' {
            healing.push('\n');
        }
    }
    // 본문+개행을 한 번의 write로 — 두 syscall 사이 크래시로 인한 torn line 방지
    f.write_all(format!("{healing}{line}\n").as_bytes())
        .map_err(journal_io_err)
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

/// Windows verbatim 접두(\\?\C:\, \\?\UNC\srv\share)를 일반 형태로 재구성한다.
/// 문자열 수술이 아니라 파싱된 Prefix 컴포넌트 기반 — UNC가 상대경로로 망가지지 않는다.
#[cfg(windows)]
fn strip_verbatim(p: &Path) -> PathBuf {
    use std::path::{Component, Prefix};
    let mut comps = p.components();
    let Some(Component::Prefix(pr)) = comps.next() else { return p.to_path_buf() };
    match pr.kind() {
        Prefix::VerbatimDisk(d) => {
            let mut out = PathBuf::from(format!("{}:\\", d as char));
            out.extend(comps.filter(|c| !matches!(c, Component::RootDir)));
            out
        }
        Prefix::VerbatimUNC(server, share) => {
            let mut out = PathBuf::from(r"\\");
            out.push(server);
            out.push(share);
            out.extend(comps.filter(|c| !matches!(c, Component::RootDir)));
            out
        }
        _ => p.to_path_buf(),
    }
}

#[cfg(not(windows))]
fn strip_verbatim(p: &Path) -> PathBuf {
    p.to_path_buf()
}

/// 앱 유일의 삭제 경로 (스펙 §7-1). 영구 삭제 API는 이 크레이트 어디에도 없다.
pub fn trash_delete(
    path: &Path,
    bytes: u64,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    // '..'는 lexical 가드를 우회해 보호 경로 밖으로 보이게 할 수 있음 — 컴포넌트 단위로 먼저 거부
    if path.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    // 가드는 정규화된 경로로 판정. canonicalize 실패(예: 이미 사라진 경로)면
    // lexical 경로로 판정한다 (ParentDir는 위에서 이미 거부됨) — 어느 쪽이든 verbatim은 재구성.
    let guard_path = strip_verbatim(&std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf()));
    if is_protected(&guard_path) {
        return Err(SafetyError::Protected(path.to_path_buf()));
    }
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "trash_delete".into(),
        path: path.to_string_lossy().into_owned(),
        bytes,
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;
    // fsync 없음(의식적 선택): 삭제는 휴지통 경유라 전원 단절로 pending 기록을 잃어도 복구 가능
    match trash::delete(path) {
        Ok(()) => {
            entry.outcome = "ok".into();
            journal_append(journal_path, &entry)?;
            Ok(())
        }
        Err(e) => {
            entry.outcome = format!("error:{e}");
            journal_append(journal_path, &entry)?;
            Err(SafetyError::Trash(e.to_string()))
        }
    }
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
    fn safety_error_display_messages() {
        assert!(SafetyError::Protected(PathBuf::from("/x")).to_string().contains("보호"));
        assert!(SafetyError::Trash("boom".into()).to_string().contains("휴지통"));
        assert!(SafetyError::Journal("boom".into()).to_string().contains("저널"));
    }

    #[test]
    fn is_home_root_false_when_env_absent() {
        // 실제 환경변수를 건드리지 않고 HOME/USERPROFILE 부재 케이스를 검증
        assert!(!is_home_root(Path::new("/whatever"), None));
    }

    #[test]
    fn protects_home_root_but_not_home_children() {
        // 한 줄: 각 arm이 별도 라인이면 플랫폼별로 반대쪽이 영구 미커버로 남는다
        let home = if cfg!(windows) { std::env::var("USERPROFILE").unwrap() } else { std::env::var("HOME").unwrap() };
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
    fn windows_guard_follows_system_root_env() {
        // 현재 머신의 실제 SystemRoot는 반드시 보호됨 (C:든 다른 드라이브든)
        let sysroot = std::env::var("SystemRoot").unwrap();
        assert!(is_protected(std::path::Path::new(&sysroot)));
        assert!(is_protected(&std::path::Path::new(&sysroot).join("System32")));
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
    fn journal_io_err_wraps_as_journal_error() {
        let e = std::io::Error::new(std::io::ErrorKind::Other, "boom");
        assert!(matches!(journal_io_err(e), SafetyError::Journal(_)));
    }

    #[test]
    fn journal_serde_err_wraps_as_journal_error() {
        let e = serde_json::from_str::<i32>("not json").unwrap_err();
        assert!(matches!(journal_serde_err(e), SafetyError::Journal(_)));
    }

    #[test]
    fn trash_delete_rejects_protected_path_without_journaling() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let root = if cfg!(windows) { "C:\\Windows" } else { "/usr" };
        let err = trash_delete(Path::new(root), 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전에 일어나야 함");
    }

    #[test]
    fn trash_delete_missing_path_journals_error_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let missing = tmp.path().join("ghost.bin");
        let err = trash_delete(&missing, 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 2); // pending + error
        assert!(recent[0].outcome.starts_with("error:"));
        assert_eq!(recent[1].outcome, "pending");
    }

    // 실제 휴지통 왕복 (스펙 §9 통합 테스트 1). trash::os_limited는 win/linux 전용.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn trash_delete_roundtrip_lands_in_trash() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let victim = tmp.path().join("disksage-roundtrip-fixture.bin");
        std::fs::write(&victim, vec![0u8; 64]).unwrap();

        trash_delete(&victim, 64, &jp, 42).unwrap();

        assert!(!victim.exists(), "원본은 사라져야 함");
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].ts_ms, 42);

        // 휴지통에서 확인 후 테스트 픽스처만 purge (제품 코드가 아닌 테스트 정리)
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|i| i.name.to_string_lossy().contains("disksage-roundtrip-fixture"))
            .collect();
        assert!(!items.is_empty(), "휴지통에 있어야 함");
        trash::os_limited::purge_all(items).unwrap();
    }

    #[test]
    fn trash_delete_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let sneaky = tmp.path().join("..");
        let err = trash_delete(&sneaky, 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn trash_delete_rejects_verbatim_protected_path() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        // 실존하는 보호 경로의 verbatim 형태 — canonicalize가 verbatim을 돌려줘도 가드가 잡아야 함
        let err = trash_delete(Path::new(r"\\?\C:\Windows\System32"), 0, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty());
    }

    #[cfg(windows)]
    #[test]
    fn strip_verbatim_reconstructs_disk_and_unc_forms() {
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\C:\Windows\System32")),
            Path::new(r"C:\Windows\System32")
        );
        assert_eq!(
            strip_verbatim(Path::new(r"\\?\UNC\srv\share\dir")),
            Path::new(r"\\srv\share\dir")
        );
        assert_eq!(strip_verbatim(Path::new(r"C:\plain")), Path::new(r"C:\plain"));
        assert_eq!(strip_verbatim(Path::new("relative/only")), Path::new("relative/only"));
        // 재구성된 UNC 공유 루트는 parent가 없어 보호된다 (fail-closed 확인)
        assert!(is_protected(&strip_verbatim(Path::new(r"\\?\UNC\srv\share"))));
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
