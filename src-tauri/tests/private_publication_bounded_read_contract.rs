#![cfg(unix)]

#[test]
fn final_private_publication_reads_are_bounded_to_expected_length_plus_one() {
    let directory_publication = include_str!("../src/private_directory_publication.rs");
    let evidence_publication = include_str!("../src/private_evidence.rs");
    let bounded_read = ".take((encoded.len() as u64).saturating_add(1))";

    assert!(
        directory_publication.contains(bounded_read),
        "private-directory finalization must cap a same-UID concurrent append to encoded.len() + 1 before read_to_end"
    );
    assert!(
        evidence_publication.contains(bounded_read),
        "private-evidence finalization must cap a same-UID concurrent append to encoded.len() + 1 before read_to_end"
    );
}
