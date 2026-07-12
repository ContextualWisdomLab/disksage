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
        // macOS는 extend로 시스템 경로를 더 넣는다 — 다른 unix에선 그 라인이 cfg-out되어 mut가
        // 미사용이므로 allow(unused_mut). Linux 게이트는 macOS 전용 라인을 컴파일하지 않아 커버 불필요.
        #[allow(unused_mut)]
        let mut denied_prefixes: Vec<&str> =
            vec!["/usr", "/etc", "/bin", "/sbin", "/lib", "/boot", "/proc", "/sys", "/dev"];
        #[cfg(target_os = "macos")]
        denied_prefixes.extend_from_slice(&[
            "/System", "/Library", "/Applications", "/private", "/Volumes", "/cores", "/Network",
        ]);
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

/// 존재하지 않을 수 있는 경로의 보호 여부 판정용 정규화: 가장 가까운 실존 조상을 canonicalize하고
/// 나머지 미존재 접미부를 붙인다 — dst의 조상이 심링크로 보호 위치를 가리켜도 is_protected가 놓치지 않게.
fn normalize_for_guard(p: &Path) -> PathBuf {
    // 이미 존재하면 그대로 canonicalize
    if let Ok(c) = std::fs::canonicalize(p) {
        return strip_verbatim(&c);
    }
    // 존재하지 않으면: 실존하는 가장 가까운 조상을 찾아 canonicalize + 나머지 접미부
    let mut suffix: Vec<std::ffi::OsString> = Vec::new();
    let mut cur = p;
    loop {
        match cur.parent() {
            Some(parent) => {
                // parent가 있으면 cur은 루트가 아니므로 file_name은 항상 Some — 그래도
                // extend(Option)로 분기 없이 처리해 도달 불가 else가 커버리지 사각을 만들지 않게
                suffix.extend(cur.file_name().map(|n| n.to_os_string()));
                if let Ok(c) = std::fs::canonicalize(parent) {
                    let mut base = strip_verbatim(&c);
                    for part in suffix.iter().rev() {
                        base.push(part);
                    }
                    return base;
                }
                cur = parent;
            }
            None => return strip_verbatim(p), // 조상이 하나도 실존하지 않음(드묾) — lexical
        }
    }
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

/// 검증 결과에 따라 목적지를 정리하거나 성공 반환. 검증-실패 정리 arm은 복사 성공 후 해시
/// 불일치라는 정직하게 재현 불가한 상황에서만 도달하므로, 판정을 파라미터로 받아 양 arm을
/// 직접 단위 테스트한다(원본은 어느 쪽이든 건드리지 않는다).
fn finalize_verified_copy(dst: &Path, verified: bool) -> std::io::Result<()> {
    if verified {
        Ok(())
    } else {
        let _ = std::fs::remove_file(dst); // 우리가 만든 목적지이므로 정리
        Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "복사 검증 실패"))
    }
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
    finalize_verified_copy(dst, hashes_match(&src_hash, &dst_hash, src_len, dst_len))
}

/// 분기 결정(same_vol)을 파라미터로 받아 양 경로를 플랫폼 무관하게 테스트 가능하게 한다.
/// 같은 볼륨 이동 io — hard_link(create-only) 후 원본 링크 제거. 두 io 에러 모두 `?`로
/// 전파(커버리지 규율: happy path에서 map_err 클로저가 미실행 라인으로 남지 않도록).
/// dst가 이미 있으면 hard_link가 AlreadyExists로 실패해 덮어쓰지 않는다.
fn hardlink_move_io(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::hard_link(src, dst)?;
    std::fs::remove_file(src)?;
    Ok(())
}

