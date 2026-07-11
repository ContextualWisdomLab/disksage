use std::collections::HashMap;
use std::path::PathBuf;

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fe(p: &str, size: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size }
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
}
