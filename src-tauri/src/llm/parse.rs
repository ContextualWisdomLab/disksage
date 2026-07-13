//! 강제-JSON 파싱 — 모델 출력에서 첫 균형 잡힌 {..}를 뽑아 serde. 모든 실패는 fail-closed(Unrated/None).
use crate::llm::{ExtReasoning, Verdict};

/// 첫 '{'부터 짝이 맞는 '}'까지 슬라이스. 없거나 안 맞으면 None.
// ponytail: 순진한 중괄호 카운트 — 문자열 값 안의 중괄호는 오분류 가능. 소형 모델 강제 JSON엔 충분.
fn extract_json(raw: &str) -> Option<&str> {
    let start = raw.find('{')?;
    let bytes = raw.as_bytes();
    let mut depth = 0usize;
    for i in start..bytes.len() {
        if bytes[i] == b'{' {
            depth += 1;
        } else if bytes[i] == b'}' {
            depth -= 1;
            if depth == 0 { return Some(&raw[start..=i]); }
        }
    }
    None
}

/// 판정만. 실패 시 Unrated.
pub fn parse_verdict(raw: &str) -> Verdict {
    parse_verdict_full(raw).0
}

/// (판정, 이유). 실패 시 (Unrated, "")로 fail-closed.
pub fn parse_verdict_full(raw: &str) -> (Verdict, String) {
    let Some(js) = extract_json(raw) else { return (Verdict::Unrated, String::new()); };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(js) else { return (Verdict::Unrated, String::new()); };
    let reason = v.get("reason").and_then(|r| r.as_str()).unwrap_or("").to_string();
    let verdict = match v.get("verdict").and_then(|x| x.as_str()) {
        Some("safe") => Verdict::Safe,
        Some("caution") => Verdict::Caution,
        Some("keep") => Verdict::Keep,
        _ => Verdict::Unrated,
    };
    (verdict, reason)
}

/// 후보 목록 중에서만 클래스 id 선택. 그 외/실패는 None(자유 생성 거부).
pub fn parse_class_pick(raw: &str, candidates: &[&str]) -> Option<String> {
    let js = extract_json(raw)?;
    let v = serde_json::from_str::<serde_json::Value>(js).ok()?;
    let pick = v.get("class")?.as_str()?;
    if candidates.contains(&pick) {
        Some(pick.to_string())
    } else {
        None
    }
}

/// 요약 문자열 추출. 실패는 None.
pub fn parse_summary(raw: &str) -> Option<String> {
    let js = extract_json(raw)?;
    let v = serde_json::from_str::<serde_json::Value>(js).ok()?;
    Some(v.get("summary")?.as_str()?.to_string())
}

