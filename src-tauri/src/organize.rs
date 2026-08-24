use std::path::{Component, Path, PathBuf};

use crate::dupes::FileEntry;
use crate::inventory::classify;
use crate::ontology::Ontology;

// Keep the probe cap aligned with the path-free export contract so every exportable plan has
// complete production metadata. The 200-item batch limit remains the bounded latency ceiling.
pub(crate) const MAX_LINEAGE_PROBES: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct LineageMetadata {
    pub production_time_ms: Option<u64>,
    pub production_time_source: Option<String>,
    pub production_time_confidence: Option<String>,
    pub lineage_fingerprint: String,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct MovePlan {
    pub src: String,
    pub dst: String,
    pub class_id: String,
    #[serde(default)]
    pub source_size: Option<u64>,
    #[serde(default)]
    pub source_mtime_ms: Option<u64>,
    #[serde(default)]
    pub lineage: LineageMetadata,
}

#[cfg(not(coverage))]
pub fn lineage_metadata_for_path(path: &Path) -> Option<LineageMetadata> {
    if crate::cloud::source_content_is_dataless(path) {
        return None;
    }
    let file_metadata = std::fs::symlink_metadata(path).ok()?;
    if file_metadata.file_type().is_symlink() || !file_metadata.is_file() {
        return None;
    }
    let content = crate::cloud::probe_content_metadata_for_audit(path);
    let filesystem_created_ms = file_metadata
        .created()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let filesystem_modified_ms = file_metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    let (production_time_ms, production_time_source, production_time_confidence) =
        if let Some(value) = content.production_time_ms {
            (
                Some(value),
                content.production_time_source,
                content.production_time_confidence,
            )
        } else if let Some(value) = crate::cloud::filename_date_ms(path) {
            (
                Some(value),
                Some("filename:path-token".into()),
                Some("low".into()),
            )
        } else if filesystem_created_ms > 0 {
            (
                Some(filesystem_created_ms),
                Some("filesystem:created".into()),
                Some("low".into()),
            )
        } else if filesystem_modified_ms > 0 {
            (
                Some(filesystem_modified_ms),
                Some("filesystem:modified-fallback".into()),
                Some("low".into()),
            )
        } else {
            (None, None, None)
        };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"disksage-organize-lineage-v1\0");
    for value in [
        production_time_ms.unwrap_or_default().to_string(),
        production_time_source.clone().unwrap_or_default(),
        production_time_confidence.clone().unwrap_or_default(),
        content.title.unwrap_or_default(),
        content.authors.join("\0"),
        content.context.join("\0"),
    ] {
        hasher.update(value.as_bytes());
        hasher.update(&[0]);
    }
    for evidence in content.evidence {
        for value in [
            evidence.field,
            evidence.value,
            evidence.source,
            evidence.confidence,
        ] {
            hasher.update(value.as_bytes());
            hasher.update(&[0]);
        }
    }
    Some(LineageMetadata {
        production_time_ms,
        production_time_source,
        production_time_confidence,
        lineage_fingerprint: hasher.finalize().to_hex().to_string(),
    })
}

