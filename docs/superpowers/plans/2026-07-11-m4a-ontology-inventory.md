# DiskSage M4a: OWL 온톨로지 + 인벤토리 (읽기 전용) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** OWL Turtle 온톨로지를 파싱해 클래스 계층 트리를 만들고, 스캔 결과를 저비용 신호(확장자·파일명)로 클래스에 매핑해 클래스별 용량을 집계하는 인벤토리 뷰를 제공한다. 순수 읽기 전용 — 파일을 이동/삭제하지 않는다 (그것은 M4b).

**Architecture:** 새 `ontology` 모듈이 Turtle을 파싱해 `owl:Class` + `rdfs:subClassOf` 계층 + `rdfs:label` + `dm:targetFolder` 주석을 트리로 만든다(추론기 없음 — 스펙 §2 비목표). `inventory` 모듈이 저비용 분류기(확장자→클래스 매핑, 미분류는 Unknown 버킷)로 스캔 파일을 클래스에 배정하고 용량을 집계한다. 기본 온톨로지 `.ttl`을 앱에 번들. 커맨드 래퍼는 `#[cfg(not(coverage))]`, 순수 로직은 100% 라인 커버리지.

**Tech Stack:** 기존 스택 + `sophia` 크레이트(Turtle 파싱 — 표준 OWL이라 Protégé 출력도 처리해야 하므로 손수 파서 대신 정식 RDF 라이브러리).

## Global Constraints

- 스펙: `docs/superpowers/specs/2026-07-10-disksage-design.md` §5(온톨로지)/§2(비목표: 추론기 없음, 클래스 계층 순회만) — 충돌 시 스펙 우선
- **읽기 전용 마일스톤**: M4a에는 삭제/이동 코드가 없다. 파일 시스템 쓰기 없음(기본 온톨로지 번들 파일 제외 — 그건 소스 트리의 정적 애셋)
- **네임스페이스**: `dm:` = `https://disksage.app/ontology#` (스펙 §5)
- **조직 CI 게이트**: 리눅스 러너 `cargo llvm-cov --all-features --fail-under-lines 100`. Tauri 래퍼는 `#[cfg(not(coverage))]`. JS는 `npm run coverage` 4지표 100 (새 순수 .ts 모듈은 vitest.config.ts include + 테스트)
- 에러 arm 한 줄 let-else. 심링크는 모든 순회서 제외(scanner::keep_entry). 커밋: conventional commits + 트레일러 `Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>`
- **원격 main 직접 push 불가** — 브랜치 `feat/m4a-ontology-inventory`, 마지막 PR (스쿼시)
- `cargo` PATH 부재: bash `export PATH="$HOME/.cargo/bin:$PATH"`. cargo test 타임아웃 600000ms

---

### Task 1: 기본 온톨로지 애셋 + `ontology` 파싱

**Files:**
- Create: `src-tauri/resources/ontology/default.ttl` (번들 온톨로지)
- Create: `src-tauri/src/ontology.rs`
- Modify: `src-tauri/src/lib.rs` (`mod ontology;`), `src-tauri/Cargo.toml` (`sophia`), `src-tauri/tauri.conf.json` (resources 번들)
- Test: `src-tauri/src/ontology.rs` 내 tests

**Interfaces:**
- Consumes: 없음
- Produces:
  - `ontology::OntoClass { id: String, label: String, parent: Option<String>, target_folder: Option<String> }` (serde::Serialize)
  - `ontology::Ontology { classes: Vec<OntoClass> }` — 파싱 결과, 선언 순서 보존
  - `ontology::parse_ttl(turtle: &str) -> Result<Ontology, String>` — owl:Class + rdfs:subClassOf + rdfs:label + dm:targetFolder 추출
  - `ontology::Ontology::resolve_target(&self, class_id: &str) -> Option<String>` — 클래스에 targetFolder 없으면 가장 가까운 조상의 것 상속 (스펙 §5)

