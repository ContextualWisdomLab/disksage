//! Real-loopback regression for callback streams accepted from a nonblocking listener.
//!
//! The listener is intentionally nonblocking while authorization is pending. The callback reader
//! must not depend on OS-specific inheritance of that mode: it owns a bounded blocking read with a
//! read timeout after accept. This fixture forces the accepted stream into nonblocking mode and
//! delays the browser-side request so the regression is observable without provider network I/O,
//! keyring access, or durable OAuth mutation.

#![cfg(not(coverage))]
#![allow(dead_code, unused_imports)]

#[path = "../src/object_bound_publication.rs"]
mod object_bound_publication;
include!("../src/provider_oauth.rs");

mod cloud {
    pub use disksage_lib::cloud::*;
}

#[test]
fn callback_reader_normalizes_nonblocking_accepted_stream_before_waiting_for_request() {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener binds");
    let address = listener.local_addr().expect("loopback address resolves");

    let client = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(address).expect("loopback client connects");
        std::thread::sleep(Duration::from_millis(200));
        stream
            .write_all(
                b"GET /?code=delayed&state=expected HTTP/1.1\r\nHost: 127.0.0.1\r\nConnection: close\r\n\r\n",
            )
            .expect("delayed callback request writes");
        stream.flush().expect("delayed callback request flushes");
    });

    let (mut stream, _) = listener.accept().expect("loopback callback accepts");
    stream
        .set_nonblocking(true)
        .expect("fixture forces inherited-nonblocking shape");

    let target = read_callback_target(&mut stream, "127.0.0.1")
        .expect("callback reader must own a blocking-with-timeout read boundary");
    assert_eq!(target, "/?code=delayed&state=expected");

    client.join().expect("loopback client joins");
}
