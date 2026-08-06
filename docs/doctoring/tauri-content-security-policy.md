# Tauri content security policy

## Decision

DiskSage enables explicit Content Security Policies for both production and development Tauri webviews. A null CSP is not an acceptable release posture because Tauri only enables its CSP protection when the policy is configured. The production policy therefore defaults to bundled application content, permits only the Tauri IPC transport needed by `@tauri-apps/api`, and does not authorize arbitrary network origins, remote scripts, remote styles, wildcard sources, inline scripts, `eval`, or WebAssembly evaluation.

The authoritative configuration is `src-tauri/tauri.conf.json` under `app.security.csp` and `app.security.devCsp`.

## Production policy

The production policy has the following reviewed boundary:

- `default-src 'self'` makes bundled application content the default authority.
- `connect-src ipc: http://ipc.localhost` permits the documented Tauri IPC transport and no general remote network origin. OAuth and provider-network activity remains in the Rust backend rather than being granted to arbitrary webview JavaScript.
- `script-src 'self'` keeps executable script local. It deliberately omits `'unsafe-inline'`, `'unsafe-eval'`, `'wasm-unsafe-eval'`, remote origins, and wildcards. Tauri adds the hashes and nonces required by bundled assets at compile time.
- `style-src 'self' 'unsafe-inline'` is the only inline exception. It is retained because the current Svelte UI uses dynamic style attributes such as percentage-width bars. This exception grants style application, not script execution, and must be removed if those dynamic styles are replaced by a nonce/hash-compatible mechanism.
- `img-src 'self' data: blob:` supports bundled UI images plus in-memory image URLs without granting a remote host.
- `font-src 'self'` keeps fonts local.
- `object-src 'none'`, `frame-src 'none'`, and `base-uri 'none'` remove plugin/object embedding, nested browsing contexts, and base-URL rewriting authority that DiskSage does not need.

The production policy intentionally does not enable Tauri's filesystem asset protocol. If a future feature needs `asset:` or `http://asset.localhost`, that change must separately enable and narrowly scope `app.security.assetProtocol`, add a failing regression test first, document the path boundary, and rerun exact-head security and packaging checks.

## Development policy

Tauri applies `csp` during development when `devCsp` is absent. DiskSage's Vite development server uses a fixed HTTP port and WebSocket-based hot-module replacement; when `TAURI_DEV_HOST` is supplied, Vite deliberately moves that HMR WebSocket to port 1421 while retaining a validated, non-wildcard bind address. A production `connect-src` that authorizes only Tauri IPC would therefore make the normal development feedback loop fail closed in the wrong place by blocking Vite's WebSocket rather than constraining it to development.

DiskSage consequently defines a separate `devCsp` with the same script, style, image, font, object, frame, and base-URI restrictions as production, but with `connect-src 'self' ipc: http://ipc.localhost ws:`. The `ws:` scheme exception exists only in `devCsp`; it is absent from the production `csp`. This preserves Vite HMR for both the default same-origin development server and the repository's validated `TAURI_DEV_HOST` path without granting production webview JavaScript a general WebSocket channel.

The `ws:` development exception is still bounded by the development lifecycle: it is not release authority, does not expand Tauri command permissions, does not enable the filesystem asset protocol, and does not authorize HTTPS provider APIs. If the development server is later made same-port-only for every supported host configuration, the scheme-wide development exception should be replaced by the narrower same-origin source.

## Threat model and limits

CSP is defense in depth against content-injection impact, not a substitute for input validation, output encoding, least-privilege Tauri capabilities, Rust-side authorization, or safe handling of untrusted file metadata. A strict `script-src` reduces the privilege available to an injected script, while `object-src 'none'` and the default deny posture reduce secondary execution and embedding paths.

The production `connect-src` exception is intentionally specific to Tauri IPC. It must not be expanded to provider APIs merely because DiskSage supports cloud providers: those network operations already cross reviewed Rust commands and provider-specific authorization boundaries. Moving them into webview fetch authority would collapse a useful privilege separation.

## Verification contract

`src/lib/tauriContentSecurityPolicy.test.ts` parses the source-controlled Tauri configuration and fails when either reviewed policy is absent, when a reviewed directive drifts, when wildcard or HTTPS remote authority appears, when script execution gains inline/eval/WebAssembly-eval authority, or when WebSocket authority leaks from development into production. The same contract binds this doctoring decision and the changelog entry so security rationale and rollback evidence cannot silently disappear while configuration tests continue to pass.

This is a source-level governance test; Tauri's build and release workflows remain responsible for proving that the exact current head packages successfully with the configured policies. The contract is intentionally exact rather than a loose substring check. Any future expansion must be reviewed as a security-boundary change and cannot silently inherit approval from an older head.

## Rollback and migration

Rollback is a reviewed source change, not a runtime bypass:

1. reproduce the application breakage against the exact current head and identify the minimum missing directive;
2. add or update a regression test that demonstrates the required behavior without authorizing unrelated sources;
3. prefer a narrow source addition over setting `csp` or `devCsp` back to `null`;
4. keep development-only transport authority in `devCsp` rather than broadening production `csp`;
5. update this document and `CHANGELOG.md` with the changed authority and rationale;
6. rerun exact-head frontend tests, Tauri development/build smoke checks, packaging, security scans, and release-acceptance checks before merge.

Setting production `csp` to `null`, adding `*`, adding a remote script/style origin, adding script `'unsafe-inline'`/`'unsafe-eval'`, or adding `ws:` to the production `connect-src` is treated as a security regression unless an independently reviewed design demonstrates a concrete product requirement and a narrower control is unavailable.

## Standalone and MSA compatibility

The CSPs are local to the DiskSage desktop webview. They do not require Naruon, contextual-orchestrator, organization-central services, or a network connection for standalone production operation. CWL service integration continues through backend contracts rather than broadening the browser-like authority of the desktop frontend. The development WebSocket exception exists only to support the local Vite toolchain and is not an MSA integration mechanism.

## APA 7th references

Tauri Programme within The Commons Conservancy. (2025, April 7). *Content Security Policy (CSP)*. Tauri. https://v2.tauri.app/security/csp/

Tauri Programme within The Commons Conservancy. (n.d.). *Configuration*. Tauri. Retrieved August 7, 2026, from https://v2.tauri.app/reference/config/

Vite. (n.d.). *Server options*. Retrieved August 7, 2026, from https://vite.dev/config/server-options

World Wide Web Consortium. (2016, December 15). *Content Security Policy Level 2* (W3C Recommendation). https://www.w3.org/TR/CSP2/

World Wide Web Consortium. (2026, May 5). *Content Security Policy Level 3* (W3C Working Draft). https://www.w3.org/TR/2026/WD-CSP3-20260505/

## Reference verification note

The Tauri v2 CSP/configuration documentation, current Vite server documentation, and W3C CSP publications were rechecked on August 7, 2026. Tauri documents that CSP protection is enabled only when configured, that `csp` is reused during development when `devCsp` is not specified, and that bundled assets receive Tauri-managed nonce/hash additions. Vite documents WebSocket transport for HMR. CSP Level 3 explicitly places WebSocket under `connect-src`; its source matching algorithm also treats same-origin HTTP pages and same-host/same-port `ws` as compatible while retaining port-sensitive matching, which is why DiskSage's separate HMR port requires an explicit development transport allowance. CSP Level 2 remains a published Recommendation, while Level 3 remains a current Working Draft and is cited as work in progress rather than as a Recommendation.
