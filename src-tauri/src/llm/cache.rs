//! 판정 캐시 — (path|size|mtime) 키의 인메모리 캐시(세션 단위). 스펙 §6 "판정 결과 로컬 캐시".
use std::collections::HashMap;

use crate::llm::Verdict;

#[derive(Default)]
pub struct VerdictCache {
    map: HashMap<String, Verdict>,
}

impl VerdictCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// 캐시 키 — 경로·크기·수정시각이 모두 같아야 같은 키(파일이 바뀌면 재판정).
    pub fn key(path: &str, size: u64, mtime_ms: u64) -> String {
        format!("{path}|{size}|{mtime_ms}")
    }

    pub fn get(&self, key: &str) -> Option<Verdict> {
        self.map.get(key).copied()
    }

    pub fn put(&mut self, key: String, v: Verdict) {
        self.map.insert(key, v);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn key_is_stable_and_distinct() {
        let a = VerdictCache::key("/x/a.bin", 100, 1_700_000_000_000);
        assert_eq!(a, VerdictCache::key("/x/a.bin", 100, 1_700_000_000_000)); // 동일 입력 → 동일 키
        assert_ne!(a, VerdictCache::key("/x/a.bin", 101, 1_700_000_000_000)); // size 다르면 다른 키
        assert_ne!(a, VerdictCache::key("/x/a.bin", 100, 1_700_000_000_001)); // mtime 다르면 다른 키
        assert_ne!(a, VerdictCache::key("/x/b.bin", 100, 1_700_000_000_000)); // path 다르면 다른 키
    }
    #[test]
    fn get_after_put_returns_value_and_miss_is_none() {
        let mut c = VerdictCache::new();
        let k = VerdictCache::key("/x/a.bin", 100, 1);
        assert_eq!(c.get(&k), None); // 미스
        c.put(k.clone(), Verdict::Safe);
        assert_eq!(c.get(&k), Some(Verdict::Safe)); // 히트
    }
}
