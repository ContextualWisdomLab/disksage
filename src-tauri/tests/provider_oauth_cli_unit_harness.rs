//! Compile the provider OAuth CLI as a test module so its parser and projection tests run in the
//! ordinary Rust test/coverage lane even though the shipped binary is feature-gated.

#[path = "../src/bin/disksage-provider-oauth.rs"]
mod provider_oauth_cli;
