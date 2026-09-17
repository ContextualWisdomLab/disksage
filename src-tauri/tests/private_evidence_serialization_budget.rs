#![cfg(unix)]

#[path = "../src/private_evidence.rs"]
mod private_evidence_core;
#[path = "../src/private_directory_publication.rs"]
mod private_directory_publication;
#[path = "../src/private_evidence_publication.rs"]
mod private_evidence;

use private_evidence::write_private_json_create_new;
use serde::ser::{SerializeSeq, Serializer};
use serde::Serialize;
use std::cell::Cell;
use std::fs;
use std::os::unix::fs::PermissionsExt;

struct StreamingOversizeValue {
    emitted: Cell<usize>,
}

impl Serialize for StreamingOversizeValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut sequence = serializer.serialize_seq(None)?;
        let chunk = "x".repeat(256);

        loop {
            let next = self.emitted.get() + 1;
            assert!(
                next < 50_000,
                "private-evidence serialization consumed input after the encoded-size budget should have terminated it"
            );
            self.emitted.set(next);
            sequence.serialize_element(&chunk)?;
        }
    }
}

#[test]
fn oversize_streaming_json_stops_during_serialization_before_publication() {
    let source = tempfile::tempdir().expect("source tempdir");
    let private = tempfile::tempdir().expect("private tempdir");
    fs::set_permissions(source.path(), fs::Permissions::from_mode(0o700))
        .expect("set source mode");
    fs::set_permissions(private.path(), fs::Permissions::from_mode(0o700))
        .expect("set private mode");
    let target = private.path().join("audit.json");
    let value = StreamingOversizeValue {
        emitted: Cell::new(0),
    };

    let error = write_private_json_create_new(source.path(), &target, &value)
        .expect_err("oversize streaming JSON must fail before filesystem publication");

    assert_eq!(error, "private-evidence-too-large");
    assert!(
        value.emitted.get() < 50_000,
        "the serializer must stop as soon as the 8 MiB encoded budget is exhausted"
    );
    assert!(!target.exists(), "oversize evidence must not create a record");
}

#[test]
fn production_core_surface_cannot_bypass_bounded_json_serialization() {
    const LIB_SOURCE: &str = include_str!("../src/lib.rs");
    const CORE_SHIM_SOURCE: &str = include_str!("../src/private_evidence_core.rs");

    assert!(
        LIB_SOURCE.contains("#[path = \"private_evidence_core.rs\"]\nmod private_evidence_core;"),
        "the crate root must compile the narrow private-evidence core shim instead of exposing the legacy implementation module directly"
    );
    assert!(
        !LIB_SOURCE.contains("#[path = \"private_evidence.rs\"]\nmod private_evidence_core;"),
        "the legacy materialize-then-check JSON writer must not remain reachable as a crate-level production capability"
    );
    assert!(
        !CORE_SHIM_SOURCE.contains("write_private_json_create_new"),
        "the production core shim must not re-export the legacy unbounded JSON writer"
    );
    assert!(
        CORE_SHIM_SOURCE.contains("write_object_bound_bytes_create_new_with_hooks"),
        "the bounded public facade still needs the descriptor-bound byte publication primitive"
    );
}
