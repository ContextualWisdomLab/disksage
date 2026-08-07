# Model artifact integrity and bounded installation

## Decision

DiskSage treats its downloadable on-device GGUF model as executable product supply-chain input. A model may be obtained from an upstream service, but it does not become trusted merely because the transport succeeded or because the upstream repository is trusted. The production registry therefore binds the default model to an immutable Hugging Face revision, an exact byte count, and a SHA-256 digest, and the installer independently verifies all three relevant local conditions before the artifact is exposed at the final model path.

The authoritative production implementation is `src-tauri/src/llm/model.rs`. The bounded installer is deliberately part of the normal Rust coverage surface; the previous `cfg(not(coverage))` exclusion around model downloading is not retained.

## Threat model

The installation path assumes the upstream network, CDN response, partial download, local staging namespace, and concurrent local destination creation can all fail or drift independently. Controls therefore address the following failure modes:

- a mutable upstream branch later resolving to different bytes;
- a response declaring or delivering a different size from the reviewed model specification;
- an oversized or unbounded response exhausting memory or disk unexpectedly;
- a truncated response that happens to parse as an ordinary file;
- bytes with the wrong SHA-256 digest being promoted into the executable model location;
- an existing destination, symlink, or stale staging file being overwritten;
- an I/O failure leaving an apparently complete final artifact;
- local or network diagnostics leaking paths or untrusted response detail through the public error contract.

This boundary does **not** claim that SHA-256 proves model safety, behavioral quality, training-data provenance, absence of backdoors, or license suitability. The digest proves only that the received bytes match the specifically reviewed artifact. Model behavioral evaluation and inference governance remain separate controls.

## Upstream identity

The default Qwen2.5 1.5B Instruct Q4_K_M GGUF URL is pinned to Hugging Face revision `a615a81362316d7b9f5a7a9c4313adfdf9b54588` instead of `main`. Hugging Face's file page for that immutable revision reports SHA-256 `6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e`; DiskSage also retains the reviewed exact byte count `1,117,320,736` in the model registry.

Hugging Face documents revision-specific downloads and content-addressed blob caching. Its current Hub documentation also describes Xet retrieval as using the LFS SHA-256 hash to obtain reconstruction metadata. DiskSage does not delegate its local trust decision to cache metadata: it recomputes the SHA-256 digest over the bytes it actually writes.

## Upstream license evidence

The official `Qwen/Qwen2.5-1.5B-Instruct-GGUF` repository declares the model repository license as `apache-2.0`. DiskSage currently downloads the reviewed artifact directly from that upstream repository on explicit user request; the model is not bundled into DiskSage's application package by this slice. This record is acquisition due-diligence evidence, not legal advice or a representation that model licensing can never change.

If a future release bundles, mirrors, or otherwise redistributes the model artifact, release acceptance must re-check the exact upstream revision's license and accompanying attribution material and satisfy the applicable Apache License, Version 2.0 redistribution conditions before publication. The Apache Software Foundation identifies Apache License 2.0 as its current license and its distribution guidance explains that license and NOTICE material must be preserved where the license requires it. A future packaging change therefore cannot inherit approval merely from this runtime-download decision.

## Bounded streaming and no-clobber finalization

`ureq` 3.3 documents that response readers are unlimited unless a body limit is configured and that `Content-Length`, when present, is enforced by its HTTP body machinery. DiskSage sets an independent reader limit of `expected bytes + 1`, allowing the installer to detect an overlong body while avoiding the previous design that accumulated the entire approximately 1.12 GB model in a `Vec<u8>` before writing.

The installer uses a fixed 64 KiB buffer, updates SHA-256 while writing, and refuses short, long, or digest-mismatched streams. The sibling staging name appends `.part` to the complete destination filename and is opened with create-new semantics. After exact-size and digest validation, the file is flushed and `sync_all` is required. Promotion uses a same-directory hard link, which fails if another entry already occupies the final destination; only after that no-clobber link succeeds is the staging name removed. Validation and I/O failures remove staging evidence rather than presenting it as an installed model.

