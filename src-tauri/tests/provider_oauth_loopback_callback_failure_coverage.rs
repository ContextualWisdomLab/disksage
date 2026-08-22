//! Credential-free coverage for rejected OAuth loopback callbacks.
//!
//! This drives the real public `finish_authorization` boundary through the ephemeral loopback
//! listener, exercises malformed/unauthorized callback handling, and terminates on an explicit
//! authorization denial before any provider network, keyring, or durable-connection mutation.

#![cfg(not(coverage))]

use disksage_lib::cloud::{CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::provider_oauth::{
    connections_path, finish_authorization, prepare_authorization,
};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

const GOOGLE_CLIENT_ID: &str = "1234567890-abcxyz.apps.googleusercontent.com";

fn query_value<'a>(url: &'a str, key: &str) -> &'a str {
    let query = url
        .split_once('?')
        .map(|(_, query)| query)
        .expect("authorization URL has query parameters");
    query
        .split('&')
        .find_map(|pair| {
            let (candidate_key, value) = pair.split_once('=')?;
            (candidate_key == key).then_some(value)
        })
        .unwrap_or_else(|| panic!("authorization URL is missing {key}"))
}

fn google_loopback_port(url: &str) -> u16 {
    const PREFIX: &str = "http%3A%2F%2F127.0.0.1%3A";
    query_value(url, "redirect_uri")
        .strip_prefix(PREFIX)
        .expect("Google redirect URI uses the registered loopback IP form")
        .parse()
        .expect("loopback port is numeric")
}

fn send_rejected_request(port: u16, request_line: &str) -> String {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("loopback listener accepts");
    stream
        .set_read_timeout(Some(Duration::from_secs(3)))
        .expect("read timeout configured");
    write!(
        stream,
        "{request_line}\r\nHost: 127.0.0.1:{port}\r\nConnection: close\r\n\r\n"
    )
    .expect("callback request written");
    stream.flush().expect("callback request flushed");

    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("bounded loopback response read");
    assert!(response.starts_with("HTTP/1.1 400 Bad Request\r\n"));
    assert!(response.contains("Cache-Control: no-store\r\n"));
    assert!(response.contains("Content-Security-Policy"));
    response
}

#[test]
fn rejected_callbacks_fail_closed_before_network_keyring_or_durable_publication() {
    let temp = tempfile::tempdir().unwrap();
    let connection_path = connections_path(temp.path());

    #[cfg(windows)]
    let root_path = r"C:\Cloud\google-account";
    #[cfg(not(windows))]
    let root_path = "/Cloud/google-account";

    let root = CloudRoot {
        id: "google-account".into(),
        provider: CloudProvider::GoogleDrive,
        account_scope: CloudAccountScope::Unknown,
        label: "Google Drive".into(),
        path: root_path.into(),
        readable: true,
        access_issue: None,
    };

    let pending = prepare_authorization(CloudProvider::GoogleDrive, GOOGLE_CLIENT_ID).unwrap();
    let authorization_url = pending.authorization_url().to_owned();
    let port = google_loopback_port(&authorization_url);
    let state = query_value(&authorization_url, "state").to_owned();

    let worker = std::thread::spawn(move || {
        finish_authorization(pending, &root, &connection_path, 123)
    });

    send_rejected_request(
        port,
        &format!("POST /?code=ignored&state={state} HTTP/1.1"),
    );
    send_rejected_request(
        port,
        &format!("GET /?code=%GG&state={state} HTTP/1.1"),
    );
    send_rejected_request(port, "GET /?code=ignored&state=wrong HTTP/1.1");
    send_rejected_request(
        port,
        &format!("GET /?error=access_denied&state={state} HTTP/1.1"),
    );

    assert_eq!(
        worker.join().expect("authorization worker joins").unwrap_err(),
        "oauth-authorization-denied"
    );
    assert!(!connections_path(temp.path()).exists());
}
