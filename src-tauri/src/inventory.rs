use std::collections::HashMap;
use std::path::Path;

use crate::dupes::FileEntry;
use crate::ontology::Ontology;

/// 저비용 분류: 확장자 → 온톨로지 클래스 로컬 id. LLM 분류는 M5.
/// ponytail: 정적 확장자 테이블 — MIME/파일명 패턴은 M5 LLM 신호로 확장
pub fn classify(path: &Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    Some(match ext.as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp" | "svg" | "heic" => "Image",
        "mp4" | "mov" | "mkv" | "avi" | "webm" | "m4v" => "Video",
        "rs" | "py" | "js" | "ts" | "go" | "java" | "c" | "cpp" | "h" | "rb" | "swift" => "Code",
        "csv" | "parquet" | "jsonl" | "arrow" | "feather" => "Dataset",
        "exe" | "msi" | "dmg" | "pkg" | "deb" | "appimage" => "Installer",
        "pdf" | "doc" | "docx" | "txt" | "md" | "rtf" | "odt" | "hwp" => "Document",
        _ => return None,
    })
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ClassTally {
    pub class_id: String,
    pub label: String,
    pub bytes: u64,
    pub count: u64,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct InventoryReport {
    pub tallies: Vec<ClassTally>,
    pub unknown_bytes: u64,
    pub unknown_count: u64,
    pub unknown_samples: Vec<String>,
}

/// unknown_samples에 담을 경로 표본 상한 — 프롬프트/페이로드 비대화 방지.
const UNKNOWN_SAMPLE_CAP: usize = 20;

/// 스캔 파일을 클래스별로 집계. 미분류·온톨로지에 없는 클래스는 Unknown(일급 시민).
pub fn build_inventory(files: &[FileEntry], onto: &Ontology) -> InventoryReport {
    // 온톨로지 클래스 로컬 id(끝부분) → (전체 id, label)
    // ponytail: 활성 온톨로지 안에서 로컬 id는 유일하다고 가정 — 서로 다른 네임스페이스의
    // 두 클래스가 같은 끝부분(#Image 등)을 가지면 뒤엣것이 앞엣것을 덮는다. 사용자 오버라이드
    // 온톨로지가 커지면 full IRI 키 매칭으로 승격
    let mut local_to_class: HashMap<String, (String, String)> = HashMap::new();
    for c in &onto.classes {
        let local = c.id.rsplit(['#', '/']).next().unwrap_or(&c.id).to_string();
        local_to_class.insert(local, (c.id.clone(), c.label.clone()));
    }

    let mut acc: HashMap<String, (String, u64, u64)> = HashMap::new(); // class_id → (label, bytes, count)
    let mut unknown_bytes = 0u64;
    let mut unknown_count = 0u64;
    let mut unknown_samples: Vec<String> = Vec::new();

    for f in files {
        let mapped = classify(&f.path).and_then(|local| local_to_class.get(local));
        match mapped {
            Some((class_id, label)) => {
                let e = acc.entry(class_id.clone()).or_insert((label.clone(), 0, 0));
                e.1 += f.size;
                e.2 += 1;
            }
            None => {
                unknown_bytes += f.size;
                unknown_count += 1;
                if unknown_samples.len() < UNKNOWN_SAMPLE_CAP {
                    unknown_samples.push(f.path.to_string_lossy().into_owned());
                }
            }
        }
    }

    let mut tallies: Vec<ClassTally> = acc
        .into_iter()
        .map(|(class_id, (label, bytes, count))| ClassTally { class_id, label, bytes, count })
        .collect();
    // bytes 내림차순, 동점은 class_id로 결정적 정렬(HashMap 순서 무작위성 → UI 깜빡임 방지)
    tallies.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.class_id.cmp(&b.class_id)));

    InventoryReport { tallies, unknown_bytes, unknown_count, unknown_samples }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dupes::FileEntry;
    use crate::ontology::parse_ttl;
    use std::path::PathBuf;

    const ONTO: &str = r#"
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm:   <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko .
dm:Code a owl:Class ; rdfs:label "코드"@ko .
"#;

    fn fe(p: &str, size: u64) -> FileEntry {
        FileEntry { path: PathBuf::from(p), size, mtime_ms: 0 }
    }

    #[test]
    fn classify_by_extension() {
        // Image
        assert_eq!(classify(&PathBuf::from("/x/a.png")), Some("Image"));
        assert_eq!(classify(&PathBuf::from("/x/b.JPG")), Some("Image")); // 대소문자 무관
        // Code
        assert_eq!(classify(&PathBuf::from("/x/c.rs")), Some("Code"));
        // Video
        assert_eq!(classify(&PathBuf::from("/x/movie.mp4")), Some("Video"));
        // Dataset
        assert_eq!(classify(&PathBuf::from("/x/data.csv")), Some("Dataset"));
        // Installer
        assert_eq!(classify(&PathBuf::from("/x/setup.exe")), Some("Installer"));
        // Document
        assert_eq!(classify(&PathBuf::from("/x/doc.pdf")), Some("Document"));
        // Unknown extension
        assert_eq!(classify(&PathBuf::from("/x/unknownext.xyz")), None);
        // No extension
        assert_eq!(classify(&PathBuf::from("/x/noext")), None);
    }

    #[test]
    fn inventory_aggregates_by_class_and_surfaces_unknown() {
        let onto = parse_ttl(ONTO).unwrap();
        let files = vec![
            fe("/a.png", 100),
            fe("/b.png", 200),   // Image 합계 300, count 2
            fe("/c.rs", 50),     // Code 50, count 1
            fe("/d.xyz", 999),   // 미분류 → unknown
        ];
        let rep = build_inventory(&files, &onto);
        // Unknown은 일급 필드
        assert_eq!(rep.unknown_bytes, 999);
        assert_eq!(rep.unknown_count, 1);
        assert_eq!(rep.unknown_samples, vec!["/d.xyz".to_string()]);
        // tallies는 바이트 내림차순: Image(300) > Code(50)
        assert_eq!(rep.tallies[0].class_id, "https://disksage.app/ontology#Image");
        assert_eq!(rep.tallies[0].bytes, 300);
        assert_eq!(rep.tallies[0].count, 2);
        assert_eq!(rep.tallies[1].class_id, "https://disksage.app/ontology#Code");
    }

    #[test]
    fn classified_file_whose_class_missing_from_onto_goes_unknown() {
        // Video로 분류되지만 온톨로지에 Video 클래스가 없으면 unknown 취급
        let onto = parse_ttl(ONTO).unwrap();
        let files = vec![fe("/movie.mp4", 500)];
        let rep = build_inventory(&files, &onto);
        assert_eq!(rep.unknown_bytes, 500);
        assert!(rep.tallies.is_empty());
    }

    #[test]
    fn empty_input_all_zero() {
        let onto = parse_ttl(ONTO).unwrap();
        let rep = build_inventory(&[], &onto);
        assert!(rep.tallies.is_empty());
        assert_eq!(rep.unknown_bytes, 0);
        assert_eq!(rep.unknown_count, 0);
    }

    #[test]
    fn unknown_samples_capped_and_unaffected_by_over_cap_files() {
        // CAP개 초과로 unknown 파일을 넣어 push 분기(len < CAP)와 cap-reached 분기(len >= CAP) 둘 다 태운다
        let onto = parse_ttl(ONTO).unwrap();
        let files: Vec<FileEntry> = (0..UNKNOWN_SAMPLE_CAP + 1)
            .map(|i| fe(&format!("/unknown{i}.xyz"), 1))
            .collect();
        let rep = build_inventory(&files, &onto);
        assert_eq!(rep.unknown_count, (UNKNOWN_SAMPLE_CAP + 1) as u64);
        assert_eq!(rep.unknown_samples.len(), UNKNOWN_SAMPLE_CAP);
    }

    #[test]
    fn equal_bytes_tie_broken_by_class_id_deterministically() {
        // Image와 Code가 정확히 같은 bytes → class_id로 결정적 정렬 (then_with 커버)
        let onto = parse_ttl(ONTO).unwrap();
        let files = vec![fe("/a.png", 100), fe("/b.rs", 100)];
        let rep = build_inventory(&files, &onto);
        assert_eq!(rep.tallies.len(), 2);
        assert_eq!(rep.tallies[0].bytes, rep.tallies[1].bytes);
        // 두 실행에서 같은 순서 (결정적) — class_id 오름차순
        assert!(rep.tallies[0].class_id < rep.tallies[1].class_id);
    }
}
