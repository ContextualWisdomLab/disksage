//! LLM 프롬프트 생성 — 메타데이터만 사용(파일 내용은 절대 전송하지 않음). 순수 함수, 강제 JSON 지시.

/// LLM에 넘기는 파일 메타데이터. **내용 필드 없음**(프라이버시: 바이트는 전송 금지).
#[derive(Debug, Clone)]
pub struct FileMeta {
    pub path: String,
    pub name: String,
    pub size: u64,
    pub mtime_days: u64,
    pub parent: String,
}

/// 삭제-안전 판정 프롬프트. 강제 JSON, 메타데이터만.
pub fn verdict_prompt(m: &FileMeta) -> String {
    format!(
        "You judge whether a file is safe to delete, using ONLY its metadata (never its contents).\n\
         File: name={name} parent={parent} size={size}B age={age}d\n\
         Reply with ONLY this JSON, no prose:\n\
         {{\"verdict\":\"safe|caution|keep\",\"reason\":\"<short>\"}}\n\
         safe = regenerable/temporary; caution = maybe needed; keep = likely important.",
        name = m.name, parent = m.parent, size = m.size, age = m.mtime_days
    )
}

/// 분류 프롬프트 — 후보 목록 중에서만 하나 선택(자유 생성 금지).
pub fn classify_prompt(m: &FileMeta, candidates: &[&str]) -> String {
    format!(
        "Classify this file into exactly one of the candidate classes, using ONLY metadata.\n\
         File: name={name} parent={parent}\n\
         Candidates: {list}\n\
         Reply with ONLY this JSON (choose exactly one id from the list above):\n\
         {{\"class\":\"<one id from Candidates>\"}}",
        name = m.name, parent = m.parent, list = candidates.join(", ")
    )
}

/// 미분류 뭉치 요약 프롬프트 — 이름만으로 "이 뭉치는 무엇인가" 한 문장.
pub fn summary_prompt(samples: &[FileMeta]) -> String {
    let names: Vec<&str> = samples.iter().map(|m| m.name.as_str()).collect();
    format!(
        "These files are unclassified. In one short sentence, say what this pile mostly is, using ONLY the names.\n\
         Files: {names}\n\
         Reply with ONLY this JSON:\n\
         {{\"summary\":\"<one sentence>\"}}",
        names = names.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn meta() -> FileMeta {
        FileMeta { path: "/downloads/old_report.pdf".into(), name: "old_report.pdf".into(),
                   size: 2_400_000, mtime_days: 420, parent: "downloads".into() }
    }
    #[test]
    fn verdict_prompt_has_metadata_and_schema() {
        let p = verdict_prompt(&meta());
        assert!(p.contains("old_report.pdf"));
        assert!(p.contains("downloads"));
        assert!(p.contains(r#"{"verdict":"#));
        assert!(p.contains("safe") && p.contains("caution") && p.contains("keep"));
    }
    #[test]
    fn classify_prompt_lists_all_candidates_and_forbids_free_text() {
        let p = classify_prompt(&meta(), &["Image", "Document", "Installer"]);
        for c in ["Image", "Document", "Installer"] { assert!(p.contains(c)); }
        assert!(p.to_lowercase().contains("exactly one"));
    }
    #[test]
    fn summary_prompt_includes_each_sample() {
        let p = summary_prompt(&[meta()]);
        assert!(p.contains("old_report.pdf"));
    }
    #[test]
    fn summary_prompt_handles_multiple_samples() {
        let a = FileMeta { path: "/a/x.bin".into(), name: "x.bin".into(), size: 1, mtime_days: 1, parent: "a".into() };
        let b = FileMeta { path: "/a/y.dat".into(), name: "y.dat".into(), size: 2, mtime_days: 2, parent: "a".into() };
        let p = summary_prompt(&[a, b]);
        assert!(p.contains("x.bin") && p.contains("y.dat"));
    }
}
