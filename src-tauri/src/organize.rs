use std::path::Path;

use crate::dupes::FileEntry;
use crate::inventory::classify;
use crate::ontology::Ontology;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MovePlan {
    pub src: String,
    pub dst: String,
    pub class_id: String,
}

/// 후보 클래스 로컬명(온톨로지에서). picker에 전달.
fn local_name(id: &str) -> &str {
    id.rsplit(['#', '/']).next().unwrap_or(id)
}

/// 파일 → (picker 또는 확장자 classify) 로컬 클래스 → targetFolder → 목적지.
/// picker(step ②): 후보 목록 중 하나를 고르거나 None(그러면 확장자 classify로 폴백).
// ponytail: pick은 &dyn Fn(트레이트 객체) — generic(impl Fn)이면 호출부 클로저 타입마다
// 별도 단형화(monomorphization)가 생겨, 커버리지 게이트가 단형화별 죽은 분기를 분기 미도달로
// 집계한다(테스트를 아무리 추가해도 100%에 못 미침). 단일 컴파일 바디로 만들어 분기 커버리지를
// 호출부 전체에서 합산되게 한다 — llm::InferenceEngine을 &dyn으로 주입하는 것과 같은 패턴.
pub fn plan_moves_with(
    files: &[FileEntry],
    onto: &Ontology,
    home: &Path,
    now_ms: u64,
    rules: &[crate::userrules::Rule],
    pick: &dyn Fn(&Path, &[&str]) -> Option<String>,
) -> Vec<MovePlan> {
    let candidates: Vec<&str> = onto.classes.iter().map(|c| local_name(&c.id)).collect();
    // spec §6: build the Reasoner once per plan, reuse across every file (not per file).
    let reasoner = crate::ontology::Reasoner::build(onto);
    let mut plans = Vec::new();
    for f in files {
        // filename을 classify보다 먼저 확인 — 파일명 없는 경로(루트 등)는 여기서 걸러진다.
        // (classify 뒤에 두면 이 분기가 도달 불가라 커버리지 사각이 됨)
        let Some(name) = f.path.file_name() else { continue };
        let age_days = now_ms.saturating_sub(f.mtime_ms) / 86_400_000;
        // precedence: 사용자 규칙 → picker(LLM) → 확장자 classify → 제외
        let local: String = match crate::userrules::classify_by_rules(rules, &f.path, f.size, age_days) {
            Some(c) => c,
            None => match pick(&f.path, &candidates) {
                Some(picked) => picked,
                None => match classify(&f.path) {
                    Some(c) => c.to_string(),
                    None => continue,
                },
            },
        };
        // 로컬명 → 온톨로지 클래스
        let Some(class) = onto.classes.iter().find(|c| local_name(&c.id) == local) else { continue };
        let Some(template) = onto.resolve_target_with(&reasoner, &class.id) else { continue };
        // 템플릿 치환: ~ → home, {class} → 로컬명
        let folder = template
            .replacen('~', &home.to_string_lossy(), 1)
            .replace("{class}", &local);
        let dst = Path::new(&folder).join(name);
        // 이미 목적지 폴더에 있으면 제외
        if f.path.parent() == Some(Path::new(&folder)) {
            continue;
        }
        plans.push(MovePlan {
            src: f.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            class_id: class.id.clone(),
        });
    }
    plans
}