The preflight destination check improves operator feedback but is not treated as durable authorization: the final hard-link operation re-establishes the no-clobber condition at mutation time. This preserves the repository's separation between local validation and durable mutation authority. A deterministic concurrency regression test deliberately creates the destination after staging has begun but before finalization; the installer must preserve the concurrently created file, fail with the stable finalization code, and remove only its own staging entry.

## Load-time verification boundary

Download-time admission and load-time trust are separate controls. A file's mere presence is not a trust decision: **file existence is not integrity evidence**. A model path can be pre-positioned before DiskSage runs, replaced after a previously valid download, redirected through a symbolic link, truncated, or modified in place. Before `LlamaEngine::new` initializes the llama.cpp backend or asks llama.cpp to parse a GGUF, DiskSage therefore re-verifies the installed artifact against the same immutable `DEFAULT` specification that authorized download installation.

The verifier first uses non-following path metadata to reject symbolic links, directories, and other non-regular entries. It then opens the file read-only, checks the opened handle's exact length against the trusted byte count, and streams the current bytes through a fixed **64 KiB** buffer while recomputing SHA-256. Short streams, overlong streams, read failures, and digest drift all fail closed. The engine source contract requires the pinned-default verification call to occur textually before both `LlamaBackend::init()` and `LlamaModel::load_from_file`; no alternate unverified constructor is introduced by this slice.

The stable load-time refusal codes are `model-installed-unavailable`, `model-installed-not-regular`, `model-installed-size-mismatch`, `model-installed-read-failed`, and `model-installed-digest-mismatch`. These codes intentionally reveal only the reason category. **No model bytes or local paths become shareable evidence**, and operating-system or llama.cpp diagnostics are not substituted for these verifier codes.

This check is a local integrity validator, not durable authorization and not an anti-malware or model-behavior verdict. It proves that the file bytes opened by the verifier match the reviewed artifact at verification time. It does not turn model content into trusted application data, does not establish training provenance, and does not waive the separate release, provenance, licensing, review, or model-quality gates. A local process with equivalent user privileges may still mutate filesystem state later; DiskSage therefore keeps the verification immediately before llama initialization and refuses to infer trust from an earlier installation receipt.

Pre-existing installations do not require a migration merely because this verifier is added. A pre-existing file with the exact reviewed size and SHA-256 remains loadable. A pre-existing missing, linked, truncated, oversized, unreadable, or tampered file is refused and must be replaced through the reviewed download path rather than grandfathered by existence or age.

## Error and privacy boundary

Public installer failures are stable codes such as `model-size-mismatch`, `model-sha256-mismatch`, and `model-download-unavailable`. They intentionally omit destination paths, response bodies, upstream diagnostic strings, account information, and other dynamic local or network context. Detailed debugging remains a local developer concern and is not part of shareable product evidence.

Load-time verification follows the same privacy boundary with its `model-installed-*` codes. Neither installer nor verifier errors include the local model directory, filename chosen by an operator, network response body, account-local context, or the model bytes themselves.

## Verification contract

The Rust tests exercise:

- the published SHA-256 helper with known vectors;
- immutable-revision pinning of the default model;
- fail-closed specification validation;
- staging-name semantics;
- successful exact-size and exact-digest streaming installation;
- short, oversized, and digest-mismatched streams;
- existing destination and stale staging refusal;
- concurrent destination creation after staging preflight without overwrite;
- deterministic reader failure cleanup;
- missing-parent staging failure without path-bearing errors;
- a real loopback HTTP success path through `ureq`;
- declared `Content-Length` drift before staging creation;
- malformed transport response redaction;
- invalid model metadata refusal before network access;
- load-time acceptance of exact installed bytes with case-insensitive expected digest matching;
- load-time refusal of missing, symbolic-link, non-regular, short, oversized, unreadable, and same-size tampered inputs;
- source binding that places pinned-default verification before llama backend and model initialization; and
- deterministic doctoring/changelog retention for the load-time integrity boundary.

