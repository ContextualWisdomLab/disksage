//! Headless OAuth lifecycle for OneDrive and Google Drive evidence and explicit API uploads.
//!
//! Refresh tokens remain in the operating-system credential store. This command emits only
//! non-secret connection descriptors and provider capacity evidence; this command itself never
//! performs a cloud file write or source eviction.

#[cfg(not(coverage))]
use std::ffi::{OsStr, OsString};
#[cfg(not(coverage))]
use std::path::{Path, PathBuf};
#[cfg(not(coverage))]
use std::process::Command;

#[cfg(not(coverage))]
use disksage_lib::cloud::{self, CloudProvider, CloudRoot};
#[cfg(not(coverage))]
use disksage_lib::provider_capacity::{self, FixedHostProviderCapacityClient};
#[cfg(not(coverage))]
use disksage_lib::provider_oauth::{self, OAuthConnection};

#[cfg(not(coverage))]
#[path = "../home_resolution.rs"]
mod home_resolution;

#[cfg(not(coverage))]
const OUTPUT_SCHEMA_VERSION: u32 = 1;
#[cfg(not(coverage))]
const APP_IDENTIFIER: &str = "com.contextualwisdomlab.disksage";

#[cfg(not(coverage))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    List,
    Connect,
    VerifyCapacity,
    Disconnect,
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    action: Action,
    home: PathBuf,
    connections: PathBuf,
    cloud_root: Option<PathBuf>,
    client_id: Option<String>,
    manual_browser: bool,
    write_access: bool,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
#[serde(tag = "action", rename_all = "kebab-case")]
enum Output {
    List {
        schema_version: u32,
        connection_count: usize,
        connections: Vec<OAuthConnection>,
        secrets_included: bool,
        connection_document_effect: &'static str,
        credential_store_effect: &'static str,
        cloud_write_executed: bool,
        source_eviction_executed: bool,
    },
    Connect {
        schema_version: u32,
        connection: OAuthConnection,
        secrets_included: bool,
        connection_document_effect: &'static str,
        credential_store_effect: &'static str,
        cloud_write_executed: bool,
        source_eviction_executed: bool,
    },
    VerifyCapacity {
        schema_version: u32,
        connection_id: String,
        capacity: provider_capacity::CloudCapacitySnapshot,
        secrets_included: bool,
        connection_document_effect: &'static str,
        credential_store_effect: &'static str,
        cloud_write_executed: bool,
        source_eviction_executed: bool,
    },
    Disconnect {
        schema_version: u32,
        connection_id: String,
        provider: CloudProvider,
        secrets_included: bool,
        connection_document_effect: &'static str,
        credential_store_effect: &'static str,
        cloud_write_executed: bool,
        source_eviction_executed: bool,
    },
}

#[cfg(not(coverage))]
fn usage() -> String {
    concat!(
        "usage: disksage-provider-oauth [--home ABSOLUTE_PATH] ",
        "[--connections ABSOLUTE_PATH] ",
        "(--list | --connect --cloud-root ABSOLUTE_PATH --client-id ID ",
        "[--manual-browser] [--write-access] | --verify-capacity --cloud-root ABSOLUTE_PATH | ",
        "--disconnect --cloud-root ABSOLUTE_PATH)"
    )
    .into()
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} requires a value"))
}

#[cfg(not(coverage))]
fn default_connections_path(home: &Path) -> PathBuf {
    #[cfg(target_os = "macos")]
    return home
        .join("Library/Application Support")
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");

    #[cfg(target_os = "windows")]
    return home
        .join("AppData/Roaming")
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json");

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    home.join(".local/share")
        .join(APP_IDENTIFIER)
        .join("cloud-oauth-connections.json")
}

