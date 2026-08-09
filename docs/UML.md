# DiskSage UML and Architecture Diagrams

## Evidence status

These diagrams describe the intended architecture represented by the current source and active architecture PRs. Labels `active_pr` and `planned` are not protected-main implementation claims.

## Component and bounded-context view

```mermaid
flowchart LR
    User[Local operator]
    UI[Svelte presentation]
    IPC[Tauri IPC boundary]
    Observe[Rust observation]
    Plan[Rust planning and decision support]
    Auth[Rust authorization]
    Execute[Rust execution]
    Evidence[Evidence and receipts]
    Model[Optional local model]
    Provider[Provider/native APIs]
    Naruon[Naruon optional consumer]
    Orch[contextual-orchestrator optional]
    Control[CWL .github control plane]

    User --> UI --> IPC
    IPC --> Observe --> Plan
    Observe --> Provider
    Plan --> Auth --> Execute --> Evidence
    Model -. advisory .-> Plan
    Orch -. optional model routing .-> Plan
    Evidence -. bounded path-free contract .-> Naruon
    Control -. repository policy only .-> Evidence
```

The `.github` control plane governs repository evidence, not local runtime authorization.

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

    Operator->>UI: Start bounded scan
    UI->>IPC: typed read-only command
    IPC->>Obs: observe(scope, limits)
    Obs-->>Plan: bounded evidence + fingerprint
    Plan-->>UI: candidates + blockers + exact phrase
    Operator->>UI: select action + rationale + phrase
    UI->>IPC: proposed approval and exact plan
    IPC->>Auth: revalidate scope, fingerprint, UTC/monotonic freshness
    alt evidence or approval changed
        Auth-->>UI: fail-closed stable refusal
    else authorization valid
        Auth->>Exec: exact single-purpose execution permit
        Exec->>Exec: revalidate mutation-time preconditions
        Exec-->>Rec: bounded result + rollback/recovery evidence
        Rec-->>UI: result summary
    end
```

## Cloud copy and existing-copy adoption authority flow

```mermaid
sequenceDiagram
    actor Operator
    participant Scanner as Local/Cloud Evidence
    participant Capacity as Capacity Evidence
    participant Sync as Sync Evidence
    participant Planner as Copy Planner
    participant Review as Human Review
    participant Executor as Copy/Adoption Executor
    participant Receipt as Restricted Receipt

    Scanner->>Planner: source lineage + destination/provider scope
    Capacity->>Planner: capacity observation
    Sync->>Planner: item/provider state or unknown
    Planner-->>Review: exact immutable candidate plan + blockers
    Review-->>Planner: approver + rationale + exact confirmation
    Planner->>Executor: current plan + approval
    Executor->>Executor: refresh source/destination/provider preconditions
    alt drift or incomplete authority
        Executor-->>Review: refuse; new plan/approval required
    else valid copy/adoption
        Executor->>Receipt: create-new/no-clobber result evidence
        Receipt-->>Operator: bounded result
    end
```

Capacity, runtime presence, copy completion, provider sync, and local eviction permission remain distinct states.

## Model download and load integrity flow

```mermaid
flowchart TD
    Spec[Immutable model specification]
    Download[Bounded HTTPS stream]
    Stage[Create-new staging file]
    VerifyInstall[Exact size + SHA-256 + sync]
    Publish[No-clobber publication]
    Installed[Installed artifact]
    VerifyLoad[Load-time non-following metadata + exact size + SHA-256]
    Llama[llama.cpp initialization]

    Spec --> Download --> Stage --> VerifyInstall --> Publish --> Installed
    Spec --> VerifyLoad
    Installed --> VerifyLoad --> Llama

    classDef active fill:#fff,stroke:#555,stroke-dasharray: 5 5;
    class Download,Stage,VerifyInstall,Publish active;
    class VerifyLoad active;
```

Installation hardening is active PR #141. Load-time re-verification is active stacked PR #142. The diagram is architecture intent until those PRs are integrated.

## Runtime evidence and authority state machine

```mermaid
stateDiagram-v2
    [*] --> Unobserved
    Unobserved --> Observed: bounded observation succeeds
    Unobserved --> Incomplete: missing/malformed/bound exceeded
    Observed --> Planned: deterministic plan generated
    Planned --> Blocked: prerequisite unknown or unsafe
    Planned --> AwaitingApproval: executable candidate
    AwaitingApproval --> Authorized: exact human approval + current fingerprints + freshness
    AwaitingApproval --> Blocked: approval mismatch/expiry/clock invalid
    Authorized --> Stale: revalidation detects drift
    Authorized --> Executing: mutation-time preconditions still match
    Executing --> Completed: verified outcome + receipt
    Executing --> RecoveryRequired: bounded partial failure
    RecoveryRequired --> Completed: invocation-owned recovery completes
    Stale --> Planned: regenerate evidence and plan
    Blocked --> Unobserved: new observation required
    Incomplete --> Unobserved: retry after cause changes
```

## Repository merge and release authority flow

```mermaid
flowchart TD
    Head[Exact current PR head]
    Base[Independently resolved live base tip]
    Tests[Required CI/coverage/security]
    Reviews[Formal independent review if required]
    Findings[Zero valid unresolved findings]
    Policy[Branch/ruleset/repository policy]
    Merge[Protected merge]
    Main[Exact protected integrated head]
    Package[Packaging + compatibility]
    Prov[SBOM/provenance/release evidence]
    Accept[Release acceptance]
    Release[Published verified release]

    Head --> Tests
    Head --> Reviews
    Head --> Findings
    Head --> Base
    Base --> Policy
    Tests --> Policy
    Reviews --> Policy
    Findings --> Policy
    Policy --> Merge --> Main
    Main --> Package --> Prov --> Accept --> Release
```

Queued, stale, predecessor-head, status-only, or synthetic-only evidence never enters the success path.

## Deployment topology

```mermaid
flowchart TB
    subgraph Workstation[Operator workstation]
        UI2[Svelte/Tauri desktop]
        Rust[Rust authority layer]
        FS[Local filesystem]
        LocalModel[On-device model optional]
        Private[Restricted private evidence]
        UI2 --> Rust
        Rust --> FS
        Rust --> LocalModel
        Rust --> Private
    end

    subgraph OptionalProviders[Optional external provider evidence]
        OneDrive[OneDrive]
        GDrive[Google Drive]
        ICloud[iCloud/native File Provider]
    end

    subgraph CWL[Optional CWL composition]
        Naruon2[Naruon]
        Orch2[contextual-orchestrator]
    end

    Rust -. explicit provider read/verify .-> OneDrive
    Rust -. explicit provider read/verify .-> GDrive
    Rust -. native bounded evidence .-> ICloud
    Rust -. path-free versioned evidence .-> Naruon2
    Rust -. optional advisory model request .-> Orch2
```

A network or CWL outage degrades the corresponding optional capability; it does not transfer remote authority into the local runtime.

## Documentation maintenance rule

When a code or ADR change modifies a bounded context, state transition, authority edge, persistence relationship, deployment boundary, or release gate, update this file or explicitly record why the diagrams remain accurate. `src/lib/architectureDocumentation.test.ts` keeps the canonical diagram document discoverable.