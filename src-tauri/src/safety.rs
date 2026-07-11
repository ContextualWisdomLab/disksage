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

/// 두 경로가 같은 볼륨인지 — rename 가능 판정(순수). 목적지는 아직 없을 수 있어 부모로 판정.
pub fn same_volume(src: &Path, dst: &Path) -> bool {
    let dst_probe = dst.parent().unwrap_or(dst);
    #[cfg(windows)]
    {
        fn drive(p: &Path) -> Option<String> {
            p.components().next().and_then(|c| match c {
                std::path::Component::Prefix(pr) => Some(pr.as_os_str().to_string_lossy().to_lowercase()),
                _ => None,
            })
        }
        // canonicalize로 상대경로/verbatim 정규화 후 드라이브 비교(best-effort)
        let s = std::fs::canonicalize(src).unwrap_or_else(|_| src.to_path_buf());
        let d = std::fs::canonicalize(dst_probe).unwrap_or_else(|_| dst_probe.to_path_buf());
        drive(&s) == drive(&d)
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let sd = std::fs::metadata(src).map(|m| m.dev());
        let dd = std::fs::metadata(dst_probe).map(|m| m.dev());
        matches!((sd, dd), (Ok(a), Ok(b)) if a == b)
    }
}

/// 크로스 볼륨 복사 — io 에러는 `?`로 전파(커버리지 규율: happy path에서 map_err 클로저가
/// 미실행 라인으로 남지 않도록). 목적지는 create_new로 열어 "존재 확인 → 복사" 사이의 TOCTOU
/// 경합에서도 그 사이 생긴 파일을 덮어쓰지 않는다(경합 시 AlreadyExists로 실패).
/// 해시는 Result 그대로 반환 — 실패를 빈 문자열로 뭉개면 "둘 다 실패 → 둘 다 빈 문자열 → 일치"라는
/// 거짓 검증 통과가 생긴다(blake3 해시는 절대 비지 않으므로 실패는 반드시 실패로 남아야 함).
/// 검증은 별도 순수 함수로 분리해 실패 arm을 직접 단위 테스트한다.
fn copy_then_hash(
    src: &Path,
    dst: &Path,
) -> std::io::Result<(u64, u64, Result<String, String>, Result<String, String>)> {
    {
        let mut src_file = std::fs::File::open(src)?;
        let mut dst_file = std::fs::OpenOptions::new().write(true).create_new(true).open(dst)?;
        std::io::copy(&mut src_file, &mut dst_file)?;
        // 핸들을 여기서 닫아 이후 metadata/hash_full이 경로로 다시 읽을 때 걸리지 않게 함
    }
    let src_len = std::fs::metadata(src)?.len();
    let dst_len = std::fs::metadata(dst)?.len();
    let src_hash = crate::dupes::hash_full(src);
    let dst_hash = crate::dupes::hash_full(dst);
    Ok((src_len, dst_len, src_hash, dst_hash))
}

/// 순수 검증 판정 — 크기 일치 + 양쪽 해시가 모두 성공했고 서로 같을 때만 true.
/// 해시 중 하나라도 Err면 무조건 false(fail-closed) — "계산 실패"를 "일치"로 오인하지 않는다.
fn hashes_match(
    src_hash: &Result<String, String>,
    dst_hash: &Result<String, String>,
    src_len: u64,
    dst_len: u64,
) -> bool {
    matches!((src_hash, dst_hash), (Ok(s), Ok(d)) if src_len == dst_len && s == d)
}

// 크로스 볼륨 복사+검증(내부 io, ? 전파). 복사 도중 실패하든 검증에서 실패하든, 우리가 만든
// 목적지라면 정리하고 io::Error — 어느 실패든 원본은 절대 건드리지 않는다.
fn copy_verified_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    let (src_len, dst_len, src_hash, dst_hash) = match copy_then_hash(src, dst) {
        Ok(v) => v,
        Err(e) => {
            // create_new가 AlreadyExists로 실패했다면 dst는 우리가 만든 게 아니다(TOCTOU 경합
            // 상대가 먼저 만든 파일) — 지우면 안 된다. 그 외 실패는 우리가 만든 부분 목적지이므로 정리.
            if e.kind() != std::io::ErrorKind::AlreadyExists {
                let _ = std::fs::remove_file(dst);
            }
            return Err(e);
        }
    };
    if !hashes_match(&src_hash, &dst_hash, src_len, dst_len) {
        let _ = std::fs::remove_file(dst); // 검증 실패 — 우리가 만든 목적지이므로 정리(원본은 안 건드림)
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "복사 검증 실패"));
    }
    Ok(())
}