#[cfg(not(coverage))]
fn parse_args(args: &[String], environment_home: Option<PathBuf>) -> Result<Args, String> {
    let mut actions = Vec::new();
    let mut home = None;
    let mut connections = None;
    let mut cloud_root = None;
    let mut client_id = None;
    let mut manual_browser = false;
    let mut write_access = false;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--list" => actions.push(Action::List),
            "--connect" => actions.push(Action::Connect),
            "--verify-capacity" => actions.push(Action::VerifyCapacity),
            "--disconnect" => actions.push(Action::Disconnect),
            "--manual-browser" => manual_browser = true,
            "--write-access" => write_access = true,
            "--home" => {
                if home
                    .replace(PathBuf::from(value(args, &mut index, "--home")?))
                    .is_some()
                {
                    return Err("--home may be supplied once".into());
                }
            }
            "--connections" => {
                if connections
                    .replace(PathBuf::from(value(args, &mut index, "--connections")?))
                    .is_some()
                {
                    return Err("--connections may be supplied once".into());
                }
            }
            "--cloud-root" => {
                if cloud_root
                    .replace(PathBuf::from(value(args, &mut index, "--cloud-root")?))
                    .is_some()
                {
                    return Err("--cloud-root may be supplied once".into());
                }
            }
            "--client-id" => {
                if client_id
                    .replace(value(args, &mut index, "--client-id")?)
                    .is_some()
                {
                    return Err("--client-id may be supplied once".into());
                }
            }
            "--help" | "-h" => return Err(usage()),
            _ => return Err("unknown argument".into()),
        }
        index += 1;
    }

    let action = match actions.as_slice() {
        [only] => *only,
        [] => return Err("exactly one action is required".into()),
        _ => return Err("actions are mutually exclusive".into()),
    };
    let home = home
        .or(environment_home)
        .ok_or_else(|| "home-directory-unavailable".to_string())?;
    if !home.is_absolute() {
        return Err("--home must be absolute".into());
    }
    let connections = connections.unwrap_or_else(|| default_connections_path(&home));
    if !connections.is_absolute() {
        return Err("--connections must be absolute".into());
    }
    if cloud_root.as_ref().is_some_and(|path| !path.is_absolute()) {
        return Err("--cloud-root must be absolute".into());
    }

    match action {
        Action::List => {
            if cloud_root.is_some() || client_id.is_some() || manual_browser || write_access {
                return Err("--list does not accept root, client, browser, or write arguments".into());
            }
        }
        Action::Connect => {
            if cloud_root.is_none() || client_id.is_none() {
                return Err("--connect requires --cloud-root and --client-id".into());
            }
        }
        Action::VerifyCapacity | Action::Disconnect => {
            if cloud_root.is_none() || client_id.is_some() || manual_browser || write_access {
                return Err(
                    "capacity verification and disconnect require only --cloud-root".into(),
                );
            }
        }
    }

    Ok(Args {
        action,
        home,
        connections,
        cloud_root,
        client_id,
        manual_browser,
        write_access,
    })
}

#[cfg(not(coverage))]
fn selected_root(home: &Path, requested: &Path) -> Result<CloudRoot, String> {
    let matches: Vec<_> = cloud::discover_cloud_roots(home)
        .into_iter()
        .filter(|root| cloud::cloud_root_path_matches(Path::new(&root.path), requested))
        .collect();
    let root = match matches.as_slice() {
        [only] => only.clone(),
        [] => return Err("cloud-root-not-discovered".into()),
        _ => return Err("cloud-root-ambiguous-after-normalization".into()),
    };
    cloud::validate_cloud_root_readable(&root)?;
    if root.provider == CloudProvider::Icloud {
        return Err("icloud-oauth-not-supported".into());
    }
    Ok(root)
}

#[cfg(all(not(coverage), target_os = "macos"))]
fn open_system_browser(url: &str) -> Result<(), String> {
    let status = Command::new("open")
        .arg(url)
        .status()
        .map_err(|_| "oauth-system-browser-open-failed".to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "oauth-system-browser-open-failed".into())
}

#[cfg(all(not(coverage), target_os = "windows"))]
fn open_system_browser(url: &str) -> Result<(), String> {
    let status = Command::new("rundll32.exe")
        .arg("url.dll,FileProtocolHandler")
        .arg(url)
        .status()
        .map_err(|_| "oauth-system-browser-open-failed".to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "oauth-system-browser-open-failed".into())
}

#[cfg(all(not(coverage), not(any(target_os = "macos", target_os = "windows"))))]
fn open_system_browser(url: &str) -> Result<(), String> {
    let status = Command::new("xdg-open")
        .arg(url)
        .status()
        .map_err(|_| "oauth-system-browser-open-failed".to_string())?;
    status
        .success()
        .then_some(())
        .ok_or_else(|| "oauth-system-browser-open-failed".into())
}

