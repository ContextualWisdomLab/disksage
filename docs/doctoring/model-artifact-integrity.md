# Model artifact integrity and bounded installation

## Decision

DiskSage treats its downloadable on-device GGUF model as executable product supply-chain input. Successful HTTPS transfer is not sufficient authorization to install or execute a model. The production registry binds the default model to an immutable upstream revision, an exact byte count, and a SHA-256 digest. The installer verifies the streamed bytes before finalization, copies from the still-open verified staging handle, and independently re-verifies the final destination bytes before reporting success.

The authoritative production implementation is `src-tauri/src/llm/model.rs`. Model installation remains inside the normal Rust coverage surface; it is not hidden behind a coverage exclusion.

## Threat model

The installation boundary assumes that the upstream response, local filesystem namespace, concurrent local processes, and I/O can fail or change independently. It therefore addresses:

- mutable upstream references resolving to different model bytes;
- incorrect declared or observed byte counts;
- oversized or unbounded responses exhausting memory or disk unexpectedly;
- truncated model transfers;
- SHA-256 mismatch;
- whole-model buffering of an approximately 1.12 GB artifact;
- an existing destination being overwritten;
- a named staging pathname being pre-created, replaced, or repurposed by another actor;
- same-file source mutation after an earlier verification pass;
- destination replacement after DiskSage creates it;
- same-file destination mutation before final acceptance;
- cleanup deleting a pathname now owned by another actor; and
- local paths, response bodies, or dynamic network diagnostics escaping the public error boundary.

SHA-256 proves only that the accepted bytes match the reviewed artifact. It does **not** prove behavioral safety, model quality, training-data provenance, absence of backdoors, or license suitability. Those remain separate governance controls.

## Upstream identity and license evidence

The default Qwen2.5 1.5B Instruct Q4_K_M GGUF URL is pinned to Hugging Face revision `a615a81362316d7b9f5a7a9c4313adfdf9b54588`, not `main`. The reviewed artifact identity is:

- exact byte count: `1,117,320,736`;
- SHA-256: `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e`.

Hugging Face documents revision-specific downloads and content-addressed artifact handling. DiskSage does not delegate the local trust decision to transport or cache metadata; it recomputes SHA-256 over the bytes it actually receives and again over the bytes it is about to accept.

The official `Qwen/Qwen2.5-1.5B-Instruct-GGUF` repository declares `apache-2.0`. This slice downloads the reviewed artifact on explicit request and does not bundle it into the DiskSage application package. If a future release bundles, mirrors, or redistributes the artifact, release acceptance must re-check the exact revision's license and required attribution/NOTICE material. This record is engineering due-diligence evidence, not legal advice.

## Bounded transfer

`ureq` 3.3 documents that response readers are otherwise unlimited unless a body limit is configured. DiskSage sets an independent reader limit of `expected bytes + 1`. When `Content-Length` is present it must equal the trusted specification before local staging begins. A fixed 64 KiB buffer streams the body while counting bytes and computing SHA-256, so the approximately 1.12 GB model is never accumulated in one in-memory `Vec<u8>`.

Short, oversized, or digest-mismatched streams fail closed. The verified staging file is flushed and `sync_all` is required before finalization.

## Unnamed staging removes pathname authority

Earlier iterations used a sibling `<destination>.part` pathname. That design could bind cleanup or promotion decisions to a mutable namespace entry and therefore retained avoidable TOCTOU authority questions. The current implementation removes the staging pathname from the authorization model entirely.

DiskSage now uses `tempfile::tempfile_in(destination_parent)` to create an unnamed temporary file in the destination directory. The `tempfile` 3.27.0 primary documentation identifies `tempfile_in` as returning an unnamed temporary `File`; its security documentation favors unnamed temporary files when a persistent pathname is unnecessary. Because DiskSage never needs to publish the staging pathname, no `.part` path is reserved, promoted, or unlinked by the installer.

Consequences:

- a pre-existing legacy `.part` file is unrelated data and is preserved;
- a foreign actor may replace such a legacy `.part` path during transfer without gaining installer authority;
- staging cleanup is handle lifetime, not pathname deletion;
- there is no staging-path check-then-unlink race to authorize; and
- the second verification pass reads from the still-open unnamed staging file itself.

## Destination no-clobber and identity binding

