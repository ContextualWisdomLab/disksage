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
}

/// 스캔 파일을 클래스별로 집계. 미분류·온톨로지에 없는 클래스는 Unknown(일급 시민).
pub fn build_inventory(files: &[FileEntry], onto: &Ontology) -> InventoryReport {
    // 온톨로지 클래스 로컬 id(끝부분) → (전체 id, label)
    let mut local_to_class: HashMap<String, (String, String)> = HashMap::new();
    for c in &onto.classes {
        let local = c.id.rsplit(['#', '/']).next().unwrap_or(&c.id).to_string();
        local_to_class.insert(local, (c.id.clone(), c.label.clone()));
    }

    let mut acc: HashMap<String, (String, u64, u64)> = HashMap::new(); // class_id → (label, bytes, count)
    let mut unknown_bytes = 0u64;
    let mut unknown_count = 0u64;

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
            }
        }
    }

    let mut tallies: Vec<ClassTally> = acc
        .into_iter()
        .map(|(class_id, (label, bytes, count))| ClassTally { class_id, label, bytes, count })
        .collect();
    tallies.sort_by(|a, b| b.bytes.cmp(&a.bytes));

    InventoryReport { tallies, unknown_bytes, unknown_count }
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
        FileEntry { path: PathBuf::from(p), size }
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
}