No live 1.12 GB model download is required for CI acceptance. The reviewed upstream revision, digest, and size remain source-controlled evidence, while deterministic tests prove the production installer and load-verifier behavior without consuming external network authority.

## Standards and acquisition mapping

NIST SP 800-218 version 1.1 remains the current **final** SSDF, while NIST published SP 800-218 Rev. 1 / SSDF 1.2 as an Initial Public Draft on December 17, 2025. DiskSage records the newer draft as forward-looking evidence but does not misrepresent it as a final standard. NIST SP 800-218A is final and explicitly extends SSDF practices to producers and acquirers of AI systems and models.

This slice supports those acquisition-oriented secure-development expectations by making the model dependency version-specific, integrity-bound, independently verified during installation **and again immediately before model initialization**, bounded during transfer and load-time hashing, and covered by deterministic tests. OWASP Top 10:2025 A03 recommends obtaining components from official sources over secure links and hardening supply-chain artifacts, while A08 specifically identifies downloading or consuming artifacts without adequate integrity verification as an integrity failure. SLSA 1.2 similarly treats fetched artifacts as dependencies whose identities and digests are material to provenance. These mappings are engineering rationale, not a claim of certification or blanket conformance.

## Standalone and MSA compatibility

The model artifact is installed and verified locally. No Naruon, contextual-orchestrator, central CWL service, tenant account, or remote authorization service is required for standalone operation. If another CWL service later supplies model metadata or download coordination, it may propose an artifact but cannot weaken the DiskSage local exact-size, exact-digest, immutable-revision, no-clobber installation, or load-time re-verification boundary. The on-device inference path therefore remains usable as a standalone component and as a bounded module in a larger MSA.

## Rollback and migration

There is no database migration and no database object is introduced by these model-integrity slices. Existing valid model files remain readable because the runtime model filename does not change. The bounded installer affects future downloads, while load-time verification applies equally to newly downloaded and pre-existing files.

Rollback is a reviewed source change, never a runtime bypass. If the pinned upstream revision becomes unavailable, update the immutable revision, exact byte count, and SHA-256 together after independently revalidating the intended model file; add a failing regression test first; update this document and `CHANGELOG.md`; and rerun exact-head Rust tests, coverage, security, packaging, provenance, and release-acceptance gates. Do not revert to `/resolve/main/`, remove the byte limit, accept a missing/mismatched digest, restore whole-model in-memory buffering, skip load-time verification for pre-existing files, or initialize llama.cpp after a verifier failure as an availability shortcut. If a legitimate new artifact replaces the current default, migrate by changing the trusted specification and obtaining a fresh verified artifact rather than weakening the verifier.

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

Souppaya, M., Scarfone, K., & Dodson, D. (2022). *Secure software development framework (SSDF) version 1.1: Recommendations for mitigating the risk of software vulnerabilities* (NIST Special Publication 800-218). National Institute of Standards and Technology. https://doi.org/10.6028/NIST.SP.800-218

ureq contributors. (2026). *ureq 3.3.0: Body and BodyWithConfig* [Rust crate documentation]. Docs.rs. https://docs.rs/ureq/3.3.0/ureq/struct.Body.html

## Evidence verification note

The NIST SSDF publication index, NIST SP 800-218A final publication, SLSA 1.2 specification, OWASP Top 10:2025 A03/A08 pages, Hugging Face Hub download documentation, official Qwen GGUF repository license declaration, immutable Qwen model-file page, Apache Software Foundation License 2.0 guidance, and ureq 3.3.0 primary crate documentation were rechecked on August 7, 2026. The load-time verifier uses the same immutable upstream identity and therefore does not introduce a new external evidence source. The final-versus-draft status distinction above reflects NIST's publication index as of that date.