- [ ] **Step 1: 기본 온톨로지 애셋 작성**

`src-tauri/resources/ontology/default.ttl`:

```turtle
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm:   <https://disksage.app/ontology#> .

dm:Document a owl:Class ;
    rdfs:label "문서"@ko , "Document"@en ;
    dm:targetFolder "~/Documents/{class}" .

dm:Receipt a owl:Class ;
    rdfs:subClassOf dm:Document ;
    rdfs:label "영수증"@ko , "Receipt"@en .

dm:Media a owl:Class ;
    rdfs:label "미디어"@ko , "Media"@en ;
    dm:targetFolder "~/Media/{class}" .

dm:Image a owl:Class ;
    rdfs:subClassOf dm:Media ;
    rdfs:label "이미지"@ko , "Image"@en .

dm:Video a owl:Class ;
    rdfs:subClassOf dm:Media ;
    rdfs:label "영상"@ko , "Video"@en .

dm:Installer a owl:Class ;
    rdfs:label "설치파일"@ko , "Installer"@en ;
    dm:targetFolder "~/Installers" .

dm:Code a owl:Class ;
    rdfs:label "코드"@ko , "Code"@en ;
    dm:targetFolder "~/Code" .

dm:Dataset a owl:Class ;
    rdfs:label "데이터셋"@ko , "Dataset"@en ;
    dm:targetFolder "~/Datasets" .
```

- [ ] **Step 2: 의존성 추가 + 실패 테스트**

Run: `cd src-tauri; cargo add sophia`

`src-tauri/src/ontology.rs` 생성, 하단에:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm:   <https://disksage.app/ontology#> .

dm:Document a owl:Class ;
    rdfs:label "문서"@ko , "Document"@en ;
    dm:targetFolder "~/Documents/{class}" .

dm:Receipt a owl:Class ;
    rdfs:subClassOf dm:Document ;
    rdfs:label "영수증"@ko , "Receipt"@en .
"#;

    #[test]
    fn parses_classes_labels_parents_and_targets() {
        let onto = parse_ttl(SAMPLE).unwrap();
        let doc = onto.classes.iter().find(|c| c.id.ends_with("Document")).unwrap();
        assert_eq!(doc.parent, None);
        assert_eq!(doc.target_folder.as_deref(), Some("~/Documents/{class}"));
        // 라벨은 최소 하나(en 또는 ko) — 존재만 확인
        assert!(!doc.label.is_empty());

        let rcpt = onto.classes.iter().find(|c| c.id.ends_with("Receipt")).unwrap();
        assert!(rcpt.parent.as_deref().unwrap().ends_with("Document"));
        assert_eq!(rcpt.target_folder, None); // 자체 targetFolder 없음
    }

    #[test]
    fn resolve_target_inherits_from_ancestor() {
        let onto = parse_ttl(SAMPLE).unwrap();
        let rcpt_id = &onto.classes.iter().find(|c| c.id.ends_with("Receipt")).unwrap().id;
        // Receipt는 자체 targetFolder 없음 → Document의 것 상속
        assert_eq!(onto.resolve_target(rcpt_id).as_deref(), Some("~/Documents/{class}"));
    }

    #[test]
    fn resolve_target_none_for_unknown_class() {
        let onto = parse_ttl(SAMPLE).unwrap();
        assert_eq!(onto.resolve_target("https://x/Nonexistent"), None);
    }

    #[test]
    fn malformed_turtle_is_err() {
        assert!(parse_ttl("this is not turtle @@@").is_err());
    }

    #[test]
    fn default_ontology_asset_parses() {
        let ttl = include_str!("../resources/ontology/default.ttl");
        let onto = parse_ttl(ttl).unwrap();
        assert!(onto.classes.len() >= 8);
    }
}
```

- [ ] **Step 3: 실패 확인**

`lib.rs`에 `#[cfg_attr(coverage, allow(dead_code))] mod ontology;` 추가 후:

