# DiskSage UML and Architecture Diagrams

## Evidence status

These diagrams document the canonical architecture and authority transitions. They do not claim that a planned capability is implemented merely because it appears in a diagram. Runtime implementation status is cross-checked in `docs/TRACEABILITY.md`.

## Component and bounded-context view

```mermaid
flowchart LR
    User[Local operator]
    UI[Svelte presentation]
    IPC[Tauri typed IPC]
    Observe[Rust observation]
    Plan[Rust planning]
    Auth[Rust authorization]
    Exec[Rust execution]
    Evidence[Evidence and receipts]
    LocalModel[On-device llama.cpp model]
    Provider[Provider/native APIs]
    Naruon[Naruon optional consumer]
    Orch[contextual-orchestrator optional]
    Control[CWL .github control plane]

    User --> UI --> IPC
    IPC --> Observe --> Plan --> Auth --> Exec --> Evidence
    Observe --> Provider
    LocalModel -. advisory .-> Plan
    Orch -. optional advisory model routing .-> Plan
    Evidence -. bounded versioned evidence .-> Naruon
    Control -. repository governance only .-> Evidence
```

The organization control plane governs software integration evidence; it does not authorize a local filesystem mutation.

## Standard scan, recommend, approve, execute sequence

```mermaid
sequenceDiagram
    actor Operator
    participant UI as Svelte UI
    participant IPC as Tauri IPC
    participant Obs as Rust Observer
    participant Plan as Rust Planner
    participant Auth as Rust Authorization
    participant Exec as Rust Executor
    participant Rec as Receipt/Evidence

    Operator->>UI: Start bounded observation
    UI->>IPC: typed read-only command
    IPC->>Obs: observe(scope, limits)
    Obs-->>Plan: evidence + completeness + fingerprint
    Plan-->>UI: candidates + blockers + exact phrase
    Operator->>UI: choose action + rationale + phrase
    UI->>IPC: exact plan + proposed approval
    IPC->>Auth: validate scope, fingerprints, approval, clock
    alt invalid, stale, expired, incomplete
        Auth-->>UI: stable fail-closed refusal
    else current authorization
        Auth->>Exec: single-purpose execution permit
        Exec->>Exec: revalidate mutation-time preconditions
        alt drift or collision
            Exec-->>UI: fail closed; fresh plan required
        else exact operation succeeds/fails boundedly
            Exec-->>Rec: result + recovery evidence
            Rec-->>UI: bounded result
        end
    end
```

## Cloud copy/adoption evidence flow

```mermaid
sequenceDiagram
    actor Operator
    participant Local as Local evidence
    participant Provider as Provider evidence
    participant Plan as Rust planner
    participant Review as Human review
    participant Exec as Rust executor
    participant Receipt as Restricted receipt

    Local->>Plan: source lineage + destination observation
    Provider->>Plan: scope + capacity/sync evidence or unknown
    Plan-->>Review: exact plan + blockers + confirmation phrase
    Review-->>Plan: approver + rationale + exact phrase
    Plan->>Exec: current plan + approval
    Exec->>Exec: revalidate source/destination/provider state
    alt drift or incomplete authority
        Exec-->>Operator: refuse; regenerate evidence/approval
    else authorized copy/adoption
        Exec->>Receipt: no-clobber result + bounded evidence
        Receipt-->>Operator: result without automatic eviction claim
    end
```

Provider runtime presence, capacity, copy completion, synchronization, remote durability, and local eviction permission remain distinct.

## Model installation and execution integrity flow

```mermaid
flowchart TD
    Spec[Immutable reviewed model specification]
    Stream[Bounded download stream]
    Stage[Unnamed/local bounded staging]
    InstallVerify[Exact size + SHA-256]
    Publish[Race-resistant no-clobber publication]
    Installed[Installed artifact]
    LoadObserve[Non-following metadata and identity binding]
    LoadVerify[Exact bytes + SHA-256]
    StableHandle[Retained verified identity]
    Llama[llama.cpp initialization/load]

    Spec --> Stream --> Stage --> InstallVerify --> Publish --> Installed
    Spec --> LoadVerify
    Installed --> LoadObserve --> LoadVerify --> StableHandle --> Llama
```

The integrated contract verifies the artifact both before installation acceptance and again immediately before execution; the model remains advisory after integrity admission.

