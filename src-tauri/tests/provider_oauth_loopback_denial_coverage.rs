//! Black-box loopback callback coverage for provider OAuth denial handling.
//!
//! This regression drives the real ephemeral loopback listener created by `prepare_authorization`
//! and proves that an OAuth denial terminates locally before token exchange, keyring access, or
//! durable connection publication.

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{finish_authorization, prepare_authorization};
use std::io::{Read, Write};
use std::net::TcpStream;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn percent_decode(value: &str) -> String {
    fn nibble(byte: u8) -> u8 {
        match byte {
            b'0'..=b'9' => byte - b'0',
            b'a'..=b'f' => byte - b'a' + 10,
            b'A'..=b'F' => byte - b'A' + 10,
            _ => panic!("authorization URL must contain valid percent encoding"),
        }
    }

    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            assert!(index + 2 < bytes.len(), "percent escape must be complete");
            decoded.push((nibble(bytes[index + 1]) << 4) | nibble(bytes[index + 2]));
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).expect("authorization URL values must decode as UTF-8")
}

fn query_value(url: &str, key: &str) -> String {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .expect("authorization URL must contain a query");
    let encoded = query
        .split('&')
        .filter_map(|item| item.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
        .unwrap_or_else(|| panic!("authorization URL must contain {key}"));
    percent_decode(encoded)
}

fn google_root() -> CloudRoot {
    #[cfg(windows)]
    let path = r"C:\Cloud\Coverage";
    #[cfg(not(windows))]
    let path = "/Cloud/Coverage";

    CloudRoot {
        id: "google-drive:coverage-account".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Coverage Google Drive".into(),
        path: path.into(),
        readable: true,
        access_issue: None,
    }
}

#[test]
fn oauth_denial_over_real_loopback_callback_fails_before_network_or_durable_state() {
    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID)
        .expect("preparation should bind only a local ephemeral callback listener");
    let authorization_url = pending.authorization_url().to_owned();
    let state = query_value(&authorization_url, "state");
    let redirect_uri = query_value(&authorization_url, "redirect_uri");
    let port: u16 = redirect_uri
        .strip_prefix("http://127.0.0.1:")
        .expect("Google desktop OAuth must use the loopback IP redirect root")
        .parse()
        .expect("loopback redirect must contain a valid port");

    let callback = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port))
            .expect("the prepared loopback listener must accept the browser callback");
        write!(
            stream,
            "GET /?error=access_denied&state={state} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
        )
        .expect("callback request must be writable");
        stream.flush().expect("callback request must be flushed");

        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .expect("bounded callback response must be readable");
        let response = String::from_utf8(response).expect("callback response must be UTF-8");
        assert!(
            response.starts_with("HTTP/1.1 400 Bad Request\r\n"),
            "denied OAuth callbacks must receive a bounded rejection response"
        );
    });

    let temp = tempfile::tempdir().expect("isolated connection-document home must exist");
    let document = temp.path().join("connections.json");
    let error = finish_authorization(pending, &google_root(), &document, 1)
        .expect_err("provider denial must stop before token exchange or credential publication");
    callback.join().expect("loopback callback client must finish");

    assert_eq!(error, "oauth-authorization-denied");
    assert!(
        !document.exists(),
        "a denied callback must not create durable OAuth connection state"
    );
}