The initial destination existence check is operator feedback only. It is **not** durable authorization. Durable mutation authority is re-established at finalization with `OpenOptions::create_new(true)`. If another actor owns the destination by then, finalization fails and the foreign file is preserved.

When create-new succeeds, DiskSage captures the operating-system identity of that exact returned open destination file through `same_file::Handle`. Subsequent cleanup may remove the pathname only while it still resolves to that captured identity. If another actor replaces the pathname, DiskSage preserves the replacement rather than deleting it as if it still owned the name.

After destination creation, DiskSage clones and rewinds the already-open unnamed staging file, then copies through the bounded buffer while recomputing exact byte count and SHA-256. This second source pass rejects same-file mutation, growth, or truncation that occurs after the first network-stream verification.

The destination is then flushed and `sync_all` is required. DiskSage rewinds the still-open destination handle, recomputes exact byte count and SHA-256 from the installed file itself, and checks that the pathname still resolves to the captured destination identity. Same-file destination mutation before final acceptance therefore fails closed. Finalization errors return the stable code `model-finalize-failed` and remove only a path still proven to identify DiskSage's captured destination file.

A deterministic regression directly exercises the durable create-new boundary with a foreign destination already present at finalization. This avoids timing-sensitive network scheduling while proving the security property that matters: the finalizer cannot replace another actor's path. Separate deterministic hooks cover replacement after destination creation and after final content verification.

## Load authorization boundary

The final destination pathname necessarily exists while verified copying and destination re-verification occur. Path existence is therefore **not** sufficient model-load authorization. A model consumer must independently verify the trusted exact byte count and SHA-256 before inference. The stacked model-load-integrity slice provides that execution-side control. Until installation and load verification are both integrated, this PR is a bounded supply-chain hardening slice rather than an end-to-end claim that every observable model path is executable.

## Error and privacy boundary

Public installer failures use stable codes such as `model-size-mismatch`, `model-sha256-mismatch`, `model-staging-create-failed`, `model-finalize-failed`, and `model-download-unavailable`. They intentionally omit destination paths, response bodies, upstream diagnostic strings, account information, and other dynamic local or network context. Detailed debugging evidence remains local and is not part of shareable product evidence.

## Deterministic verification contract

Rust tests exercise:

- known SHA-256 vectors;
- immutable upstream revision pinning;
- fail-closed trusted-spec validation;
- relative and nested destination-parent behavior;
- exact-size/exact-digest streamed installation;
- short, oversized, and wrong-digest streams;
- existing destination refusal;
- preservation of a foreign destination at the durable create-new boundary;
- preservation and isolation of pre-existing and concurrently replaced legacy `.part` paths;
- destination replacement after create-new identity capture;
- destination replacement after final content verification;
- same-file source wrong-digest mutation, growth, and truncation after first-pass verification;
- same-file destination mutation before final re-verification;
- regular-file identity matching, symlink rejection, idempotent owned cleanup, and preservation of unowned paths;
- deterministic reader-failure cleanup of unnamed staging;
- missing-parent staging failure without path-bearing errors;
- a real loopback HTTP success path through `ureq`;
- `Content-Length` drift before staging creation;
- malformed transport-response redaction; and
- invalid model metadata refusal before network access.

No live 1.12 GB download is required for CI acceptance. Source-controlled immutable revision, exact size, and digest bind the reviewed artifact while deterministic fixtures exercise production transfer and installation semantics without external network authority.

## Standards and acquisition mapping

NIST SP 800-218 version 1.1 remains the current **final** Secure Software Development Framework. NIST SP 800-218 Rev. 1 / SSDF 1.2 was published as an Initial Public Draft on December 17, 2025 and is recorded only as forward-looking evidence. NIST SP 800-218A is final and extends SSDF practices to producers and acquirers of AI systems and models.

This slice supports acquisition-oriented secure-development expectations by making the model dependency immutable-revision-specific, integrity-bound, transfer-bounded, independently re-verified, namespace-race resistant, privacy-safe, and deterministically tested. OWASP Top 10:2025 A03 addresses software supply-chain failures and A08 addresses software/data integrity failures. SLSA 1.2 treats fetched artifacts as dependencies whose identity and digest are provenance-relevant. These are engineering mappings, not claims of certification or blanket conformance.