/// 확장자 추론 파싱. type 문자열이 있어야 Some; class는 후보에 있을 때만 Some(그 외/none은 None).
pub fn parse_ext_reasoning(raw: &str, candidates: &[&str]) -> Option<ExtReasoning> {
    let js = extract_json(raw)?;
    let v = serde_json::from_str::<serde_json::Value>(js).ok()?;
    let type_desc = v.get("type")?.as_str()?.to_string();
    let class = v.get("class").and_then(|c| c.as_str()).filter(|c| candidates.contains(c)).map(|c| c.to_string());
    Some(ExtReasoning { type_desc, class })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_clean_json() {
        assert_eq!(parse_verdict(r#"{"verdict":"safe","reason":"cache file"}"#), Verdict::Safe);
        assert_eq!(parse_verdict(r#"{"verdict":"caution"}"#), Verdict::Caution);
        assert_eq!(parse_verdict(r#"{"verdict":"keep"}"#), Verdict::Keep);
    }
    #[test]
    fn parses_nested_json_object() {
        // 중첩 객체 — 안쪽 {..}가 먼저 닫혀 depth가 0이 아닌 값으로 감소하는 fall-through 경로 커버.
        // extract_json은 바깥 객체 전체를 반환해야 한다.
        assert_eq!(parse_verdict(r#"{"verdict":"safe","meta":{"x":1}}"#), Verdict::Safe);
    }

    #[test]
    fn parses_json_with_prose_and_fences() {
        let raw = "Sure!\n```json\n{\"verdict\": \"keep\", \"reason\": \"user doc\"}\n```\n";
        assert_eq!(parse_verdict(raw), Verdict::Keep);
    }
    #[test]
    fn verdict_full_returns_reason() {
        assert_eq!(parse_verdict_full(r#"{"verdict":"safe","reason":"tmp"}"#), (Verdict::Safe, "tmp".to_string()));
        // reason 없으면 빈 문자열
        assert_eq!(parse_verdict_full(r#"{"verdict":"safe"}"#), (Verdict::Safe, String::new()));
    }
    #[test]
    fn unknown_verdict_value_is_unrated() {
        assert_eq!(parse_verdict(r#"{"verdict":"delete"}"#), Verdict::Unrated); // 알 수 없는 값
        assert_eq!(parse_verdict(r#"{"note":"no verdict field"}"#), Verdict::Unrated); // 필드 없음
    }
    #[test]
    fn no_braces_is_unrated() {
        assert_eq!(parse_verdict("no json here"), Verdict::Unrated);
        assert_eq!(parse_verdict(""), Verdict::Unrated);
    }
    #[test]
    fn malformed_braced_json_is_unrated() {
        assert_eq!(parse_verdict("{not valid json}"), Verdict::Unrated); // 중괄호는 있으나 파싱 실패
    }
    #[test]
    fn unbalanced_open_brace_is_unrated() {
        assert_eq!(parse_verdict("{\"verdict\":\"safe\""), Verdict::Unrated); // 닫는 중괄호 없음 → extract None
    }
    #[test]
    fn class_pick_only_from_candidates() {
        assert_eq!(parse_class_pick(r#"{"class":"Image"}"#, &["Image","Doc"]), Some("Image".into()));
        assert_eq!(parse_class_pick(r#"{"class":"Video"}"#, &["Image","Doc"]), None); // 자유 생성 거부
    }
    #[test]
    fn class_pick_failure_paths_are_none() {
        assert_eq!(parse_class_pick("no json", &["Image"]), None);          // extract None
        assert_eq!(parse_class_pick("{bad json}", &["Image"]), None);       // serde err
        assert_eq!(parse_class_pick(r#"{"other":"x"}"#, &["Image"]), None); // class 필드 없음
        assert_eq!(parse_class_pick(r#"{"class":5}"#, &["Image"]), None);   // class가 문자열 아님
    }
    #[test]
    fn summary_extracted_or_none() {
        assert_eq!(parse_summary(r#"{"summary":"old installers"}"#), Some("old installers".into()));
        assert_eq!(parse_summary("no json"), None);          // extract None
        assert_eq!(parse_summary("{bad}"), None);            // serde err
        assert_eq!(parse_summary(r#"{"x":1}"#), None);       // summary 필드 없음
        assert_eq!(parse_summary(r#"{"summary":9}"#), None); // 문자열 아님
    }
    #[test]
    fn ext_reasoning_extracts_type_and_validates_class() {
        // class가 후보에 있으면 Some
        let r = parse_ext_reasoning(r#"{"type":"3D model","class":"Model3D"}"#, &["Model3D"]).unwrap();
        assert_eq!(r.type_desc, "3D model");
        assert_eq!(r.class.as_deref(), Some("Model3D"));
        // class가 후보 밖이면 type은 유지, class는 None(자유 생성 거부)
        let r2 = parse_ext_reasoning(r#"{"type":"3D model","class":"Nope"}"#, &["Model3D"]).unwrap();
        assert_eq!(r2.class, None);
        assert_eq!(r2.type_desc, "3D model");
        // class:"none" → None
        let r3 = parse_ext_reasoning(r#"{"type":"data","class":"none"}"#, &["Model3D"]).unwrap();
        assert_eq!(r3.class, None);
    }
    #[test]
    fn ext_reasoning_failure_paths_are_none() {
        assert!(parse_ext_reasoning("no json", &["X"]).is_none());       // extract None
        assert!(parse_ext_reasoning("{bad}", &["X"]).is_none());         // serde err
        assert!(parse_ext_reasoning(r#"{"class":"X"}"#, &["X"]).is_none()); // type 필드 없음 → None
        assert!(parse_ext_reasoning(r#"{"type":9}"#, &["X"]).is_none());  // type이 문자열 아님
    }
}
