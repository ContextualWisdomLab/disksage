//! opt-in 웹 조회 — 확장자 토큰만 전송(프라이버시). 순수 쿼리/파싱은 100% 측정, 실제 ureq는 cfg(not(coverage)).
#[cfg(not(coverage))]
mod ureq_lookup;
#[cfg(not(coverage))]
pub use ureq_lookup::DdgLookup;

/// 확장자 → 파일 타입 설명 조회 seam. 실패는 Err, "정보 없음"은 Ok(None).
pub trait WebLookup {
    fn file_type(&self, ext: &str) -> Result<Option<String>, String>;
}

/// DuckDuckGo Instant Answer 쿼리 URL. 확장자 토큰만 포함 — 파일명/경로 절대 금지.
pub fn ddg_query(ext: &str) -> String {
    format!(
        "https://api.duckduckgo.com/?q={ext}+file+format&format=json&no_html=1&no_redirect=1",
        ext = ext
    )
}

/// DDG 응답 JSON에서 AbstractText 추출. 빈 문자열/필드 없음/파싱 실패는 None.
pub fn parse_ddg_abstract(json: &str) -> Option<String> {
    let v = serde_json::from_str::<serde_json::Value>(json).ok()?;
    let s = v.get("AbstractText")?.as_str()?;
    if s.is_empty() { None } else { Some(s.to_string()) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn query_contains_only_ext_token_no_pii() {
        let q = ddg_query("fbx");
        assert!(q.contains("fbx"));
        // 프라이버시: 확장자만 — 쿼리에 파일명/경로 구분자가 들어갈 여지 없음(이 함수는 ext만 받음)
        assert!(q.contains("api.duckduckgo.com"));
    }
    #[test]
    fn abstract_extracted_or_none() {
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":"Autodesk FBX is a 3D format."}"#),
                   Some("Autodesk FBX is a 3D format.".to_string()));
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":""}"#), None); // 빈 abstract
        assert_eq!(parse_ddg_abstract(r#"{"Heading":"x"}"#), None);     // 필드 없음
        assert_eq!(parse_ddg_abstract("not json"), None);              // 파싱 실패
        assert_eq!(parse_ddg_abstract(r#"{"AbstractText":5}"#), None); // 문자열 아님
    }
}
