//! Native OAuth 2.0 authorization for read-only cloud-provider metadata checks.
//!
//! DiskSage uses the system browser, PKCE S256, an ephemeral loopback listener, exact provider
//! hosts, and an OS credential store. Refresh tokens never enter settings or command responses;
//! access tokens live only long enough to perform one provider metadata request.

use crate::cloud::{CloudProvider, CloudRoot};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use zeroize::Zeroizing;

#[cfg(not(coverage))]
use std::io::{Read, Write};
#[cfg(not(coverage))]
use std::net::{TcpListener, TcpStream};
#[cfg(not(coverage))]
use std::time::{Duration, Instant};

const CONNECTION_DOCUMENT_VERSION: u32 = 1;
const MAX_CONNECTION_DOCUMENT_BYTES: u64 = 256 * 1024;
const MAX_CONNECTIONS: usize = 32;
const MAX_CLIENT_ID_BYTES: usize = 512;
const MAX_TOKEN_BYTES: usize = 64 * 1024;
const KEYRING_SERVICE: &str = "org.contextualwisdomlab.disksage.cloud-oauth";

#[cfg(not(coverage))]
const MAX_CALLBACK_REQUEST_BYTES: usize = 16 * 1024;
#[cfg(not(coverage))]
const MAX_TOKEN_RESPONSE_BYTES: u64 = 256 * 1024;
#[cfg(not(coverage))]
const CALLBACK_TIMEOUT: Duration = Duration::from_secs(180);

const ONEDRIVE_AUTH_ENDPOINT: &str =
    "https://login.microsoftonline.com/common/oauth2/v2.0/authorize";
#[cfg(not(coverage))]
const ONEDRIVE_TOKEN_ENDPOINT: &str = "https://login.microsoftonline.com/common/oauth2/v2.0/token";
const ONEDRIVE_SCOPE: &str = "Files.Read offline_access";

const GOOGLE_AUTH_ENDPOINT: &str = "https://accounts.google.com/o/oauth2/v2/auth";
#[cfg(not(coverage))]
const GOOGLE_TOKEN_ENDPOINT: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_SCOPE: &str = "https://www.googleapis.com/auth/drive.metadata.readonly";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OAuthConnection {
    pub connection_id: String,
    pub provider: CloudProvider,
    pub cloud_root_id: String,
    pub cloud_root_path: String,
    pub client_id: String,
    pub scope: String,
    pub connected_at_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct ConnectionDocument {
    version: u32,
    connections: Vec<OAuthConnection>,
}

impl Default for ConnectionDocument {
    fn default() -> Self {
        Self {
            version: CONNECTION_DOCUMENT_VERSION,
            connections: Vec::new(),
        }
    }
}

pub fn requested_scope(provider: CloudProvider) -> Result<&'static str, String> {
    match provider {
        CloudProvider::Onedrive => Ok(ONEDRIVE_SCOPE),
        CloudProvider::GoogleDrive => Ok(GOOGLE_SCOPE),
        CloudProvider::Icloud => Err("icloud-oauth-not-supported".into()),
    }
}

fn authorization_endpoint(provider: CloudProvider) -> Result<&'static str, String> {
    match provider {
        CloudProvider::Onedrive => Ok(ONEDRIVE_AUTH_ENDPOINT),
        CloudProvider::GoogleDrive => Ok(GOOGLE_AUTH_ENDPOINT),
        CloudProvider::Icloud => Err("icloud-oauth-not-supported".into()),
    }
}

#[cfg(not(coverage))]
fn token_endpoint(provider: CloudProvider) -> Result<&'static str, String> {
    match provider {
        CloudProvider::Onedrive => Ok(ONEDRIVE_TOKEN_ENDPOINT),
        CloudProvider::GoogleDrive => Ok(GOOGLE_TOKEN_ENDPOINT),
        CloudProvider::Icloud => Err("icloud-oauth-not-supported".into()),
    }
}