Run: `cd src-tauri; cargo test ontology`
Expected: COMPILE ERROR — `parse_ttl` not found

- [ ] **Step 4: 구현**

`src-tauri/src/ontology.rs` 상단. sophia 0.8 API 기준 (구현자: 설치된 sophia 버전의 실제 API에 맞춰 조정 — 아래는 의도를 담은 참조. Turtle 파싱 → 트리플 순회 → 관심 술어만 추출):

```rust
use sophia::api::prelude::*;
use sophia::api::term::Term;
use sophia::turtle::parser::turtle;
use std::collections::BTreeMap;

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const DM_TARGET: &str = "https://disksage.app/ontology#targetFolder";

#[derive(Debug, Clone, serde::Serialize)]
pub struct OntoClass {
    pub id: String,
    pub label: String,
    pub parent: Option<String>,
    pub target_folder: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Ontology {
    pub classes: Vec<OntoClass>,
}

/// owl:Class 선언 + subClassOf/label/targetFolder를 추출. 추론 없음 — 명시된 트리플만.
pub fn parse_ttl(turtle_src: &str) -> Result<Ontology, String> {
    use sophia::inmem::graph::LightGraph;
    let mut graph: LightGraph = LightGraph::new();
    turtle::parse_str(turtle_src)
        .add_to_graph(&mut graph)
        .map_err(|e| e.to_string())?;

    // subject IRI → 필드. 선언 순서 보존을 위해 owl:Class 등장 순서를 별도 벡터로.
    let mut order: Vec<String> = Vec::new();
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut targets: BTreeMap<String, String> = BTreeMap::new();

    let iri_of = |t: &dyn Term| -> Option<String> { t.iri().map(|i| i.to_string()) };

    for tr in graph.triples() {
        let tr = tr.map_err(|e| e.to_string())?;
        let s = match iri_of(tr.s()) { Some(x) => x, None => continue };
        let p = match iri_of(tr.p()) { Some(x) => x, None => continue };
        match p.as_str() {
            RDF_TYPE => {
                if iri_of(tr.o()).as_deref() == Some(OWL_CLASS) && !order.contains(&s) {
                    order.push(s);
                }
            }
            RDFS_SUBCLASS => {
                if let Some(o) = iri_of(tr.o()) {
                    parents.insert(s, o);
                }
            }
            RDFS_LABEL => {
                // 첫 라벨만 취함(언어 무관) — 이미 있으면 유지
                if let Some(lit) = tr.o().lexical_form() {
                    labels.entry(s).or_insert_with(|| lit.to_string());
                }
            }
            DM_TARGET => {
                if let Some(lit) = tr.o().lexical_form() {
                    targets.insert(s, lit.to_string());
                }
            }
            _ => {}
        }
    }

    let classes = order
        .into_iter()
        .map(|id| OntoClass {
            label: labels.get(&id).cloned().unwrap_or_else(|| id.clone()),
            parent: parents.get(&id).cloned(),
            target_folder: targets.get(&id).cloned(),
            id,
        })
        .collect();

    Ok(Ontology { classes })
}

impl Ontology {
    /// targetFolder를 자기 자신부터 조상까지 올라가며 찾는다 (스펙 §5 상속).
    pub fn resolve_target(&self, class_id: &str) -> Option<String> {
        let mut cur = class_id.to_string();
        // 사이클/최대 깊이 방어
        for _ in 0..64 {
            let cls = self.classes.iter().find(|c| c.id == cur)?;
            if let Some(t) = &cls.target_folder {
                return Some(t.clone());
            }
            cur = cls.parent.clone()?;
        }
        None
    }
}
```

