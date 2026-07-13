//! OWL Turtle 온톨로지 파싱 (스펙 §5). 추론 없음 — 명시된 트리플만 읽고
//! owl:Class 선언, rdfs:subClassOf, rdfs:label, dm:targetFolder를 추출한다.
//!
//! RDF 크레이트: `oxttl` + `oxrdf` (oxigraph 계열). 브리프는 `sophia`를 참조로
//! 제시했으나 설치 시점 최신 버전이 0.10(브리프 기준 0.8과 API 상이)이고
//! 기본 피처에 jsonld/sparql/xml/reasoner 등 이번 파싱에 불필요한 의존성이
//! 딸려온다. oxttl/oxrdf는 Turtle 파싱 전용이라 의존성이 가볍고 API가 안정적.
use oxrdf::{NamedOrBlankNode, Term, Triple};
use oxttl::TurtleParser;
use std::collections::{BTreeMap, BTreeSet};

const OWL_CLASS: &str = "http://www.w3.org/2002/07/owl#Class";
const RDF_TYPE: &str = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type";
const RDFS_SUBCLASS: &str = "http://www.w3.org/2000/01/rdf-schema#subClassOf";
const RDFS_LABEL: &str = "http://www.w3.org/2000/01/rdf-schema#label";
const DM_TARGET: &str = "https://disksage.app/ontology#targetFolder";
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";

#[derive(Debug, Clone, serde::Serialize)]
pub struct OntoClass {
    pub id: String,
    pub label: String,
    pub parents: Vec<String>,
    pub equivalents: Vec<String>,
    pub disjoints: Vec<String>,
    pub target_folder: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Ontology {
    pub classes: Vec<OntoClass>,
}

/// owl:Class 주어를 선언 순서로 수집하고 subClassOf/label/targetFolder를 매칭한다.
pub fn parse_ttl(turtle_src: &str) -> Result<Ontology, String> {
    let mut order: Vec<String> = Vec::new();
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut equivalents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut disjoints: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut labels: BTreeMap<String, String> = BTreeMap::new();
    let mut targets: BTreeMap<String, String> = BTreeMap::new();

    // 명명 노드 오브젝트만 채택, 순서 보존, 동일 오브젝트 중복 무시.
    let push = |map: &mut BTreeMap<String, Vec<String>>, s: String, o: &Term| {
        if let Term::NamedNode(o) = o {
            let v = map.entry(s).or_default();
            let o = o.as_str().to_string();
            if !v.contains(&o) {
                v.push(o);
            }
        }
    };

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
            // 다중 rdfs:subClassOf/owl:equivalentClass/owl:disjointWith를 선언 순서대로
            // 모두 수집한다(Task 2 추론기 입력). resolve_target은 여전히 첫 부모만 사용.
            RDFS_SUBCLASS => push(&mut parents, s, &triple.object),
            OWL_EQUIVALENT_CLASS => push(&mut equivalents, s, &triple.object),
            OWL_DISJOINT_WITH => push(&mut disjoints, s, &triple.object),
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
            parents: parents.get(&id).cloned().unwrap_or_default(),
            equivalents: equivalents.get(&id).cloned().unwrap_or_default(),
            disjoints: disjoints.get(&id).cloned().unwrap_or_default(),
            target_folder: targets.get(&id).cloned(),
            id,
        })
        .collect();

    Ok(Ontology { classes })
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Issue {
    /// C ⊑ c1, C ⊑ c2, c1 disjointWith c2 ⇒ C ⊑ owl:Nothing (sound TBox consequence of cax-dw).
    UnsatisfiableClass { class: String, via_disjoint: (String, String) },
}

pub struct Reasoner {
    rep: BTreeMap<String, String>,        // class id → equivalence representative
    groups: BTreeMap<String, Vec<String>>, // rep → sorted members
    sup: BTreeMap<String, BTreeSet<String>>, // rep → direct super-reps (acyclic after fixpoint)
    disjoint_pairs: Vec<(String, String)>, // raw (subject, disjointWith-object) axiom ids, captured at build time
    // (target_folder is read from the Ontology, only needed by Ontology::resolve_target)
}

