//! Platform-aware entrypoint for the provider OAuth operational CLI.
//!
//! The domain implementation stays outside `src/bin` so Cargo/Tauri binary discovery sees only
//! this real entrypoint. This entry owns host argument decoding, terminal help, and platform
//! home-directory selection before delegating to the existing OAuth execution boundary.

use std::ffi::{OsStr, OsString};
use std::path::PathBuf;

mod implementation {
    use super::OsString;

    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/provider_oauth_cli_impl.rs.inc"
    ));

    #[cfg(not(coverage))]
    pub(super) fn usage_text() -> String {
        usage()
    }

    #[cfg(not(coverage))]
    fn host_path_surrogate(path: &Path) -> &'static str {
        if path.is_absolute() {
            #[cfg(windows)]
            return "C:\\";
            #[cfg(not(windows))]
            return "/";
        }
        "relative"
    }

    #[cfg(not(coverage))]
    pub(super) fn run_with_environment_home(
        args: Vec<OsString>,
        environment_home: Option<PathBuf>,
    ) -> Result<(), String> {
        let mut normalized = Vec::with_capacity(args.len());
        let mut native_home = Vec::new();
        let mut native_connections = Vec::new();
        let mut native_cloud_root = Vec::new();
        let mut index = 0usize;

        while index < args.len() {
            let option = args[index]
                .to_str()
                .ok_or_else(|| "provider-oauth-invalid-utf8-argument".to_string())?;
            match option {
                "--home" | "--connections" | "--cloud-root" => {
                    normalized.push(option.to_string());
                    index += 1;
                    let raw = args
                        .get(index)
                        .ok_or_else(|| format!("{option} requires a value"))?;
                    let path = PathBuf::from(raw);
                    normalized.push(host_path_surrogate(&path).to_string());
                    match option {
                        "--home" => native_home.push(path),
                        "--connections" => native_connections.push(path),
                        "--cloud-root" => native_cloud_root.push(path),
                        _ => unreachable!("path option match is exhaustive"),
                    }
                }
                "--client-id" => {
                    normalized.push(option.to_string());
                    index += 1;
                    let raw = args
                        .get(index)
                        .ok_or_else(|| "--client-id requires a value".to_string())?;
                    normalized.push(
                        raw.to_str()
                            .ok_or_else(|| "provider-oauth-invalid-utf8-argument".to_string())?
                            .to_string(),
                    );
                }
                "--list" | "--connect" | "--verify-capacity" | "--disconnect"
                | "--manual-browser" | "--write-access" | "--help" | "-h" => {
                    normalized.push(option.to_string());
                }
                _ => return Err("unknown argument".into()),
            }
            index += 1;
        }

        let explicit_home = !native_home.is_empty();
        let explicit_connections = !native_connections.is_empty();
        let mut parsed = parse_args(&normalized, environment_home)?;
        if let Some(home) = native_home.into_iter().next() {
            parsed.home = home;
        }
        if let Some(connections) = native_connections.into_iter().next() {
            parsed.connections = connections;
        } else if explicit_home && !explicit_connections {
            parsed.connections = default_connections_path(&parsed.home);
        }
        if let Some(cloud_root) = native_cloud_root.into_iter().next() {
            parsed.cloud_root = Some(cloud_root);
        }

        let output = execute(parsed)?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output)
                .map_err(|_| "provider-oauth-output-serialization-failed".to_string())?
        );
        Ok(())
    }
}

/// Resolve the platform home authority supplied to the existing OAuth parser.
///
/// Windows follows the same authority as DiskSage core (`USERPROFILE`). Other platforms keep
/// `HOME`. Missing canonical authority fails closed instead of borrowing another platform's
/// environment convention.
pub(crate) fn environment_home_from(
    home: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    windows: bool,
) -> Option<PathBuf> {
    if windows {
        user_profile
    } else {
        home
    }
}

#[cfg(not(coverage))]
fn command_line_args() -> Vec<OsString> {
    std::env::args_os().skip(1).collect()
}

/// Inject the XDG user-data location as the implicit connection document only when the caller did
/// not select an explicit path. Relative XDG values are invalid authority and are ignored.
#[cfg(all(not(coverage), unix, not(target_os = "macos")))]
fn apply_xdg_data_home_default_connections(args: &mut Vec<OsString>) {
    if args
        .iter()
        .any(|argument| argument == OsStr::new("--connections"))
    {
        return;
    }
    let Some(raw_data_home) = std::env::var_os("XDG_DATA_HOME") else {
        return;
    };
    if raw_data_home.is_empty() {
        return;
    }
    let data_home = PathBuf::from(raw_data_home);
    if !data_home.is_absolute() {
        return;
    }
    args.push(OsString::from("--connections"));
    args.push(
        data_home
            .join("com.contextualwisdomlab.disksage")
            .join("cloud-oauth-connections.json")
            .into_os_string(),
    );
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let mut args = command_line_args();
    if matches!(args.as_slice(), [flag] if flag == OsStr::new("--help") || flag == OsStr::new("-h")) {
        println!("{}", implementation::usage_text());
        return Ok(());
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    apply_xdg_data_home_default_connections(&mut args);

    let environment_home = environment_home_from(
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        cfg!(windows),
    );
    implementation::run_with_environment_home(args, environment_home)
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        let exit_code = if error == "provider-oauth-invalid-utf8-argument" {
            2
        } else {
            1
        };
        eprintln!("{error}");
        std::process::exit(exit_code);
    }
}

#[cfg(coverage)]
fn main() {}
