# Cloud Provider OAuth PKCE Security Design

## Scope

This slice replaces manually pasted OneDrive and Google Drive access tokens with native desktop
OAuth. Read-only connections authorize provider metadata requests that bind an immutable copy
receipt to a provider-native object ID, size, revision, and checksum. An explicitly separate
write-scope connection can also authorize the headless provider-API copy fallback; it never
authorizes source eviction or cloud deletion.

The flow is deterministic Rust code. It does not need an AI agent, an external LLM, or an
LLM-as-a-Judge, so `noema`, `contextual-orchestrator`, and `fast-mlsirm` are deliberately outside
this security boundary.

## Native authorization flow

1. The user selects an already-discovered local cloud root and supplies that provider's public
   Desktop OAuth Client ID. DiskSage rejects client IDs that do not match the provider's format.
2. Rust binds an ephemeral loopback port before opening the system browser. Google uses
   `http://127.0.0.1:<port>`; Microsoft uses `http://localhost:<port>` and listens on IPv4 plus IPv6
   loopback when available.
3. Each attempt generates a cryptographically random 64-byte PKCE verifier, its S256 challenge,
   and a separate random state value. Embedded webviews, custom URL schemes, and OOB copy/paste are
   not used.
4. The callback accepts one bounded HTTP GET on the exact root path. It requires an exact state
   match, ignores malformed or mismatched requests, times out after three minutes, and returns a
   static no-store HTML page without a code or token.
5. The authorization code is exchanged only at the provider's fixed HTTPS token endpoint with
   redirects disabled and a bounded response body. Native clients never send a client secret.
6. The refresh token is written to the operating-system credential store. Only a non-secret
   connection descriptor (provider, root identity/path, client ID, fixed scope, timestamp) is
   written to app data with an identity-derived connection ID.

## Permissions

| Provider | Delegated scope | Reason |
| --- | --- | --- |
| OneDrive | `Files.Read offline_access` | Read the signed-in user's existing drive item metadata and refresh access without write permission. |
| Google Drive | `https://www.googleapis.com/auth/drive.metadata.readonly` | Read metadata for an existing locally synced Drive file. `drive.file` cannot generally see pre-existing files unless the user selected/shared/created them through the app. |

The explicit provider-API copy fallback requests a separate write connection only when the
operator chooses `--provider-api-copy-fingerprint`:

| Provider | Write scope | Boundary |
| --- | --- | --- |
| OneDrive | `Files.ReadWrite offline_access` | Upload the exact reviewed candidate to the exact destination; no source eviction. |
| Google Drive | `https://www.googleapis.com/auth/drive` | Create destination folders/file and upload the exact reviewed candidate; no source eviction. |

Google classifies `drive.metadata.readonly` as a restricted scope. A Google OAuth consent-screen
configuration, test-user registration, and possibly app verification are therefore prerequisites.
DiskSage displays this before consent. It does not silently fall back to a broader read/write scope.

## Credential lifecycle

- Refresh tokens are stored under the DiskSage service name in macOS Keychain, Windows Credential
  Manager, or the Linux Secret Service through the Rust `keyring` backend.
- Access tokens are obtained just in time for one attestation, wrapped in zeroizing memory, and
  never accepted from or returned to the webview, settings, receipt, log, or command response.
- Provider token response bodies, authorization codes, PKCE verifiers, and retrieved refresh tokens
  are zeroized after use where the process controls their allocation.
- A rotated refresh token replaces the previous credential. A missing, revoked, malformed, or
  under-scoped token fails closed before provider evidence can be approved.
- Local disconnect removes both the credential-store entry and its non-secret descriptor, rolling
  the descriptor back if credential deletion fails. It does not claim to revoke Microsoft or Google
  server-side consent.

## Headless lifecycle CLI

`disksage-provider-oauth` exposes the same Rust PKCE and credential lifecycle outside the Tauri
webview so an operator can prepare a headless `disksage-cloud-plan` run without pasting a bearer or
refresh token. An explicit `--connections` value must be absolute and is the highest descriptor-path
authority. An explicit `--home` is next and derives the platform default from that supplied home;
this keeps hermetic operator/test roots independent of ambient environment data-home variables.
Without either explicit option, the shipped host entrypoint resolves the default descriptor path as
follows:

