use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io::Read;

#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub size: u64,
}

/// 1단계: 크기가 같은 파일만 중복 후보. size 0과 단독 크기는 제외.
/// 반환 그룹은 크기 내림차순 (큰 중복부터 사용자에게 보여주기 위함).
pub fn group_by_size(files: Vec<FileEntry>) -> Vec<Vec<FileEntry>> {
    let mut by_size: HashMap<u64, Vec<FileEntry>> = HashMap::new();
    for f in files {
        if f.size == 0 {
            continue;
        }
        by_size.entry(f.size).or_default().push(f);
    }
    let mut groups: Vec<Vec<FileEntry>> =
        by_size.into_iter().filter(|(_, v)| v.len() >= 2).map(|(_, v)| v).collect();
    groups.sort_by(|a, b| b[0].size.cmp(&a[0].size));
    groups
}

/// 2단계: 앞 prefix_len 바이트만 해시 — 대용량 파일의 전체 해시를 피하는 저비용 필터
pub fn hash_prefix(path: &Path, prefix_len: usize) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = vec![0u8; prefix_len];
    let mut filled = 0;
    // 짧은 read를 대비해 EOF까지 채운다
    loop {
        let n = f.read(&mut buf[filled..]).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        filled += n;
        if filled == prefix_len {
            break;
        }
    }
    Ok(blake3::hash(&buf[..filled]).to_hex().to_string())
}

/// 3단계: 전체 스트리밍 해시 — 부분 해시가 충돌한 후보만 여기 도달
pub fn hash_full(path: &Path) -> Result<String, String> {
    let mut f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 65536];
    loop {
        let n = f.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::io::Write;

    fn fe(p: &str, size: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size }
    }

    fn write_file(dir: &std::path::Path, name: &str, bytes: &[u8]) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).unwrap();
        f.write_all(bytes).unwrap();
        p
    }

    #[test]
    fn groups_only_collisions_excludes_singletons_and_zero() {
        let files = vec![
            fe("/a", 100),
            fe("/b", 100),
            fe("/c", 50),   // 단독 — 제외
            fe("/d", 0),    // 0바이트 — 제외
            fe("/e", 0),    // 0바이트 — 제외
            fe("/f", 100),
        ];
        let groups = group_by_size(files);
        // 100바이트 그룹 하나만 (3개), 정렬은 크기 내림차순인데 동일 크기라 그룹 내부는 유지
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 3);
        assert!(groups[0].iter().all(|f| f.size == 100));
    }

    #[test]
    fn multiple_size_groups_sorted_desc() {
        let files = vec![
            fe("/a", 10), fe("/b", 10),
            fe("/c", 999), fe("/d", 999),
        ];
        let groups = group_by_size(files);
        assert_eq!(groups.len(), 2);
        // 그룹은 크기 내림차순: 999 그룹이 먼저
        assert_eq!(groups[0][0].size, 999);
        assert_eq!(groups[1][0].size, 10);
    }

    #[test]
    fn empty_input_is_empty() {
        assert!(group_by_size(Vec::new()).is_empty());
    }

    #[test]
    fn prefix_hash_same_head_matches_regardless_of_tail() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write_file(tmp.path(), "a.bin", b"HEADHEADHEAD-tailA");
        let b = write_file(tmp.path(), "b.bin", b"HEADHEADHEAD-tailB");
        // 앞 12바이트만 보면 같음
        assert_eq!(hash_prefix(&a, 12).unwrap(), hash_prefix(&b, 12).unwrap());
        // 전체는 다름
        assert_ne!(hash_full(&a).unwrap(), hash_full(&b).unwrap());
    }

    #[test]
    fn full_hash_identical_content_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write_file(tmp.path(), "a.bin", b"identical bytes here");
        let b = write_file(tmp.path(), "b.bin", b"identical bytes here");
        assert_eq!(hash_full(&a).unwrap(), hash_full(&b).unwrap());
    }

    #[test]
    fn hash_prefix_shorter_file_uses_available_bytes() {
        let tmp = tempfile::tempdir().unwrap();
        let a = write_file(tmp.path(), "a.bin", b"tiny");
        // prefix_len이 파일보다 커도 성공 (있는 4바이트만)
        assert!(hash_prefix(&a, 4096).is_ok());
    }

    #[test]
    fn hash_missing_file_is_err() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(hash_full(&tmp.path().join("ghost")).is_err());
        assert!(hash_prefix(&tmp.path().join("ghost"), 16).is_err());
    }
}