fn valid_microsoft_client_id(client_id: &str) -> bool {
    let lengths = [8, 4, 4, 4, 12];
    let parts: Vec<_> = client_id.split('-').collect();
    parts.len() == lengths.len()
        && parts.iter().zip(lengths).all(|(part, len)| {
            part.len() == len && part.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

fn valid_google_client_id(client_id: &str) -> bool {
    let Some(prefix) = client_id.strip_suffix(".apps.googleusercontent.com") else {
        return false;
    };
    !prefix.is_empty()
        && prefix
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

pub fn validate_client_id(provider: CloudProvider, client_id: &str) -> Result<(), String> {
    if client_id.is_empty()
        || client_id.len() > MAX_CLIENT_ID_BYTES
        || client_id.trim() != client_id
        || !client_id.is_ascii()
        || client_id.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("oauth-client-id-invalid".into());
    }
    let valid = match provider {
        CloudProvider::Onedrive => valid_microsoft_client_id(client_id),
        CloudProvider::GoogleDrive => valid_google_client_id(client_id),
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    };
    valid
        .then_some(())
        .ok_or_else(|| "oauth-client-id-provider-format-invalid".to_string())
}

fn percent_encode(value: &str) -> String {
    use std::fmt::Write as _;
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            write!(&mut encoded, "%{byte:02X}").expect("writing to String cannot fail");
        }
    }
    encoded
}

fn query_url(endpoint: &str, params: &[(&str, &str)]) -> String {
    let mut url = endpoint.to_owned();
    for (index, (key, value)) in params.iter().enumerate() {
        url.push(if index == 0 { '?' } else { '&' });
        url.push_str(&percent_encode(key));
        url.push('=');
        url.push_str(&percent_encode(value));
    }
    url
}

fn random_urlsafe(bytes: usize) -> Result<String, String> {
    let mut random = vec![0_u8; bytes];
    getrandom::fill(&mut random).map_err(|_| "secure-random-unavailable".to_string())?;
    Ok(URL_SAFE_NO_PAD.encode(random))
}

struct PkceMaterial {
    verifier: Zeroizing<String>,
    challenge: String,
    state: String,
}

fn generate_pkce() -> Result<PkceMaterial, String> {
    let verifier = Zeroizing::new(random_urlsafe(64)?);
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = random_urlsafe(32)?;
    Ok(PkceMaterial {
        verifier,
        challenge,
        state,
    })
}

fn build_authorization_url(
    provider: CloudProvider,
    client_id: &str,
    redirect_uri: &str,
    challenge: &str,
    state: &str,
) -> Result<String, String> {
    validate_client_id(provider, client_id)?;
    let redirect_prefix = match provider {
        CloudProvider::Onedrive => "http://localhost:",
        CloudProvider::GoogleDrive => "http://127.0.0.1:",
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    };
    let Some(port) = redirect_uri.strip_prefix(redirect_prefix) else {
        return Err("oauth-redirect-uri-invalid".into());
    };
    if port.is_empty()
        || !port.bytes().all(|byte| byte.is_ascii_digit())
        || port.parse::<u16>().ok().is_none_or(|port| port == 0)
    {
        return Err("oauth-redirect-uri-invalid".into());
    }
    if challenge.len() != 43 || state.len() != 43 {
        return Err("oauth-pkce-material-invalid".into());
    }
    let endpoint = authorization_endpoint(provider)?;
    let scope = requested_scope(provider)?;
    let mut params = vec![
        ("client_id", client_id),
        ("redirect_uri", redirect_uri),
        ("response_type", "code"),
        ("scope", scope),
        ("state", state),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
    ];
    match provider {
        CloudProvider::Onedrive => {
            params.push(("response_mode", "query"));
            params.push(("prompt", "select_account"));
        }
        CloudProvider::GoogleDrive => {
            params.push(("access_type", "offline"));
            params.push(("prompt", "consent"));
            params.push(("include_granted_scopes", "true"));
        }
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    }
    Ok(query_url(endpoint, &params))
}

fn connection_id(root: &CloudRoot) -> String {
    let mut hasher = Sha256::new();
    for value in [root.provider.as_str(), root.id.as_str(), root.path.as_str()] {
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    use std::fmt::Write as _;
    let digest = hasher.finalize();
    let mut encoded = String::with_capacity(digest.len() * 2);
    for byte in digest {
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn validate_connection(connection: &OAuthConnection) -> Result<(), String> {
    if connection.connection_id.len() != 64
        || !connection
            .connection_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
        || connection.cloud_root_id.trim().is_empty()
        || connection.cloud_root_path.trim().is_empty()
        || !Path::new(&connection.cloud_root_path).is_absolute()
        || connection.scope != requested_scope(connection.provider)?
    {
        return Err("oauth-connection-invalid".into());
    }
    validate_client_id(connection.provider, &connection.client_id)?;
    let root = CloudRoot {
        id: connection.cloud_root_id.clone(),
        provider: connection.provider,
        account_scope: crate::cloud::CloudAccountScope::Unknown,
        label: String::new(),
        path: connection.cloud_root_path.clone(),
    };
    if connection.connection_id != connection_id(&root) {
        return Err("oauth-connection-id-mismatch".into());
    }
    Ok(())
}

pub fn connections_path(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("cloud-oauth-connections.json")
}

pub fn load_connections(path: &Path) -> Result<Vec<OAuthConnection>, String> {
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(_) => return Err("oauth-connection-document-unavailable".into()),
    };
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("oauth-connection-document-not-regular-file".into());
    }
    if metadata.len() > MAX_CONNECTION_DOCUMENT_BYTES {
        return Err("oauth-connection-document-too-large".into());
    }
    let bytes = std::fs::read(path).map_err(|_| "oauth-connection-document-unreadable")?;
    let document: ConnectionDocument =
        serde_json::from_slice(&bytes).map_err(|_| "oauth-connection-document-invalid")?;
    if document.version != CONNECTION_DOCUMENT_VERSION
        || document.connections.len() > MAX_CONNECTIONS
    {
        return Err("oauth-connection-document-version-or-count-invalid".into());
    }
    for connection in &document.connections {
        validate_connection(connection)?;
    }
    Ok(document.connections)
}

fn save_connections(path: &Path, connections: &[OAuthConnection]) -> Result<(), String> {
    if connections.len() > MAX_CONNECTIONS {
        return Err("oauth-connection-count-invalid".into());
    }
    for connection in connections {
        validate_connection(connection)?;
    }
    let parent = path
        .parent()
        .ok_or_else(|| "oauth-connection-directory-invalid".to_string())?;
    std::fs::create_dir_all(parent).map_err(|_| "oauth-connection-directory-unavailable")?;
    if let Ok(metadata) = std::fs::symlink_metadata(path) {
        if metadata.file_type().is_symlink() || !metadata.is_file() {
            return Err("oauth-connection-document-not-regular-file".into());
        }
    }
    let document = ConnectionDocument {
        version: CONNECTION_DOCUMENT_VERSION,
        connections: connections.to_vec(),
    };
    let encoded = serde_json::to_vec_pretty(&document)
        .map_err(|_| "oauth-connection-document-encode-failed")?;
    let temporary = parent.join(format!(
        ".cloud-oauth-connections.{}.tmp",
        random_urlsafe(12)?
    ));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&temporary)
        .map_err(|_| "oauth-connection-document-create-failed")?;
    use std::io::Write as _;
    if file.write_all(&encoded).is_err() || file.sync_all().is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("oauth-connection-document-write-failed".into());
    }
    #[cfg(windows)]
    if path.exists() {
        std::fs::remove_file(path).map_err(|_| "oauth-connection-document-replace-failed")?;
    }
    if std::fs::rename(&temporary, path).is_err() {
        let _ = std::fs::remove_file(&temporary);
        return Err("oauth-connection-document-replace-failed".into());
    }
    Ok(())
}