## Standalone and MSA compatibility

The artifact is downloaded, staged, and verified locally. No Naruon, contextual-orchestrator, central CWL service, tenant account, or remote authorization service is required for standalone installation. A future CWL integration may propose model metadata or coordinate transfer, but it cannot weaken the local immutable-revision, exact-size, exact-digest, create-new destination, open-handle identity, or re-verification boundaries.

## Migration and rollback

This slice introduces no database object and requires no database migration. Existing valid model filenames do not change; already-installed valid model files remain readable. `tempfile` moves from test-only use into the production Rust dependency graph because unnamed staging is now a production security primitive.

Rollback must be a reviewed source change. If the pinned upstream artifact changes or becomes unavailable, update the immutable revision, exact byte count, and SHA-256 together after independent revalidation; add a failing regression first; update this document and `CHANGELOG.md`; and rerun exact-head tests, 100% production coverage, security, packaging, provenance, review, approval, and release-acceptance gates. Do not restore `/resolve/main/`, whole-model buffering, named staging authority, overwrite behavior, missing/mismatched digest acceptance, pathname-derived ownership, or weaker destination verification as an availability shortcut.

## APA 7th references

Apache Software Foundation. (2004). *Apache License, Version 2.0*. https://www.apache.org/licenses/LICENSE-2.0

Booth, H., Souppaya, M., Vassilev, A., Ogata, M., Stanley, M., & Scarfone, K. (2024). *Secure software development practices for generative AI and dual-use foundation models: An SSDF community profile* (NIST Special Publication 800-218A). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-218A

Hugging Face. (n.d.). *Download files from the Hub*. Retrieved August 7, 2026, from https://huggingface.co/docs/huggingface_hub/en/guides/download

Hugging Face. (n.d.). *Qwen/Qwen2.5-1.5B-Instruct-GGUF*. Retrieved August 7, 2026, from https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF

Hugging Face. (n.d.). *qwen2.5-1.5b-instruct-q4_k_m.gguf at revision a615a81362316d7b9f5a7a9c4313adfdf9b54588*. Retrieved August 7, 2026, from https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/blob/a615a81362316d7b9f5a7a9c4313adfdf9b54588/qwen2.5-1.5b-instruct-q4_k_m.gguf

National Institute of Standards and Technology. (2025). *Secure software development framework (SSDF) version 1.2: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218 Rev. 1, Initial Public Draft). https://csrc.nist.gov/pubs/sp/800/218/r1/ipd

Open Worldwide Application Security Project. (2025). *A03:2025 software supply chain failures*. https://owasp.org/Top10/2025/A03_2025-Software_Supply_Chain_Failures/

Open Worldwide Application Security Project. (2025). *A08:2025 software or data integrity failures*. https://owasp.org/Top10/2025/A08_2025-Software_or_Data_Integrity_Failures/

OpenSSF. (n.d.). *Supply-chain Levels for Software Artifacts specification, version 1.2*. Retrieved August 7, 2026, from https://slsa.dev/spec/v1.2/

same-file contributors. (n.d.). *same-file 1.0.6: Handle* [Rust crate documentation]. Retrieved August 7, 2026, from https://docs.rs/same-file/1.0.6/same_file/struct.Handle.html

Souppaya, M., Scarfone, K., & Dodson, D. (2022). *Secure software development framework (SSDF) version 1.1: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-218

tempfile contributors. (2026). *tempfile 3.27.0: tempfile_in* [Rust crate documentation]. Docs.rs. https://docs.rs/tempfile/3.27.0/tempfile/fn.tempfile_in.html

ureq contributors. (2026). *ureq 3.3.0: Body and BodyWithConfig* [Rust crate documentation]. Docs.rs. https://docs.rs/ureq/3.3.0/ureq/struct.Body.html

## Evidence verification note

The NIST SSDF publication status, NIST SP 800-218A, SLSA 1.2, OWASP Top 10:2025 A03/A08, Hugging Face Hub/model evidence, Apache License 2.0 guidance, same-file 1.0.6, and ureq 3.3.0 evidence were checked on August 7, 2026. The current docs.rs package page and changelog for `tempfile` 3.27.0 were rechecked on August 8, 2026; docs.rs reports version 3.27.0 as published March 11, 2026. The final-versus-draft NIST distinction above is intentional.
