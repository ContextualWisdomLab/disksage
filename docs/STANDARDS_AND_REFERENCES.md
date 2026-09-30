# DiskSage Standards and Primary References

## Purpose and status discipline

This file is the canonical cross-cutting reference registry for the active acquisition-documentation branch. Feature doctoring may cite narrower standards or primary research; when a reference becomes a product-wide design, security, quality, accessibility, architecture, or release input it should be indexed here.

A citation means **design/evidence input**, not certification, attestation, legal compliance, or blanket conformance. Published final standards are normative design references when adopted by an accepted DiskSage requirement/ADR. Drafts are watch items only and never silently replace a published baseline.

The statuses below were revalidated from publisher-controlled primary sources on 2026-08-10. A later release must recheck fast-moving specifications rather than relying indefinitely on this dated status snapshot.

## Product quality and architecture

| Reference | Publisher status used by DiskSage | DiskSage use |
| --- | --- | --- |
| ISO/IEC 25010:2023 | published international standard, Edition 2 | product-quality model and acceptance-evidence framing; see `QUALITY_ATTRIBUTES.md` |
| ISO/IEC/IEEE 42010:2022 | published international standard, Edition 2 | architecture-description concerns, viewpoints, traceable models; see `ARCHITECTURE.md`, `UML.md`, and `INTEROPERABILITY.md` |

## Accessibility

| Reference | Publisher status used by DiskSage | DiskSage use |
| --- | --- | --- |
| W3C WCAG 2.2, Recommendation 2024-12-12 | published W3C Recommendation | testable accessibility criteria for DiskSage-owned webview content |
| ISO/IEC 40500:2025 | published international standard, Edition 2 | published ISO adoption of WCAG 2.2 |
| ISO/IEC DIS 40500, Edition 3 | draft/watch item | tracked only; not the normative published baseline |

See `ACCESSIBILITY_ACCEPTANCE.md` for release-evidence rules.

## Secure development and supply chain

| Reference | Publisher status used by DiskSage | DiskSage use |
| --- | --- | --- |
| NIST SP 800-218, SSDF v1.1 | final | secure development, provenance, security-requirement and root-cause practice vocabulary |
| NIST SP 800-218A | final | additional guidance where DiskSage handles model artifacts or model-backed development paths |
| NIST SP 800-218 Rev. 1, SSDF v1.2 | draft/watch item | forward-looking only until final publication |
| OWASP ASVS 5.0.0 | latest stable version reported by OWASP | scoped application-security verification input; not a claim that every ASVS requirement applies to a local Tauri desktop product |
| SLSA 1.2 | approved specification | source/build/provenance vocabulary for release evidence; release claims require exact repository evidence rather than citation alone |
| ISO/IEC 27001:2022 with Amendment 1:2024 | published standard plus published amendment | management/control readiness input only; DiskSage does not claim ISMS certification |
| ISO/IEC 27040:2024 | published international standard, Edition 2 | storage-security design input for local evidence, artifacts, caches, and deletion/reclaim boundaries |

## Observability interoperability

OpenTelemetry is implementation guidance rather than a DiskSage certification target. If telemetry export is implemented, pin the concrete OpenTelemetry SDK/specification and semantic-convention version in code/release evidence. Prefer stable convention groups for durable public contracts and treat unstable groups as migration-bearing dependencies. See `OBSERVABILITY.md`.

## Reference lifecycle rules

1. Revalidate publisher status before a material requirement, ADR, or release claim depends on a fast-moving standard/specification.
2. Keep final and draft references visibly separate.
3. Cite the exact edition/version used by a test, contract, ADR, or release evidence item.
4. Record a standard-to-requirement-to-code/test mapping in `TRACEABILITY.md`; citations without an executable or reviewable effect are context, not proof.
5. If two sources conflict, the relevant ADR records the conflict, alternatives, chosen rule, migration/rollback impact, and supersession condition.
6. Do not reproduce paywalled standard text. Repository docs record scoped interpretations and acceptance evidence instead.
7. Feature-specific research remains in doctoring until it becomes a cross-cutting product decision.

## APA 7th references

International Organization for Standardization. (2023). *ISO/IEC 25010:2023 Systems and software engineering—Systems and software Quality Requirements and Evaluation (SQuaRE)—Product quality model* (2nd ed.). https://www.iso.org/standard/78176.html

International Organization for Standardization. (2024). *ISO/IEC 27001:2022/Amd 1:2024 Information security, cybersecurity and privacy protection—Information security management systems—Requirements—Amendment 1: Climate action changes*. https://www.iso.org/standard/88435.html

International Organization for Standardization. (2024). *ISO/IEC 27040:2024 Information technology—Security techniques—Storage security* (2nd ed.). https://www.iso.org/standard/80194.html

International Organization for Standardization. (2025). *ISO/IEC 40500:2025 Information technology—W3C Web Content Accessibility Guidelines (WCAG) 2.2* (2nd ed.). https://www.iso.org/standard/91029.html

International Organization for Standardization, International Electrotechnical Commission, & Institute of Electrical and Electronics Engineers. (2022). *ISO/IEC/IEEE 42010:2022 Software, systems and enterprise—Architecture description* (2nd ed.). https://www.iso.org/standard/74393.html

National Institute of Standards and Technology. (2022). *Secure Software Development Framework (SSDF) Version 1.1: Recommendations for mitigating the risk of software vulnerabilities (NIST SP 800-218)*. https://doi.org/10.6028/NIST.SP.800-218

National Institute of Standards and Technology. (2024). *Secure software development practices for generative AI and dual-use foundation models: An SSDF community profile (NIST SP 800-218A)*. https://csrc.nist.gov/pubs/sp/800/218/a/final

Open Worldwide Application Security Project. (2025). *OWASP Application Security Verification Standard 5.0.0*. https://owasp.org/www-project-application-security-verification-standard/

OpenTelemetry Authors. (2026). *OpenTelemetry specification*. https://opentelemetry.io/docs/specs/otel/

OpenTelemetry Authors. (2026). *OpenTelemetry semantic conventions*. https://opentelemetry.io/docs/specs/semconv/

SLSA Community. (2025). *SLSA specification, version 1.2*. https://slsa.dev/spec/v1.2/

World Wide Web Consortium. (2024, December 12). *Web Content Accessibility Guidelines (WCAG) 2.2*. https://www.w3.org/TR/WCAG22/

## Non-normative watch items

- National Institute of Standards and Technology. (2025). *Secure Software Development Framework (SSDF) Version 1.2: Recommendations for mitigating the risk of software vulnerabilities (NIST SP 800-218 Rev. 1, draft)*. Publisher status remains draft as of this assessment.
- International Organization for Standardization. (2026). *ISO/IEC DIS 40500 Information technology—W3C Web Content Accessibility Guidelines (WCAG) 2.2* (Draft, Edition 3). https://www.iso.org/standard/94018.html
