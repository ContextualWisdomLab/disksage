# Cloud Provider OAuth PKCE Security Design

## Scope

This slice replaces manually pasted OneDrive and Google Drive access tokens with native desktop
OAuth. It exists only to authorize the read-only provider metadata requests that bind an immutable
copy receipt to a provider-native object ID, size, revision, and checksum. It does not upload,
move, evict, trash, or delete a file.

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

## Remaining boundary

The provider-native object ID is still entered explicitly. Automating object-ID discovery from a
local sync root is a separate provider-mapping slice and must prove that the discovered object maps
to the exact receipt destination before it can remove this input. No source-removal command is
introduced by this design.

## Primary references

- [Microsoft identity platform authorization-code flow](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-auth-code-flow)
- [Microsoft redirect URI restrictions and native loopback behavior](https://learn.microsoft.com/en-us/entra/identity-platform/reply-url)
- [Microsoft Graph permission reference](https://learn.microsoft.com/en-us/graph/permissions-reference)
- [Google OAuth for Desktop apps](https://developers.google.com/identity/protocols/oauth2/native-app)
- [Google Drive API scope classification](https://developers.google.com/workspace/drive/api/guides/api-specific-auth)
