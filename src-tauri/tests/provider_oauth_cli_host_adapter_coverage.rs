//! Compile and execute the provider OAuth host adapter's pure host-boundary contracts in the
//! default Rust test suite, independent of provider credentials or the feature-gated binary run.

#![allow(dead_code, unused_imports)]

#[path = "../src/bin/disksage-provider-oauth-host.rs"]
mod provider_oauth_cli_host;
