//! This integration-test seam is intentionally empty.
//!
//! `brew_cleanup` and `llm` are private production modules, so coverage for their internal
//! boundaries must be exercised by source-contained unit tests rather than widening the crate's
//! public API solely for instrumentation. Keeping this file compile-safe preserves that boundary
//! while the dedicated coverage line adds tests through supported public seams.