주의(구현자): sophia의 실제 버전 API가 위와 다르면(트리플 접근자, 리터럴 lexical_form, 파서 진입점 이름 등) 컴파일되도록 조정하되 **의도를 유지**: (1) Turtle 파싱, (2) owl:Class 주어를 선언 순서로 수집, (3) subClassOf/label/targetFolder 추출, (4) resolve_target의 조상 상속. API 조정이 크면 보고서에 정확히 기록. sophia 버전이 심하게 안 맞으면 `oxttl`+`oxrdf`(oxigraph 계열, 더 가벼운 Turtle 파서)로 대체 가능 — 이 경우 Produces 시그니처는 동일 유지.

- [ ] **Step 5: 통과 확인 + tauri.conf.json 번들 등록**

`src-tauri/tauri.conf.json`의 `bundle` 섹션에 `"resources": ["resources/ontology/default.ttl"]` 추가 (기존 resources 배열 있으면 병합).

Run: `cd src-tauri; cargo test ontology`
Expected: 5 tests PASS

- [ ] **Step 6: Commit**

```powershell
git add src-tauri
git commit -m "feat(ontology): OWL Turtle parsing with target-folder inheritance

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 2: `inventory` — 저비용 분류 + 집계

**Files:**
- Create: `src-tauri/src/inventory.rs`
- Modify: `src-tauri/src/lib.rs` (`mod inventory;`)
- Test: `src-tauri/src/inventory.rs` 내 tests

**Interfaces:**
- Consumes: `dupes::FileEntry`(재사용 — path+size), `ontology::Ontology`
- Produces:
  - `inventory::classify(path: &Path) -> Option<&'static str>` — 확장자 기반 저비용 분류기. 클래스 로컬 id 반환(예: "Image"), 미분류는 None
  - `inventory::ClassTally { class_id: String, label: String, bytes: u64, count: u64 }` (serde::Serialize)
  - `inventory::InventoryReport { tallies: Vec<ClassTally>, unknown_bytes: u64, unknown_count: u64 }` (serde::Serialize) — tallies는 바이트 내림차순, Unknown은 별도 필드(일급 시민, 스펙 §5)
  - `inventory::build_inventory(files: &[FileEntry], onto: &Ontology) -> InventoryReport`

- [ ] **Step 1: 실패 테스트 작성**

`src-tauri/src/inventory.rs` 생성, 하단에:

```rust
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
        assert_eq!(classify(&PathBuf::from("/x/a.png")), Some("Image"));
        assert_eq!(classify(&PathBuf::from("/x/b.JPG")), Some("Image")); // 대소문자 무관
        assert_eq!(classify(&PathBuf::from("/x/c.rs")), Some("Code"));
        assert_eq!(classify(&PathBuf::from("/x/unknownext.xyz")), None);
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
        assert_eq!(rep.tallies[0].class_id, "Image");
        assert_eq!(rep.tallies[0].bytes, 300);
        assert_eq!(rep.tallies[0].count, 2);
        assert_eq!(rep.tallies[1].class_id, "Code");
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
```

- [ ] **Step 2: 실패 확인**

`lib.rs`에 `#[cfg_attr(coverage, allow(dead_code))] mod inventory;` 추가 후:

Run: `cd src-tauri; cargo test inventory`
Expected: COMPILE ERROR

- [ ] **Step 3: 구현**

`src-tauri/src/inventory.rs` 상단:

```rust
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
```

- [ ] **Step 4: 통과 확인 + Commit**

Run: `cd src-tauri; cargo test inventory`
Expected: 4 tests PASS

```powershell
git add src-tauri
git commit -m "feat(inventory): extension-based classification and class tallies

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 3: 커맨드 계층 — 인벤토리 IPC

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`
- Test: commands.rs tests (순수 로더 함수)

**Interfaces:**
- Consumes: `ontology`, `inventory`, `dupes::collect_files`
- Produces:
  - `commands::load_ontology_from(ttl: &str) -> Result<ontology::Ontology, String>` (순수 — 테스트 대상, parse_ttl 래핑 + 폴백 없음)
  - command `disk_inventory(root: String) -> Result<inventory::InventoryReport, String>` (`#[cfg(not(coverage))]`) — 번들 온톨로지 로드 → collect_files → build_inventory
  - command `get_ontology() -> Result<ontology::Ontology, String>` (`#[cfg(not(coverage))]`) — 번들(또는 사용자 설정 디렉토리 오버라이드) 온톨로지 반환

