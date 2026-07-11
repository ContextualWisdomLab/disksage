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
    // 사용자 홈 루트 자체 (하위는 허용)
    let home = std::env::var(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).ok();
    if let Some(h) = home {
        if path == Path::new(&h) {
            return true;
        }
    }
    #[cfg(windows)]
    {
        let lower = path.to_string_lossy().to_lowercase();
        let denied_prefixes = ["c:\\windows", "c:\\program files", "c:\\program files (x86)"];
        if denied_prefixes.iter().any(|d| lower.starts_with(d)) {
            return true;
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
}