fn plan_moves_impl(
    files: &[FileEntry],
    onto: &Ontology,
    home: &Path,
    now_ms: u64,
    rules: &[crate::userrules::Rule],
    pick: &dyn Fn(&Path, &[&str]) -> Option<String>,
    lineage_probe: Option<&dyn Fn(&Path) -> Option<LineageMetadata>>,
) -> Vec<MovePlan> {
    let candidates: Vec<&str> = onto.classes.iter().map(|c| local_name(&c.id)).collect();
    let reasoner = crate::ontology::Reasoner::build(onto);
    let mut plans = Vec::new();
    let mut lineage_probe_count = 0;
    for f in files {
        let Some(name) = f.path.file_name() else {
            continue;
        };
        let age_days = now_ms.saturating_sub(f.mtime_ms) / 86_400_000;
        let local: String =
            match crate::userrules::classify_by_rules(rules, &f.path, f.size, age_days) {
                Some(c) => c,
                None => match pick(&f.path, &candidates) {
                    Some(picked) => picked,
                    None => match classify(&f.path) {
                        Some(c) => c.to_string(),
                        None => continue,
                    },
                },
            };
        let Some(class) = onto.classes.iter().find(|c| local_name(&c.id) == local) else {
            continue;
        };
        let Some(template) = onto.resolve_target_with(&reasoner, &class.id) else {
            continue;
        };
        let Some(folder_path) = resolve_target_folder(&template, home, &local) else {
            continue;
        };
        let dst = folder_path.join(name);
        if f.path.parent() == Some(folder_path.as_path()) {
            continue;
        }
        let lineage = match lineage_probe {
            Some(probe) if lineage_probe_count < MAX_LINEAGE_PROBES => {
                lineage_probe_count += 1;
                probe(&f.path)
            }
            Some(_) => Some(LineageMetadata::default()),
            None => Some(LineageMetadata::default()),
        };
        let Some(lineage) = lineage else { continue };
        plans.push(MovePlan {
            src: f.path.to_string_lossy().into_owned(),
            dst: dst.to_string_lossy().into_owned(),
            class_id: class.id.clone(),
            source_size: lineage_probe.map(|_| f.size),
            source_mtime_ms: lineage_probe.map(|_| f.mtime_ms),
            lineage,
        });
    }
    plans
}

/// 후보 클래스 로컬명(온톨로지에서). picker에 전달.
fn local_name(id: &str) -> &str {
    id.rsplit(['#', '/']).next().unwrap_or(id)
}

/// Resolve an ontology target without letting `~` become an ambient-path wildcard.
///
/// Only an exact `~` or leading `~/` token means the supplied home directory. A
/// named-user-looking token such as `~other` is not supported and therefore stays
/// relative and is rejected. Literal tildes inside an already-absolute target are
/// preserved. Home-relative targets also fail closed when the supplied home path
/// itself is not absolute. Their relative suffix is rebuilt from normal path components so
/// Windows emits native separators and parent/root/prefix traversal cannot escape the home root.
fn resolve_target_folder(template: &str, home: &Path, local: &str) -> Option<PathBuf> {
    let resolved_template = template.replace("{class}", local);
    if resolved_template == "~" {
        return home.is_absolute().then(|| home.to_path_buf());
    }
    if let Some(relative) = resolved_template.strip_prefix("~/") {
        if !home.is_absolute() {
            return None;
        }
        let mut folder_path = home.to_path_buf();
        for component in Path::new(relative).components() {
            match component {
                Component::Normal(segment) => folder_path.push(segment),
                _ => return None,
            }
        }
        return Some(folder_path);
    }

    let folder_path = PathBuf::from(resolved_template);
    folder_path.is_absolute().then_some(folder_path)
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
    plan_moves_impl(files, onto, home, now_ms, rules, pick, None)
}

pub fn plan_moves_with_metadata(
    files: &[FileEntry],
    onto: &Ontology,
    home: &Path,
    now_ms: u64,
    rules: &[crate::userrules::Rule],
    pick: &dyn Fn(&Path, &[&str]) -> Option<String>,
    lineage_probe: &dyn Fn(&Path) -> Option<LineageMetadata>,
) -> Vec<MovePlan> {
    plan_moves_impl(files, onto, home, now_ms, rules, pick, Some(lineage_probe))
}

pub fn validate_move_source(plan: &MovePlan) -> Result<(), String> {
    let path = Path::new(&plan.src);
    let metadata =
        std::fs::symlink_metadata(path).map_err(|_| "organize-source-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("organize-source-not-regular-file".into());
    }
    if plan.source_size.is_some_and(|size| size != metadata.len()) {
        return Err("organize-source-size-changed".into());
    }
    let modified_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default();
    if plan
        .source_mtime_ms
        .is_some_and(|mtime_ms| mtime_ms != modified_ms)
    {
        return Err("organize-source-mtime-changed".into());
    }
    #[cfg(not(coverage))]
    if !plan.lineage.lineage_fingerprint.is_empty() {
        let current = lineage_metadata_for_path(path)
            .ok_or_else(|| "organize-source-lineage-unavailable".to_string())?;
        if current.lineage_fingerprint != plan.lineage.lineage_fingerprint {
            return Err("organize-source-lineage-changed".into());
        }
    }
    Ok(())
}