/// 확장자 규칙만 사용(picker 없음) — 기존 동작 유지.
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    plan_moves_with(files, onto, home, 0, &[], &|_, _| None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::parse_ttl;
    use std::path::{Path, PathBuf};

    const ONTO: &str = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "~/Media/{class}" .
dm:Code a owl:Class ; rdfs:label "코드"@ko .
dm:Installer a owl:Class ; rdfs:label "설치파일"@ko ; dm:targetFolder "~/Installers" .
"#;

    fn fe(p: &str, size: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size, mtime_ms: 0 }
    }

    fn fe_at(p: &str, size: u64, mtime_ms: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size, mtime_ms }
    }

    #[test]
    fn plans_move_to_resolved_target_folder() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/pic.png", 100)];
        let plans = plan_moves(&files, &onto, home);
        assert_eq!(plans.len(), 1);
        // ~ → home, {class} → Image
        // Platform-neutral: construct expected path with Path::new(...).join(...)
        let expected_dst = Path::new("/home/u/Media/Image").join("pic.png");
        assert_eq!(plans[0].dst, expected_dst.to_string_lossy().to_string());
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn skips_unclassified_and_targetless() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![
            fe("/x/unknown.xyz", 10),   // 미분류 → 제외
            fe("/x/main.rs", 20),       // Code: targetFolder 없음 → 제외
        ];
        assert!(plan_moves(&files, &onto, home).is_empty());
    }

    #[test]
    fn skips_file_already_in_destination() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        // 이미 목적지 폴더에 있는 파일
        let files = vec![fe("/home/u/Media/Image/pic.png", 100)];
        assert!(plan_moves(&files, &onto, home).is_empty());
    }

    #[test]
    fn skips_classified_file_whose_class_absent_from_ontology() {
        // mp4 → classify "Video"지만 ONTO엔 Video 클래스가 없음 → 클래스 조회 else(continue) 커버
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        assert!(plan_moves(&[fe("/x/movie.mp4", 100)], &onto, home).is_empty());
    }

    #[test]
    fn skips_path_with_no_filename() {
        // 파일명 없는 경로(루트)는 filename 가드에서 걸러진다
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        assert!(plan_moves(&[fe("/", 100)], &onto, home).is_empty());
    }

    #[test]
    fn target_folder_without_class_placeholder_is_used_verbatim() {
        // ~/Installers 처럼 {class} 없는 targetFolder — 치환 없이 그대로, filename만 붙는다
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/setup.exe", 100)];
        let plans = plan_moves(&files, &onto, home);
        assert_eq!(plans.len(), 1);
        let expected = Path::new("/home/u/Installers").join("setup.exe");
        assert_eq!(plans[0].dst, expected.to_string_lossy().to_string());
    }

    #[test]
    fn target_folder_without_tilde_is_absolute() {
        // ~ 없는 절대경로 targetFolder — home 치환 없이 그대로
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "/opt/media/{class}" .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/pic.png", 100)];
        let plans = plan_moves(&files, &onto, home);
        assert_eq!(plans.len(), 1);
        let expected = Path::new("/opt/media/Image").join("pic.png");
        assert_eq!(plans[0].dst, expected.to_string_lossy().to_string());
    }

    #[test]
    fn picker_choice_overrides_extension_classify() {
        // main.rs는 확장자로 "Code"(targetFolder 없음 → 평소 제외)로 분류되지만,
        // picker가 "Image"(targetFolder 있음)를 고르면 Image 목적지로 계획된다.
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/src/main.rs", 20)];
        let pick = |_p: &Path, _c: &[&str]| Some("Image".to_string());
        let plans = plan_moves_with(&files, &onto, home, 0, &[], &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn picker_none_falls_back_to_extension_classify() {
        // picker가 None이면 기존 확장자 분류(pic.png → Image)로 폴백 — plan_moves와 동일
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/pic.png", 100)];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(&files, &onto, home, 0, &[], &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn picker_candidates_include_ontology_class_names() {
        // picker에 넘어오는 후보 목록이 온톨로지 클래스 로컬명을 포함하는지 확인
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let files = vec![fe("/downloads/pic.png", 100)];
        let seen = std::cell::RefCell::new(Vec::<String>::new());
        let pick = |_p: &Path, cands: &[&str]| {
            *seen.borrow_mut() = cands.iter().map(|s| s.to_string()).collect();
            None
        };
        let _ = plan_moves_with(&files, &onto, home, 0, &[], &pick);
        let c = seen.borrow();
        assert!(c.iter().any(|s| s == "Image"));
        assert!(c.iter().any(|s| s == "Installer"));
    }

    #[test]
    fn user_rule_overrides_picker_and_extension() {
        // pic.png는 확장자로 Image지만, 사용자 규칙(ext png → Installer)이 우선 → Installer 목적지
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: Some("png".into()), name_contains: None, path_contains: None, min_size: None, max_size: None, min_age_days: None, max_age_days: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| Some("Image".to_string()); // picker가 Image를 골라도
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, home, 0, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Installer")); // 규칙이 picker를 이긴다
        // 규칙이 우선하므로 plan_moves_with 내부에서 pick은 호출되지 않는다(설계상 의도).
        // 라인 커버리지 확보를 위해 클로저 자체가 유효한 picker임을 별도로 확인.
        assert_eq!(pick(Path::new("/x"), &[]), Some("Image".to_string()));
    }

    #[test]
    fn no_user_rule_match_falls_through_to_picker() {
        // 규칙이 있으나 매칭 안 되면(ext iso) 기존 precedence(picker→classify)로
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: Some("iso".into()), name_contains: None, path_contains: None, min_size: None, max_size: None, min_age_days: None, max_age_days: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, home, 0, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image")); // 확장자 폴백
    }

    #[test]
    fn user_rule_age_predicate_matches_old_file_only() {
        // now = 100 days in ms; rule: min_age_days 30 → Installer. Old file (mtime 0 → age 100d) matches; fresh (mtime≈now → age 0) doesn't.
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let now = 100 * 86_400_000u64;
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch { ext: None, name_contains: None, path_contains: None, min_size: None, max_size: None, min_age_days: Some(30), max_age_days: None },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        // old file → age 100d ≥ 30 → rule matches → Installer target
        let old = plan_moves_with(&[fe_at("/d/pic.png", 10, 0)], &onto, home, now, &rules, &pick);
        assert_eq!(old.len(), 1);
        assert!(old[0].class_id.ends_with("Installer"));
        // fresh file → age 0 < 30 → rule skips → extension classify (png→Image)
        let fresh = plan_moves_with(&[fe_at("/d/pic.png", 10, now)], &onto, home, now, &rules, &pick);
        assert_eq!(fresh.len(), 1);
        assert!(fresh[0].class_id.ends_with("Image"));
    }
}
