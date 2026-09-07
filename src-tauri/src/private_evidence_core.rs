//! Narrow production surface for object-bound private-evidence filesystem authority.
//!
//! The historical implementation module still contains JSON-focused test helpers that materialize
//! encoded payloads before enforcing the evidence-size ceiling. Production callers must serialize
//! through `private_evidence_publication`, which enforces the 8 MiB budget while encoding. This shim
//! therefore exposes only the receipt contract and descriptor-bound byte primitive required by that
//! facade; it deliberately does not re-export a JSON writer.

#[path = "private_evidence.rs"]
mod implementation;

pub use implementation::{PrivateEvidenceReceipt, MAX_PRIVATE_EVIDENCE_BYTES};

#[cfg(unix)]
pub(crate) use implementation::{
    write_object_bound_bytes_create_new_with_hooks, ObjectBoundPublicationError,
};
