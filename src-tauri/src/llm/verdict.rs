//! LLM 삭제-안전 판정. 자문(advisory)일 뿐 — 삭제 트리거가 될 수 없음(스펙 §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    Safe,
    Caution,
    Keep,
    /// 모델 없음·추론 실패 → 규칙 기반 동작 유지, 배지만 "미판정"
    Unrated,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct FileVerdict {
    pub path: String,
    pub verdict: Verdict,
    pub reason: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn verdict_serde_roundtrip() {
        for v in [Verdict::Safe, Verdict::Caution, Verdict::Keep, Verdict::Unrated] {
            let s = serde_json::to_string(&v).unwrap();
            assert_eq!(serde_json::from_str::<Verdict>(&s).unwrap(), v);
        }
        // 프런트엔드가 소문자 문자열 리터럴로 switch하므로 와이어 포맷을 고정한다.
        assert_eq!(serde_json::to_string(&Verdict::Safe).unwrap(), "\"safe\"");
        assert_eq!(serde_json::to_string(&Verdict::Unrated).unwrap(), "\"unrated\"");
        let fv = FileVerdict { path: "/a".into(), verdict: Verdict::Safe, reason: "cache".into() };
        let s = serde_json::to_string(&fv).unwrap();
        assert_eq!(serde_json::from_str::<FileVerdict>(&s).unwrap(), fv);
    }
}