impl Reasoner {
    pub fn build(onto: &Ontology) -> Reasoner {
        let ids: Vec<String> = onto.classes.iter().map(|c| c.id.clone()).collect();
        // union-find
        let mut rep: BTreeMap<String, String> = ids.iter().map(|i| (i.clone(), i.clone())).collect();
        fn find(rep: &mut BTreeMap<String, String>, x: &str) -> String {
            // ponytail: every call site below only ever passes an id already seeded into `rep`
            // (either a class id from `ids`, or a `contains_key`-guarded axiom target) and `union`
            // only ever overwrites existing keys, so `x` is always present — no silent fallback needed.
            let p = rep[x].clone();
            if p == x { return p; }
            let r = find(rep, &p);
            rep.insert(x.to_string(), r.clone());
            r
        }
        fn union(rep: &mut BTreeMap<String, String>, a: &str, b: &str) {
            let (ra, rb) = (find(rep, a), find(rep, b));
            if ra != rb {
                // deterministic: smaller id becomes representative
                let (keep, drop) = if ra <= rb { (ra, rb) } else { (rb, ra) };
                rep.insert(drop, keep);
            }
        }
        // scm-eqc1: explicit equivalentClass pairs (only among known classes)
        for c in &onto.classes {
            for e in &c.equivalents {
                if rep.contains_key(e) { union(&mut rep, &c.id, e); }
            }
        }
        // scm-eqc2 fixpoint: collapse subClassOf cycles among representatives until none remain
        loop {
            // edges on current reps
            let mut edges: BTreeSet<(String, String)> = BTreeSet::new();
            for c in &onto.classes {
                let rc = find(&mut rep, &c.id);
                for p in &c.parents {
                    if !rep.contains_key(p) { continue; }
                    let rp = find(&mut rep, p);
                    if rc != rp { edges.insert((rc.clone(), rp.clone())); }
                }
            }
            // reachability on rep graph
            let mut reach: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (u, v) in &edges { reach.entry(u.clone()).or_default().insert(v.clone()); }
            let nodes: BTreeSet<String> = edges.iter().flat_map(|(u, v)| [u.clone(), v.clone()]).collect();
            loop {
                let mut changed = false;
                for n in &nodes {
                    let outs: Vec<String> = reach.get(n).cloned().unwrap_or_default().into_iter().collect();
                    for m in outs {
                        let ms: Vec<String> = reach.get(&m).cloned().unwrap_or_default().into_iter().collect();
                        for t in ms { if reach.entry(n.clone()).or_default().insert(t) { changed = true; } }
                    }
                }
                if !changed { break; }
            }
            // find a mutual-reachability pair (cycle) and union it
            let mut merged = false;
            'outer: for a in &nodes {
                for b in reach.get(a).cloned().unwrap_or_default() {
                    if a != &b && reach.get(&b).map(|s| s.contains(a)).unwrap_or(false) {
                        union(&mut rep, a, &b);
                        merged = true;
                        break 'outer;
                    }
                }
            }
            if !merged { break; }
        }
        // groups
        let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for id in &ids {
            let r = find(&mut rep, id);
            groups.entry(r).or_default().push(id.clone());
        }
        for m in groups.values_mut() { m.sort(); m.dedup(); }
        // super-reps (direct), acyclic
        let mut sup: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for c in &onto.classes {
            let rc = find(&mut rep, &c.id);
            for p in &c.parents {
                if !rep.contains_key(p) { continue; }
                let rp = find(&mut rep, p);
                if rc != rp { sup.entry(rc.clone()).or_default().insert(rp); }
            }
        }
        // raw disjointWith axiom pairs, captured for check_coherence (no Ontology re-access needed)
        let mut disjoint_pairs: Vec<(String, String)> = Vec::new();
        for c in &onto.classes {
            for d in &c.disjoints {
                disjoint_pairs.push((c.id.clone(), d.clone()));
            }
        }
        Reasoner { rep, groups, sup, disjoint_pairs }
    }

    fn rep_of(&self, id: &str) -> Option<String> { self.rep.get(id).cloned() }

    /// reps reachable from `r` (excl. self), transitive.
    fn closure(&self, r: &str) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        let mut stack: Vec<String> = self.sup.get(r).into_iter().flatten().cloned().collect();
        while let Some(x) = stack.pop() {
            if out.insert(x.clone()) {
                stack.extend(self.sup.get(&x).into_iter().flatten().cloned());
            }
        }
        out
    }

    /// All (proper + improper via equivalents) superclass ids of `class_id`, sorted.
    pub fn ancestors(&self, class_id: &str) -> Vec<String> {
        let Some(r) = self.rep_of(class_id) else { return Vec::new() };
        let mut reps = self.closure(&r);
        reps.insert(r); // scm-cls reflexive
        let mut out: BTreeSet<String> = BTreeSet::new();
        for rep in reps { out.extend(self.groups.get(&rep).into_iter().flatten().cloned()); }
        out.insert(class_id.to_string()); // include self (scm-cls c ⊑ c)
        out.into_iter().collect()
    }

    /// Members equivalent to `class_id` (its group), sorted.
    pub fn equivalents(&self, class_id: &str) -> Vec<String> {
        self.rep_of(class_id).and_then(|r| self.groups.get(&r).cloned()).unwrap_or_default()
    }

    /// Groups formed by folding a subClassOf cycle (size > 1), advisory only.
    pub fn cycle_equivalences(&self) -> Vec<Vec<String>> {
        self.groups.values().filter(|g| g.len() > 1).cloned().collect()
    }

    /// Unsatisfiable classes (cax-dw TBox consequence). Empty = coherent.
    pub fn check_coherence(&self) -> Vec<Issue> {
        // disjoint axioms as (rep_a, rep_b, a_id, b_id); skip axioms whose object is an undeclared class
        let mut dis: Vec<(String, String, String, String)> = Vec::new();
        for (a_id, b_id) in &self.disjoint_pairs {
            if let (Some(ra), Some(rb)) = (self.rep_of(a_id), self.rep_of(b_id)) {
                dis.push((ra, rb, a_id.clone(), b_id.clone()));
            }
        }
        let mut out: Vec<Issue> = Vec::new();
        for c_id in self.rep.keys() {
            let Some(cr) = self.rep_of(c_id) else { continue };
            let mut clo = self.closure(&cr);
            clo.insert(cr); // incl self
            // C unsat iff some disjoint pair has BOTH reps in C's closure (covers ra==rb corner)
            if let Some((_, _, a, b)) = dis.iter().find(|(ra, rb, _, _)| clo.contains(ra) && clo.contains(rb)) {
                out.push(Issue::UnsatisfiableClass { class: c_id.clone(), via_disjoint: (a.clone(), b.clone()) });
            }
        }
        out
    }
}