- Linux/non-macOS Unix: an absolute, non-empty `$XDG_DATA_HOME`, otherwise
  `$HOME/.local/share/com.contextualwisdomlab.disksage/cloud-oauth-connections.json`. Relative XDG
  values are invalid authority and are ignored.
- Windows: an absolute, non-empty `%APPDATA%` so redirected roaming AppData remains authoritative;
  otherwise `%USERPROFILE%\AppData\Roaming\com.contextualwisdomlab.disksage\cloud-oauth-connections.json`.
- macOS: `$HOME/Library/Application Support/com.contextualwisdomlab.disksage/cloud-oauth-connections.json`.

The entrypoint resolves these process/platform values and passes one explicit path into the OAuth
domain. The domain does not read process-global environment state. `--list` remains read-only: a
missing descriptor returns an empty list and does not create the app-data directory or document.

```bash
cargo run --locked --features cloud-cli --bin disksage-provider-oauth -- --list

cargo run --locked --features cloud-cli --bin disksage-provider-oauth -- \
  --connect --cloud-root /absolute/provider/root --client-id PUBLIC_DESKTOP_CLIENT_ID

cargo run --locked --features cloud-cli --bin disksage-provider-oauth -- \
  --verify-capacity --cloud-root /absolute/provider/root

cargo run --locked --features cloud-cli --bin disksage-provider-oauth -- \
  --disconnect --cloud-root /absolute/provider/root
```

Root selection is limited to a unique, currently readable root discovered by DiskSage. `--list`
does not read Keychain or create a descriptor document. `--connect` and `--disconnect` report their
local descriptor and credential-store effects explicitly. `--verify-capacity` may persist a rotated
refresh token, but its JSON contains only a connection ID and bounded provider-capacity evidence.
Every action declares that no cloud file write or source eviction occurred. `--manual-browser`
prints the public authorization URL and waits on the already-bound loopback callback for a session
where automatic browser launch is unavailable.

## Provider setup

### Microsoft

- Register a public Mobile/Desktop application that supports the intended account audience.
- Register `http://localhost` as the native loopback redirect URI. Microsoft ignores the runtime
  ephemeral port for localhost matching.
- Add delegated Microsoft Graph `Files.Read`; do not add a client secret to DiskSage.

### Google

- Enable the Google Drive API and configure the OAuth consent screen, including
  `drive.metadata.readonly` and required test users or verification.
- Create an OAuth Client ID of type **Desktop app**. Desktop loopback clients use the runtime
  `http://127.0.0.1:<port>` redirect and do not embed a client secret.

## Provider API copy fallback

The headless planner keeps the normal File Provider copy as the default. If that local admission
gate is unavailable, `--provider-api-copy-fingerprint` requires a write-scope connection, fresh
capacity, a fresh human-attributed copy approval, and a source pre-hash/re-hash pair. The upload
is performed by the Rust provider API transport, the immutable receipt records
`CopiedByProviderApi`, and DiskSage immediately attempts API attestation. A failed attestation
leaves the source retained and the dynamic ADR/Goal in `copy-verified` or `pending-provider-sync`;
it never upgrades the state from a missing proof.

## Remaining boundary

Standalone attestation still requires the provider-native object ID explicitly (Google Drive). The
provider-API copy fallback obtains that ID from its upload response and returns it for a later
attestation hand-off; object discovery from a local sync root remains a separate provider-mapping
slice that must prove the exact receipt destination. No source-removal command is introduced by
this design.

## Primary references

- [XDG Base Directory Specification 0.8](https://specifications.freedesktop.org/basedir/latest/)
- [Windows Folder Redirection with Group Policy](https://learn.microsoft.com/en-us/windows-server/storage/folder-redirection/folder-redirection-using-group-policy)
- [Microsoft identity platform authorization-code flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)
- [Microsoft redirect URI restrictions and native loopback behavior](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- [Microsoft Graph permission reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
- [Google OAuth for Desktop apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google Drive API scope classification](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
