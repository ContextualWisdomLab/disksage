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
| `subClassOf` transitivity | `scm-sco`: `c1 ⊑ c2 ∧ c2 ⊑ c3 ⇒ c1 ⊑ c3` | transitive closure of ancestors |
| `equivalentClass` | `scm-eqc1`/`scm-eqc2`: `c1 ≡ c2 ⇔ c1 ⊑ c2 ∧ c2 ⊑ c1` | mutual subclassing → equivalence classes |
| `subClassOf` reflexivity | `scm-sco` (reflexive base) | a class is its own (improper) ancestor — for resolve lookup |
| `disjointWith` (consistency) | `cax-dw` + derived TBox unsatisfiability: `C ⊑ c1 ∧ C ⊑ c2 ∧ c1 disjointWith c2 ⇒ C ⊑ ⊥` | flag `C` as unsatisfiable/inconsistent |

Because the fragment excludes existential/complex expressions, the TBox closure is finite and computable by simple fixpoint (union-find + transitive closure) — no tableau needed. Soundness follows directly from applying only these standard rules.

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
    /// Ontology consistency issues (empty = consistent).
    pub fn check_consistency(&self) -> Vec<Inconsistency>;
}

pub enum Inconsistency {
    SubClassCycle(Vec<String>),          // e.g. A ⊑ B ⊑ A
    DisjointViolation { class: String, via: (String, String) }, // C ⊑ c1, C ⊑ c2, c1 disjoint c2
}
```

**Algorithm (fixpoint, no external deps):**
1. **Equivalence classes**: union-find over `equivalentClass` pairs (rule `scm-eqc`). Each class maps to a representative; `equivalents()` returns the group.
2. **subClassOf closure**: build a DAG on equivalence-class representatives from all `parents` edges; compute transitive closure (rule `scm-sco`). Detect cycles during the walk → `SubClassCycle` (a cycle of *distinct* representatives; note `A ≡ B` is not a cycle — it's one representative).
3. **Consistency (`cax-dw`)**: for each class `C`, take its ancestor set (incl. self); if it contains two reps `c1`, `c2` with `c1 disjointWith c2`, emit `DisjointViolation`. (This is the standard TBox consequence `C ⊑ ⊥`.)

`resolve_target(class_id)`: walk `ancestors(class_id)` in **nearest-first** order — nearest = fewest `subClassOf` hops from `class_id` (BFS distance; the class itself and its equivalents are distance 0), ties broken by ascending class id for determinism — and return the first class in that order with a `target_folder`. This subsumes and replaces the old single-parent walk and is fully deterministic regardless of declaration/parse order.

## 6. Integration

- **`organize::plan_moves*`**: unchanged call shape — it already goes file → class → `onto.resolve_target(class.id)`. Now resolution is reasoner-backed (richer inheritance). The `Reasoner` is built once per plan and reused (avoid rebuilding per file).
- **`inventory` / ontology surfacing**: add a command `ontology_consistency() -> Vec<Inconsistency>` (Tauri `#[cfg(not(coverage))]` wrapper over the pure `check_consistency`). The Inventory (or a small ontology panel) shows "온톨로지 유효" or lists warnings, so a user editing `~/.../ontology.ttl` sees errors.
- **LLM class-picker (M5)**: unaffected — it still picks among candidate class ids; the candidates now benefit from equivalentClass aliasing when resolving targets.

## 7. Error handling

- Malformed axioms / unknown IRIs: ignored (parse best-effort, as today), never panic.
- **Cycles**: `subClassOf` cycles are reported as `Inconsistency::SubClassCycle`, and closure computation terminates safely (visited-set guard) — a cyclic ontology still yields a usable (if flagged) reasoner rather than looping. `resolve_target` on a cyclic class returns the nearest reachable `target_folder` without infinite recursion.
- Consistency issues are **advisory**: they surface as warnings; they do not block classification or moves (which stay behind the existing safety layer + user confirmation). A file whose class is unsatisfiable still resolves a target via its ancestors; the warning tells the user their ontology is contradictory.

## 8. Testing

Pure unit tests in `ontology.rs` (tempdir-free), targeting **100% line coverage** on the Linux gate, including OWL 2 RL conformance cases:
- transitive `subClassOf` across multiple parents (`A ⊑ B`, `A ⊑ C`, `B ⊑ D` ⇒ ancestors(A) ⊇ {B,C,D}).
- `equivalentClass` sharing: `A ≡ B`, `B ⊑ C` ⇒ `A ⊑ C`; `A` inherits `B`'s `targetFolder`.
- `resolve_target` picks nearest ancestor's folder deterministically; falls back through the closure.
- `disjointWith` consistency: `C ⊑ A`, `C ⊑ B`, `A disjointWith B` ⇒ one `DisjointViolation`; consistent ontology ⇒ empty.
- `subClassOf` cycle: `A ⊑ B ⊑ A` ⇒ `SubClassCycle`, no infinite loop, `resolve_target` terminates.
- determinism: same ontology parsed twice ⇒ identical `ancestors`/consistency output (no `HashMap`-order flakiness).

## 9. How it fits

Completes the "reasoning engine" the original design (§2 non-goals) deferred, for the class-axiom fragment. It is additive: no new deps, one module (`ontology.rs`) plus one thin command; existing `organize`/`inventory` behavior is preserved and enriched. Sub-projects **A** (rule-based classification) and **C** (LLM reasoning + opt-in web search) follow as separate spec → plan → implement cycles.