impl Ontology {
    fn reasoner(&self) -> Reasoner { Reasoner::build(self) }

    /// targetFolder from the nearest ancestor/equivalent (BFS hops; ties by ascending class id).
    pub fn resolve_target(&self, class_id: &str) -> Option<String> {
        let r = self.reasoner();
        let start = r.rep_of(class_id)?;
        // BFS distance over rep sup-graph
        let mut dist: BTreeMap<String, usize> = BTreeMap::new();
        dist.insert(start.clone(), 0);
        let mut q = std::collections::VecDeque::from([start]);
        while let Some(u) = q.pop_front() {
            let d = dist[&u];
            for v in r.sup.get(&u).into_iter().flatten() {
                if !dist.contains_key(v) { dist.insert(v.clone(), d + 1); q.push_back(v.clone()); }
            }
        }
        // candidates: classes with a target whose rep is reachable; pick min (dist, id)
        let mut best: Option<(usize, String, String)> = None;
        for c in &self.classes {
            let Some(t) = &c.target_folder else { continue };
            let Some(cr) = r.rep_of(&c.id) else { continue };
            let Some(&d) = dist.get(&cr) else { continue };
            let cand = (d, c.id.clone(), t.clone());
            if best.as_ref().map(|b| (cand.0, &cand.1) < (b.0, &b.1)).unwrap_or(true) { best = Some(cand); }
        }
        best.map(|(_, _, t)| t)
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
        assert!(doc.parents.is_empty());
        assert_eq!(doc.target_folder.as_deref(), Some("~/Documents/{class}"));
        assert!(!doc.label.is_empty());
        let rcpt = onto.classes.iter().find(|c| c.id.ends_with("Receipt")).unwrap();
        assert!(rcpt.parents.iter().any(|p| p.ends_with("Document")));
        assert_eq!(rcpt.target_folder, None);
    }