pub fn connection_for_root(
    connections: &[OAuthConnection],
    root: &CloudRoot,
) -> Result<OAuthConnection, String> {
    let expected_id = connection_id(root);
    let matches: Vec<_> = connections
        .iter()
        .filter(|connection| {
            connection.connection_id == expected_id
                && connection.provider == root.provider
                && connection.cloud_root_id == root.id
                && connection.cloud_root_path == root.path
        })
        .cloned()
        .collect();
    match matches.as_slice() {
        [only] => Ok(only.clone()),
        [] => Err("provider-oauth-connection-missing".into()),
        _ => Err("provider-oauth-connection-ambiguous".into()),
    }
}

#[cfg(not(coverage))]
pub struct PendingOAuth {
    provider: CloudProvider,
    client_id: String,
    redirect_uri: String,
    state: String,
    verifier: Zeroizing<String>,
    listeners: Vec<TcpListener>,
    authorization_url: String,
}

#[cfg(not(coverage))]
impl PendingOAuth {
    pub fn authorization_url(&self) -> &str {
        &self.authorization_url
    }
}

#[cfg(not(coverage))]
pub fn prepare_authorization(
    provider: CloudProvider,
    client_id: &str,
) -> Result<PendingOAuth, String> {
    validate_client_id(provider, client_id)?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|_| "oauth-loopback-bind-failed".to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|_| "oauth-loopback-config-failed".to_string())?;
    let port = listener
        .local_addr()
        .map_err(|_| "oauth-loopback-address-unavailable".to_string())?
        .port();
    // Google Desktop clients require the loopback IP root form. Microsoft ignores the ephemeral
    // port when matching a registered localhost native redirect URI.
    let redirect_uri = match provider {
        CloudProvider::Onedrive => format!("http://localhost:{port}"),
        CloudProvider::GoogleDrive => format!("http://127.0.0.1:{port}"),
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    };
    let mut listeners = vec![listener];
    if provider == CloudProvider::Onedrive {
        if let Ok(ipv6) = TcpListener::bind(("::1", port)) {
            ipv6.set_nonblocking(true)
                .map_err(|_| "oauth-loopback-config-failed".to_string())?;
            listeners.push(ipv6);
        }
    }
    let pkce = generate_pkce()?;
    let authorization_url = build_authorization_url(
        provider,
        client_id,
        &redirect_uri,
        &pkce.challenge,
        &pkce.state,
    )?;
    Ok(PendingOAuth {
        provider,
        client_id: client_id.to_owned(),
        redirect_uri,
        state: pkce.state,
        verifier: pkce.verifier,
        listeners,
        authorization_url,
    })
}