/// move_file이 same_volume()로 실제 결정을 주입한다.
fn do_move(
    src: &Path,
    dst: &Path,
    same_vol: bool,
    journal_path: &Path,
    now_ms: u64,
) -> Result<(), SafetyError> {
    let mut entry = JournalEntry {
        ts_ms: now_ms,
        op: "move".into(),
        path: format!("{} -> {}", src.display(), dst.display()),
        bytes: std::fs::metadata(src).map(|m| m.len()).unwrap_or(0),
        outcome: "pending".into(),
    };
    journal_append(journal_path, &entry)?;

    let result = if same_vol {
        // rename은 dst를 원자적으로 덮어쓴다(REPLACE) → dst.exists() 체크 이후 경합으로 생긴
        // 파일이 휴지통도 안 거치고 영구 소실될 수 있다. hard_link는 create-only라 dst가 이미
        // 있으면 AlreadyExists로 실패(덮어쓰지 않음) — 링크 성공 후 원본 링크만 제거한다.
        // 두 단계 사이 크래시 시엔 양쪽이 같은 inode를 가리키는 무해한 중복이 남는다(손실 아님).
        // io는 헬퍼가 `?`로 전파 → happy path에서 map_err 클로저가 미실행 라인으로 남지 않는다.
        // 단일 경계 map_err은 hard_link 실패 테스트(dest-exists)가 커버한다.
        hardlink_move_io(src, dst).map_err(|e| SafetyError::Trash(e.to_string()))
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
        let guard = normalize_for_guard(p);
        if is_protected(&guard) {
            return Err(SafetyError::Protected(p.to_path_buf()));
        }
    }
    // 목적지 충돌 금지 (덮어쓰기 방지)
    if dst.exists() {
        return Err(SafetyError::Trash(format!("목적지가 이미 존재: {}", dst.display())));
    }
    // 목적지 부모 디렉토리 생성. 위 protected 검사가 parent 없는 경로를 이미 거부했으므로
    // parent는 항상 Some — 폴백(dst 자신)은 실제로 도달 불가지만, 패닉(expect) 대신 한 줄
    // unwrap_or로 두어 라인 커버리지를 유지하면서 방어한다(도달 시 create_dir_all이 에러로 귀결).
    let dst_parent = dst.parent().unwrap_or(dst);
    std::fs::create_dir_all(dst_parent).map_err(|e| SafetyError::Trash(e.to_string()))?;

    do_move(src, dst, same_volume(src, dst), journal_path, now_ms)
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

    #[cfg(target_os = "macos")]
    #[test]
    fn protects_macos_system_paths() {
        // macOS 전용 시스템 경로 — extend_from_slice 라인을 macOS서 커버.
        for p in [
            "/System",
            "/System/Library/CoreServices",
            "/Library",
            "/Applications",
            "/private/etc",
            "/Volumes/Macintosh HD",
            "/cores",
            "/Network",
        ] {
            assert!(is_protected(Path::new(p)), "{p} must be protected on macOS");
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

    // Fix 2 회귀 테스트: 존재하는 경로는 그대로 canonicalize — 기존 가드와 동일한 결과.
    #[test]
    fn normalize_for_guard_existing_path_canonicalizes_directly() {
        let tmp = tempfile::tempdir().unwrap();
        let f = tmp.path().join("exists.bin");
        std::fs::write(&f, b"x").unwrap();
        let expected = strip_verbatim(&std::fs::canonicalize(&f).unwrap());
        assert_eq!(normalize_for_guard(&f), expected);
    }

    // Fix 2 회귀 테스트: dst는 보통 존재하지 않는다 — 실존하는 가장 가까운 조상(tmp 자체)까지
    // 걸어 올라가 canonicalize하고, 미존재 접미부("nested/does-not-exist.bin")를 그대로 붙여야 한다.
    #[test]
    fn normalize_for_guard_walks_up_to_existing_ancestor_for_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("nested").join("does-not-exist.bin");
        let expected_base = strip_verbatim(&std::fs::canonicalize(tmp.path()).unwrap());
        assert_eq!(normalize_for_guard(&missing), expected_base.join("nested").join("does-not-exist.bin"));
    }

    // Fix 2 회귀 테스트: 슬래시 없는 단일 상대 컴포넌트는 조상이 ""까지 내려가고 canonicalize("")도
    // 실패해 `cur.parent() == None` 최종 폴백(조상이 하나도 실존하지 않음)에 도달 — lexical 그대로 반환.
    #[test]
    fn normalize_for_guard_no_existing_ancestor_falls_back_to_lexical() {
        let p = Path::new("disksage-nonexistent-relative-xyz-zzz");
        assert_eq!(normalize_for_guard(p), strip_verbatim(p));
    }

    // Fix 2 회귀 테스트: dst의 조상이 심링크로 보호 위치(/usr)를 가리키면, dst 자신은 존재하지
    // 않아도(그래서 lexical 폴백이 아니라 조상-워크가 심링크를 실제로 resolve해서) is_protected가
    // 우회되지 않고 걸려야 한다.
    #[cfg(unix)]
    #[test]
    fn move_file_rejects_dst_via_symlinked_protected_ancestor() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("src.bin");
        std::fs::write(&src, b"x").unwrap();
        let link = tmp.path().join("media_link");
        std::os::unix::fs::symlink("/usr", &link).unwrap(); // 사용자가 심어놓은 ~/Media -> /usr 시뮬레이션
        let dst = link.join("evil.bin"); // lexical로는 안전해 보이지만 실제로는 /usr/evil.bin
        let err = move_file(&src, &dst, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Protected(_))));
        assert!(src.exists(), "거부 시 원본 보존");
        assert!(journal_recent(&jp, 10).is_empty(), "보호 거부는 저널 이전");
    }

    #[test]
    fn do_move_same_volume_branch_renames() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, vec![7u8; 30]).unwrap();
        do_move(&src, &dst, true, &jp, 1).unwrap();
        assert!(!src.exists());
        assert_eq!(std::fs::read(&dst).unwrap().len(), 30);
    }

    // Fix 1 회귀 테스트: hard_link는 create-only라 dst가 이미 있으면(TOCTOU 경합으로 그 사이
    // 생긴 파일 시뮬레이션) AlreadyExists로 실패해야 하며, 그 경합 상대의 dst도 원본 src도
    // 절대 건드리면 안 된다 — rename의 REPLACE 시맨틱이었다면 여기서 dst가 파괴됐을 것.
    #[test]
    fn do_move_same_volume_hard_link_fails_when_dest_exists() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("a.bin");
        let dst = tmp.path().join("b.bin");
        std::fs::write(&src, b"original").unwrap();
        std::fs::write(&dst, b"pre-existing").unwrap(); // TOCTOU 경합에서 먼저 생긴 것처럼 시뮬레이션
        let err = do_move(&src, &dst, true, &jp, 1);
        assert!(matches!(err, Err(SafetyError::Trash(_))));
        assert!(src.exists(), "원본은 실패 시 보존");
        assert_eq!(
            std::fs::read(&dst).unwrap(),
            b"pre-existing",
            "경합 상대의 목적지를 덮어쓰면 안 됨"
        );
    }

    #[cfg(any(windows, target_os = "linux"))]
    #[test]
    fn do_move_cross_volume_branch_copies_verifies_and_trashes() {
        let tmp = tempfile::tempdir().unwrap();
        let jp = tmp.path().join("j.jsonl");
        let src = tmp.path().join("disksage-xvol-fixture.bin");
        let dst = tmp.path().join("moved-disksage-xvol-fixture.bin");
        std::fs::write(&src, vec![9u8; 40]).unwrap();
        // same_vol=false 강제 → 실제 같은 볼륨이어도 copy+verify+trash 경로 실행
        do_move(&src, &dst, false, &jp, 2).unwrap();
        assert!(!src.exists(), "원본은 휴지통으로");
        assert_eq!(std::fs::read(&dst).unwrap().len(), 40);
        // 원본이 휴지통에 있음 확인 후 테스트 픽스처만 purge
        let items: Vec<_> = trash::os_limited::list().unwrap().into_iter()
            .filter(|i| i.name.to_string_lossy().contains("disksage-xvol-fixture")).collect();
        trash::os_limited::purge_all(items).unwrap();
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

    #[test]
    fn same_volume_missing_path_is_not_same_volume() {
        let tmp = tempfile::tempdir().unwrap();
        let missing = tmp.path().join("no-such-file.tmp");
        // 존재하지 않는 경로 → 볼륨 판정 불가 → false. 플랫폼 무관:
        // Windows는 canonicalize 실패 후 drive() 불일치, unix는 metadata Err.
        // (unix same_volume의 metadata-Err/false 경로를 리눅스 게이트에서 커버)
        assert!(!same_volume(&missing, tmp.path()));
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
    fn finalize_verified_copy_removes_dst_and_errors_when_unverified() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("partial.bin");
        std::fs::write(&dst, b"partial").unwrap();
        assert!(finalize_verified_copy(&dst, false).is_err());
        assert!(!dst.exists(), "검증 실패 시 우리가 만든 목적지를 정리");
    }

    #[test]
    fn finalize_verified_copy_keeps_dst_when_verified() {
        let tmp = tempfile::tempdir().unwrap();
        let dst = tmp.path().join("good.bin");
        std::fs::write(&dst, b"good").unwrap();
        assert!(finalize_verified_copy(&dst, true).is_ok());
        assert!(dst.exists());
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

    // 크로스 볼륨 분기(복사+검증+trash_delete)의 결정적 커버리지는 do_move_cross_volume_*
    // 테스트가 same_vol=false를 강제해 양 플랫폼에서 담당한다. 실제 두 볼륨에 의존하는 통합
    // 테스트는 어느 단일 볼륨 게이트에서도 본문이 스킵돼 커버리지 갭을 만들므로 두지 않는다.
}