    #[test]
    fn parses_multiple_subclassof_equivalent_and_disjoint() {
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:A a owl:Class . dm:B a owl:Class . dm:P a owl:Class . dm:Q a owl:Class .
dm:C a owl:Class ; rdfs:subClassOf dm:A ; rdfs:subClassOf dm:B ;
    owl:equivalentClass dm:P ; owl:disjointWith dm:Q .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let c = onto.classes.iter().find(|c| c.id.ends_with("#C")).unwrap();
        assert_eq!(c.parents.len(), 2);
        assert!(c.parents.iter().any(|p| p.ends_with("#A")) && c.parents.iter().any(|p| p.ends_with("#B")));
        assert!(c.equivalents.iter().any(|e| e.ends_with("#P")));
        assert!(c.disjoints.iter().any(|d| d.ends_with("#Q")));
    }

    #[test]
    fn dedup_skips_repeated_axiom_object_for_same_subject() {
        // 브리프의 dedup 규칙(이미 존재하는 오브젝트는 재수집하지 않음) 커버리지.
        let ttl = r#"
@prefix owl: <http://www.w3.org/2002/07/owl#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix dm: <https://disksage.app/ontology#> .
dm:A a owl:Class . dm:C a owl:Class ; rdfs:subClassOf dm:A ; rdfs:subClassOf dm:A ;
    rdfs:subClassOf "리터럴은 명명 노드 아님" .
"#;
        let onto = parse_ttl(ttl).unwrap();
        let c = onto.classes.iter().find(|c| c.id.ends_with("#C")).unwrap();
        assert_eq!(c.parents.len(), 1, "중복 subClassOf 오브젝트는 한 번만 채택, 리터럴 오브젝트는 무시");
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
        assert!(c.parents.iter().any(|p| p.ends_with("#A")));
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

    fn onto(ttl: &str) -> Ontology { parse_ttl(ttl).unwrap() }
    const PRE: &str = "@prefix owl: <http://www.w3.org/2002/07/owl#> .\n@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .\n@prefix dm: <https://disksage.app/ontology#> .\n";
    fn ends<'a>(v: &'a [String], suf: &str) -> bool { v.iter().any(|x| x.ends_with(suf)) }

    #[test]
    fn transitive_ancestors_across_multiple_parents() {
        let o = onto(&format!("{PRE}dm:A a owl:Class ; rdfs:subClassOf dm:B , dm:C .\ndm:B a owl:Class ; rdfs:subClassOf dm:D .\ndm:C a owl:Class .\ndm:D a owl:Class .\n"));
        let r = Reasoner::build(&o);
        let a = o.classes.iter().find(|c| c.id.ends_with("#A")).unwrap().id.clone();
        let anc = r.ancestors(&a);
        assert!(ends(&anc, "#B") && ends(&anc, "#C") && ends(&anc, "#D"));
    }

