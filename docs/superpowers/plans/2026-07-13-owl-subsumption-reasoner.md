# OWL Subsumption Reasoner Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace DiskSage's single-parent `subClassOf` walk with a standards-conformant (OWL 2 RL) reasoner over the `subClassOf`/`equivalentClass`/`disjointWith` fragment — transitive `targetFolder` inheritance, equivalence handling, and ontology coherence (unsatisfiable-class) checking.

**Architecture:** Extend `src-tauri/src/ontology.rs`: the parsed model gains multi-parent/equivalent/disjoint axioms; a pure `Reasoner` computes an equivalence fixpoint (union-find seeded by `equivalentClass` + iterated `subClassOf`-SCC collapse, per `scm-eqc1`/`scm-eqc2`), a transitive-closure ancestor index (`scm-sco`), and unsatisfiable classes (`cax-dw` TBox consequence `C ⊑ owl:Nothing`); `resolve_target` becomes reasoner-backed. One thin `#[cfg(not(coverage))]` command surfaces coherence.

**Tech Stack:** Rust, existing `oxttl`/`oxrdf` (NO new deps), Svelte 5 frontend.

## Global Constraints

- **Spec:** `docs/superpowers/specs/2026-07-13-owl-subsumption-reasoner-design.md` (verified vs W3C OWL 2 RL + literature, 2 adversarial rounds). Rule names are load-bearing: `scm-sco`, `scm-eqc1`, `scm-eqc2`, `scm-cls`, `cax-dw`.
- **Coverage gate:** `cargo llvm-cov --all-features --fail-under-lines 100` on Linux. All new logic in `ontology.rs` is pure → must be 100% line-covered. Verify locally with the DEFAULT `cargo llvm-cov --lib --summary-only` (NOT `--no-cfg-coverage`). The Tauri command (Task 3) is `#[cfg(not(coverage))]`; its pure inner is measured.
- **No new dependencies.** Use only `oxttl`/`oxrdf`/std.
- **Determinism:** no `HashMap` iteration in output. Use `BTreeMap`/`BTreeSet` and sort. Same ontology parsed twice ⇒ identical `ancestors`/coherence/`resolve_target`.
- **Standards fidelity (the reason this exists):** a `subClassOf` cycle is **equivalence, not an error** (`scm-eqc2`); disjointness yields **class unsatisfiability** `C ⊑ owl:Nothing`, including the shared-representative corner (`A ≡ B ∧ A disjointWith B`, self-disjoint `A disjointWith A`); reflexivity/self-equivalence is `scm-cls`.
- **Advisory only:** coherence issues are warnings; they never block classification or moves (existing safety layer + user confirmation unchanged).

## File Structure

- `src-tauri/src/ontology.rs` — MODIFY. Model (`OntoClass` axiom lists), `parse_ttl` (multi-subClassOf + equivalentClass + disjointWith), `Reasoner` (fixpoint + closure + coherence), reasoner-backed `resolve_target`. Grows to ~450 lines — acceptable; one cohesive responsibility (the ontology + its reasoning).
- `src-tauri/src/commands.rs` — MODIFY. `ontology_coherence` command + pure inner.
- `src-tauri/src/lib.rs` — MODIFY. Register `ontology_coherence`.
- `src/lib/api.ts` — MODIFY. `OntoClass` interface (parent→parents + equivalents/disjoints), `Issue` type, `ontologyCoherence()` wrapper.
- `src/lib/Inventory.svelte` — MODIFY. Show coherence ("온톨로지 정합" / unsatisfiable-class warnings).

---

## Task 1: Data model + parsing (multi-parent, equivalentClass, disjointWith)

Behavior-preserving: `resolve_target` still walks the first parent (kept green); the Reasoner is Task 2.

**Files:**
- Modify: `src-tauri/src/ontology.rs`
- Modify: `src/lib/api.ts`

