# DiskSage Observability and Evidence Boundary

## Status

This document defines the cross-cutting observability contract on the active canonical documentation branch. It does not claim that protected `main` currently exports OpenTelemetry data or provides a production telemetry backend.

DiskSage is local-first. Observability must help users/operators diagnose product health without turning filesystem contents, provider identities, credentials, model prompts, or user activity into an implicit remote data product.

## Authority separation

Observability is **evidence**, not authorization. A metric, trace, log, workflow status, model judgement, or health signal can explain what occurred; it cannot authorize filesystem/provider mutation, approve a pull request, satisfy a security gate, or authorize a release by itself.

## Signal classes

| Signal | Intended use | Default privacy posture |
| --- | --- | --- |
| Structured local event | bounded product state transition, failure class, recovery step | no raw paths, filenames, document/media content, provider credentials, account IDs, prompts, model output, or command output |
| Metric | aggregate count/duration/size class needed to understand product health | aggregate/bucket where practical; no user-content labels or unbounded cardinality |
| Trace/span | optional diagnosis of multi-stage local or host-integrated operation | opt-in or host-controlled export; identifiers must be opaque and bounded |
| Release/CI evidence | exact source/artifact/check/review provenance | repository evidence only; separate from installed-product telemetry |
| Security/incident evidence | minimum data needed to reproduce/contain a failure | purpose-bound, access-controlled, retention-bounded; sensitive payload collection requires explicit justification |

## Stable event envelope

If product telemetry is persisted or exported, a versioned envelope should use descriptive fields such as:

```text
event_name
event_version
occurred_at_ms
component_name
operation_name
outcome_code
duration_ms
evidence_token
```

`evidence_token` is an opaque correlation value, not a path, user ID, provider account, credential, or content hash that leaks user material. Optional fields must remain bounded and schema-versioned.

## Prohibited default fields

Do not emit by default:

- absolute or relative user filesystem paths;
- filenames or directory names derived from user content;
- document, archive, media, prompt, model-response, or command-output contents;
- OAuth/API credentials, cookies, tokens, key material, or authorization headers;
- provider account identifiers, machine names, tenant secrets, or raw remote object identifiers;
- environment-variable dumps, stack traces containing secrets, or unrestricted process output;
- unbounded high-cardinality labels whose values effectively reconstruct private user activity.

A feature needing one of these for a user-requested diagnostic bundle must document purpose, minimization, authorization, retention, export destination, and deletion behavior before implementation.

## Failure taxonomy

Prefer stable codes over raw exception text. At minimum distinguish:

- `operation_succeeded`;
- `operation_incomplete`;
- `input_invalid`;
- `evidence_stale`;
- `dependency_unavailable`;
- `permission_denied`;
- `integrity_failed`;
- `resource_limit_reached`;
- `operation_cancelled`;
- `internal_error`.

Feature-specific codes may be narrower. A code must not conceal a security-relevant failure behind success.

## OpenTelemetry interoperability

OpenTelemetry is the preferred external telemetry vocabulary **if** a future host/exporter is implemented. The exact SDK/specification/semantic-convention version must be pinned by that implementation and release rather than inferred from this document. Use only stable semantic-convention groups for durable public contracts unless an ADR explicitly accepts an unstable convention and migration plan.

OpenTelemetry's current specification separates traces, metrics, and logs and its semantic conventions define common attribute meanings; individual convention groups carry explicit stability levels. DiskSage therefore treats telemetry schema/version and stability as interoperability evidence, not as permission to copy arbitrary upstream attributes into a privacy-sensitive local product.

## Acceptance evidence

Before claiming production observability support, require as applicable:

1. deterministic schema tests for event names/types/versions;
2. tests proving forbidden private fields are absent from representative error/success paths;
3. bounded-cardinality tests for dimensions derived from external/user-controlled values;
4. dependency/exporter outage tests proving product operations fail independently where telemetry is optional;
5. retention/export controls documented for any durable or remote collector;
6. trace/metric/log correlation tests that use opaque identifiers;
7. operator documentation mapping a signal to a concrete action without exposing private payloads.

No remote telemetry endpoint is implied by this document. A future durable collector, SaaS backend, or host-owned observability plane requires its own accepted architecture and privacy contract.

## References (APA 7th)

OpenTelemetry Authors. (2026). *OpenTelemetry specification*. https://opentelemetry.io/docs/specs/otel/

OpenTelemetry Authors. (2026). *OpenTelemetry semantic conventions*. https://opentelemetry.io/docs/specs/semconv/