## Runtime evidence and authority state machine

```mermaid
stateDiagram-v2
    [*] --> Unobserved
    Unobserved --> Observed: bounded observation succeeds
    Unobserved --> Incomplete: missing/malformed/bound exceeded
    Observed --> Planned: deterministic plan generated
    Planned --> Blocked: prerequisite unknown/unsafe
    Planned --> AwaitingApproval: executable candidate
    AwaitingApproval --> Authorized: exact human approval + current fingerprints + freshness
    AwaitingApproval --> Blocked: mismatch/expiry/clock failure
    Authorized --> Stale: current-state revalidation detects drift
    Authorized --> Executing: preconditions still match
    Executing --> Completed: verified result + receipt
    Executing --> RecoveryRequired: bounded partial failure
    RecoveryRequired --> Completed: invocation-owned recovery completes
    Stale --> Planned: regenerate evidence and plan
    Blocked --> Unobserved: new evidence required
    Incomplete --> Unobserved: retry after cause changes
```

## Repository merge and release authority flow

```mermaid
flowchart TD
    Head[Exact current source head]
    Base[Independently resolved live base tip]
    CI[Required CI and coverage]
    Security[Security/scanner gates]
    Review[Qualifying formal review if required]
    Findings[Zero valid unresolved findings]
    Policy[Branch/ruleset/repository policy]
    Merge[Protected merge]
    Main[Exact integrated protected head]
    Package[Packaging and compatibility]
    SBOM[SBOM + provenance + integrity]
    Acceptance[Release acceptance]
    Publish[Published verified release]

    Head --> CI
    Head --> Security
    Head --> Review
    Head --> Findings
    Base --> Policy
    CI --> Policy
    Security --> Policy
    Review --> Policy
    Findings --> Policy
    Policy --> Merge --> Main --> Package --> SBOM --> Acceptance --> Publish
```

Older-head, predecessor, synthetic-only, queued, skipped-required, or status-only evidence never enters the success path.

## Work-conserving maintenance sequence

```mermaid
sequenceDiagram
    participant Loop as DiskSage maintainer loop
    participant GitHub as GitHub live state
    participant PR as Current PR lane
    participant Other as Other safe lane

    Loop->>GitHub: refetch all PRs/issues/main/checks/reviews
    Loop->>PR: choose highest-value safe action
    alt PR becomes waiting or externally blocked
        PR-->>Loop: defer exact head/run identity
        Loop->>Other: immediately execute another safe action
    else mutation succeeds
        PR-->>Loop: new exact head/state
        Loop->>GitHub: refetch affected evidence
    end
    Loop->>GitHub: fresh whole-queue exit sweep
    alt any safe work remains
        GitHub-->>Loop: continue queue
    else no safe work or practical run budget exhausted
        GitHub-->>Loop: end this finite invocation
    end
```

A reviewer/check/provider wait is branch/action-local, not a run-wide stop signal.

## Deployment topology

```mermaid
flowchart TB
    subgraph Workstation[Operator workstation]
        UI2[Svelte/Tauri desktop]
        Rust[Rust authority layer]
        FS[Local filesystem]
        Model[On-device model]
        Private[Restricted local evidence]
        UI2 --> Rust
        Rust --> FS
        Rust --> Model
        Rust --> Private
    end

    subgraph Providers[Optional provider evidence]
        ICloud[iCloud/native File Provider]
        OneDrive[OneDrive]
        Google[Google Drive]
    end

    subgraph CWL[Optional CWL composition]
        Naruon2[Naruon]
        Orch2[contextual-orchestrator]
    end

    subgraph Repo[Software delivery control plane]
        Central[ContextualWisdomLab/.github]
        GitHub2[GitHub checks/reviews/releases]
    end

    Rust -. explicit provider observation .-> ICloud
    Rust -. explicit provider observation .-> OneDrive
    Rust -. explicit provider observation .-> Google
    Rust -. bounded evidence .-> Naruon2
    Rust -. optional advisory model routing .-> Orch2
    Central --> GitHub2
```

## Diagram maintenance rule

A change to bounded contexts, authority edges, lifecycle/state transitions, persistence, deployment, model/provider trust, or release evidence updates this file or records why the diagrams remain valid.