fn decode_hex_nibble(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, String> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 2 < bytes.len() => {
                let high = decode_hex_nibble(bytes[index + 1])
                    .ok_or_else(|| "oauth-callback-query-invalid".to_string())?;
                let low = decode_hex_nibble(bytes[index + 2])
                    .ok_or_else(|| "oauth-callback-query-invalid".to_string())?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            b'%' => return Err("oauth-callback-query-invalid".into()),
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| "oauth-callback-query-invalid".into())
}

fn constant_time_eq(left: &str, right: &str) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.bytes()
        .zip(right.bytes())
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn callback_code(target: &str, expected_state: &str) -> Result<String, String> {
    let Some(query) = target.strip_prefix("/?") else {
        return Err("oauth-callback-path-invalid".into());
    };
    let mut code: Option<String> = None;
    let mut state: Option<String> = None;
    let mut denied = false;
    for item in query.split('&') {
        let (raw_key, raw_value) = item
            .split_once('=')
            .ok_or_else(|| "oauth-callback-query-invalid".to_string())?;
        let key = percent_decode(raw_key)?;
        let value = percent_decode(raw_value)?;
        match key.as_str() {
            "code" => {
                if code.is_some() {
                    return Err("oauth-callback-query-duplicate".into());
                }
                code = Some(value);
            }
            "state" => {
                if state.is_some() {
                    return Err("oauth-callback-query-duplicate".into());
                }
                state = Some(value);
            }
            "error" => denied = true,
            _ => {}
        }
    }
    let state = state.ok_or_else(|| "oauth-callback-state-missing".to_string())?;
    if !constant_time_eq(&state, expected_state) {
        return Err("oauth-callback-state-mismatch".into());
    }
    if denied {
        return Err("oauth-authorization-denied".into());
    }
    let code = code.ok_or_else(|| "oauth-callback-code-missing".to_string())?;
    if code.is_empty()
        || code.len() > MAX_TOKEN_BYTES
        || code.bytes().any(|byte| byte.is_ascii_control())
    {
        return Err("oauth-callback-code-invalid".into());
    }
    Ok(code)
}

#[cfg(not(coverage))]
fn read_callback_target(stream: &mut TcpStream) -> Result<String, String> {
    stream
        .set_read_timeout(Some(Duration::from_secs(2)))
        .map_err(|_| "oauth-callback-read-config-failed".to_string())?;
    let mut request = Vec::new();
    let mut buffer = [0_u8; 1024];
    while request.len() < MAX_CALLBACK_REQUEST_BYTES {
        let count = stream
            .read(&mut buffer)
            .map_err(|_| "oauth-callback-read-failed".to_string())?;
        if count == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..count]);
        if request.windows(4).any(|window| window == b"\r\n\r\n") {
            break;
        }
    }
    if request.len() >= MAX_CALLBACK_REQUEST_BYTES {
        return Err("oauth-callback-request-too-large".into());
    }
    let request = std::str::from_utf8(&request).map_err(|_| "oauth-callback-request-invalid")?;
    let first_line = request
        .split("\r\n")
        .next()
        .ok_or_else(|| "oauth-callback-request-invalid".to_string())?;
    let mut fields = first_line.split_whitespace();
    if fields.next() != Some("GET") {
        return Err("oauth-callback-method-invalid".into());
    }
    let target = fields
        .next()
        .ok_or_else(|| "oauth-callback-request-invalid".to_string())?;
    if fields.next() != Some("HTTP/1.1") || fields.next().is_some() {
        return Err("oauth-callback-request-invalid".into());
    }
    Ok(target.to_owned())
}

#[cfg(not(coverage))]
fn send_callback_response(stream: &mut TcpStream, accepted: bool) {
    let (status, title, body) = if accepted {
        (
            "200 OK",
            "DiskSage authorization complete",
            "You may return to DiskSage.",
        )
    } else {
        (
            "400 Bad Request",
            "DiskSage authorization rejected",
            "Return to DiskSage and try again.",
        )
    };
    let html = format!(
        "<!doctype html><meta charset=\"utf-8\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'\"><title>{title}</title><body><h1>{title}</h1><p>{body}</p></body>"
    );
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nCache-Control: no-store\r\nPragma: no-cache\r\nConnection: close\r\n\r\n{html}",
        html.len()
    );
    let _ = stream.write_all(response.as_bytes());
    let _ = stream.flush();
}

