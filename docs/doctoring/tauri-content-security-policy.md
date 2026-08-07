# Tauri content security policy

## Decision

DiskSage enables explicit Content Security Policies for both production and development Tauri webviews. A null CSP is not an acceptable release posture because Tauri only enables its CSP protection when the policy is configured. The production policy therefore defaults to bundled application content, permits only the Tauri IPC transport needed by `@tauri-apps/api`, denies form submission targets, explicitly denies unused worker, media, and web-app-manifest fetch authority, and does not authorize arbitrary network origins, remote scripts, remote styles, wildcard sources, inline scripts, `eval`, or WebAssembly evaluation.

The authoritative configuration is `src-tauri/tauri.conf.json` under `app.security.csp` and `app.security.devCsp`.

## Production policy

The production policy has the following reviewed boundary:

- `default-src 'self'` makes bundled application content the default authority for fetch directives that inherit from it.
- `connect-src ipc: http://ipc.localhost` permits the documented Tauri IPC transport and no general remote network origin. OAuth and provider-network activity remains in the Rust backend rather than being granted to arbitrary webview JavaScript.
- `script-src 'self'` keeps executable script local. It deliberately omits `'unsafe-inline'`, `'unsafe-eval'`, `'wasm-unsafe-eval'`, remote origins, and wildcards. Tauri adds the hashes and nonces required by bundled assets at compile time.
- `style-src 'self' 'unsafe-inline'` is the only inline exception. It is retained because the current Svelte UI uses dynamic style attributes such as percentage-width bars. This exception grants style application, not script execution, and must be removed if those dynamic styles are replaced by a nonce/hash-compatible mechanism.
- `img-src 'self' data: blob:` supports bundled UI images plus in-memory image URLs without granting a remote host.
- `font-src 'self'` keeps fonts local.
- `worker-src 'none'`, `media-src 'none'`, and `manifest-src 'none'` explicitly remove Worker/SharedWorker/ServiceWorker script loading, audio/video/text-track loading, and web-app-manifest loading because DiskSage has no current product requirement for those browser resource classes. These are explicit rather than relying on `default-src 'self'`: the W3C fetch-directive fallback model would otherwise allow same-origin resources, and `worker-src` can additionally fall back through script-related directives. A future need must therefore be reviewed as an authority addition instead of silently inheriting bundled-content authority.
- `form-action 'none'` denies every HTML form submission target. This directive is explicit because `form-action` is a navigation directive and does not fall back to `default-src`; relying on `default-src 'self'` would therefore leave a form-submission authority gap even though DiskSage currently has no product form-submission requirement.
- `object-src 'none'`, `frame-src 'none'`, and `base-uri 'none'` remove plugin/object embedding, nested browsing contexts, and base-URL rewriting authority that DiskSage does not need.

The production policy intentionally does not enable Tauri's filesystem asset protocol. If a future feature needs `asset:` or `http://asset.localhost`, that change must separately enable and narrowly scope `app.security.assetProtocol`, add a failing regression test first, document the path boundary, and rerun exact-head security and packaging checks.

## Development policy

Tauri applies `csp` during development when `devCsp` is absent. DiskSage's Vite development server uses a fixed HTTP port and WebSocket-based hot-module replacement; when `TAURI_DEV_HOST` is supplied, Vite deliberately moves that HMR WebSocket to port 1421 while retaining a validated, non-wildcard bind address. A production `connect-src` that authorizes only Tauri IPC would therefore make the normal development feedback loop fail closed in the wrong place by blocking Vite's WebSocket rather than constraining it to development.

DiskSage consequently defines a separate `devCsp` with the same script, style, image, font, form-action, worker, media, manifest, object, frame, and base-URI restrictions as production, but with `connect-src 'self' ipc: http://ipc.localhost ws:`. The `ws:` scheme exception exists only in `devCsp`; it is absent from the production `csp`. This preserves Vite HMR for both the default same-origin development server and the repository's validated `TAURI_DEV_HOST` path without granting production webview JavaScript a general WebSocket channel. Development also retains `form-action 'none'` and the explicit unused-resource denials; HMR needs a WebSocket transport, not HTML form submission, worker, media, or manifest authority.

The `ws:` development exception is still bounded by the development lifecycle: it is not release authority, does not expand Tauri command permissions, does not enable the filesystem asset protocol, and does not authorize HTTPS provider APIs. If the development server is later made same-port-only for every supported host configuration, the scheme-wide development exception should be replaced by the narrower same-origin source.

## Threat model and limits

CSP is defense in depth against content-injection impact, not a substitute for input validation, output encoding, least-privilege Tauri capabilities, Rust-side authorization, or safe handling of untrusted file metadata. A strict `script-src` reduces the privilege available to an injected script, while `form-action 'none'`, the explicit worker/media/manifest denials, `object-src 'none'`, and the default deny posture reduce data-exfiltration, secondary execution, background execution, and embedding paths that the current product does not require.

The production `connect-src` exception is intentionally specific to Tauri IPC. It must not be expanded to provider APIs merely because DiskSage supports cloud providers: those network operations already cross reviewed Rust commands and provider-specific authorization boundaries. Moving them into webview fetch authority would collapse a useful privilege separation.

