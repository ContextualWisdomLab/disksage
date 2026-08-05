//! 미분류 확장자 추론 병합 — 오프라인 LLM 결과 + (opt-in) 웹 결과를 자문용 ExtInsight로. 순수·100% 측정.
use crate::llm::ExtReasoning;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct ExtInsight {
    pub ext: String,
    pub type_desc: Option<String>,
    pub suggested_class: Option<String>,
    pub source: String, // "llm" | "web" | "both" | "none"
}

/// 경로 표본에서 서로 다른 확장자(소문자) 추출 — 정렬·중복 제거. 확장자 없는 경로는 무시.
pub fn distinct_extensions(samples: &[String]) -> Vec<String> {
    let mut exts: Vec<String> = samples
        .iter()
        .filter_map(|p| std::path::Path::new(p).extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase()))
        .collect();
    exts.sort();
    exts.dedup();
    exts
}

/// 한 확장자의 LLM/웹 결과를 병합. type_desc는 웹 우선(있으면), 없으면 LLM. class는 LLM만 제안.
pub fn merge_insight(ext: &str, llm: Option<ExtReasoning>, web: Option<String>) -> ExtInsight {
    let (llm_type, suggested_class) = match &llm {
        Some(r) => (
            if r.type_desc.is_empty() { None } else { Some(r.type_desc.clone()) },
            r.class.clone(),
        ),
        None => (None, None),
    };
    let source = match (llm.is_some(), web.is_some()) {
        (true, true) => "both",
        (false, true) => "web",
        (true, false) => "llm",
        (false, false) => "none",
    }.to_string();
    let type_desc = web.or(llm_type); // 웹 우선
    ExtInsight { ext: ext.to_string(), type_desc, suggested_class, source }
}

/// 확장자별 오프라인 추론 + (online일 때만) 웹 조회 병합. web=None이면 웹 분기 절대 미실행(default offline).
pub fn build_insights(
    exts: &[String],
    reason: &dyn Fn(&str) -> Option<ExtReasoning>,
    web: Option<&dyn Fn(&str) -> Option<String>>,
) -> Vec<ExtInsight> {
    exts.iter()
        .map(|ext| {
            let llm = reason(ext);
            let w = web.and_then(|f| f(ext));
            merge_insight(ext, llm, w)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distinct_extensions_lowercased_sorted_deduped() {
        let s = vec!["/a/x.FBX".into(), "/b/y.fbx".into(), "/c/z.parquet".into(), "/d/noext".into()];
        assert_eq!(distinct_extensions(&s), vec!["fbx".to_string(), "parquet".to_string()]);
    }
    #[test]
    fn merge_prefers_web_type_keeps_llm_class() {
        let llm = Some(ExtReasoning { type_desc: "3D model".into(), class: Some("Model3D".into()) });
        let ins = merge_insight("fbx", llm, Some("Autodesk FBX 3D format".into()));
        assert_eq!(ins.type_desc.as_deref(), Some("Autodesk FBX 3D format")); // 웹 우선
        assert_eq!(ins.suggested_class.as_deref(), Some("Model3D"));
        assert_eq!(ins.source, "both");
    }
    #[test]
    fn merge_llm_only_and_web_only_and_none() {
        let llm = Some(ExtReasoning { type_desc: "data".into(), class: None });
        assert_eq!(merge_insight("dat", llm, None).source, "llm");
        assert_eq!(merge_insight("dat", None, Some("desc".into())).source, "web");
        let none = merge_insight("dat", None, None);
        assert_eq!(none.source, "none");
        assert_eq!(none.type_desc, None);
    }
    #[test]
    fn build_insights_offline_never_calls_web() {
        // web=None → 웹 클로저가 없으므로 호출 자체가 불가능(프라이버시: default offline)
        let reason = |e: &str| Some(ExtReasoning { type_desc: format!("t-{e}"), class: None });
        let out = build_insights(&["fbx".into()], &reason, None);
        assert_eq!(out[0].source, "llm");
        assert_eq!(out[0].type_desc.as_deref(), Some("t-fbx"));
    }
    #[test]
    fn merge_llm_with_empty_type_desc_yields_no_type_but_keeps_class() {
        // LLM이 type을 "none"으로 답해 빈 문자열로 파싱된 경우 — type_desc는 None, class 제안은 유지
        let llm = Some(ExtReasoning { type_desc: "".into(), class: Some("Model3D".into()) });
        let ins = merge_insight("fbx", llm, None);
        assert_eq!(ins.type_desc, None);
        assert_eq!(ins.suggested_class.as_deref(), Some("Model3D"));
        assert_eq!(ins.source, "llm");
    }
    #[test]
    fn build_insights_online_receives_only_ext_token() {
        // 프라이버시: 웹 클로저에 넘어오는 값은 확장자 토큰뿐(경로 구분자 없음)
        let reason = |_: &str| None;
        let web = |e: &str| { assert!(!e.contains('/') && !e.contains('.')); Some(format!("web-{e}")) };
        let out = build_insights(&["parquet".into()], &reason, Some(&web));
        assert_eq!(out[0].source, "web");
        assert_eq!(out[0].type_desc.as_deref(), Some("web-parquet"));
    }
}
