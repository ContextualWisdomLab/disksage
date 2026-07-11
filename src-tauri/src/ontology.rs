//! OWL Turtle 온톨로지 파싱 (스펙 §5). 추론 없음 — 명시된 트리플만 읽고
//! owl:Class 선언, rdfs:subClassOf, rdfs:label, dm:targetFolder를 추출한다.
//!
//! RDF 크레이트: `oxttl` + `oxrdf` (oxigraph 계열). 브리프는 `sophia`를 참조로
//! 제시했으나 설치 시점 최신 버전이 0.10(브리프 기준 0.8과 API 상이)이고
//! 기본 피처에 jsonld/sparql/xml/reasoner 등 이번 파싱에 불필요한 의존성이
//! 딸려온다. oxttl/oxrdf는 Turtle 파싱 전용이라 의존성이 가볍고 API가 안정적.
use oxrdf::{NamedOrBlankNode, Term, Triple};
use oxttl::TurtleParser;
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

/// owl:Class 주어를 선언 순서로 수집하고 subClassOf/label/targetFolder를 매칭한다.
pub fn parse_ttl(turtle_src: &str) -> Result<Ontology, String> {
    let mut order: Vec<String> = Vec::new();
    let mut parents: BTreeMap<String, String> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut targets: BTreeMap<String, String> = BTreeMap::new();

    for triple in TurtleParser::new().for_reader(turtle_src.as_bytes()) {
        let triple: Triple = triple.map_err(|e| e.to_string())?;
        let s = match &triple.subject {
            NamedOrBlankNode::NamedNode(n) => n.as_str().to_string(),
            // 블랭크노드 주어는 온톨로지 클래스 식별자가 아니므로 무시
            NamedOrBlankNode::BlankNode(_) => continue,
        };
        match triple.predicate.as_str() {
            RDF_TYPE => {
                if let Term::NamedNode(o) = &triple.object {
                    if o.as_str() == OWL_CLASS && !order.contains(&s) {
                        order.push(s);
                    }
                }
            }
            RDFS_SUBCLASS => {
                if let Term::NamedNode(o) = &triple.object {
                    // 다중 rdfs:subClassOf는 첫 선언만 부모로 채택(단일 부모 트리 가정) —
                    // targetFolder 상속의 결정성을 위해. OWL은 다중 상위를 허용하지만
                    // 이동 목적지가 arbitrary 분기에 좌우되지 않도록 first-wins 고정.
                    parents.entry(s).or_insert_with(|| o.as_str().to_string());
                }
            }
            RDFS_LABEL => {
                // 첫 라벨만 취함(언어 무관) — 이미 있으면 유지
                if let Term::Literal(lit) = &triple.object {
                    labels.entry(s).or_insert_with(|| lit.value().to_string());
                }
            }
            DM_TARGET => {
                if let Term::Literal(lit) = &triple.object {
                    targets.insert(s, lit.value().to_string());
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
    /// targetFolder를 자기 자신부터 조상까지 올라가며 찾는다 (스펙 §5 상속). 추론 없음.
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

# 아래 줄들은 파싱 분기 커버리지용 잡음 트리플 — Document 값에는 영향 없음
dm:Document a owl:Class .                 # 중복 선언: order 중복 방지 분기
dm:Document a "리터럴은 클래스 아님" .        # rdf:type 오브젝트가 IRI가 아닌 경우
dm:Document rdfs:label dm:Receipt .       # 라벨 오브젝트가 리터럴이 아닌 경우
dm:Document dm:targetFolder dm:Receipt .  # targetFolder 오브젝트가 리터럴이 아닌 경우
dm:Document rdfs:comment "메모"@ko .       # 관심 없는 predicate(무시) 분기
[] a owl:Class .                          # 블랭크노드 주어는 무시

dm:Receipt a owl:Class ;
    rdfs:subClassOf dm:Document ;
    rdfs:label "영수증"@ko , "Receipt"@en .
"#;

    const CYCLE: &str = r#"
@prefix owl:  <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm:   <https://disksage.app/ontology#> .

dm:A a owl:Class ;
    rdfs:subClassOf dm:B .
dm:B a owl:Class ;
    rdfs:subClassOf dm:A .
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
    fn preserves_declaration_order() {
        // 계약: owl:Class 선언 순서 보존. SAMPLE은 Document 먼저, Receipt 다음.
        let onto = parse_ttl(SAMPLE).unwrap();
        assert!(onto.classes[0].id.ends_with("Document"));
        assert!(onto.classes[1].id.ends_with("Receipt"));
    }

    #[test]
    fn multiple_subclassof_keeps_first_parent_deterministically() {
        // OWL은 다중 상위클래스를 허용하지만 targetFolder 상속은 결정적이어야 한다.
        // 정책: 첫 subClassOf 선언(문서 순서)을 부모로 채택, 이후는 무시.
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:A a owl:Class ; dm:targetFolder "~/A" .
dm:B a owl:Class ; dm:targetFolder "~/B" .
dm:C a owl:Class ; rdfs:subClassOf dm:A ; rdfs:subClassOf dm:B .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let c = onto.classes.iter().find(|c| c.id.ends_with("#C")).unwrap();
        assert!(c.parent.as_deref().unwrap().ends_with("#A"), "첫 부모 A 채택");
        assert_eq!(onto.resolve_target(&c.id).as_deref(), Some("~/A"));
    }

    #[test]
    fn default_ontology_asset_parses() {
        let ttl = include_str!("../resources/ontology/default.ttl");
        let onto = parse_ttl(ttl).unwrap();
        assert!(onto.classes.len() >= 8);
    }

    #[test]
    fn default_asset_inheritance_resolves_against_real_ttl() {
        // 실제 번들 애셋에서 상속이 동작하는지 (오타/네임스페이스 불일치 방어)
        let ttl = include_str!("../resources/ontology/default.ttl");
        let onto = parse_ttl(ttl).unwrap();
        let find = |suffix: &str| {
            onto.classes.iter().find(|c| c.id.ends_with(suffix)).map(|c| c.id.clone())
        };
        // Receipt → Document의 폴더 상속
        let receipt = find("Receipt").unwrap();
        assert_eq!(onto.resolve_target(&receipt).as_deref(), Some("~/Documents/{class}"));
        // Image → Media의 폴더 상속
        let image = find("Image").unwrap();
        assert_eq!(onto.resolve_target(&image).as_deref(), Some("~/Media/{class}"));
        // Installer는 자체 폴더
        let installer = find("Installer").unwrap();
        assert_eq!(onto.resolve_target(&installer).as_deref(), Some("~/Installers"));
    }

    #[test]
    fn resolve_target_none_when_parent_chain_cycles() {
        // targetFolder가 없는 상호 순환 subClassOf — 최대 깊이 방어가 None으로 종료되어야 함
        let onto = parse_ttl(CYCLE).unwrap();
        let a_id = &onto.classes.iter().find(|c| c.id.ends_with('A')).unwrap().id;
        assert_eq!(onto.resolve_target(a_id), None);
    }
}