    #[test]
    fn equivalent_classes_share_ancestors_and_target() {
        // A ≡ B, B ⊑ C(target) ⇒ A inherits C's folder (scm-eqc1 + scm-sco)
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:equivalentClass dm:B .\ndm:B a owl:Class ; rdfs:subClassOf dm:C .\ndm:C a owl:Class ; dm:targetFolder \"~/C\" .\n"));
        let r = Reasoner::build(&o);
        let a = o.classes.iter().find(|c| c.id.ends_with("#A")).unwrap().id.clone();
        assert!(ends(&r.equivalents(&a), "#B"));
        assert!(ends(&r.ancestors(&a), "#C"));
        assert_eq!(o.resolve_target(&a).as_deref(), Some("~/C"));
    }

    #[test]
    fn subclassof_cycle_is_equivalence_not_error() {
        // scm-eqc2: A ⊑ B ⊑ A ⇒ equivalent, coherent, resolve terminates
        let o = onto(&format!("{PRE}dm:A a owl:Class ; rdfs:subClassOf dm:B .\ndm:B a owl:Class ; rdfs:subClassOf dm:A .\n"));
        let r = Reasoner::build(&o);
        let a = o.classes.iter().find(|c| c.id.ends_with("#A")).unwrap().id.clone();
        assert!(ends(&r.equivalents(&a), "#B"));
        assert!(r.check_coherence().is_empty());
        assert_eq!(o.resolve_target(&a), None);
    }

    #[test]
    fn mixed_equiv_subclass_cycle_needs_fixpoint() {
        // A ≡ B, B ⊑ C, C ⊑ A ⇒ {A,B,C} equivalent (merge exposes 2nd-round SCC)
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:equivalentClass dm:B .\ndm:B a owl:Class ; rdfs:subClassOf dm:C .\ndm:C a owl:Class ; rdfs:subClassOf dm:A .\n"));
        let r = Reasoner::build(&o);
        let a = o.classes.iter().find(|c| c.id.ends_with("#A")).unwrap().id.clone();
        let eq = r.equivalents(&a);
        assert!(ends(&eq, "#B") && ends(&eq, "#C"));
        assert!(r.check_coherence().is_empty());
    }

    #[test]
    fn disjoint_distinct_ancestors_unsatisfiable() {
        // C ⊑ A, C ⊑ B, A disjointWith B ⇒ C unsatisfiable
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:disjointWith dm:B .\ndm:B a owl:Class .\ndm:C a owl:Class ; rdfs:subClassOf dm:A , dm:B .\n"));
        let r = Reasoner::build(&o);
        let issues = r.check_coherence();
        assert!(issues.iter().any(|i| matches!(i, Issue::UnsatisfiableClass { class, .. } if class.ends_with("#C"))));
    }

    #[test]
    fn disjoint_shared_representative_and_self_disjoint_unsatisfiable() {
        // (ii) A ≡ B ∧ A disjointWith B ⇒ A unsat; self-disjoint D disjointWith D ⇒ D unsat
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:equivalentClass dm:B ; owl:disjointWith dm:B .\ndm:B a owl:Class .\ndm:D a owl:Class ; owl:disjointWith dm:D .\n"));
        let r = Reasoner::build(&o);
        let issues = r.check_coherence();
        assert!(issues.iter().any(|i| matches!(i, Issue::UnsatisfiableClass { class, .. } if class.ends_with("#A"))));
        assert!(issues.iter().any(|i| matches!(i, Issue::UnsatisfiableClass { class, .. } if class.ends_with("#D"))));
    }

    #[test]
    fn coherent_ontology_has_no_issues() {
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:disjointWith dm:B .\ndm:B a owl:Class .\ndm:C a owl:Class ; rdfs:subClassOf dm:A .\n"));
        assert!(Reasoner::build(&o).check_coherence().is_empty());
    }

    #[test]
    fn resolve_target_nearest_first_id_tiebreak() {
        // C ⊑ A(~/A), C ⊑ B(~/B): both distance-1 ⇒ id-tiebreak picks A
        let o = onto(&format!("{PRE}dm:A a owl:Class ; dm:targetFolder \"~/A\" .\ndm:B a owl:Class ; dm:targetFolder \"~/B\" .\ndm:C a owl:Class ; rdfs:subClassOf dm:A , dm:B .\n"));
        let c = o.classes.iter().find(|c| c.id.ends_with("#C")).unwrap().id.clone();
        assert_eq!(o.resolve_target(&c).as_deref(), Some("~/A"));
    }

    #[test]
    fn cycle_equivalences_reports_merged_groups() {
        let o = onto(&format!("{PRE}dm:A a owl:Class ; rdfs:subClassOf dm:B .\ndm:B a owl:Class ; rdfs:subClassOf dm:A .\ndm:X a owl:Class .\n"));
        let r = Reasoner::build(&o);
        assert!(r.cycle_equivalences().iter().any(|g| g.len() == 2 && ends(g, "#A") && ends(g, "#B")));
    }

    #[test]
    fn redundant_symmetric_equivalent_class_stays_one_group() {
        // owl:equivalentClass is symmetric; asserting it in BOTH directions (A≡B and B≡A) is
        // redundant but valid TTL. The second union() call finds both sides already sharing a
        // representative (a no-op) — coverage for that union() skip branch, and a correctness
        // check that redundant declarations don't fragment or duplicate the equivalence group.
        let o = onto(&format!("{PRE}dm:A a owl:Class ; owl:equivalentClass dm:B .\ndm:B a owl:Class ; owl:equivalentClass dm:A .\n"));
        let r = Reasoner::build(&o);
        let a = o.classes.iter().find(|c| c.id.ends_with("#A")).unwrap().id.clone();
        let eq = r.equivalents(&a);
        assert_eq!(eq.len(), 2);
        assert!(ends(&eq, "#A") && ends(&eq, "#B"));
    }
}
