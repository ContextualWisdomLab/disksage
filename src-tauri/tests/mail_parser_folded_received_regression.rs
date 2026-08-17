//! Compatibility regression for mail-parser's bounded RFC 5322 metadata path.
//!
//! DiskSage parses only the bounded header block of `.eml` archive candidates. A folded
//! `Received` header at the end of that block previously exercised an upstream panic path, so
//! this test reaches the production archive collector and requires the metadata probe to remain
//! non-panicking while preserving useful header lineage.

use disksage_lib::cloud::collect_archive_files;

#[test]
fn folded_received_header_at_end_of_block_is_safe() {
    let temp = tempfile::tempdir().expect("create temporary archive root");
    let message_path = temp.path().join("folded-received.eml");
    std::fs::write(
        &message_path,
        concat!(
            "Date: Mon, 17 Aug 2026 12:00:00 +0000\r\n",
            "Subject: Folded Received regression\r\n",
            "Received: from relay.example\r\n",
            "\tby mx.example with ESMTP\r\n",
            "\r\n",
            "body is deliberately outside the bounded metadata parser\r\n",
        ),
    )
    .expect("write RFC 5322 fixture");

    let files = collect_archive_files(temp.path(), &[]);
    assert_eq!(files.len(), 1, "the .eml fixture must remain an archive candidate");

    let metadata = &files[0].content_metadata;
    assert_eq!(metadata.title.as_deref(), Some("Folded Received regression"));
    assert!(metadata.evidence.iter().any(|evidence| {
        evidence.field == "email-header-bytes-inspected"
            && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
    }));
    assert!(metadata.evidence.iter().any(|evidence| {
        evidence.field == "email-body-inspected"
            && evidence.value == "false"
            && evidence.source == "local:metadata-probe:bounded-rfc5322-header"
    }));
}
