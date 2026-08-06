# Tauri content security policy

## Decision

DiskSage enables an explicit Content Security Policy for the production Tauri webview. A null CSP is not an acceptable release posture because Tauri only enables its CSP protection when the policy is configured. The production policy therefore defaults to bundled application content, permits only the Tauri IPC transport needed by `@tauri-apps/api`, and does not authorize arbitrary network origins, remote scripts, remote styles, wildcard sources, inline scripts, `eval`, or WebAssembly evaluation.

The authoritative configuration is `src-tauri/tauri.conf.json` under `app.security.csp`.

## Production policy

The policy has the following reviewed boundary:

- `default-src 'self'` makes bundled application content the default authority.
- `connect-src ipc: http://ipc.localhost` permits the documented Tauri IPC transport and no general remote network origin. OAuth and provider-network activity remains in the Rust backend rather than being granted to arbitrary webview JavaScript.
- `script-src 'self'` keeps executable script local. It deliberately omits `'unsafe-inline'`, `'unsafe-eval'`, `'wasm-unsafe-eval'`, remote origins, and wildcards. Tauri adds the hashes and nonces required by bundled assets at compile time.
- `style-src 'self' 'unsafe-inline'` is the only inline exception. It is retained because the current Svelte UI uses dynamic style attributes such as percentage-width bars. This exception grants style application, not script execution, and must be removed if those dynamic styles are replaced by a nonce/hash-compatible mechanism.
- `img-src 'self' data: blob:` supports bundled UI images plus in-memory image URLs without granting a remote host.
- `font-src 'self'` keeps fonts local.
- `object-src 'none'`, `frame-src 'none'`, and `base-uri 'none'` remove plugin/object embedding, nested browsing contexts, and base-URL rewriting authority that DiskSage does not need.

The policy intentionally does not enable Tauri's filesystem asset protocol. If a future feature needs `asset:` or `http://asset.localhost`, that change must separately enable and narrowly scope `app.security.assetProtocol`, add a failing regression test first, document the path boundary, and rerun exact-head security and packaging checks.

## Threat model and limits

CSP is defense in depth against content-injection impact, not a substitute for input validation, output encoding, least-privilege Tauri capabilities, Rust-side authorization, or safe handling of untrusted file metadata. A strict `script-src` reduces the privilege available to an injected script, while `object-src 'none'` and the default deny posture reduce secondary execution and embedding paths.

The `connect-src` exception is intentionally specific to Tauri IPC. It must not be expanded to provider APIs merely because DiskSage supports cloud providers: those network operations already cross reviewed Rust commands and provider-specific authorization boundaries. Moving them into webview fetch authority would collapse a useful privilege separation.

## Verification contract

`src/lib/tauriContentSecurityPolicy.test.ts` parses the source-controlled Tauri configuration and fails when the CSP becomes null, when any reviewed directive drifts, when wildcard or HTTPS remote authority appears, or when script execution gains inline/eval/WebAssembly-eval authority. This is a source-level governance test; Tauri's build and release workflows remain responsible for proving that the exact current head packages successfully with the configured policy.

The contract is intentionally exact rather than a loose substring check. Any future expansion must be reviewed as a security-boundary change and cannot silently inherit approval from an older head.

## Rollback and migration

Rollback is a reviewed source change, not a runtime bypass:

1. reproduce the application breakage against the exact current head and identify the minimum missing directive;
2. add or update a regression test that demonstrates the required behavior without authorizing unrelated sources;
3. prefer a narrow source addition over setting `csp` back to `null`;
4. update this document and `CHANGELOG.md` with the changed authority and rationale;
5. rerun exact-head frontend tests, Tauri build/packaging, security scans, and release-acceptance checks before merge.

Setting `csp` to `null`, adding `*`, adding a remote script/style origin, or adding script `'unsafe-inline'`/`'unsafe-eval'` is treated as a security regression unless an independently reviewed design demonstrates a concrete product requirement and a narrower control is unavailable.

## Standalone and MSA compatibility

The CSP is local to the DiskSage desktop webview. It does not require Naruon, contextual-orchestrator, organization-central services, or a network connection for standalone operation. CWL service integration continues through backend contracts rather than broadening the browser-like authority of the desktop frontend.

## APA 7th references

Tauri Programme within The Commons Conservancy. (2025, April 7). *Content Security Policy (CSP)*. Tauri. https://v2.tauri.app/security/csp/

Tauri Programme within The Commons Conservancy. (n.d.). *Configuration*. Tauri. Retrieved August 7, 2026, from https://v2.tauri.app/reference/config/

World Wide Web Consortium. (2016, December 15). *Content Security Policy Level 2* (W3C Recommendation). https://www.w3.org/TR/CSP2/

World Wide Web Consortium. (2026, May 5). *Content Security Policy Level 3* (W3C Working Draft). https://www.w3.org/TR/2026/WD-CSP3-20260505/

## Reference verification note

The Tauri v2 CSP and configuration documentation and the W3C CSP publications were rechecked on August 7, 2026. Tauri documents that CSP protection is enabled only when configured and recommends making it as restrictive as practical; it also documents the `ipc:` and `http://ipc.localhost` transport sources used in its CSP example. W3C CSP Level 2 is the published Recommendation, while Level 3 is a current Working Draft and is cited as work in progress rather than as a Recommendation.