#[cfg(not(coverage))]
fn output_common() -> (bool, bool, bool) {
    (false, false, false)
}

#[cfg(not(coverage))]
fn execute(args: Args) -> Result<Output, String> {
    let (secrets_included, cloud_write_executed, source_eviction_executed) = output_common();
    if args.action == Action::List {
        let connections = provider_oauth::load_connections(&args.connections)?;
        return Ok(Output::List {
            schema_version: OUTPUT_SCHEMA_VERSION,
            connection_count: connections.len(),
            connections,
            secrets_included,
            connection_document_effect: "none",
            credential_store_effect: "none",
            cloud_write_executed,
            source_eviction_executed,
        });
    }

    let requested = args
        .cloud_root
        .as_deref()
        .ok_or_else(|| "--cloud-root is required".to_string())?;
    let root = selected_root(&args.home, requested)?;
    match args.action {
        Action::List => unreachable!("list returned before root selection"),
        Action::Connect => {
            let client_id = args
                .client_id
                .as_deref()
                .ok_or_else(|| "--client-id is required".to_string())?;
            let pending = provider_oauth::prepare_authorization_with_write_access(
                root.provider,
                client_id,
                args.write_access,
            )?;
            if args.manual_browser {
                eprintln!("Open this provider authorization URL in a browser:");
                eprintln!("{}", pending.authorization_url());
            } else {
                open_system_browser(pending.authorization_url())?;
            }
            let connection = provider_oauth::finish_authorization(
                pending,
                &root,
                &args.connections,
                cloud::system_now_ms(),
            )?;
            Ok(Output::Connect {
                schema_version: OUTPUT_SCHEMA_VERSION,
                connection,
                secrets_included,
                connection_document_effect: "connection-upserted",
                credential_store_effect: "refresh-token-stored",
                cloud_write_executed,
                source_eviction_executed,
            })
        }
        Action::VerifyCapacity => {
            let connections = provider_oauth::load_connections(&args.connections)?;
            let connection = provider_oauth::connection_for_root(&connections, &root)?;
            let token = provider_oauth::refreshed_access_token(&args.connections, &root)?;
            let capacity = provider_capacity::collect_authenticated_capacity(
                root.provider,
                token.as_str(),
                cloud::system_now_ms(),
                &FixedHostProviderCapacityClient::default(),
            )?;
            Ok(Output::VerifyCapacity {
                schema_version: OUTPUT_SCHEMA_VERSION,
                connection_id: connection.connection_id,
                capacity,
                secrets_included,
                connection_document_effect: "none",
                credential_store_effect: "refresh-token-may-rotate",
                cloud_write_executed,
                source_eviction_executed,
            })
        }
        Action::Disconnect => {
            let connections = provider_oauth::load_connections(&args.connections)?;
            let connection = provider_oauth::connection_for_root(&connections, &root)?;
            provider_oauth::disconnect(&args.connections, &root)?;
            Ok(Output::Disconnect {
                schema_version: OUTPUT_SCHEMA_VERSION,
                connection_id: connection.connection_id,
                provider: connection.provider,
                secrets_included,
                connection_document_effect: "connection-removed",
                credential_store_effect: "refresh-token-deleted",
                cloud_write_executed,
                source_eviction_executed,
            })
        }
    }
}

#[cfg(not(coverage))]
#[derive(Debug, Clone, PartialEq, Eq)]
enum TerminalArgs {
    Help,
    Run(Vec<String>),
}

#[cfg(not(coverage))]
fn is_help(value: &OsStr) -> bool {
    value == OsStr::new("--help") || value == OsStr::new("-h")
}

#[cfg(not(coverage))]
fn parse_terminal_args(args: Vec<OsString>) -> Result<TerminalArgs, String> {
    if args.iter().any(|value| value.to_str().is_none()) {
        return Err("argument-encoding-invalid".into());
    }
    match args.as_slice() {
        [only] if is_help(only) => return Ok(TerminalArgs::Help),
        values if values.iter().any(|value| is_help(value)) => {
            return Err("help must be used alone".into());
        }
        _ => {}
    }
    Ok(TerminalArgs::Run(
        args.into_iter()
            .map(|value| {
                value
                    .into_string()
                    .expect("non-UTF-8 arguments were rejected before domain parsing")
            })
            .collect(),
    ))
}

