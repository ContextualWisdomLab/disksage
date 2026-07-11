mod backend;
mod cache;
#[cfg(all(not(coverage), feature = "llm-engine"))]
mod engine;
mod model;
mod parse;
mod prompt;
mod verdict;
// coverage 빌드에서는 아직 이 재-export를 쓰는 소비자(commands 등)가 없어 unused_imports 경고 발생 — dead_code와 함께 억제
#[cfg_attr(coverage, allow(unused_imports))]
pub use backend::{choose_backend, Backend};
#[cfg_attr(coverage, allow(unused_imports))]
pub use cache::VerdictCache;
#[cfg(all(not(coverage), feature = "llm-engine"))]
pub use engine::LlamaEngine;
#[cfg_attr(coverage, allow(unused_imports))]
pub use model::{verify_sha256, ModelSpec, DEFAULT};
// commands.rs의 download_model 래퍼가 필요로 하는 다운로드 fn — model 모듈 자체는 private이라 재-export로만 노출.
// download_to는 네트워크 io라 coverage 빌드에서 이미 제외돼 있으므로(model.rs) 이 재-export도 동일하게 게이트.
#[cfg(not(coverage))]
pub use model::download_to;
#[cfg_attr(coverage, allow(unused_imports))]
pub use parse::{parse_class_pick, parse_summary, parse_verdict, parse_verdict_full};
#[cfg_attr(coverage, allow(unused_imports))]
pub use prompt::{classify_prompt, summary_prompt, verdict_prompt, FileMeta};
#[cfg_attr(coverage, allow(unused_imports))]
pub use verdict::{FileVerdict, Verdict};

/// 추론 엔진 seam — 실제 llama-cpp-2 구현은 engine.rs(Task 6b, cfg-gated); 테스트는 가짜 엔진.
/// 순수 오케스트레이션(프롬프트→infer→파싱)은 이 trait에만 의존하므로 게이트에서 100% 측정 가능.
pub trait InferenceEngine {
    /// 프롬프트를 받아 모델 출력 텍스트를 반환. 실패는 Err(메시지).
    fn infer(&self, prompt: &str) -> Result<String, String>;
}

/// 파일 삭제-안전 판정. infer 실패 시 Unrated로 degrade(스펙 §6 graceful degradation).
pub fn verdict_for(engine: &dyn InferenceEngine, meta: &FileMeta) -> FileVerdict {
    let (verdict, reason) = match engine.infer(&verdict_prompt(meta)) {
        Ok(out) => parse_verdict_full(&out),
        Err(_) => (Verdict::Unrated, String::new()),
    };
    FileVerdict { path: meta.path.clone(), verdict, reason }
}

/// 후보 목록 중 클래스 선택. infer 실패·범위 밖은 None(자유 생성 거부).
pub fn pick_class(engine: &dyn InferenceEngine, meta: &FileMeta, candidates: &[&str]) -> Option<String> {
    let out = engine.infer(&classify_prompt(meta, candidates)).ok()?;
    parse_class_pick(&out, candidates)
}

/// 미분류 뭉치 요약. infer 실패·파싱 실패는 None.
pub fn summarize_unknown(engine: &dyn InferenceEngine, samples: &[FileMeta]) -> Option<String> {
    let out = engine.infer(&summary_prompt(samples)).ok()?;
    parse_summary(&out)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fake(Result<String, String>);
    impl InferenceEngine for Fake {
        fn infer(&self, _p: &str) -> Result<String, String> { self.0.clone() }
    }
    fn meta() -> FileMeta {
        FileMeta { path: "/downloads/old_report.pdf".into(), name: "old_report.pdf".into(),
                   size: 2_400_000, mtime_days: 420, parent: "downloads".into() }
    }

    #[test]
    fn verdict_for_maps_model_json() {
        let e = Fake(Ok(r#"{"verdict":"safe","reason":"cache"}"#.into()));
        let fv = verdict_for(&e, &meta());
        assert_eq!(fv.verdict, Verdict::Safe);
        assert_eq!(fv.reason, "cache");
        assert_eq!(fv.path, "/downloads/old_report.pdf");
    }
    #[test]
    fn verdict_for_infer_error_is_unrated() {
        let e = Fake(Err("no model".into()));
        let fv = verdict_for(&e, &meta());
        assert_eq!(fv.verdict, Verdict::Unrated);
        assert_eq!(fv.reason, "");
    }
    #[test]
    fn pick_class_returns_candidate() {
        let e = Fake(Ok(r#"{"class":"Image"}"#.into()));
        assert_eq!(pick_class(&e, &meta(), &["Image", "Doc"]), Some("Image".into()));
    }
    #[test]
    fn pick_class_rejects_out_of_list() {
        let e = Fake(Ok(r#"{"class":"Video"}"#.into()));
        assert_eq!(pick_class(&e, &meta(), &["Image"]), None);
    }
    #[test]
    fn pick_class_error_is_none() {
        let e = Fake(Err("x".into()));
        assert_eq!(pick_class(&e, &meta(), &["Image"]), None);
    }
    #[test]
    fn summarize_unknown_maps_summary() {
        let e = Fake(Ok(r#"{"summary":"old stuff"}"#.into()));
        assert_eq!(summarize_unknown(&e, &[meta()]), Some("old stuff".into()));
    }
    #[test]
    fn summarize_unknown_error_is_none() {
        let e = Fake(Err("x".into()));
        assert_eq!(summarize_unknown(&e, &[meta()]), None);
    }
}