#[cfg(not(coverage))]
fn wait_for_callback(listeners: &[TcpListener], expected_state: &str) -> Result<String, String> {
    let deadline = Instant::now() + CALLBACK_TIMEOUT;
    while Instant::now() < deadline {
        for listener in listeners {
            match listener.accept() {
                Ok((mut stream, peer)) => {
                    if !peer.ip().is_loopback() {
                        send_callback_response(&mut stream, false);
                        continue;
                    }
                    let result = read_callback_target(&mut stream)
                        .and_then(|target| callback_code(&target, expected_state));
                    match result {
                        Ok(code) => {
                            send_callback_response(&mut stream, true);
                            return Ok(code);
                        }
                        Err(error) if error == "oauth-authorization-denied" => {
                            send_callback_response(&mut stream, false);
                            return Err(error);
                        }
                        Err(_) => {
                            send_callback_response(&mut stream, false);
                        }
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(_) => return Err("oauth-loopback-accept-failed".into()),
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    Err("oauth-callback-timeout".into())
}

#[derive(Deserialize)]
struct TokenDocument {
    access_token: Option<String>,
    refresh_token: Option<String>,
    token_type: Option<String>,
    expires_in: Option<u64>,
    scope: Option<String>,
    error: Option<String>,
}

struct OAuthGrant {
    access_token: Zeroizing<String>,
    refresh_token: Option<Zeroizing<String>>,
}

fn validate_token_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_TOKEN_BYTES
        && !value.bytes().any(|byte| byte.is_ascii_control())
}

fn parse_token_document(
    provider: CloudProvider,
    json: &str,
    refresh_required: bool,
) -> Result<OAuthGrant, String> {
    let document: TokenDocument =
        serde_json::from_str(json).map_err(|_| "oauth-token-response-invalid")?;
    if document.error.is_some() {
        return Err("oauth-token-endpoint-rejected".into());
    }
    let access_token = document
        .access_token
        .filter(|token| validate_token_value(token))
        .ok_or_else(|| "oauth-access-token-invalid".to_string())?;
    if !document
        .token_type
        .as_deref()
        .is_some_and(|value| value.eq_ignore_ascii_case("Bearer"))
    {
        return Err("oauth-token-type-invalid".into());
    }
    if document
        .expires_in
        .is_some_and(|seconds| seconds == 0 || seconds > 86_400)
    {
        return Err("oauth-token-expiry-invalid".into());
    }
    if let Some(scope) = &document.scope {
        let required_resource_scope = requested_scope(provider)?
            .split_whitespace()
            .next()
            .expect("provider scope is non-empty");
        if !scope
            .split_whitespace()
            .any(|granted| granted.eq_ignore_ascii_case(required_resource_scope))
        {
            return Err("oauth-required-scope-missing".into());
        }
    }
    let refresh_token = document
        .refresh_token
        .map(|token| {
            validate_token_value(&token)
                .then(|| Zeroizing::new(token))
                .ok_or_else(|| "oauth-refresh-token-invalid".to_string())
        })
        .transpose()?;
    if refresh_required && refresh_token.is_none() {
        return Err("oauth-refresh-token-missing".into());
    }
    Ok(OAuthGrant {
        access_token: Zeroizing::new(access_token),
        refresh_token,
    })
}

#[cfg(not(coverage))]
fn oauth_agent() -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .max_redirects(0)
        .timeout_global(Some(Duration::from_secs(20)))
        .build();
    ureq::Agent::new_with_config(config)
}

#[cfg(not(coverage))]
fn safe_oauth_transport_error(error: ureq::Error) -> String {
    match error {
        ureq::Error::StatusCode(code) => format!("oauth-token-http-status:{code}"),
        ureq::Error::Timeout(_) => "oauth-token-timeout".into(),
        ureq::Error::HostNotFound => "oauth-token-host-not-found".into(),
        ureq::Error::BodyExceedsLimit(_) => "oauth-token-response-too-large".into(),
        _ => "oauth-token-request-failed".into(),
    }
}

#[cfg(not(coverage))]
fn read_token_response(
    provider: CloudProvider,
    response: ureq::http::Response<ureq::Body>,
    refresh_required: bool,
) -> Result<OAuthGrant, String> {
    let mut response = response;
    let status = response.status().as_u16();
    if !(200..300).contains(&status) {
        return Err(format!("oauth-token-http-status:{status}"));
    }
    let body = Zeroizing::new(
        response
            .body_mut()
            .with_config()
            .limit(MAX_TOKEN_RESPONSE_BYTES)
            .read_to_string()
            .map_err(safe_oauth_transport_error)?,
    );
    parse_token_document(provider, body.as_str(), refresh_required)
}

#[cfg(not(coverage))]
fn exchange_authorization_code(pending: &PendingOAuth, code: &str) -> Result<OAuthGrant, String> {
    let endpoint = token_endpoint(pending.provider)?;
    let agent = oauth_agent();
    let response = match pending.provider {
        CloudProvider::Onedrive => agent.post(endpoint).send_form([
            ("client_id", pending.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("code_verifier", pending.verifier.as_str()),
            ("scope", requested_scope(pending.provider)?),
        ]),
        CloudProvider::GoogleDrive => agent.post(endpoint).send_form([
            ("client_id", pending.client_id.as_str()),
            ("grant_type", "authorization_code"),
            ("code", code),
            ("redirect_uri", pending.redirect_uri.as_str()),
            ("code_verifier", pending.verifier.as_str()),
        ]),
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    }
    .map_err(safe_oauth_transport_error)?;
    read_token_response(pending.provider, response, true)
}

#[cfg(not(coverage))]
fn refresh_grant(connection: &OAuthConnection, refresh_token: &str) -> Result<OAuthGrant, String> {
    let endpoint = token_endpoint(connection.provider)?;
    let agent = oauth_agent();
    let response = match connection.provider {
        CloudProvider::Onedrive => agent.post(endpoint).send_form([
            ("client_id", connection.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
            ("scope", connection.scope.as_str()),
        ]),
        CloudProvider::GoogleDrive => agent.post(endpoint).send_form([
            ("client_id", connection.client_id.as_str()),
            ("grant_type", "refresh_token"),
            ("refresh_token", refresh_token),
        ]),
        CloudProvider::Icloud => return Err("icloud-oauth-not-supported".into()),
    }
    .map_err(safe_oauth_transport_error)?;
    read_token_response(connection.provider, response, false)
}

#[cfg(not(coverage))]
fn keyring_entry(connection_id: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(KEYRING_SERVICE, connection_id)
        .map_err(|_| "provider-oauth-keyring-unavailable".to_string())
}

#[cfg(not(coverage))]
fn store_refresh_token(connection_id: &str, token: &str) -> Result<(), String> {
    keyring_entry(connection_id)?
        .set_password(token)
        .map_err(|_| "provider-oauth-keyring-write-failed".to_string())
}

#[cfg(not(coverage))]
fn read_refresh_token(connection_id: &str) -> Result<Zeroizing<String>, String> {
    let token = keyring_entry(connection_id)?
        .get_password()
        .map_err(|_| "provider-oauth-refresh-token-unavailable".to_string())?;
    if !validate_token_value(&token) {
        return Err("provider-oauth-refresh-token-invalid".into());
    }
    Ok(Zeroizing::new(token))
}

#[cfg(not(coverage))]
fn delete_refresh_token(connection_id: &str) -> Result<(), String> {
    match keyring_entry(connection_id)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(_) => Err("provider-oauth-keyring-delete-failed".into()),
    }
}

#[cfg(not(coverage))]
pub fn finish_authorization(
    pending: PendingOAuth,
    root: &CloudRoot,
    connection_document_path: &Path,
    connected_at_ms: u64,
) -> Result<OAuthConnection, String> {
    if pending.provider != root.provider {
        return Err("provider-oauth-root-mismatch".into());
    }
    let code = Zeroizing::new(wait_for_callback(&pending.listeners, &pending.state)?);
    let grant = exchange_authorization_code(&pending, code.as_str())?;
    let refresh_token = grant
        .refresh_token
        .as_deref()
        .ok_or_else(|| "oauth-refresh-token-missing".to_string())?;
    let connection = OAuthConnection {
        connection_id: connection_id(root),
        provider: root.provider,
        cloud_root_id: root.id.clone(),
        cloud_root_path: root.path.clone(),
        client_id: pending.client_id.clone(),
        scope: requested_scope(root.provider)?.into(),
        connected_at_ms,
    };
    validate_connection(&connection)?;
    let original = load_connections(connection_document_path)?;
    let mut updated = original.clone();
    updated.retain(|entry| entry.connection_id != connection.connection_id);
    updated.push(connection.clone());
    updated.sort_by(|left, right| left.connection_id.cmp(&right.connection_id));
    save_connections(connection_document_path, &updated)?;
    if let Err(error) = store_refresh_token(&connection.connection_id, refresh_token) {
        if save_connections(connection_document_path, &original).is_err() {
            return Err("provider-oauth-keyring-write-and-config-rollback-failed".into());
        }
        return Err(error);
    }
    Ok(connection)
}

#[cfg(not(coverage))]
pub fn refreshed_access_token(
    connection_document_path: &Path,
    root: &CloudRoot,
) -> Result<Zeroizing<String>, String> {
    let connections = load_connections(connection_document_path)?;
    let connection = connection_for_root(&connections, root)?;
    let refresh_token = read_refresh_token(&connection.connection_id)?;
    let grant = refresh_grant(&connection, &refresh_token)?;
    if let Some(rotated) = grant.refresh_token.as_deref() {
        store_refresh_token(&connection.connection_id, rotated)?;
    }
    Ok(grant.access_token)
}

#[cfg(not(coverage))]
pub fn disconnect(connection_document_path: &Path, root: &CloudRoot) -> Result<(), String> {
    let original = load_connections(connection_document_path)?;
    let connection = connection_for_root(&original, root)?;
    let updated: Vec<_> = original
        .iter()
        .filter(|entry| entry.connection_id != connection.connection_id)
        .cloned()
        .collect();
    save_connections(connection_document_path, &updated)?;
    if let Err(error) = delete_refresh_token(&connection.connection_id) {
        if save_connections(connection_document_path, &original).is_err() {
            return Err("provider-oauth-keyring-delete-and-config-rollback-failed".into());
        }
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const MICROSOFT_CLIENT_ID: &str = "12345678-1234-4abc-8def-1234567890ab";
    const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

    fn root(provider: CloudProvider) -> CloudRoot {
        #[cfg(windows)]
        let path = r"C:\Cloud";
        #[cfg(not(windows))]
        let path = "/Cloud";
        CloudRoot {
            id: format!("{}:account", provider.as_str()),
            provider,
            account_scope: crate::cloud::CloudAccountScope::Unknown,
            label: "Cloud".into(),
            path: path.into(),
        }
    }

    fn connection(provider: CloudProvider) -> OAuthConnection {
        let root = root(provider);
        OAuthConnection {
            connection_id: connection_id(&root),
            provider,
            cloud_root_id: root.id,
            cloud_root_path: root.path,
            client_id: match provider {
                CloudProvider::Onedrive => MICROSOFT_CLIENT_ID,
                CloudProvider::GoogleDrive => GOOGLE_CLIENT_ID,
                CloudProvider::Icloud => "unsupported",
            }
            .into(),
            scope: requested_scope(provider).unwrap_or_default().into(),
            connected_at_ms: 123,
        }
    }

    #[test]
    fn provider_client_ids_and_scopes_are_fail_closed() {
        assert!(validate_client_id(CloudProvider::Onedrive, MICROSOFT_CLIENT_ID).is_ok());
        assert!(validate_client_id(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).is_ok());
        assert_eq!(
            requested_scope(CloudProvider::Onedrive).unwrap(),
            "Files.Read offline_access"
        );
        assert_eq!(
            requested_scope(CloudProvider::GoogleDrive).unwrap(),
            "https://www.googleapis.com/auth/drive.metadata.readonly"
        );
        for invalid in ["", " not-a-client", "not-a-client\n"] {
            assert!(validate_client_id(CloudProvider::Onedrive, invalid).is_err());
            assert!(validate_client_id(CloudProvider::GoogleDrive, invalid).is_err());
        }
        assert!(validate_client_id(CloudProvider::Icloud, MICROSOFT_CLIENT_ID).is_err());
    }

    #[test]
    fn generated_pkce_is_high_entropy_url_safe_s256() {
        let first = generate_pkce().unwrap();
        let second = generate_pkce().unwrap();
        assert_eq!(first.verifier.len(), 86);
        assert_eq!(first.challenge.len(), 43);
        assert_eq!(first.state.len(), 43);
        assert_ne!(first.verifier.as_str(), second.verifier.as_str());
        assert_ne!(first.state, second.state);
        assert_eq!(
            first.challenge,
            URL_SAFE_NO_PAD.encode(Sha256::digest(first.verifier.as_bytes()))
        );
        for value in [first.verifier.as_str(), &first.challenge, &first.state] {
            assert!(value
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')));
        }
    }

    #[test]
    fn authorization_urls_use_fixed_hosts_pkce_and_read_only_scopes() {
        let challenge = "A".repeat(43);
        let state = "B".repeat(43);
        let microsoft = build_authorization_url(
            CloudProvider::Onedrive,
            MICROSOFT_CLIENT_ID,
            "http://localhost:49152",
            &challenge,
            &state,
        )
        .unwrap();
        assert!(microsoft.starts_with(ONEDRIVE_AUTH_ENDPOINT));
        assert!(microsoft.contains("scope=Files.Read%20offline_access"));
        assert!(microsoft.contains("code_challenge_method=S256"));
        assert!(!microsoft.contains("ReadWrite"));

        let google = build_authorization_url(
            CloudProvider::GoogleDrive,
            GOOGLE_CLIENT_ID,
            "http://127.0.0.1:49153",
            &challenge,
            &state,
        )
        .unwrap();
        assert!(google.starts_with(GOOGLE_AUTH_ENDPOINT));
        assert!(google.contains("drive.metadata.readonly"));
        assert!(google.contains("access_type=offline"));
        assert!(google.contains("prompt=consent"));
        assert!(!google.contains("drive.file"));
        assert!(build_authorization_url(
            CloudProvider::Onedrive,
            MICROSOFT_CLIENT_ID,
            "https://attacker.invalid/callback",
            &challenge,
            &state,
        )
        .is_err());
        assert!(build_authorization_url(
            CloudProvider::GoogleDrive,
            GOOGLE_CLIENT_ID,
            "http://localhost:49153",
            &challenge,
            &state,
        )
        .is_err());
    }

    #[test]
    fn callback_parser_requires_exact_path_state_and_one_code() {
        let state = "s".repeat(43);
        assert_eq!(
            callback_code(&format!("/?code=abc%2D123&state={state}"), &state).unwrap(),
            "abc-123"
        );
        for invalid in [
            format!("/other?code=abc&state={state}"),
            "/?code=abc&state=wrong".into(),
            format!("/?code=one&code=two&state={state}"),
            format!("/?error=denied&state={state}"),
            format!("/?state={state}"),
            format!("/?code=%GG&state={state}"),
        ] {
            assert!(callback_code(&invalid, &state).is_err(), "{invalid}");
        }
    }

    #[test]
    fn connection_document_round_trips_and_rejects_tampering() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("connections.json");
        let connections = vec![
            connection(CloudProvider::Onedrive),
            connection(CloudProvider::GoogleDrive),
        ];
        save_connections(&path, &connections).unwrap();
        let loaded = load_connections(&path).unwrap();
        assert_eq!(loaded, connections);
        assert_eq!(
            connection_for_root(&loaded, &root(CloudProvider::GoogleDrive)).unwrap(),
            connections[1]
        );

        let mut tampered = connections[0].clone();
        tampered.connection_id = "0".repeat(64);
        assert!(save_connections(&path, &[tampered]).is_err());
        std::fs::write(&path, b"not-json").unwrap();
        assert!(load_connections(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn connection_document_rejects_symlinks() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("target.json");
        std::fs::write(&target, b"{}").unwrap();
        let link = temp.path().join("connections.json");
        symlink(&target, &link).unwrap();
        assert!(load_connections(&link).is_err());
        assert!(save_connections(&link, &[]).is_err());
    }

    #[test]
    fn token_values_and_percent_codec_are_bounded() {
        assert!(validate_token_value("token-value"));
        assert!(!validate_token_value(""));
        assert!(!validate_token_value("token\nvalue"));
        assert_eq!(percent_decode("a%2Fb+c").unwrap(), "a/b c");
        assert!(percent_decode("%0").is_err());
        assert!(percent_decode("%GG").is_err());
        assert_eq!(
            percent_encode("Files.Read offline_access"),
            "Files.Read%20offline_access"
        );
    }

    #[test]
    fn token_documents_require_bearer_resource_scope_and_refresh_on_consent() {
        let microsoft = parse_token_document(
            CloudProvider::Onedrive,
            r#"{"access_token":"access","refresh_token":"refresh","token_type":"Bearer","expires_in":3599,"scope":"Files.Read offline_access"}"#,
            true,
        )
        .unwrap();
        assert_eq!(microsoft.access_token.as_str(), "access");
        assert_eq!(microsoft.refresh_token.unwrap().as_str(), "refresh");

        let google = parse_token_document(
            CloudProvider::GoogleDrive,
            r#"{"access_token":"access","token_type":"bearer","expires_in":3600,"scope":"https://www.googleapis.com/auth/drive.metadata.readonly"}"#,
            false,
        )
        .unwrap();
        assert!(google.refresh_token.is_none());

        for invalid in [
            r#"{"access_token":"access","token_type":"MAC","refresh_token":"refresh"}"#,
            r#"{"access_token":"access","token_type":"Bearer","refresh_token":"refresh","scope":"https://www.googleapis.com/auth/drive.file"}"#,
            r#"{"error":"invalid_grant"}"#,
            r#"{"access_token":"access","token_type":"Bearer","expires_in":0}"#,
        ] {
            assert!(parse_token_document(CloudProvider::GoogleDrive, invalid, true).is_err());
        }
    }
}
