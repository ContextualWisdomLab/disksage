//! Compile and execute the provider OAuth CLI's pure host-boundary contracts in the default Rust
//! test suite, independent of provider credentials or feature-gated process execution.

#![allow(dead_code, unused_imports)]

#[path = "../src/bin/disksage-provider-oauth.rs"]
mod provider_oauth_cli;