If a future product feature genuinely requires HTML form submission, its destination must be added narrowly to `form-action` only after a failing product-specific test demonstrates the need and an independent review confirms that a Rust-side command or ordinary application state transition is not the safer architecture. Enabling form submission must not be inferred from an unrelated `connect-src`, development-HMR, or cloud-provider requirement.

If a future feature genuinely requires a Worker, media resource, or web-app manifest, only the corresponding directive may be widened after a failing product-specific test demonstrates the requirement. That review must identify the exact source class and origin or scheme needed, preserve the unrelated `'none'` directives, and avoid treating a same-origin bundle as blanket authority for browser capabilities DiskSage does not use.

## Verification contract

`src/lib/tauriContentSecurityPolicy.test.ts` parses the source-controlled Tauri configuration and fails when either reviewed policy is absent, when a reviewed directive drifts, when form submissions or unused worker/media/manifest fetch classes regain authority, when wildcard or HTTPS remote authority appears, when script execution gains inline/eval/WebAssembly-eval authority, or when WebSocket authority leaks from development into production. The same contract binds this doctoring decision and the changelog entry so security rationale and rollback evidence cannot silently disappear while configuration tests continue to pass. It also pins the current W3C CSP Level 3 Working Draft citation so standards evidence cannot silently regress to an older draft.

This is a source-level governance test; Tauri's build and release workflows remain responsible for proving that the exact current head packages successfully with the configured policies. The contract is intentionally exact rather than a loose substring check. Any future expansion must be reviewed as a security-boundary change and cannot silently inherit approval from an older head.

## Rollback and migration

Rollback is a reviewed source change, not a runtime bypass:

1. reproduce the application breakage against the exact current head and identify the minimum missing directive;
2. add or update a regression test that demonstrates the required behavior without authorizing unrelated sources;
3. prefer a narrow source addition over setting `csp` or `devCsp` back to `null`;
4. keep development-only transport authority in `devCsp` rather than broadening production `csp`;
5. if HTML form submission becomes necessary, add only the minimum reviewed `form-action` destination rather than deleting the directive;
6. if a Worker, media resource, or web-app manifest becomes necessary, widen only the matching directive from `'none'` to the minimum reviewed source list and keep the other unused-resource directives denied;
7. update this document and `CHANGELOG.md` with the changed authority and rationale;
8. rerun exact-head frontend tests, Tauri development/build smoke checks, packaging, security scans, and release-acceptance checks before merge.

Setting production `csp` to `null`, adding `*`, adding a remote script/style origin, adding script `'unsafe-inline'`/`'unsafe-eval'`, deleting `form-action` without a reviewed replacement, deleting an unused-resource denial without a reviewed replacement, or adding `ws:` to the production `connect-src` is treated as a security regression unless an independently reviewed design demonstrates a concrete product requirement and a narrower control is unavailable.

## Standalone and MSA compatibility

The CSPs are local to the DiskSage desktop webview. They do not require Naruon, contextual-orchestrator, organization-central services, or a network connection for standalone production operation. CWL service integration continues through backend contracts rather than broadening the browser-like authority of the desktop frontend. The development WebSocket exception exists only to support the local Vite toolchain and is not an MSA integration mechanism. Denying HTML form submissions, workers, media, and web-app manifests likewise does not affect service interoperability because integrations are mediated through reviewed backend commands rather than browser form navigation or browser background/resource-loading features.

## APA 7th references

Tauri Programme within The Commons Conservancy. (2025, April 7). *Content Security Policy (CSP)*. Tauri. https://v2.tauri.app/security/csp/

Tauri Programme within The Commons Conservancy. (n.d.). *Configuration*. Tauri. Retrieved August 7, 2026, from https://v2.tauri.app/reference/config/

Vite. (n.d.). *Server options*. Retrieved August 7, 2026, from https://vite.dev/config/server-options

World Wide Web Consortium. (2016, December 15). *Content Security Policy Level 2* (W3C Recommendation). https://www.w3.org/TR/CSP2/

World Wide Web Consortium. (2026, July 29). *Content Security Policy Level 3* (W3C Working Draft). https://www.w3.org/TR/2026/WD-CSP3-20260729/

## Reference verification note

The Tauri v2 CSP/configuration documentation, current Vite server documentation, and W3C CSP publications were rechecked on August 7, 2026. Tauri documents that CSP protection is enabled only when configured, that `csp` is reused during development when `devCsp` is not specified, that CSP object configuration accepts directive/source mappings, and that bundled assets receive Tauri-managed nonce/hash additions. Vite documents WebSocket transport for HMR. The July 29, 2026 CSP Level 3 Working Draft defines `form-action` as a navigation directive that restricts form submission targets and places WebSocket under `connect-src`; CSP Level 2 explicitly states that `form-action` does not fall back to default sources. The current Level 3 fetch-directive fallback model makes `manifest-src` and `media-src` fall back to `default-src` when they are absent and makes `worker-src` fall back through child/script/default policy, so explicit `'none'` values remove same-origin authority that DiskSage does not need. Level 3 source matching defines `'none'` as matching no URL. CSP Level 2 remains a published Recommendation, while Level 3 remains a current Working Draft and is cited as work in progress rather than as a Recommendation.
