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

/// 파일 → 클래스 → targetFolder → 목적지 경로. 미분류·targetFolder 없음·이미 목적지 안은 제외.
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    let mut plans = Vec::new();
    for f in files {
        // filename을 classify보다 먼저 확인 — 파일명 없는 경로(루트 등)는 여기서 걸러진다.
        // (classify 뒤에 두면 이 분기가 도달 불가라 커버리지 사각이 됨)
        let Some(name) = f.path.file_name() else { continue };
        let Some(local) = classify(&f.path) else { continue };
        // 로컬명 → 온톨로지 클래스
        let Some(class) = onto.classes.iter().find(|c| {
            c.id.rsplit(['#', '/']).next().unwrap_or(&c.id) == local
        }) else { continue };
        let Some(template) = onto.resolve_target(&class.id) else { continue };
        // 템플릿 치환: ~ → home, {class} → 로컬명
        let folder = template
            .replacen('~', &home.to_string_lossy(), 1)
            .replace("{class}", local);
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
        FileEntry { path: PathBuf::from(p), size }
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
}