/// 확장자 규칙만 사용(picker 없음) — 기존 동작 유지.
pub fn plan_moves(files: &[FileEntry], onto: &Ontology, home: &Path) -> Vec<MovePlan> {
    plan_moves_with(files, onto, home, 0, &[], &|_, _| None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ontology::parse_ttl;
    use std::cell::Cell;
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
        FileEntry {
            path: PathBuf::from(p),
            size,
            mtime_ms: 0,
        }
    }

    fn fe_at(p: &str, size: u64, mtime_ms: u64) -> FileEntry {
        FileEntry {
            path: PathBuf::from(p),
            size,
            mtime_ms,
        }
    }

    fn platform_home() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\u")
        } else {
            PathBuf::from("/home/u")
        }
    }

    fn platform_absolute_template(unix: &str, windows: &str) -> String {
        if cfg!(windows) {
            windows.to_string()
        } else {
            unix.to_string()
        }
    }

    #[test]
    fn plans_move_to_resolved_target_folder() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/downloads/pic.png", 100)];
        let plans = plan_moves(&files, &onto, &home);
        assert_eq!(plans.len(), 1);
        // ~ → home, {class} → Image
        let expected_dst = home.join("Media").join("Image").join("pic.png");
        assert_eq!(plans[0].dst, expected_dst.to_string_lossy().to_string());
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn metadata_aware_plan_binds_lineage_and_rejects_source_drift() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = Path::new("/home/u");
        let tmp = tempfile::tempdir().unwrap();
        let source = tmp.path().join("pic.png");
        std::fs::write(&source, b"image").unwrap();
        let files = vec![FileEntry {
            path: source.clone(),
            size: 5,
            mtime_ms: std::fs::metadata(&source)
                .unwrap()
                .modified()
                .unwrap()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
        }];
        let lineage = LineageMetadata {
            production_time_ms: Some(1_700_000_000_000),
            production_time_source: Some("embedded:exiftool:CreateDate".into()),
            production_time_confidence: Some("high".into()),
            lineage_fingerprint: String::new(),
        };
        let plans = plan_moves_with_metadata(
            &files,
            &onto,
            home,
            1_800_000_000_000,
            &[],
            &|_, _| None,
            &|_| Some(lineage.clone()),
        );
        assert_eq!(plans.len(), 1);
        assert_eq!(
            plans[0].lineage.production_time_source.as_deref(),
            Some("embedded:exiftool:CreateDate")
        );
        assert!(validate_move_source(&plans[0]).is_ok());
        std::fs::write(&source, b"changed").unwrap();
        assert_eq!(
            validate_move_source(&plans[0]),
            Err("organize-source-size-changed".into())
        );
    }

    #[test]
    fn metadata_probe_is_bounded_per_plan() {
        let onto = parse_ttl(ONTO).unwrap();
        let files = (0..MAX_LINEAGE_PROBES + 1)
            .map(|i| fe(&format!("/downloads/{i}.png"), 1))
            .collect::<Vec<_>>();
        let probes = Cell::new(0);
        let plans = plan_moves_with_metadata(
            &files,
            &onto,
            Path::new("/home/u"),
            1_800_000_000_000,
            &[],
            &|_, _| None,
            &|_| {
                probes.set(probes.get() + 1);
                Some(LineageMetadata::default())
            },
        );
        assert_eq!(probes.get(), MAX_LINEAGE_PROBES);
        assert_eq!(plans.len(), MAX_LINEAGE_PROBES + 1);
        assert_eq!(
            plans[MAX_LINEAGE_PROBES].src,
            format!("/downloads/{}.png", MAX_LINEAGE_PROBES)
        );
        assert_eq!(plans[MAX_LINEAGE_PROBES].source_size, Some(1));
        assert!(plans[MAX_LINEAGE_PROBES]
            .lineage
            .lineage_fingerprint
            .is_empty());
    }

    #[test]
    fn skips_unclassified_and_targetless() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/x/unknown.xyz", 10), fe("/x/main.rs", 20)];
        assert!(plan_moves(&files, &onto, &home).is_empty());
    }

    #[test]
    fn skips_file_already_in_destination() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let existing = home.join("Media").join("Image").join("pic.png");
        let files = vec![FileEntry {
            path: existing,
            size: 100,
            mtime_ms: 0,
        }];
        assert!(plan_moves(&files, &onto, &home).is_empty());
    }

    #[test]
    fn skips_classified_file_whose_class_absent_from_ontology() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        assert!(plan_moves(&[fe("/x/movie.mp4", 100)], &onto, &home).is_empty());
    }

    #[test]
    fn skips_path_with_no_filename() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let root_path = if cfg!(windows) { r"C:\" } else { "/" };
        assert!(plan_moves(&[fe(root_path, 100)], &onto, &home).is_empty());
    }

    #[test]
    fn target_folder_without_class_placeholder_is_used_verbatim() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/downloads/setup.exe", 100)];
        let plans = plan_moves(&files, &onto, &home);
        assert_eq!(plans.len(), 1);
        let expected = home.join("Installers").join("setup.exe");
        assert_eq!(plans[0].dst, expected.to_string_lossy().to_string());
    }

    #[test]
    fn target_folder_without_tilde_is_absolute() {
        let target_template =
            platform_absolute_template("/opt/media/{class}", "C:/opt/media/{class}");
        let ttl = format!(
            r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "{}" .
"#,
            target_template
        );
        let onto = parse_ttl(&ttl).unwrap();
        let home = platform_home();
        let files = vec![fe("/downloads/pic.png", 100)];
        let plans = plan_moves(&files, &onto, &home);
        assert_eq!(plans.len(), 1);
        let expected = PathBuf::from(target_template.replace("{class}", "Image")).join("pic.png");
        assert_eq!(plans[0].dst, expected.to_string_lossy().to_string());
    }

    #[test]
    fn rejects_relative_target_folder_that_depends_on_process_cwd() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "relative/{class}" .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let home = platform_home();
        let plans = plan_moves(&[fe("/downloads/pic.png", 100)], &onto, &home);
        assert!(
            plans.is_empty(),
            "relative ontology targets must fail closed"
        );
    }

    #[test]
    fn rejects_parent_traversal_in_home_relative_target_folder() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "~/Media/../escape/{class}" .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let home = platform_home();
        let plans = plan_moves(&[fe("/downloads/pic.png", 100)], &onto, &home);
        assert!(
            plans.is_empty(),
            "home-relative ontology targets must not traverse above their rooted suffix"
        );
    }

    #[test]
    fn rejects_named_tilde_target_that_is_not_home_token() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "~other/{class}" .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let home = platform_home();
        let plans = plan_moves(&[fe("/downloads/pic.png", 100)], &onto, &home);
        assert!(
            plans.is_empty(),
            "only an exact leading ~/ token may expand to home"
        );
    }

    #[test]
    fn preserves_literal_tilde_inside_absolute_target_folder() {
        let target_template = if cfg!(windows) {
            "C:/opt/~archive/{class}"
        } else {
            "/opt/~archive/{class}"
        };
        let ttl = format!(
            r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko ; dm:targetFolder "{}" .
"#,
            target_template
        );
        let onto = parse_ttl(&ttl).unwrap();
        let home = platform_home();
        let plans = plan_moves(&[fe("/downloads/pic.png", 100)], &onto, &home);
        assert_eq!(plans.len(), 1);
        let expected = PathBuf::from(target_template.replace("{class}", "Image")).join("pic.png");
        assert_eq!(plans[0].dst, expected.to_string_lossy().to_string());
    }

    #[test]
    fn rejects_home_relative_target_when_home_is_relative() {
        let onto = parse_ttl(ONTO).unwrap();
        let plans = plan_moves(&[fe("/downloads/pic.png", 100)], &onto, Path::new("."));
        assert!(
            plans.is_empty(),
            "home-relative targets require an absolute home path"
        );
    }

    #[test]
    fn picker_choice_overrides_extension_classify() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/src/main.rs", 20)];
        let pick = |_p: &Path, _c: &[&str]| Some("Image".to_string());
        let plans = plan_moves_with(&files, &onto, &home, 0, &[], &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn picker_none_falls_back_to_extension_classify() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/downloads/pic.png", 100)];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(&files, &onto, &home, 0, &[], &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn picker_candidates_include_ontology_class_names() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let files = vec![fe("/downloads/pic.png", 100)];
        let seen = std::cell::RefCell::new(Vec::<String>::new());
        let pick = |_p: &Path, cands: &[&str]| {
            *seen.borrow_mut() = cands.iter().map(|s| s.to_string()).collect();
            None
        };
        let _ = plan_moves_with(&files, &onto, &home, 0, &[], &pick);
        let c = seen.borrow();
        assert!(c.iter().any(|s| s == "Image"));
        assert!(c.iter().any(|s| s == "Installer"));
    }

    #[test]
    fn user_rule_overrides_picker_and_extension() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch {
                ext: Some("png".into()),
                name_contains: None,
                path_contains: None,
                min_size: None,
                max_size: None,
                min_age_days: None,
                max_age_days: None,
            },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| Some("Image".to_string());
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, &home, 0, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Installer"));
        assert_eq!(pick(Path::new("/x"), &[]), Some("Image".to_string()));
    }

    #[test]
    fn no_user_rule_match_falls_through_to_picker() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch {
                ext: Some("iso".into()),
                name_contains: None,
                path_contains: None,
                min_size: None,
                max_size: None,
                min_age_days: None,
                max_age_days: None,
            },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(&[fe("/d/pic.png", 10)], &onto, &home, 0, &rules, &pick);
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }

    #[test]
    fn user_rule_age_predicate_matches_old_file_only() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let now = 100 * 86_400_000u64;
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch {
                ext: None,
                name_contains: None,
                path_contains: None,
                min_size: None,
                max_size: None,
                min_age_days: Some(30),
                max_age_days: None,
            },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        let old = plan_moves_with(
            &[fe_at("/d/pic.png", 10, 0)],
            &onto,
            &home,
            now,
            &rules,
            &pick,
        );
        assert_eq!(old.len(), 1);
        assert!(old[0].class_id.ends_with("Installer"));
        let fresh = plan_moves_with(
            &[fe_at("/d/pic.png", 10, now)],
            &onto,
            &home,
            now,
            &rules,
            &pick,
        );
        assert_eq!(fresh.len(), 1);
        assert!(fresh[0].class_id.ends_with("Image"));
    }

    #[test]
    fn future_dated_file_saturates_to_age_zero() {
        let onto = parse_ttl(ONTO).unwrap();
        let home = platform_home();
        let now = 100 * 86_400_000u64;
        let future = 200 * 86_400_000u64;
        let rules = vec![crate::userrules::Rule {
            r#match: crate::userrules::RuleMatch {
                ext: None,
                name_contains: None,
                path_contains: None,
                min_size: None,
                max_size: None,
                min_age_days: Some(1),
                max_age_days: None,
            },
            class: "Installer".into(),
        }];
        let pick = |_p: &Path, _c: &[&str]| None;
        let plans = plan_moves_with(
            &[fe_at("/d/pic.png", 10, future)],
            &onto,
            &home,
            now,
            &rules,
            &pick,
        );
        assert_eq!(plans.len(), 1);
        assert!(plans[0].class_id.ends_with("Image"));
    }
}