#[cfg(not(coverage))]
fn environment_home_from(
    home: Option<PathBuf>,
    user_profile: Option<PathBuf>,
    windows_home_drive_path: Option<PathBuf>,
    windows: bool,
) -> Option<PathBuf> {
    let candidates = if windows {
        vec![home, user_profile, windows_home_drive_path]
    } else {
        vec![home]
    };
    home_resolution::select_absolute_home(candidates).ok()
}

#[cfg(all(not(coverage), windows))]
fn environment_home() -> Option<PathBuf> {
    environment_home_from(
        std::env::var_os("HOME").map(PathBuf::from),
        std::env::var_os("USERPROFILE").map(PathBuf::from),
        home_resolution::windows_home_drive_path(),
        true,
    )
}

#[cfg(all(not(coverage), not(windows)))]
fn environment_home() -> Option<PathBuf> {
    environment_home_from(
        std::env::var_os("HOME").map(PathBuf::from),
        None,
        None,
        false,
    )
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let terminal_args = parse_terminal_args(std::env::args_os().skip(1).collect())?;
    let args = match terminal_args {
        TerminalArgs::Help => {
            println!("{}", usage());
            return Ok(());
        }
        TerminalArgs::Run(args) => args,
    };
    let parsed = parse_args(&args, environment_home())?;
    let output = execute(parsed)?;
    println!(
        "{}",
        serde_json::to_string_pretty(&output)
            .map_err(|_| "provider-oauth-output-serialization-failed".to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn absolute_home() -> PathBuf {
        std::env::temp_dir().join("disksage-provider-oauth-test-home")
    }

    #[test]
    fn list_defaults_to_the_app_connection_document() {
        let home = absolute_home();
        let parsed = parse_args(&strings(&["--list"]), Some(home.clone())).unwrap();
        assert_eq!(parsed.action, Action::List);
        assert_eq!(parsed.home, home.clone());
        assert_eq!(parsed.connections, default_connections_path(&home));
        assert!(parsed.cloud_root.is_none());
        assert!(parsed.client_id.is_none());
        assert!(!parsed.manual_browser);
        assert!(!parsed.write_access);
    }

    #[test]
    fn connect_requires_an_absolute_discovered_root_and_client_id() {
        let home = absolute_home();
        let cloud_root = home.join("Library/CloudStorage/OneDrive-Personal");
        let connections = home.join("private/connections.json");
        let parsed = parse_args(
            &[
                "--connect".into(),
                "--cloud-root".into(),
                cloud_root.to_string_lossy().into_owned(),
                "--client-id".into(),
                "12345678-1234-4abc-8def-1234567890ab".into(),
                "--manual-browser".into(),
                "--connections".into(),
                connections.to_string_lossy().into_owned(),
            ],
            Some(home.clone()),
        )
        .unwrap();
        assert_eq!(parsed.action, Action::Connect);
        assert!(parsed.manual_browser);
        assert!(!parsed.write_access);
        assert_eq!(parsed.connections, connections);
        assert!(parse_args(
            &strings(&["--connect", "--cloud-root", "relative", "--client-id", "id"]),
            Some(home.clone()),
        )
        .is_err());
        assert!(parse_args(
            &[
                "--connect".into(),
                "--cloud-root".into(),
                cloud_root.to_string_lossy().into_owned(),
            ],
            Some(home),
        )
        .is_err());
        let write = parse_args(
            &[
                "--connect".into(),
                "--cloud-root".into(),
                cloud_root.to_string_lossy().into_owned(),
                "--client-id".into(),
                "12345678-1234-4abc-8def-1234567890ab".into(),
                "--write-access".into(),
            ],
            Some(absolute_home()),
        )
        .unwrap();
        assert!(write.write_access);
    }

    #[test]
    fn actions_and_action_specific_options_are_fail_closed() {
        let absolute_root = absolute_home().join("cloud");
        let root = absolute_root.to_string_lossy().into_owned();
        let home = Some(absolute_home());
        assert!(parse_args(&[], home.clone()).is_err());
        assert!(parse_args(&strings(&["--list", "--disconnect"]), home.clone()).is_err());
        assert!(parse_args(&strings(&["--list", "--manual-browser"]), home.clone()).is_err());
        assert!(parse_args(&strings(&["--list", "--write-access"]), home.clone()).is_err());
        assert!(parse_args(
            &[
                "--verify-capacity".into(),
                "--cloud-root".into(),
                root.clone(),
                "--client-id".into(),
                "not-allowed".into(),
            ],
            home.clone(),
        )
        .is_err());
        assert!(parse_args(
            &[
                "--disconnect".into(),
                "--cloud-root".into(),
                root.clone(),
                "--manual-browser".into(),
            ],
            home.clone(),
        )
        .is_err());
        assert!(parse_args(
            &[
                "--verify-capacity".into(),
                "--cloud-root".into(),
                root,
                "--write-access".into(),
            ],
            home,
        )
        .is_err());
    }

    #[test]
    fn duplicate_and_relative_global_options_are_rejected() {
        let home = Some(absolute_home());
        assert!(parse_args(
            &strings(&["--list", "--home", "/one", "--home", "/two"]),
            home.clone(),
        )
        .is_err());
        assert!(parse_args(
            &strings(&["--list", "--connections", "relative.json"]),
            home.clone(),
        )
        .is_err());
        assert!(parse_args(&strings(&["--list", "--home", "relative"]), home).is_err());
    }

    #[test]
    fn unknown_argument_does_not_echo_its_value() {
        let sensitive = "private-token-or-path";
        let error =
            parse_args(&strings(&["--list", sensitive]), Some(absolute_home())).unwrap_err();

        assert_eq!(error, "unknown argument");
        assert!(!error.contains(sensitive));
    }

    #[test]
    fn root_selection_requires_a_unique_readable_non_icloud_provider_root() {
        let temp = tempfile::tempdir().unwrap();
        let onedrive = temp.path().join("Library/CloudStorage/OneDrive-Personal");
        std::fs::create_dir_all(&onedrive).unwrap();
        let selected = selected_root(temp.path(), &onedrive).unwrap();
        assert_eq!(selected.provider, CloudProvider::Onedrive);

        let icloud = temp
            .path()
            .join("Library/Mobile Documents/com~apple~CloudDocs");
        std::fs::create_dir_all(&icloud).unwrap();
        assert_eq!(
            selected_root(temp.path(), &icloud).unwrap_err(),
            "icloud-oauth-not-supported"
        );
        assert_eq!(
            selected_root(temp.path(), &temp.path().join("missing")).unwrap_err(),
            "cloud-root-not-discovered"
        );
    }

    #[test]
    fn list_output_makes_non_mutation_and_secret_boundaries_explicit() {
        let output = Output::List {
            schema_version: OUTPUT_SCHEMA_VERSION,
            connection_count: 0,
            connections: Vec::new(),
            secrets_included: false,
            connection_document_effect: "none",
            credential_store_effect: "none",
            cloud_write_executed: false,
            source_eviction_executed: false,
        };
        let encoded = serde_json::to_value(output).unwrap();
        assert_eq!(encoded["action"], "list");
        assert_eq!(encoded["secrets_included"], false);
        assert_eq!(encoded["connection_document_effect"], "none");
        assert_eq!(encoded["credential_store_effect"], "none");
        assert_eq!(encoded["cloud_write_executed"], false);
        assert_eq!(encoded["source_eviction_executed"], false);
        assert!(encoded.get("access_token").is_none());
        assert!(encoded.get("refresh_token").is_none());
    }

    #[test]
    fn terminal_host_parser_keeps_help_success_separate_from_domain_parsing() {
        assert_eq!(
            parse_terminal_args(vec![OsString::from("--help")]).unwrap(),
            TerminalArgs::Help
        );
        assert_eq!(
            parse_terminal_args(vec![OsString::from("--help"), OsString::from("--list")])
                .unwrap_err(),
            "help must be used alone"
        );
    }

    #[test]
    fn windows_environment_home_falls_back_to_user_profile_without_importing_it_on_unix() {
        let profile = std::env::temp_dir().join("disksage-user-profile");
        let drive_path = std::env::temp_dir().join("disksage-home-drive-path");
        assert_eq!(
            environment_home_from(None, Some(profile.clone()), Some(drive_path), true),
            Some(profile.clone())
        );
        assert_eq!(environment_home_from(None, Some(profile), None, false), None);
    }
}