- [ ] **Step 1: 순수 로더 테스트**

commands.rs tests에 추가:

```rust
    #[test]
    fn load_ontology_from_valid_ttl_ok() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:Image a owl:Class ; rdfs:label "이미지"@ko .
"#;
        let onto = load_ontology_from(ttl).unwrap();
        assert_eq!(onto.classes.len(), 1);
    }

    #[test]
    fn load_ontology_from_garbage_is_err() {
        assert!(load_ontology_from("@@@ not turtle").is_err());
    }
```

- [ ] **Step 2: 실패 확인**

Run: `cd src-tauri; cargo test commands`
Expected: COMPILE ERROR — `load_ontology_from` not found

- [ ] **Step 3: 구현**

commands.rs에 추가 (`use crate::{inventory, ontology};` — 순수 로더가 ontology를 쓰므로 ontology import는 무조건, inventory/dupes 등 래퍼 전용은 `#[cfg(not(coverage))]`):

```rust
/// 순수: TTL 문자열 → Ontology (테스트 대상). 잘못된 TTL은 Err.
pub fn load_ontology_from(ttl: &str) -> Result<ontology::Ontology, String> {
    ontology::parse_ttl(ttl)
}

#[cfg(not(coverage))]
fn bundled_ontology_ttl(app: &AppHandle) -> Result<String, String> {
    use tauri::Manager;
    // 사용자 설정 디렉토리 오버라이드 우선, 없으면 번들 리소스
    if let Ok(dir) = app.path().app_config_dir() {
        let user_ttl = dir.join("ontology.ttl");
        if let Ok(s) = std::fs::read_to_string(&user_ttl) {
            return Ok(s);
        }
    }
    let res = app
        .path()
        .resolve("resources/ontology/default.ttl", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())?;
    std::fs::read_to_string(&res).map_err(|e| e.to_string())
}

#[cfg(not(coverage))]
#[tauri::command]
pub fn get_ontology(app: AppHandle) -> Result<ontology::Ontology, String> {
    load_ontology_from(&bundled_ontology_ttl(&app)?)
}

#[cfg(not(coverage))]
#[tauri::command(async)]
pub fn disk_inventory(root: String, app: AppHandle) -> Result<inventory::InventoryReport, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    let files = crate::dupes::collect_files(std::path::Path::new(&root));
    Ok(inventory::build_inventory(&files, &onto))
}
```

`lib.rs` invoke_handler에 `commands::get_ontology, commands::disk_inventory` 추가.

- [ ] **Step 4: 전체 테스트 + 양쪽 check**

Run: `cd src-tauri; cargo test`
Expected: 전체 PASS

Run (bash): `RUSTFLAGS="--cfg coverage" cargo check`
Expected: 경고 0

- [ ] **Step 5: Commit**

```powershell
git add src-tauri
git commit -m "feat(commands): ontology and disk-inventory IPC

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 4: 프론트엔드 — 인벤토리 뷰

**Files:**
- Create: `src/lib/Inventory.svelte`
- Modify: `src/lib/api.ts`, `src/routes/+page.svelte`

**Interfaces:**
- Consumes: Task 3 커맨드, `fmtBytes`
- Produces: api.ts `diskInventory(root)`, `getOntology()` + 타입 `InventoryReport, ClassTally, OntoClass, Ontology`; `Inventory.svelte` props `{ scannedRoot: string | null }`

- [ ] **Step 1: api.ts 추가**

`src/lib/api.ts`에 추가:

```typescript
export interface ClassTally {
  class_id: string;
  label: string;
  bytes: number;
  count: number;
}
export interface InventoryReport {
  tallies: ClassTally[];
  unknown_bytes: number;
  unknown_count: number;
}
export interface OntoClass {
  id: string;
  label: string;
  parent: string | null;
  target_folder: string | null;
}
export interface Ontology {
  classes: OntoClass[];
}

