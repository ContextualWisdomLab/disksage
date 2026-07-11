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

#[derive(Debug, Clone, serde::Serialize)]
pub struct DupeGroup {
    pub hash: String,
    pub size: u64,
    pub paths: Vec<String>,
}

/// 같은 (size, hash) 파일을 그룹으로 묶는 헬퍼. 해시 실패 파일은 제외.
fn regroup_by_hash(
    group: Vec<FileEntry>,
    hash_fn: impl Fn(&Path) -> Result<String, String>,
) -> HashMap<String, Vec<FileEntry>> {
    let mut by_hash: HashMap<String, Vec<FileEntry>> = HashMap::new();
    for f in group {
        let Ok(h) = hash_fn(&f.path) else { continue };
        by_hash.entry(h).or_default().push(f);
    }
    by_hash
}

/// 전체 3단계 파이프라인. 최종 그룹은 낭비 용량(size*(n-1)) 내림차순.
pub fn find_duplicates(files: Vec<FileEntry>, prefix_len: usize) -> Vec<DupeGroup> {
    let mut out: Vec<DupeGroup> = Vec::new();
    for size_group in group_by_size(files) {
        let size = size_group[0].size;
        // 2단계: 부분 해시로 재그룹, 2개 이상만
        for (_, prefix_group) in regroup_by_hash(size_group, |p| hash_prefix(p, prefix_len)) {
            if prefix_group.len() < 2 {
                continue;
            }
            // 3단계: 전체 해시로 확정, 2개 이상만
            for (hash, full_group) in regroup_by_hash(prefix_group, hash_full) {
                if full_group.len() < 2 {
                    continue;
                }
                out.push(DupeGroup {
                    hash,
                    size,
                    paths: full_group
                        .iter()
                        .map(|f| f.path.to_string_lossy().into_owned())
                        .collect(),
                });
            }
        }
    }
    // 낭비 용량 내림차순
    out.sort_by(|a, b| {
        // saturating: DupeGroup는 항상 paths>=2로 생성되지만, 다른 곳에서 만들어져도 패닉 없이
        let wa = a.size.saturating_mul((a.paths.len() as u64).saturating_sub(1));
        let wb = b.size.saturating_mul((b.paths.len() as u64).saturating_sub(1));
        wb.cmp(&wa)
    });
    out
}

/// jwalk로 파일만 수집 — 심링크/reparse는 scanner::keep_entry가 순회에서 제외
pub fn collect_files(root: &Path) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let walker = jwalk::WalkDir::new(root)
        .follow_links(false)
        .skip_hidden(false)
        .process_read_dir(|_d, _p, _s, children| {
            children.retain(|r| r.as_ref().map(crate::scanner::keep_entry).unwrap_or(true));
        });
    for entry in walker {
        let Ok(e) = entry else { continue };
        if !e.file_type().is_file() {
            continue;
        }
        let Ok(md) = e.metadata() else { continue };
        out.push(FileEntry { path: e.path(), size: md.len() });
    }
    out
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

    #[test]
    fn end_to_end_finds_true_duplicates_only() {
        let tmp = tempfile::tempdir().unwrap();
        // dup1/dup2: 완전 동일. near1/near2: 같은 크기+같은 앞부분, 다른 꼬리 → 전체해시서 갈림.
        let d1 = write_file(tmp.path(), "d1", b"AAAABBBBCCCCDDDD");
        let d2 = write_file(tmp.path(), "d2", b"AAAABBBBCCCCDDDD");
        let n1 = write_file(tmp.path(), "n1", b"AAAABBBBCCCCXXX1");
        let n2 = write_file(tmp.path(), "n2", b"AAAABBBBCCCCXXX2");
        let solo = write_file(tmp.path(), "solo", b"different length entirely");

        let files = vec![
            FileEntry { path: d1, size: 16 },
            FileEntry { path: d2, size: 16 },
            FileEntry { path: n1, size: 16 },
            FileEntry { path: n2, size: 16 },
            FileEntry { path: solo.clone(), size: std::fs::metadata(&solo).unwrap().len() },
        ];
        let groups = find_duplicates(files, 8);

        // d1/d2만 진짜 중복. n1/n2는 전체해시서 갈리고, solo는 크기 단독.
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
        assert_eq!(groups[0].size, 16);
        let names: Vec<String> = groups[0]
            .paths
            .iter()
            .map(|p| std::path::Path::new(p).file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"d1".to_string()) && names.contains(&"d2".to_string()));
    }

    #[test]
    fn groups_sorted_by_wasted_space_desc() {
        let tmp = tempfile::tempdir().unwrap();
        // 작은-쌍(10B x2 = 낭비 10) vs 큰-쌍(1000B x2 = 낭비 1000)
        let big = vec![0u8; 1000];
        let b1 = write_file(tmp.path(), "b1", &big);
        let b2 = write_file(tmp.path(), "b2", &big);
        let s1 = write_file(tmp.path(), "s1", b"tenbytes!!");
        let s2 = write_file(tmp.path(), "s2", b"tenbytes!!");
        let files = vec![
            FileEntry { path: b1, size: 1000 },
            FileEntry { path: b2, size: 1000 },
            FileEntry { path: s1, size: 10 },
            FileEntry { path: s2, size: 10 },
        ];
        let groups = find_duplicates(files, 4096);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].size, 1000); // 큰 낭비가 먼저
    }

    #[test]
    fn same_size_different_prefix_drops_at_prefix_stage() {
        let tmp = tempfile::tempdir().unwrap();
        // 크기는 같지만(8B) 앞부분이 달라 부분해시 단계서 각자 싱글턴 → 그룹 없음
        let a = write_file(tmp.path(), "a", b"AAAA1111");
        let b = write_file(tmp.path(), "b", b"BBBB2222");
        let files = vec![
            FileEntry { path: a, size: 8 },
            FileEntry { path: b, size: 8 },
        ];
        assert!(find_duplicates(files, 4).is_empty());
    }

    #[test]
    fn hash_failures_are_skipped_not_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let d1 = write_file(tmp.path(), "d1", b"same content x");
        let d2 = write_file(tmp.path(), "d2", b"same content x");
        let files = vec![
            FileEntry { path: d1, size: 14 },
            FileEntry { path: d2, size: 14 },
            FileEntry { path: tmp.path().join("ghost"), size: 14 }, // 존재하지 않음
        ];
        // ghost는 크기 그룹엔 들어가지만 해시 단계서 실패 → 조용히 빠지고 d1/d2는 확정
        let groups = find_duplicates(files, 4096);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].paths.len(), 2);
    }

    #[cfg(unix)]
    #[test]
    fn collect_files_excludes_dirs_and_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();
        write_file(root, "real.bin", b"data");
        std::fs::create_dir(root.join("sub")).unwrap();
        write_file(&root.join("sub"), "nested.bin", b"more");
        std::os::unix::fs::symlink(root.join("real.bin"), root.join("link.bin")).unwrap();

        let files = collect_files(root);
        let names: Vec<String> = files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"real.bin".to_string()));
        assert!(names.contains(&"nested.bin".to_string()));
        assert!(!names.contains(&"link.bin".to_string()), "심링크 제외");
    }
}
