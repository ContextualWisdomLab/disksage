//! Process boundary for the DiskSage cloud planner CLI.
//!
//! Host arguments are screened before any HOME/provider/filesystem work. The implementation stays
//! in the sibling include so the process contract can fail closed on non-UTF-8 input and terminate
//! help successfully even when no user-home environment is available.

#[cfg(not(coverage))]
mod implementation {
    include!(concat!(
        env!("OUT_DIR"),
        "/disksage-cloud-plan-implementation.rs"
    ));

    pub(crate) mod entry {
        use std::path::Path;

        pub(crate) fn help_text() -> String {
            super::parse_args(&["--help".to_string()], Path::new("/"))
                .expect_err("the implementation parser must expose the stable help synopsis")
        }

        pub(crate) fn run() -> Result<(), String> {
            super::run()
        }
    }
}

#[cfg(not(coverage))]
fn main() {
    let raw = std::env::args_os().skip(1).collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("--help" | "-h")) {
        println!("{}", implementation::entry::help_text());
        return;
    }
    if raw.iter().any(|argument| argument.to_str().is_none()) {
        eprintln!("DiskSage cloud planner: invalid-argument-encoding");
        std::process::exit(2);
    }
    if let Err(error) = implementation::entry::run() {
        eprintln!("DiskSage cloud planner: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}