export const diskInventory = (root: string) =>
  invoke<InventoryReport>("disk_inventory", { root });
export const getOntology = () => invoke<Ontology>("get_ontology");
```

- [ ] **Step 2: Inventory.svelte 작성**

`src/lib/Inventory.svelte`:

```svelte
<script lang="ts">
  import * as api from "./api";
  import { fmtBytes } from "./fmt";

  let { scannedRoot }: { scannedRoot: string | null } = $props();

  let report: api.InventoryReport | null = $state(null);
  let busy = $state(false);
  let loadError = $state("");

  async function load() {
    if (!scannedRoot) return;
    busy = true;
    loadError = "";
    try {
      report = await api.diskInventory(scannedRoot);
    } catch (e) {
      loadError = String(e);
    } finally {
      busy = false;
    }
  }

  let totalBytes = $derived(
    report
      ? report.tallies.reduce((s, t) => s + t.bytes, 0) + report.unknown_bytes
      : 0,
  );

  function pct(bytes: number): number {
    return totalBytes > 0 ? Math.round((bytes / totalBytes) * 100) : 0;
  }
</script>

<section>
  <h2>
    인벤토리 {scannedRoot ? "" : "(먼저 스캔하세요)"}
    <button onclick={load} disabled={busy || !scannedRoot}>{busy ? "집계 중…" : "인벤토리 집계"}</button>
  </h2>
  {#if loadError}<p class="error">{loadError}</p>{/if}

  {#if report}
    <ul class="bars">
      {#each report.tallies as t (t.class_id)}
        <li>
          <div class="row">
            <span class="label">{t.label}</span>
            <span class="size">{fmtBytes(t.bytes)} · {t.count}개 · {pct(t.bytes)}%</span>
          </div>
          <div class="bar"><div class="fill" style="width:{pct(t.bytes)}%"></div></div>
        </li>
      {/each}
      {#if report.unknown_count > 0}
        <li class="unknown">
          <div class="row">
            <span class="label">미분류 <em>(무엇인지 모르는 용량)</em></span>
            <span class="size">{fmtBytes(report.unknown_bytes)} · {report.unknown_count}개 · {pct(report.unknown_bytes)}%</span>
          </div>
          <div class="bar"><div class="fill unk" style="width:{pct(report.unknown_bytes)}%"></div></div>
        </li>
      {/if}
    </ul>
  {/if}
</section>

<style>
  section { margin-top: 1.5rem; border-top: 1px solid #ddd; padding-top: 1rem; }
  h2 { display: flex; gap: 0.75rem; align-items: center; }
  .bars { list-style: none; padding: 0; }
  .bars li { margin: 0.4rem 0; }
  .row { display: flex; justify-content: space-between; font-size: 0.9rem; }
  .size { color: #666; font-variant-numeric: tabular-nums; }
  .bar { background: #eee; border-radius: 3px; height: 8px; overflow: hidden; }
  .fill { background: #4a90d9; height: 100%; }
  .fill.unk { background: #d98a4a; }
  .unknown .label em { color: #a60; font-style: normal; font-size: 0.8rem; }
  .error { color: #b00; }
</style>
```

- [ ] **Step 3: +page.svelte 연결**

script에 `import Inventory from "$lib/Inventory.svelte";`, markup에서 `<Duplicates .../>` 위 또는 아래에:

```svelte
  <Inventory scannedRoot={crumbs.length > 0 ? crumbs[0] : null} />
```

- [ ] **Step 4: 게이트**

Run: `npm run build`, `npm run check`, `npm test`, `npm run coverage`
Expected: 모두 클린 (Inventory.svelte는 순수 .ts 아님 — vitest.config.ts 변경 없음)

- [ ] **Step 5: Commit**

```powershell
git add src
git commit -m "feat(ui): disk inventory view with first-class unknown bucket

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

---

### Task 5: 커버리지 정합 + 최종 검증 + PR

**Files:** 필요 시 테스트 추가

- [ ] **Step 1: 로컬 측정**

Run (bash, src-tauri): `cargo llvm-cov --all-features --fail-under-lines 100 --show-missing-lines`

- [ ] **Step 2: 갭 폐쇄**

**중요 교훈(M3서 확인)**: `let Ok(x) = ... else { ... }` 에러 arm은 해피패스 테스트로 라인 커버 안 됨 — else 본문이 미커버로 잡힌다. 로컬 Windows llvm-cov는 cfg(unix) 코드가 빠져 예측이 빗나가니, 새 함수의 모든 에러 arm에 실제로 그 arm을 실행하는 테스트를 붙일 것. sophia 파싱 에러, resolve_target의 `?`/None 경로 등.

허용 미커버: cfg(unix)-only 테스트 커버 arm(리눅스서 실행), cfg(windows)-only 경로, `#[cfg(not(coverage))]` 래퍼(게이트서 사라짐). 그 외는 테스트 추가. 특히:
- `ontology::parse_ttl`의 에러 arm(malformed) — `malformed_turtle_is_err` 커버. sophia API 조정으로 새 arm이 생기면 테스트 추가
- `resolve_target`의 사이클 방어 루프(0..64) — 정상 상속과 없는 클래스 케이스가 커버; 사이클 자체 케이스가 미커버면 자기참조 온톨로지 테스트 추가
- `classify`의 각 확장자 arm — 대표 확장자별 케이스가 커버, `_ => None`도 커버
- `build_inventory`의 Some/None 분기 — 기존 테스트 커버
- `#[cfg(not(coverage))]` 커맨드/로더(bundled_ontology_ttl 등) 재확인 `RUSTFLAGS="--cfg coverage" cargo check`

- [ ] **Step 3: 전체 게이트**

Run: `cd src-tauri; cargo test` → PASS
Run: `npm test; npm run coverage; npm run build; npm run check` → 클린
Run (bash): `RUSTFLAGS="--cfg coverage" cargo check` → 경고 0

- [ ] **Step 4: Commit (보강분 있으면)**

```powershell
git add src-tauri
git commit -m "test(coverage): close m4a line-coverage gaps for the linux gate

Co-Authored-By: Claude Fable 5 <noreply@anthropic.com>"
```

- [ ] **Step 5: 푸시 + PR**

```powershell
git push -u origin feat/m4a-ontology-inventory
gh pr create --repo ContextualWisdomLab/disksage --base main --head feat/m4a-ontology-inventory --title "feat: M4a OWL ontology + disk inventory (read-only)" --body "M4a milestone per docs/superpowers/specs/2026-07-10-disksage-design.md §5. First half of M4, read-only: OWL Turtle ontology parsing (owl:Class + rdfs:subClassOf + rdfs:label + dm:targetFolder, target-folder inheritance up the class hierarchy, no reasoner per spec non-goals), extension-based low-cost classification, class-tally inventory with a first-class Unknown bucket (surfaces 'space you can't account for'). Bundled default.ttl (user-overridable, Protege-editable). No delete/move code — reorganization (the destructive move) is M4b.

🤖 Generated with [Claude Code](https://claude.com/claude-code)"
```

- [ ] **Step 6: 사람 검증 체크리스트 (사용자 전달용)**

1. `npm run tauri dev` → 스캔 → "인벤토리 집계" → 클래스별 막대 + 미분류 버킷 표시
2. 이미지/코드/문서 섞인 폴더에서 분류가 맞는지, 미분류 용량이 드러나는지
3. (선택) 앱 설정 디렉토리에 `ontology.ttl` 놓고 오버라이드되는지