**Interfaces:**
- Produces: `OntoClass { id: String, label: String, parents: Vec<String>, equivalents: Vec<String>, disjoints: Vec<String>, target_folder: Option<String> }`; `parse_ttl(&str) -> Result<Ontology, String>` collecting all `rdfs:subClassOf`, `owl:equivalentClass`, `owl:disjointWith` objects per subject.

- [ ] **Step 1: Update the failing tests** for the new model. Replace the three `.parent` assertions and add axiom-parsing coverage:

```rust
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
```
Also fix `multiple_subclassof_keeps_first_parent_deterministically`: replace `c.parent.as_deref().unwrap().ends_with("#A")` with `assert!(c.parents.iter().any(|p| p.ends_with("#A")));` and keep the `resolve_target ... Some("~/A")` line (Task 1 first-parent walk still yields ~/A; Task 2 preserves it via id-tiebreak).

- [ ] **Step 2: Run — FAIL** `cargo test --lib ontology` (field `parents` doesn't exist).

- [ ] **Step 3: Implement the model + parse.** Add constants and switch the collectors to `Vec`:

```rust
const OWL_EQUIVALENT_CLASS: &str = "http://www.w3.org/2002/07/owl#equivalentClass";
const OWL_DISJOINT_WITH: &str = "http://www.w3.org/2002/07/owl#disjointWith";
```
```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct OntoClass {
    pub id: String,
    pub label: String,
    pub parents: Vec<String>,
    pub equivalents: Vec<String>,
    pub disjoints: Vec<String>,
    pub target_folder: Option<String>,
}
```
In `parse_ttl`, replace `parents: BTreeMap<String, String>` with three `BTreeMap<String, Vec<String>>` (`parents`, `equivalents`, `disjoints`); add a helper closure `push` and match the two new predicates. Each named-node object is pushed (dedup: skip if already present) preserving order:
```rust
    let mut parents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut equivalents: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut disjoints: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // ... inside the predicate match, for RDFS_SUBCLASS / OWL_EQUIVALENT_CLASS / OWL_DISJOINT_WITH:
    //   if let Term::NamedNode(o) = &triple.object {
    //       let v = <map>.entry(s).or_default();
    //       let o = o.as_str().to_string();
    //       if !v.contains(&o) { v.push(o); }
    //   }
```
Build each `OntoClass` with `parents: parents.get(&id).cloned().unwrap_or_default()` (and same for equivalents/disjoints). Update `resolve_target` to walk `cls.parents.first()` (behavior-preserving for Task 1):
```rust
    pub fn resolve_target(&self, class_id: &str) -> Option<String> {
        let mut cur = class_id.to_string();
        for _ in 0..64 {
            let cls = self.classes.iter().find(|c| c.id == cur)?;
            if let Some(t) = &cls.target_folder { return Some(t.clone()); }
            cur = cls.parents.first().cloned()?;
        }
        None
    }
```

- [ ] **Step 4: Run — PASS** `cargo test --lib ontology`, then `cargo test --lib` (whole suite). `cargo llvm-cov --lib --summary-only` → `ontology.rs` 100%.

- [ ] **Step 5: Update `api.ts`** `OntoClass` interface to match the serialized shape:
```ts
export interface OntoClass {
  id: string;
  label: string;
  parents: string[];
  equivalents: string[];
  disjoints: string[];
  target_folder: string | null;
}
```
Run `npm run check` → 0 errors (no component reads `.parent`; confirm with a grep).

- [ ] **Step 6: Commit** `git commit -m "feat(ontology): parse multiple subClassOf + equivalentClass + disjointWith"`.

---

## Task 2: The reasoner (equivalence fixpoint + closure + coherence) + reasoner-backed resolve_target

**Files:** Modify: `src-tauri/src/ontology.rs`

**Interfaces:**
- Consumes: `Ontology`/`OntoClass` from Task 1.
- Produces: `Reasoner::build(&Ontology) -> Reasoner`; `reasoner.ancestors(&str) -> Vec<String>`; `reasoner.equivalents(&str) -> Vec<String>`; `reasoner.check_coherence() -> Vec<Issue>`; `reasoner.cycle_equivalences() -> Vec<Vec<String>>`; `Issue::UnsatisfiableClass { class: String, via_disjoint: (String, String) }`. `Ontology::resolve_target` rewritten to build a `Reasoner` and use it.

- [ ] **Step 1: Write failing tests** (append to the tests module) — the OWL 2 RL conformance cases from spec §8:

```rust
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
```

- [ ] **Step 2: Run — FAIL** `cargo test --lib ontology` (`Reasoner` undefined).

- [ ] **Step 3: Implement the reasoner.** Add to `ontology.rs`:

```rust
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Issue {
    /// C ⊑ c1, C ⊑ c2, c1 disjointWith c2 ⇒ C ⊑ owl:Nothing (sound TBox consequence of cax-dw).
    UnsatisfiableClass { class: String, via_disjoint: (String, String) },
}

pub struct Reasoner {
    rep: BTreeMap<String, String>,        // class id → equivalence representative
    groups: BTreeMap<String, Vec<String>>, // rep → sorted members
    sup: BTreeMap<String, BTreeSet<String>>, // rep → direct super-reps (acyclic after fixpoint)
    // (target_folder / disjoint axioms are read from the Ontology, passed to methods that need them)
}

impl Reasoner {
    pub fn build(onto: &Ontology) -> Reasoner {
        let ids: Vec<String> = onto.classes.iter().map(|c| c.id.clone()).collect();
        // union-find
        let mut rep: BTreeMap<String, String> = ids.iter().map(|i| (i.clone(), i.clone())).collect();
        fn find(rep: &mut BTreeMap<String, String>, x: &str) -> String {
            let p = rep.get(x).cloned().unwrap_or_else(|| x.to_string());
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
        Reasoner { rep, groups, sup }
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
        out.remove(class_id); // ancestors = supers; keep equivalents but not the query id? -> spec: incl self+equivalents
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
}
```

Then implement coherence + reasoner-backed resolve_target as methods that take the `Ontology` (for target_folder / disjoint axioms) — put them on `Ontology` for a clean call site:

```rust
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

    /// Unsatisfiable classes (cax-dw TBox consequence). Empty = coherent.
    pub fn check_coherence(&self) -> Vec<Issue> {
        let r = self.reasoner();
        // disjoint axioms as (rep_a, rep_b, a_id, b_id)
        let mut dis: Vec<(String, String, String, String)> = Vec::new();
        for c in &self.classes {
            for d in &c.disjoints {
                if let (Some(ra), Some(rb)) = (r.rep_of(&c.id), r.rep_of(d)) {
                    dis.push((ra, rb, c.id.clone(), d.clone()));
                }
            }
        }
        let mut out: Vec<Issue> = Vec::new();
        for c in &self.classes {
            let Some(cr) = r.rep_of(&c.id) else { continue };
            let mut clo = r.closure(&cr);
            clo.insert(cr); // incl self
            // C unsat iff some disjoint pair has BOTH reps in C's closure (covers ra==rb corner)
            if let Some((_, _, a, b)) = dis.iter().find(|(ra, rb, _, _)| clo.contains(ra) && clo.contains(rb)) {
                out.push(Issue::UnsatisfiableClass { class: c.id.clone(), via_disjoint: (a.clone(), b.clone()) });
            }
        }
        out
    }
}
```
Remove the old single-parent `resolve_target` (replaced above). Delete `use std::collections::BTreeMap;` duplicate if the top `use` now covers it (keep one `use std::collections::{BTreeMap, BTreeSet};`).

**Perf note (`// ponytail:` in code):** `resolve_target` rebuilds the `Reasoner` on each call — fine for small ontologies (dozens of classes). Spec §6 wants it built once per organize plan; if a large scan makes this hot, have `organize::plan_moves_with` build one `Reasoner` and thread it through (a follow-up, not this plan). Correctness is unaffected.

- [ ] **Step 4: Run — PASS** `cargo test --lib ontology`; then `cargo test --lib`. `cargo llvm-cov --lib --summary-only` → `ontology.rs` **100% lines**; if any line is <100%, add a targeted test (the `find`/`union` recursion base, the `!rep.contains_key` skip for axioms referencing undeclared classes, the `best` tiebreak branch — each is exercised by the tests above; add a case for a subClassOf/equivalent/disjoint pointing at an *undeclared* class if the skip branch is uncovered).

- [ ] **Step 5: Commit** `git commit -m "feat(ontology): OWL 2 RL reasoner (equivalence fixpoint, closure, unsatisfiability); reasoner-backed resolve_target"`.

---

## Task 3: Coherence command + Inventory surfacing

**Files:** Modify `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs`, `src/lib/api.ts`, `src/lib/Inventory.svelte`.

**Interfaces:**
- Produces: `#[cfg(not(coverage))] ontology_coherence(app) -> Result<Vec<ontology::Issue>, String>` (loads the bundled/override ontology via the existing `bundled_ontology_ttl` + `load_ontology_from`, then `onto.check_coherence()`); api.ts `Issue` type + `ontologyCoherence()`.

- [ ] **Step 1:** No new pure logic to unit-test (the command is a thin `#[cfg(not(coverage))]` wrapper over `check_coherence`, already tested in Task 2). Add the command to `commands.rs`:
```rust
#[cfg(not(coverage))]
#[tauri::command]
pub fn ontology_coherence(app: AppHandle) -> Result<Vec<crate::ontology::Issue>, String> {
    let onto = load_ontology_from(&bundled_ontology_ttl(&app)?)?;
    Ok(onto.check_coherence())
}
```
Register in `lib.rs` `generate_handler!`: add `commands::ontology_coherence`.

- [ ] **Step 2:** `api.ts` — add:
```ts
export type Issue = { UnsatisfiableClass: { class: string; via_disjoint: [string, string] } };
export const ontologyCoherence = () => invoke<Issue[]>("ontology_coherence");
```

- [ ] **Step 3:** `Inventory.svelte` — after loading the report, call `ontologyCoherence()`; render "온톨로지 정합 ✓" when empty, else list each unsatisfiable class (`i.UnsatisfiableClass.class` with its disjoint pair) as an advisory warning. Keep it non-blocking (does not gate any existing control).

- [ ] **Step 4: Verify** `cargo build --lib` (compiles), `cargo test --lib` (unchanged pass), `cargo llvm-cov --lib --summary-only` (commands.rs pure helpers still 100%; the new command is `#[cfg(not(coverage))]`), `npm run check` + `npm run build` clean.

- [ ] **Step 5: Commit** `git commit -m "feat(ontology): ontology_coherence command + Inventory surfacing of unsatisfiable classes"`.

---

## Post-implementation

- Whole-branch review (most capable model) focused on the Global Constraints — especially: the equivalence **fixpoint** correctness (mixed `≡`/`⊑` cycles), the disjoint **shared-representative** corner, determinism (BTree everywhere, no HashMap in output), and 100% coverage on the Linux gate. Because standards-conformance is the whole point, the review should re-check the reasoner against spec §3/§5 (scm-eqc2: cycles are equivalence not errors; cax-dw TBox unsatisfiability incl. the shared-rep case).
- Open the PR on `feat/owl-reasoner` (the spec is already committed there). Expect the coverage gate + existing organize/inventory tests to stay green (resolve_target behavior for the existing single-parent ontology is preserved).
- Sub-projects **A** (rule-based classification) and **C** (LLM reasoning + opt-in web search) are separate spec → plan → implement cycles.
