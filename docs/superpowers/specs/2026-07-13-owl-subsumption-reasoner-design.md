# DiskSage — OWL Subsumption Reasoner Design Spec

> Sub-project **B** of the "advanced reasoning" initiative. Siblings (own specs): **A** rule-based classification (size/age/path conditions — a rule engine, not OWL, since OWL datatype reasoning can't do numeric comparisons), **C** LLM advanced reasoning + opt-in web search. This spec covers **B only**.

## 1. Overview & goals

Replace DiskSage's single-parent `subClassOf` walk with a **standards-conformant, sound reasoner** over the class-axiom fragment of the ontology, so that:

1. **`targetFolder` resolution** follows the full class hierarchy — a class inherits a target folder from *any* ancestor or equivalent class, across multiple `rdfs:subClassOf` parents, not just the first-declared parent.
2. **`owl:equivalentClass`** aliases are interchangeable (they share ancestors, subclasses, and target folders).
3. **The ontology is validated** for consistency — `owl:disjointWith` violations and `subClassOf` cycles are detected and surfaced, so a user-edited ontology's errors are caught instead of silently mis-classifying.

The current `resolve_target` (single `parent: Option<String>` chain, first-`subClassOf`-wins) is replaced by reasoner-backed resolution. Everything is pure logic in `ontology.rs`, unit-testable to 100% line coverage — no new dependencies (keeps `oxttl`/`oxrdf`).

## 2. Scope & non-goals

**In scope** (the OWL 2 RL class-axiom fragment DiskSage uses):
- `rdfs:subClassOf` — multiple parents, transitive closure.
- `owl:equivalentClass` — class equivalence.
- `owl:disjointWith` — disjointness, used for consistency checking.

**Out of scope** (deliberately — belongs to sibling sub-projects or a future full reasoner):
- **Complex class expressions / restrictions** (`owl:Restriction`, `someValuesFrom`, `allValuesFrom`, `hasValue`, `intersectionOf`, `unionOf`, `oneOf`) — defining a class by conditions and auto-classifying files into it. Property/condition-based classification is **sub-project A** (a rule engine; OWL can't express the numeric size/age comparisons file rules need).
- **Datatype/property reasoning** (`subPropertyOf`, transitive/symmetric properties, domain/range, datatype ranges).
- **Individuals/ABox realization** beyond what file classification already does (files are not asserted as OWL individuals; classification stays the extension→class + LLM-pick pipeline).
- Full OWL 2 DL/EL completeness. This reasoner is **sound** (every inference is standard-correct) and **conformant within the RL fragment**, but intentionally **incomplete** outside it.

## 3. Standards basis

The reasoner implements the relevant **W3C OWL 2 RL/RDF entailment rules** (OWL 2 RL is the W3C profile *designed* for sound rule-based reasoners), applied at the class (TBox) level. This is not an approximation — it is the standard rule set restricted to the constructs above:

| Construct | OWL 2 RL / RDFS rule | Effect |
|-----------|----------------------|--------|
| `subClassOf` transitivity | `scm-sco`: `T(c1,⊑,c2) ∧ T(c2,⊑,c3) ⇒ T(c1,⊑,c3)` | transitive closure of ancestors |
| `equivalentClass` → mutual `subClassOf` | `scm-eqc1`: `T(c1,≡,c2) ⇒ T(c1,⊑,c2) ∧ T(c2,⊑,c1)` | fold into one equivalence group |
| mutual `subClassOf` → `equivalentClass` | `scm-eqc2`: `T(c1,⊑,c2) ∧ T(c2,⊑,c1) ⇒ T(c1,≡,c2)` | **a `subClassOf` cycle IS equivalence, not an error** |
| reflexive `⊑`/`≡` + top/bottom | `scm-cls`: `T(c,rdf:type,owl:Class) ⇒ T(c,⊑,c), T(c,≡,c), T(c,⊑,owl:Thing), T(owl:Nothing,⊑,c)` | a class is its own distance-0 ancestor/equivalent (for resolve lookup) |
| `disjointWith` → class unsatisfiability | `cax-dw` (ABox: `x∈c1 ∧ x∈c2 ∧ c1 disjointWith c2 ⇒ ⊥`) + its **sound TBox consequence** `C ⊑ c1 ∧ C ⊑ c2 ∧ c1 disjointWith c2 ⇒ C ⊑ owl:Nothing` | flag `C` as an **unsatisfiable class** (provably empty) |

**Two corrections a first-pass draft got wrong** (caught by an adversarial standards check against the W3C text): (1) a `subClassOf` cycle is **not** an inconsistency — `scm-eqc2` entails the classes are *equivalent*, a consistent state; the reasoner folds cycles into equivalence groups rather than flagging them. (2) reflexivity is entailed by `scm-cls` (premised on `rdf:type owl:Class`), not by `scm-sco` (pure transitivity). The `disjointWith` check reports **class unsatisfiability** (`C ⊑ owl:Nothing`), a standard sound DL-reasoning service (as Pellet/HermiT report unsatisfiable classes), grounded in Direct Semantics — distinguished from the literal ABox rule `cax-dw`, which fires only on a shared individual.

Because the fragment excludes existential/complex expressions, the TBox closure is finite and computable by simple fixpoint (union-find over equivalence + transitive closure) — no tableau needed. Soundness follows from applying only these standard rules; the technique's academic lineage is Grosof et al. **Description Logic Programs** (WWW 2003) → ter Horst **pD\*** soundness/completeness+decidability proofs (JWS 2005; ISWC 2005) → the standardized **OWL 2 RL/RDF** rule set (W3C Rec. 2012), with the completion/consequence-based subsumption method from Baader, Brandt, Lutz **Pushing the EL Envelope** (IJCAI 2005), Krötzsch **The Not-So-Easy Task of Computing Class Subsumptions in OWL RL** (ISWC 2012), and Kazakov et al. **ELK** (JAR 2014). See References.

## 4. Data model

Extend `ontology.rs`. `OntoClass` gains axiom lists (parsed from Turtle via the existing `oxttl` reader):

```rust
pub struct OntoClass {
    pub id: String,
    pub label: String,
    pub parents: Vec<String>,       // was: parent: Option<String>  — all rdfs:subClassOf objects
    pub equivalents: Vec<String>,   // owl:equivalentClass objects
    pub disjoints: Vec<String>,     // owl:disjointWith objects
    pub target_folder: Option<String>,
}
```

`parse_ttl` collects, per subject IRI: every `rdfs:subClassOf` object (not just the first), `owl:equivalentClass`, `owl:disjointWith`, `rdfs:label`, `dm:targetFolder`. Blank-node subjects/objects are ignored (as today). Determinism: objects are stored in declaration order; the reasoner sorts where needed so results are stable (no `HashMap` iteration order in output).

**Breaking change note:** `OntoClass.parent: Option<String>` → `parents: Vec<String>`. `resolve_target`'s signature is unchanged (`resolve_target(&self, class_id: &str) -> Option<String>`), but its body is reasoner-backed. Consumers (`organize::plan_moves*`, `inventory`, `commands`) use `resolve_target` / `classes` and need only mechanical updates where they read `parent`.

## 5. The reasoner

A pure value built once from an `Ontology`:

```rust
pub struct Reasoner { /* precomputed indices */ }

impl Reasoner {
    pub fn build(onto: &Ontology) -> Reasoner;
    /// All (proper + improper) superclasses of `class`, following subClassOf
    /// transitivity across every parent and merging equivalentClass members.
    /// Deterministic order (topological then id-sorted).
    pub fn ancestors(&self, class_id: &str) -> Vec<String>;
    /// Every class equivalent to `class` (its equivalence-class members).
    pub fn equivalents(&self, class_id: &str) -> Vec<String>;
    /// Unsatisfiable classes (empty = ontology is coherent). A subClassOf cycle is NOT
    /// reported here — per scm-eqc2 it is entailed equivalence, folded into groups (§step 1).
    pub fn check_coherence(&self) -> Vec<Issue>;
    /// Optional, advisory (not an error): classes made equivalent via a subClassOf cycle
    /// (scm-eqc2), so the UI can note "A, B are equivalent via mutual subClassOf".
    pub fn cycle_equivalences(&self) -> Vec<Vec<String>>;
}

pub enum Issue {
    /// C ⊑ c1, C ⊑ c2, c1 disjointWith c2 ⇒ C ⊑ owl:Nothing (sound TBox consequence of cax-dw).
    UnsatisfiableClass { class: String, via_disjoint: (String, String) },
}
```

**Algorithm (fixpoint, no external deps):**
1. **Equivalence classes (scm-eqc1 + scm-eqc2)**: union-find seeded by (a) every explicit `owl:equivalentClass` pair, **and (b) every strongly-connected component of the `subClassOf` graph** — mutual `subClassOf` entails equivalence (`scm-eqc2`), so a cycle collapses into one representative rather than being an error. `equivalents()` returns the group; `cycle_equivalences()` reports groups formed via (b) for advisory UI only.
2. **subClassOf closure (scm-sco)**: build the graph on representatives from all `parents` edges — **acyclic by construction**, since step 1 collapsed every SCC — and compute the transitive closure. `scm-cls` makes each class its own distance-0 ancestor/equivalent for lookup.
3. **Class unsatisfiability (cax-dw TBox consequence)**: for each class `C`, take its ancestor set (incl. self); if it contains two representatives `c1`, `c2` with `c1 owl:disjointWith c2`, emit `UnsatisfiableClass` — the sound consequence `C ⊑ owl:Nothing` (a provably empty class signals the user's ontology is contradictory there).

`resolve_target(class_id)`: walk `ancestors(class_id)` in **nearest-first** order — nearest = fewest `subClassOf` hops from `class_id` (BFS distance; the class itself and its equivalents are distance 0), ties broken by ascending class id for determinism — and return the first class in that order with a `target_folder`. This subsumes and replaces the old single-parent walk and is fully deterministic regardless of declaration/parse order.

## 6. Integration

- **`organize::plan_moves*`**: unchanged call shape — it already goes file → class → `onto.resolve_target(class.id)`. Now resolution is reasoner-backed (richer inheritance). The `Reasoner` is built once per plan and reused (avoid rebuilding per file).
- **`inventory` / ontology surfacing**: add a command `ontology_coherence() -> Vec<Issue>` (Tauri `#[cfg(not(coverage))]` wrapper over the pure `check_coherence`). The Inventory (or a small ontology panel) shows "온톨로지 정합" or lists **unsatisfiable classes**, so a user editing `~/.../ontology.ttl` sees contradictions. A subClassOf cycle is **not** surfaced as an error — `cycle_equivalences()` may optionally note it as informational ("A, B are equivalent via mutual subClassOf").
- **LLM class-picker (M5)**: unaffected — it still picks among candidate class ids; the candidates now benefit from equivalentClass aliasing when resolving targets.

## 7. Error handling

- Malformed axioms / unknown IRIs: ignored (parse best-effort, as today), never panic.
- **Cycles**: a `subClassOf` cycle is **not** an error — per `scm-eqc2` it entails equivalence, so step 1 collapses the SCC into one representative. The closure (built on collapsed representatives) is acyclic by construction, so there is no infinite recursion; `resolve_target` on any class in the cycle sees the whole equivalence group at distance 0.
- Coherence issues (`UnsatisfiableClass`) are **advisory**: they surface as warnings; they do not block classification or moves (which stay behind the existing safety layer + user confirmation). A file whose class is unsatisfiable still resolves a target via its ancestors; the warning tells the user their ontology is contradictory there.

## 8. Testing

Pure unit tests in `ontology.rs` (tempdir-free), targeting **100% line coverage** on the Linux gate, including OWL 2 RL conformance cases:
- transitive `subClassOf` across multiple parents (`A ⊑ B`, `A ⊑ C`, `B ⊑ D` ⇒ ancestors(A) ⊇ {B,C,D}).
- `equivalentClass` sharing: `A ≡ B`, `B ⊑ C` ⇒ `A ⊑ C`; `A` inherits `B`'s `targetFolder`.
- `resolve_target` picks nearest ancestor's folder deterministically; falls back through the closure.
- `disjointWith` unsatisfiability: `C ⊑ A`, `C ⊑ B`, `A disjointWith B` ⇒ one `UnsatisfiableClass{class:C, ...}`; coherent ontology ⇒ empty.
- **`subClassOf` cycle = equivalence (scm-eqc2 conformance)**: `A ⊑ B ⊑ A` ⇒ `equivalents(A)` = {A, B}, they share ancestors + `targetFolder`, `check_coherence()` is empty (NOT an error), and `resolve_target` terminates. Also `A ⊑ B`, `B ⊑ A`, `B ⊑ C` ⇒ `ancestors(A) ⊇ {C}`.
- determinism: same ontology parsed twice ⇒ identical `ancestors`/coherence output (no `HashMap`-order flakiness).

## 9. How it fits

Completes the "reasoning engine" the original design (§2 non-goals) deferred, for the class-axiom fragment. It is additive: no new deps, one module (`ontology.rs`) plus one thin command; existing `organize`/`inventory` behavior is preserved and enriched. Sub-projects **A** (rule-based classification) and **C** (LLM reasoning + opt-in web search) follow as separate spec → plan → implement cycles.

## 10. Standards & literature basis (verified)

Both the **standard** and the **published literature** were independently verified (adversarial multi-agent check against the primary sources) before adopting this design.

- **W3C standard.** Boris Motik, Bernardo Cuenca Grau, Ian Horrocks, Zhe Wu, Achille Fokoue, Carsten Lutz (eds.), *OWL 2 Web Ontology Language: Profiles (2nd ed.)*, W3C Recommendation, 11 Dec 2012, §4.3 (OWL 2 RL/RDF rules `scm-sco`, `scm-eqc1`, `scm-eqc2`, `scm-cls`, `cax-dw`). The rule names/semantics above are verbatim from this source.
- **Rules-as-DL / materialization.** B. Grosof, I. Horrocks, R. Volz, S. Decker, *Description Logic Programs: Combining Logic Programs with Description Logic*, WWW 2003, 48–57. H. ter Horst, *Completeness, decidability and complexity of entailment for RDF Schema and a semantic extension involving the OWL vocabulary*, J. Web Semantics 3(2–3):79–115, 2005 (and *Combining RDF and Part of OWL with Rules*, ISWC 2005, LNCS 3729:668–684 — the pD\* semantics, proving R-entailment sound, complete, decidable, PTIME for the blank-node-free case). This is the proven basis for reasoning over the RDFS+OWL-vocabulary fragment by a fixpoint of rules.
- **Completion-based subsumption.** F. Baader, S. Brandt, C. Lutz, *Pushing the EL Envelope*, IJCAI 2005, 364–369. Y. Kazakov, M. Krötzsch, F. Simančík, *The Incredible ELK…*, J. Automated Reasoning 53(1):1–61, 2014. These establish computing subsumption by completion/closure over the taxonomy (not tableau) for tractable profiles.
- **RL subsumption caveat, and why it doesn't apply here.** M. Krötzsch, *The Not-So-Easy Task of Computing Class Subsumptions in OWL RL*, ISWC 2012, LNCS 7649:279–294, shows the OWL 2 RL/RDF rules are **incomplete for TBox class subsumption in general** — that incompleteness arises from the *interaction of complex class expressions* (restrictions, intersection/union), which this design **explicitly excludes** (§2). For the restricted taxonomic fragment here — `subClassOf` + `equivalentClass` + `disjointWith` over named classes — transitive closure with equivalence collapse is both **sound and complete** for subsumption and for unsatisfiability detection. Scoping the fragment is precisely what buys this completeness; extending to restrictions (sub-project A / a future full reasoner) is where a heavier method would be required.