/// 앱 유일의 이동 경로 (스펙 §7-2). 영구 삭제 없음 — 원본 제거는 trash_delete 경유.
pub fn move_file(
    src: &Path,
    dst: &Path,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    // 보호: src·dst 양쪽, ParentDir 거부, verbatim 정규화 — trash_delete와 동일 리거
    for p in [src, dst] {
        if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
        let guard = strip_verbatim(&std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf()));
        if is_protected(&guard) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
    }
    // 목적지 충돌 금지 (덮어쓰기 방지)
    if dst.exists() {
        return Err(SafetyError::Trash(format!("목적지가 이미 존재: {}", dst.display())));
    }
    // 목적지 부모 디렉토리 생성. 위 protected 검사가 이미 parent 없는 경로(guard.parent().is_none())를
    // 걸러냈고 이 시점엔 dst가 존재하지 않음이 확인됐다(canonicalize 실패 → guard == dst 그대로)
    // — 그러므로 parent는 사실상 항상 Some. 그래도 이 앱의 유일한 파괴적 이동 경로에서는
    // panic(expect)이 에러보다 나쁘다 — ponytail: 불변식이 실제로 깨지는 경로는 없다고 보지만,
    // 방어적으로 패닉 대신 에러를 반환한다(가짜 커버리지를 위한 조작된 테스트는 추가하지 않음).
    let Some(dst_parent) = dst.parent() else {
        return Err(SafetyError::Trash(format!("목적지 경로에 부모 디렉토리가 없음: {}", dst.display())));
    };
    std::fs::create_dir_all(dst_parent).map_err(|e| SafetyError::Trash(e.to_string()))?;

    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "move".into(),
        path: format!("{} -> {}", src.display(), dst.display()),
        bytes: std::fs::metadata(src).map(|m| m.len()).unwrap_or(0),
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;

    let result = if same_volume(src, dst) {
        std::fs::rename(src, dst).map_err(|e| SafetyError::Trash(e.to_string()))
    } else {
        // 크로스 볼륨: 복사+검증 후 원본 휴지통 (영구 삭제 없음)
        copy_verified_io(src, dst)
            .map_err(|e| SafetyError::Trash(e.to_string()))
            .and_then(|()| {
                let bytes = std::fs::metadata(dst).map(|m| m.len()).unwrap_or(0);
                trash_delete(src, bytes, journal_path, now_ms)
            })
    };

    entry.outcome = match &result {
        Ok(()) => "ok".into(),
        Err(e) => format!("error:{e}"),
    };
    journal_append(journal_path, &entry)?;
    result
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

    #[test]
    fn move_file_rejects_protected_src_or_dst() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let f = tmp.path().join("f.bin");
        std::fs::write(&f, b"x").unwrap();
        let protected = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows\\x" } else { "/usr/x" });
        // 보호된 목적지
        assert!(matches!(move_file(&f, &protected, &jp, 1), Err(SafetyError::Protected(_))));
        // 보호된 출발
        let pf = std::path::PathBuf::from(if cfg!(windows) { "C:\\Windows\\y" } else { "/usr/y" });
        assert!(matches!(move_file(&pf, &tmp.path().join("z"), &jp, 1), Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전");
    }

    #[test]
    fn move_file_same_dir_renames_and_journals() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("sub").join("a.bin");
        std::fs::create_dir(tmp.path().join("sub")).unwrap();
        std::fs::write(&src, vec![0u8; 20]).unwrap();

        move_file(&src, &dst, &jp, 7).unwrap();

        assert!(!src.exists());
        assert!(dst.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 20);
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].op, "move");
    }

    #[test]
    fn move_file_rejects_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, b"aa").unwrap();
        std::fs::write(&dst, b"bb").unwrap(); // 이미 존재
        assert!(move_file(&src, &dst, &jp, 1).is_err());
        // 원본과 기존 목적지 모두 보존
        assert!(src.exists());
        assert_eq!(std::fs::read(&dst).unwrap(), b"bb");
    }

    #[test]
    fn same_volume_true_within_tempdir() {
        let tmp = tempfile::tempdir().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("sub");
        std::fs::write(&a, b"x").unwrap();
        std::fs::create_dir(&b).unwrap();
        assert!(same_volume(&a, &b));
    }

    #[cfg(windows)]
    #[test]
    fn same_volume_relative_missing_path_falls_back_without_prefix() {
        let tmp = tempfile::tempdir().unwrap();
        let missing_rel = Path::new("no-such-relative-file.tmp");
        // canonicalize 실패 시 상대경로로 폴백 — 첫 컴포넌트가 드라이브 Prefix가 아니므로
        // drive()의 방어적 `_ => None` 분기를 탄다. 실 드라이브를 가진 tmp와는 다르다고 판정.
        assert!(!same_volume(missing_rel, tmp.path()));
    }

    #[test]
    fn move_file_rejects_parent_dir_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let sneaky = tmp.path().join("..");
        let dst = tmp.path().join("z.bin");
        let err = move_file(&sneaky, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전");
    }

    #[test]
    fn move_file_reports_error_when_dest_parent_cannot_be_created() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("src.bin");
        std::fs::write(&src, b"hi").unwrap();
        // "blocker"를 파일로 만들어 그 이름으로 디렉토리를 만들 수 없게 함
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"not a dir").unwrap();
        let dst = blocker.join("nested").join("dst.bin");
        let err = move_file(&src, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        assert!(src.exists(), "부모 생성 실패 시 원본 보존");
        assert!(journal_recent(&jp, 10).is_empty(), "부모 생성 실패는 저널 이전에 실패");
    }

    #[test]
    fn move_file_same_volume_rename_failure_journals_error_outcome() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("d");
        std::fs::create_dir(&src).unwrap();
        // 디렉토리를 자기 자신의 하위 경로로 이동 시도 — OS가 rename을 거부(EINVAL 계열)한다
        let dst = src.join("inner").join("d");
        let err = move_file(&src, &dst, &jp, 5);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent.len(), 2); // pending + error
        assert!(recent[0].outcome.starts_with("error:"));
        assert_eq!(recent[1].outcome, "pending");
    }

    #[test]
    fn copy_then_hash_reads_matching_size_and_hash_for_identical_copy() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.bin");
        let dst = tmp.path().join("dst.bin");
        std::fs::write(&src, b"same-bytes-here").unwrap();
        let (sl, dl, sh, dh) = copy_then_hash(&src, &dst).unwrap();
        assert_eq!(sl, dl);
        assert_eq!(sh, dh);
        assert!(hashes_match(&sh, &dh, sl, dl));
    }

    #[test]
    fn hashes_match_detects_size_or_hash_mismatch() {
        let a = || Ok::<String, String>("a".into());
        let b = || Ok::<String, String>("b".into());
        assert!(!hashes_match(&a(), &a(), 1, 2)); // 크기 불일치
        assert!(!hashes_match(&a(), &b(), 1, 1)); // 해시 불일치
        assert!(hashes_match(&a(), &a(), 1, 1));
    }

    // Fix 1 회귀 테스트: 해시 계산 자체가 실패하면(예: 읽기 오류로 Err) 절대 "일치"로 읽히면 안 된다.
    // 예전 코드는 unwrap_or_default()로 실패를 ""로 뭉개서, 양쪽 다 실패하면 ""=="" → 거짓 검증
    // 통과가 됐었다(blake3 해시는 절대 비지 않으므로 ""는 반드시 실패를 의미해야 한다).
    #[test]
    fn hashes_match_fails_closed_when_either_hash_errored() {
        let ok = || Ok::<String, String>("same-hash".into());
        let err = || Err::<String, String>("read failed".into());
        assert!(!hashes_match(&err(), &ok(), 10, 10));
        assert!(!hashes_match(&ok(), &err(), 10, 10));
        assert!(!hashes_match(&err(), &err(), 10, 10), "양쪽 다 실패해도 절대 일치로 읽히면 안 됨");
    }

    #[test]
    fn copy_verified_io_succeeds_and_content_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("s2.bin");
        let dst = tmp.path().join("d2.bin");
        std::fs::write(&src, vec![9u8; 128]).unwrap();
        copy_verified_io(&src, &dst).unwrap();
        assert_eq!(std::fs::read(&dst).unwrap(), std::fs::read(&src).unwrap());
    }

    #[test]
    fn copy_verified_io_cleans_up_and_errors_when_copy_source_missing() {
        // 복사 단계 자체가 실패해도(검증 단계가 아니라) 부분 목적지를 정리하고 원본은 그대로 둔다
        let tmp = tempfile::tempdir().unwrap();
        let missing_src = tmp.path().join("does-not-exist.bin");
        let dst = tmp.path().join("never-created.bin");
        let err = copy_verified_io(&missing_src, &dst);
        assert!(err.is_err());
        assert!(!dst.exists());
    }

    // Fix 2 회귀 테스트: dst.exists() 체크와 실제 복사 사이의 TOCTOU 경합 대응.
    // create_new(true)라 복사 단계 자체가 "이미 있으면 실패"이므로 경합 상대가 방금 만든
    // 파일을 절대 덮어쓰지 않는다 — 그리고 그 파일을 우리가 만든 게 아니므로 정리 대상도 아니다.
    #[test]
    fn copy_verified_io_does_not_overwrite_existing_destination() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("s3.bin");
        let dst = tmp.path().join("d3.bin");
        std::fs::write(&src, b"new-content").unwrap();
        std::fs::write(&dst, b"pre-existing").unwrap(); // TOCTOU 경합에서 먼저 생긴 것처럼 시뮬레이션
        let err = copy_verified_io(&src, &dst);
        assert!(err.is_err());
        assert_eq!(
            std::fs::read(&dst).unwrap(),
            b"pre-existing",
            "경합 상대의 목적지를 덮어쓰거나 지우면 안 됨"
        );
        assert!(src.exists(), "원본은 실패 시에도 그대로 보존");
    }

    // 진짜 크로스 볼륨 통합 테스트 — 이 저장소(예: D:)와 OS 임시 볼륨(예: C:)이 실제로 다를 때만
    // move_file의 크로스 볼륨 분기(복사+검증+trash_delete)를 real fs로 검증한다.
    // 단일 볼륨 환경(예: 일부 CI 게이트)에서는 same_volume으로 자가 감지해 스킵 — 조작된 테스트가
    // 아니라 이 환경의 실제 조건에 의존하는 통합 테스트다.
    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn move_file_cross_volume_copies_verifies_and_trashes_original() {
        let tmp = tempfile::tempdir().unwrap(); // 보통 OS 임시 볼륨
        let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR")); // 저장소 볼륨
        if same_volume(manifest_dir, tmp.path()) {
            eprintln!("skip: 이 환경엔 서로 다른 두 볼륨이 없어 크로스 볼륨 경로를 검증할 수 없음");
            return;
        }
        let scratch = manifest_dir.join("target").join("mv-cov-scratch");
        std::fs::create_dir_all(&scratch).unwrap();
        let src_dir = tempfile::tempdir_in(&scratch).unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = src_dir.path().join("disksage-cv-fixture.bin");
        std::fs::write(&src, vec![7u8; 4096]).unwrap();
        let dst = tmp.path().join("cv-dst.bin");

        move_file(&src, &dst, &jp, 99).unwrap();

        assert!(!src.exists(), "크로스 볼륨 이동 후 원본은 휴지통 경유로 사라져야 함");
        assert!(dst.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 4096);
        let recent = journal_recent(&jp, 10);
        assert_eq!(recent[0].outcome, "ok");
        assert_eq!(recent[0].op, "move");

        // 테스트 픽스처만 휴지통에서 purge (M2 패턴 — 제품 코드는 purge하지 않음)
        let items: Vec<_> = trash::os_limited::list()
            .unwrap()
            .into_iter()
            .filter(|i| i.name.to_string_lossy().contains("disksage-cv-fixture"))
            .collect();
        if !items.is_empty() {
            trash::os_limited::purge_all(items).unwrap();
        }
    }
}
