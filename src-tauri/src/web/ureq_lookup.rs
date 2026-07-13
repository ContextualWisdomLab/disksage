//! 실제 DDG 조회(ureq) — 네트워크 egress 유일 지점. coverage 빌드서 제외. 익명 UA, 짧은 타임아웃.
use super::{ddg_query, parse_ddg_abstract, WebLookup};

pub struct DdgLookup;

impl WebLookup for DdgLookup {
    fn file_type(&self, ext: &str) -> Result<Option<String>, String> {
        // 익명: 일반 UA만, 쿠키/키/식별자 없음. 확장자 토큰만 쿼리에 포함.
        let ua = concat!("DiskSage/", env!("CARGO_PKG_VERSION"));
        let body = ureq::get(&ddg_query(ext))
            .header("User-Agent", ua)
            .call()
            .map_err(|e| e.to_string())?
            .body_mut()
            .read_to_string()
            .map_err(|e| e.to_string())?;
        Ok(parse_ddg_abstract(&body))
    }
}
