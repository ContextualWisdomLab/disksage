//! 사용자 정의 분류 규칙 — 확장자/이름/경로/크기 술어로 파일→온톨로지 클래스. 첫 매칭 규칙 승리.
//! 순수 로직(파싱/매칭)만 여기 — 파일 로드/커맨드는 commands.rs(cfg not coverage). 캐시 카탈로그 rules.rs와 무관.
use std::path::Path;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuleMatch {
    #[serde(default)] pub ext: Option<String>,
    #[serde(default)] pub name_contains: Option<String>,
    #[serde(default)] pub path_contains: Option<String>,
    #[serde(default)] pub min_size: Option<u64>,
    #[serde(default)] pub max_size: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rule {
    pub r#match: RuleMatch,
    pub class: String,
}

/// JSON 배열 → 규칙들. 손상 JSON은 Err(사용자에게 알림 — 온톨로지 오버라이드와 동일 원칙).
pub fn parse_rules(json: &str) -> Result<Vec<Rule>, String> {
    serde_json::from_str(json).map_err(|e| e.to_string())
}

/// 첫 매칭 규칙의 클래스. 매칭 규칙 없으면 None.
pub fn classify_by_rules(rules: &[Rule], path: &Path, size: u64) -> Option<String> {
    rules.iter().find(|r| rule_matches(&r.r#match, path, size)).map(|r| r.class.clone())
}

/// 존재하는 모든 술어가 AND로 일치해야 매칭. 술어 전무(all-None)면 catch-all(true).
fn rule_matches(m: &RuleMatch, path: &Path, size: u64) -> bool {
    if let Some(ext) = &m.ext {
        let want = ext.to_lowercase();
        let got = path.extension().and_then(|e| e.to_str()).map(|e| e.to_lowercase());
        if got.as_deref() != Some(want.as_str()) { return false; }
    }
    if let Some(sub) = &m.name_contains {
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.contains(sub.as_str()) { return false; }
    }
    if let Some(sub) = &m.path_contains {
        if !path.to_string_lossy().contains(sub.as_str()) { return false; }
    }
    if let Some(min) = m.min_size { if size < min { return false; } }
    if let Some(max) = m.max_size { if size > max { return false; } }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn m() -> RuleMatch { RuleMatch { ext: None, name_contains: None, path_contains: None, min_size: None, max_size: None } }

    #[test]
    fn parse_valid_and_malformed() {
        let json = r#"[{"match":{"ext":"iso"},"class":"Installer"}]"#;
        let rules = parse_rules(json).unwrap();
        assert_eq!(rules.len(), 1);
        assert_eq!(rules[0].class, "Installer");
        assert_eq!(rules[0].r#match.ext.as_deref(), Some("iso"));
        assert!(parse_rules("not json").is_err());
        assert!(parse_rules("[]").unwrap().is_empty());
    }

    #[test]
    fn ext_predicate_case_insensitive() {
        let r = vec![Rule { r#match: RuleMatch { ext: Some("ISO".into()), ..m() }, class: "Installer".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/x.iso"), 0).as_deref(), Some("Installer"));
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/x.zip"), 0), None); // 확장자 불일치
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/d/noext"), 0), None); // 확장자 없음
    }

    #[test]
    fn name_and_path_contains() {
        let rn = vec![Rule { r#match: RuleMatch { name_contains: Some("backup".into()), ..m() }, class: "Archive".into() }];
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/d/my_backup.tar"), 0).as_deref(), Some("Archive"));
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/d/report.tar"), 0), None);
        assert_eq!(classify_by_rules(&rn, &PathBuf::from("/"), 0), None); // 파일명 없음 → "" → 불일치
        let rp = vec![Rule { r#match: RuleMatch { path_contains: Some("Downloads".into()), ..m() }, class: "Dl".into() }];
        assert_eq!(classify_by_rules(&rp, &PathBuf::from("/home/Downloads/x.bin"), 0).as_deref(), Some("Dl"));
        assert_eq!(classify_by_rules(&rp, &PathBuf::from("/home/Docs/x.bin"), 0), None);
    }

    #[test]
    fn size_bounds_inclusive() {
        let r = vec![Rule { r#match: RuleMatch { min_size: Some(100), max_size: Some(200), ..m() }, class: "Mid".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 100).as_deref(), Some("Mid")); // 하한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 200).as_deref(), Some("Mid")); // 상한 포함
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 99), None);  // 하한 미만
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x"), 201), None); // 상한 초과
    }

    #[test]
    fn and_semantics_and_first_match_wins_and_catch_all() {
        // AND: ext+min_size 둘 다 만족해야
        let r = vec![Rule { r#match: RuleMatch { ext: Some("mp4".into()), min_size: Some(1000), ..m() }, class: "BigVid".into() }];
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.mp4"), 2000).as_deref(), Some("BigVid"));
        assert_eq!(classify_by_rules(&r, &PathBuf::from("/x.mp4"), 500), None); // ext OK, size 미달 → AND 실패
        // 첫 매칭 승리
        let ord = vec![
            Rule { r#match: RuleMatch { ext: Some("log".into()), ..m() }, class: "First".into() },
            Rule { r#match: RuleMatch { ext: Some("log".into()), ..m() }, class: "Second".into() },
        ];
        assert_eq!(classify_by_rules(&ord, &PathBuf::from("/x.log"), 0).as_deref(), Some("First"));
        // all-None catch-all
        let catch = vec![Rule { r#match: m(), class: "Any".into() }];
        assert_eq!(classify_by_rules(&catch, &PathBuf::from("/anything.zzz"), 0).as_deref(), Some("Any"));
        // 빈 규칙 → None
        assert_eq!(classify_by_rules(&[], &PathBuf::from("/x"), 0), None);
    }

    #[test]
    fn unknown_field_is_rejected_not_silent_catch_all() {
        // 예: "exten"(오타) — deny_unknown_fields로 조용한 catch-all 대신 에러
        assert!(parse_rules(r#"[{"match":{"exten":"iso"},"class":"X"}]"#).is_err());
        assert!(parse_rules(r#"[{"match":{},"class":"X","typo":1}]"#).is_err());
    }
}
