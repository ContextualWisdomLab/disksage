//! Evidence-bound reclamation of explicitly identified PostgreSQL test clusters.
//!
//! Database names and paths are operator inputs, never inferred from age or naming. Destructive
//! execution re-runs the native PostgreSQL observations, requires an exact approval phrase, writes
//! a pending journal before shutdown, and records the measured filesystem free-space delta.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const SCHEMA_VERSION: u32 = 1;
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_OUTPUT_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Eq, PartialEq)]
/// Operator-supplied authority for one PostgreSQL test-cluster observation.
pub struct PostgresTestClusterRequest {
    /// Exact local PostgreSQL data directory to inspect.
    pub data_directory: PathBuf,
    /// Absolute native `psql` executable path.
    pub psql_path: PathBuf,
    /// Absolute native `pg_ctl` executable path.
    pub pg_ctl_path: PathBuf,
    /// Database role used only for bounded read-only observations.
    pub database_user: String,
    /// Complete expected set of non-template, non-default databases.
    pub expected_databases: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// Private, identity-bound read-only plan for one running test cluster.
pub struct PostgresTestClusterPlan {
    /// Receipt schema revision.
    pub schema_version: u32,
    /// Canonical private data-directory path.
    pub data_directory: String,
    /// Device, inode, and owner identity of the directory.
    pub data_directory_identity: String,
    /// Version read from the cluster-owned `PG_VERSION` file.
    pub postgres_version: String,
    /// Postmaster PID read from the ready PID file.
    pub postmaster_pid: u32,
    /// Digest of the complete ready PID-file observation.
    pub postmaster_identity: String,
    /// Cluster port read from the PID file.
    pub port: u16,
    /// Cluster socket directory read from the PID file.
    pub socket_directory: String,
    /// Explicit role used for read-only native queries.
    pub database_user: String,
    /// Sorted operator-provided database allowlist.
    pub expected_databases: Vec<String>,
    /// Sorted live database set returned by PostgreSQL.
    pub observed_databases: Vec<String>,
    /// Other client backends observed immediately before planning.
    pub external_client_count: u64,
    /// Filesystem identity of the `psql` executable object.
    pub psql_identity: String,
    /// Filesystem identity of the `pg_ctl` executable object.
    pub pg_ctl_identity: String,
    /// Digest of native `pg_ctl status` output bound to the postmaster PID.
    pub pg_ctl_status_identity: String,
    /// Allocated bytes under the symlink-free data directory.
    pub allocated_bytes: u64,
    /// Wall-clock observation time, excluded from the stable fingerprint.
    pub observed_at_ms: u64,
    /// Stable digest of all deletion-authority evidence.
    pub plan_fingerprint: String,
    /// Exact operator phrase required for execution.
    pub exact_approval_phrase: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// Immutable private record written before shutdown begins.
pub struct PostgresTestClusterPendingJournal {
    /// Receipt schema revision.
    pub schema_version: u32,
    /// Unique identity of this execution attempt.
    pub operation_id: String,
    /// Complete approved private plan.
    pub plan: PostgresTestClusterPlan,
    /// Exact phrase supplied by the operator.
    pub approved_phrase: String,
    /// Time the pending state was made durable.
    pub written_at_ms: u64,
    /// Stable lifecycle state (`pending`).
    pub state: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
/// Path-free result recorded after an attempted destructive lifecycle.
pub struct PostgresTestClusterResult {
    /// Receipt schema revision.
    pub schema_version: u32,
    /// Identity shared with the pending journal.
    pub operation_id: String,
    /// Approved plan fingerprint.
    pub plan_fingerprint: String,
    /// Whether shutdown and exact-directory removal both completed.
    pub completed: bool,
    /// Stable machine-readable outcome.
    pub reason_code: String,
    /// Whether native shutdown and PID disappearance were confirmed.
    pub shutdown_completed: bool,
    /// Whether the identity-bound quarantine directory was removed.
    pub directory_removed: bool,
    /// Available bytes on the containing filesystem before mutation.
    pub free_bytes_before: u64,
    /// Available bytes on the containing filesystem after mutation.
    pub free_bytes_after: u64,
    /// Saturating measured increase in available filesystem bytes.
    pub physically_reclaimed_bytes: u64,
    /// Completion observation time.
    pub completed_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Public evidence handles plus the path-free execution outcome.
pub struct ExecutionEvidence {
    /// Receipt proving the pending journal was durably created.
    pub pending: crate::private_evidence::PrivateEvidenceReceipt,
    /// Receipt proving the result journal was durably created.
    pub result: crate::private_evidence::PrivateEvidenceReceipt,
    /// Path-free operation outcome.
    pub outcome: PostgresTestClusterResult,
}

#[derive(Clone, Debug, Eq, PartialEq)]
/// Bounded native command result used by deterministic planners and tests.
pub struct CommandOutput {
    /// Native process exit code, or `-1` when unavailable.
    pub status: i32,
    /// Bounded UTF-8 standard output.
    pub stdout: String,
    /// Bounded UTF-8 standard error.
    pub stderr: String,
    /// Identity of the executable object checked before and after execution.
    pub executable_identity: String,
}

/// Minimal process boundary needed for native PostgreSQL evidence and lifecycle tests.
pub trait PostgresCommandRunner {
    /// Executes one fixed native program with a wall-clock bound.
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutput, String>;
    /// Returns whether the exact planned PID still exists.
    fn pid_is_alive(&self, pid: u32) -> bool;
}

/// Production identity-checked native process runner.
pub struct NativePostgresCommandRunner;

impl PostgresCommandRunner for NativePostgresCommandRunner {
    fn run(
        &self,
        program: &Path,
        args: &[String],
        timeout: Duration,
    ) -> Result<CommandOutput, String> {
        #[cfg(unix)]
        let verified_program = {
            use std::os::unix::fs::OpenOptionsExt;
            std::fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
                .open(program)
                .map_err(|_| "postgres-native-command-open-failed".to_string())?
        };
        #[cfg(unix)]
        let executable_identity = metadata_identity(
            &verified_program
                .metadata()
                .map_err(|_| "postgres-native-command-metadata-failed".to_string())?,
        );
        #[cfg(not(unix))]
        let executable_identity = String::new();
        let mut command = Command::new(program);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setpgid(0, 0) == -1 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut child = command
            .spawn()
            .map_err(|_| "postgres-native-command-spawn-failed".to_string())?;
        let started = Instant::now();
        loop {
            if child
                .try_wait()
                .map_err(|_| "postgres-native-command-wait-failed".to_string())?
                .is_some()
            {
                break;
            }
            if started.elapsed() >= timeout {
                #[cfg(unix)]
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err("postgres-native-command-timeout".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let output = child
            .wait_with_output()
            .map_err(|_| "postgres-native-command-output-failed".to_string())?;
        if canonical_regular_executable(program)?.1 != executable_identity {
            return Err("postgres-native-executable-changed".into());
        }
        if output.stdout.len() > MAX_OUTPUT_BYTES || output.stderr.len() > MAX_OUTPUT_BYTES {
            return Err("postgres-native-command-output-too-large".into());
        }
        Ok(CommandOutput {
            status: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8(output.stdout)
                .map_err(|_| "postgres-native-command-output-invalid".to_string())?,
            stderr: String::from_utf8(output.stderr)
                .map_err(|_| "postgres-native-command-output-invalid".to_string())?,
            executable_identity,
        })
    }

    fn pid_is_alive(&self, pid: u32) -> bool {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid as i32, 0) == 0
        }
        #[cfg(not(unix))]
        {
            let _ = pid;
            false
        }
    }
}

fn valid_database_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value
            .chars()
            .any(|character| character == '\0' || character.is_control())
}

#[cfg(unix)]
fn metadata_identity(metadata: &std::fs::Metadata) -> String {
    use std::os::unix::fs::MetadataExt;
    format!(
        "{}:{}:{}:{}",
        metadata.dev(),
        metadata.ino(),
        metadata.uid(),
        metadata.len()
    )
}

fn canonical_regular_executable(path: &Path) -> Result<(PathBuf, String), String> {
    if !path.is_absolute() {
        return Err("postgres-native-executable-not-absolute".into());
    }
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "postgres-native-executable-unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("postgres-native-executable-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o111 == 0 {
            return Err("postgres-native-executable-not-executable".into());
        }
        return Ok((
            std::fs::canonicalize(path)
                .map_err(|_| "postgres-native-executable-unavailable".to_string())?,
            metadata_identity(&metadata),
        ));
    }
    #[cfg(not(unix))]
    Err("postgres-test-cluster-reclaim-unsupported-platform".into())
}

fn directory_identity(path: &Path) -> Result<String, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "postgres-data-directory-unavailable".to_string())?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return Err("postgres-data-directory-unsafe".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        if metadata.uid() != unsafe { libc::geteuid() }
            || metadata.permissions().mode() & 0o077 != 0
        {
            return Err("postgres-data-directory-authority-unsafe".into());
        }
        return Ok(format!(
            "{}:{}:{}",
            metadata.dev(),
            metadata.ino(),
            metadata.uid()
        ));
    }
    #[cfg(not(unix))]
    Err("postgres-test-cluster-reclaim-unsupported-platform".into())
}

fn required_structure(data_directory: &Path) -> Result<(String, u64), String> {
    let version_path = data_directory.join("PG_VERSION");
    let version_metadata = std::fs::symlink_metadata(&version_path)
        .map_err(|_| "postgres-version-file-missing".to_string())?;
    if !version_metadata.is_file() || version_metadata.file_type().is_symlink() {
        return Err("postgres-version-file-unsafe".into());
    }
    for name in ["base", "global", "pg_wal"] {
        let metadata = std::fs::symlink_metadata(data_directory.join(name))
            .map_err(|_| "postgres-structure-incomplete".to_string())?;
        if !metadata.is_dir() || metadata.file_type().is_symlink() {
            return Err("postgres-structure-unsafe".into());
        }
    }
    let version = std::fs::read_to_string(version_path)
        .map_err(|_| "postgres-version-file-unreadable".to_string())?
        .trim()
        .to_string();
    if version.is_empty()
        || !version
            .chars()
            .all(|character| character.is_ascii_digit() || character == '.')
    {
        return Err("postgres-version-invalid".into());
    }
    Ok((version, allocated_bytes(data_directory)?))
}

#[cfg(unix)]
fn allocated_bytes(root: &Path) -> Result<u64, String> {
    use std::os::unix::fs::MetadataExt;
    let mut total = 0u64;
    let mut stack = vec![root.to_path_buf()];
    while let Some(path) = stack.pop() {
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "postgres-data-directory-traversal-failed".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("postgres-data-directory-symlink-found".into());
        }
        total = total
            .checked_add(metadata.blocks().saturating_mul(512))
            .ok_or_else(|| "postgres-allocated-size-overflow".to_string())?;
        if metadata.is_dir() {
            for entry in std::fs::read_dir(&path)
                .map_err(|_| "postgres-data-directory-traversal-failed".to_string())?
            {
                stack.push(
                    entry
                        .map_err(|_| "postgres-data-directory-traversal-failed".to_string())?
                        .path(),
                );
            }
        }
    }
    Ok(total)
}

#[cfg(not(unix))]
fn allocated_bytes(_root: &Path) -> Result<u64, String> {
    Err("postgres-test-cluster-reclaim-unsupported-platform".into())
}

struct PostmasterState {
    pid: u32,
    port: u16,
    socket_directory: String,
    identity: String,
}

fn postmaster_state(data_directory: &Path) -> Result<PostmasterState, String> {
    let path = data_directory.join("postmaster.pid");
    let metadata = std::fs::symlink_metadata(&path)
        .map_err(|_| "postgres-postmaster-pid-missing".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return Err("postgres-postmaster-pid-unsafe".into());
    }
    let contents = std::fs::read_to_string(path)
        .map_err(|_| "postgres-postmaster-pid-unreadable".to_string())?;
    let lines = contents.lines().collect::<Vec<_>>();
    if lines.len() < 8 || lines[7].trim() != "ready" {
        return Err("postgres-postmaster-not-ready".into());
    }
    if std::fs::canonicalize(lines[1])
        .map_err(|_| "postgres-postmaster-data-directory-unavailable".to_string())?
        != data_directory
    {
        return Err("postgres-postmaster-data-directory-mismatch".into());
    }
    let pid = lines[0]
        .parse::<u32>()
        .map_err(|_| "postgres-postmaster-pid-invalid".to_string())?;
    let port = lines[3]
        .parse::<u16>()
        .map_err(|_| "postgres-postmaster-port-invalid".to_string())?;
    let socket_directory = lines[4].to_string();
    if !Path::new(&socket_directory).is_absolute() {
        return Err("postgres-postmaster-socket-directory-unsafe".into());
    }
    let identity = hex_sha256(contents.as_bytes());
    Ok(PostmasterState {
        pid,
        port,
        socket_directory,
        identity,
    })
}

fn psql_args(state: &PostmasterState, user: &str, query: &str) -> Vec<String> {
    vec![
        "--no-psqlrc".into(),
        "--tuples-only".into(),
        "--no-align".into(),
        "--set".into(),
        "ON_ERROR_STOP=1".into(),
        "--host".into(),
        state.socket_directory.clone(),
        "--port".into(),
        state.port.to_string(),
        "--username".into(),
        user.into(),
        "--dbname".into(),
        "postgres".into(),
        "--command".into(),
        query.into(),
    ]
}

fn successful_stdout(
    runner: &impl PostgresCommandRunner,
    program: &Path,
    expected_identity: &str,
    args: &[String],
) -> Result<String, String> {
    let output = runner.run(program, args, COMMAND_TIMEOUT)?;
    if output.executable_identity != expected_identity {
        return Err("postgres-native-executable-changed".into());
    }
    if output.status != 0 {
        return Err("postgres-native-command-failed".into());
    }
    Ok(output.stdout)
}

fn hex_sha256(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn plan_fingerprint(plan: &PostgresTestClusterPlan) -> Result<String, String> {
    let mut value = plan.clone();
    value.observed_at_ms = 0;
    value.plan_fingerprint.clear();
    value.exact_approval_phrase.clear();
    serde_json::to_vec(&value)
        .map(|bytes| hex_sha256(&bytes))
        .map_err(|_| "postgres-plan-serialization-failed".into())
}

/// Builds a non-mutating plan only when every structural and live database gate is exact.
pub fn plan_with_runner(
    request: &PostgresTestClusterRequest,
    runner: &impl PostgresCommandRunner,
    observed_at_ms: u64,
) -> Result<PostgresTestClusterPlan, String> {
    if !request.data_directory.is_absolute() || request.database_user.trim().is_empty() {
        return Err("postgres-plan-input-invalid".into());
    }
    let expected = request
        .expected_databases
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    if expected.is_empty()
        || expected.len() != request.expected_databases.len()
        || expected
            .iter()
            .any(|name| !valid_database_name(name) || name == "postgres")
    {
        return Err("postgres-expected-database-allowlist-invalid".into());
    }
    let data_directory = std::fs::canonicalize(&request.data_directory)
        .map_err(|_| "postgres-data-directory-unavailable".to_string())?;
    let data_directory_identity = directory_identity(&data_directory)?;
    let (postgres_version, allocated_bytes) = required_structure(&data_directory)?;
    let state = postmaster_state(&data_directory)?;
    if !runner.pid_is_alive(state.pid) {
        return Err("postgres-postmaster-not-running".into());
    }
    let (psql_path, psql_identity) = canonical_regular_executable(&request.psql_path)?;
    let (pg_ctl_path, pg_ctl_identity) = canonical_regular_executable(&request.pg_ctl_path)?;
    let pg_ctl_status = successful_stdout(
        runner,
        &pg_ctl_path,
        &pg_ctl_identity,
        &[
            "-D".into(),
            data_directory.to_string_lossy().into_owned(),
            "status".into(),
        ],
    )?;
    if !pg_ctl_status.contains(&state.pid.to_string()) {
        return Err("postgres-pg-ctl-status-identity-mismatch".into());
    }
    let database_output = successful_stdout(
        runner,
        &psql_path,
        &psql_identity,
        &psql_args(
            &state,
            &request.database_user,
            "SELECT datname FROM pg_database WHERE NOT datistemplate AND datname <> 'postgres' ORDER BY datname;",
        ),
    )?;
    let observed_databases = database_output
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if observed_databases.iter().cloned().collect::<BTreeSet<_>>() != expected
        || observed_databases.len() != expected.len()
    {
        return Err("postgres-observed-database-allowlist-mismatch".into());
    }
    let client_output = successful_stdout(
        runner,
        &psql_path,
        &psql_identity,
        &psql_args(
            &state,
            &request.database_user,
            "SELECT count(*) FROM pg_stat_activity WHERE backend_type = 'client backend' AND pid <> pg_backend_pid();",
        ),
    )?;
    let external_client_count = client_output
        .trim()
        .parse::<u64>()
        .map_err(|_| "postgres-client-count-invalid".to_string())?;
    if external_client_count != 0 {
        return Err("postgres-external-clients-active".into());
    }
    let mut plan = PostgresTestClusterPlan {
        schema_version: SCHEMA_VERSION,
        data_directory: data_directory.to_string_lossy().into_owned(),
        data_directory_identity,
        postgres_version,
        postmaster_pid: state.pid,
        postmaster_identity: state.identity,
        port: state.port,
        socket_directory: state.socket_directory,
        database_user: request.database_user.clone(),
        expected_databases: expected.into_iter().collect(),
        observed_databases,
        external_client_count,
        psql_identity,
        pg_ctl_identity,
        pg_ctl_status_identity: hex_sha256(pg_ctl_status.as_bytes()),
        allocated_bytes,
        observed_at_ms,
        plan_fingerprint: String::new(),
        exact_approval_phrase: String::new(),
    };
    plan.plan_fingerprint = plan_fingerprint(&plan)?;
    plan.exact_approval_phrase = format!(
        "DiskSage PostgreSQL test cluster reclaim 승인 {}",
        plan.plan_fingerprint
    );
    Ok(plan)
}

fn free_bytes(path: &Path) -> Result<u64, String> {
    fs4::statvfs(path)
        .map(|stats| stats.available_space())
        .map_err(|_| "postgres-filesystem-capacity-unavailable".into())
}

fn operation_id(plan: &PostgresTestClusterPlan, now_ms: u64) -> String {
    hex_sha256(format!("{}\0{now_ms}", plan.plan_fingerprint).as_bytes())
}

/// Revalidates an approved plan, journals it, and removes only the exact stopped directory.
pub fn execute_with_runner(
    request: &PostgresTestClusterRequest,
    approved_plan: &PostgresTestClusterPlan,
    exact_approval_phrase: &str,
    record_directory: &Path,
    source_root: &Path,
    runner: &impl PostgresCommandRunner,
    now_ms: u64,
) -> Result<ExecutionEvidence, String> {
    if exact_approval_phrase != approved_plan.exact_approval_phrase
        || plan_fingerprint(approved_plan)? != approved_plan.plan_fingerprint
    {
        return Err("postgres-exact-approval-invalid".into());
    }
    let live_plan = plan_with_runner(request, runner, now_ms)?;
    if live_plan.plan_fingerprint != approved_plan.plan_fingerprint {
        return Err("postgres-plan-stale".into());
    }
    let operation_id = operation_id(approved_plan, now_ms);
    let data_directory = Path::new(&approved_plan.data_directory);
    let volume_root = data_directory
        .parent()
        .ok_or_else(|| "postgres-data-directory-parent-missing".to_string())?;
    let free_before = free_bytes(volume_root)?;
    let pending_value = PostgresTestClusterPendingJournal {
        schema_version: SCHEMA_VERSION,
        operation_id: operation_id.clone(),
        plan: approved_plan.clone(),
        approved_phrase: exact_approval_phrase.into(),
        written_at_ms: now_ms,
        state: "pending".into(),
    };
    let pending_path =
        record_directory.join(format!("postgres-reclaim-{operation_id}-pending.json"));
    let pending = crate::private_evidence::write_private_json_create_new(
        source_root,
        &pending_path,
        &pending_value,
    )?;
    let shutdown = runner.run(
        &request.pg_ctl_path,
        &[
            "-D".into(),
            approved_plan.data_directory.clone(),
            "-m".into(),
            "fast".into(),
            "-w".into(),
            "stop".into(),
        ],
        COMMAND_TIMEOUT,
    );
    let shutdown_completed = shutdown.as_ref().is_ok_and(|output| {
        output.status == 0 && output.executable_identity == approved_plan.pg_ctl_identity
    }) && !runner.pid_is_alive(approved_plan.postmaster_pid);
    let quarantine = volume_root.join(format!(".disksage-postgres-reclaim-{operation_id}"));
    let mut identity_still_matches = false;
    let directory_removed = if shutdown_completed
        && !quarantine.exists()
        && std::fs::rename(data_directory, &quarantine).is_ok()
    {
        identity_still_matches = directory_identity(&quarantine)
            .is_ok_and(|identity| identity == approved_plan.data_directory_identity);
        if identity_still_matches {
            std::fs::remove_dir_all(&quarantine).is_ok()
                && !quarantine.exists()
                && !data_directory.exists()
        } else {
            let _ = std::fs::rename(&quarantine, data_directory);
            false
        }
    } else {
        false
    };
    let free_after = free_bytes(volume_root).unwrap_or(free_before);
    let completed = shutdown_completed && directory_removed;
    let reason_code = if completed {
        "postgres-test-cluster-reclaimed"
    } else if !shutdown_completed {
        "postgres-shutdown-not-confirmed"
    } else if !identity_still_matches {
        "postgres-data-directory-identity-changed"
    } else {
        "postgres-data-directory-removal-failed"
    };
    let outcome = PostgresTestClusterResult {
        schema_version: SCHEMA_VERSION,
        operation_id: operation_id.clone(),
        plan_fingerprint: approved_plan.plan_fingerprint.clone(),
        completed,
        reason_code: reason_code.into(),
        shutdown_completed,
        directory_removed,
        free_bytes_before: free_before,
        free_bytes_after: free_after,
        physically_reclaimed_bytes: free_after.saturating_sub(free_before),
        completed_at_ms: now_ms,
    };
    let result_path = record_directory.join(format!("postgres-reclaim-{operation_id}-result.json"));
    let result = crate::private_evidence::write_private_json_create_new(
        source_root,
        &result_path,
        &outcome,
    )?;
    Ok(ExecutionEvidence {
        pending,
        result,
        outcome,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};
    use std::os::unix::fs::PermissionsExt;

    struct FakeRunner {
        alive: Cell<bool>,
        databases: RefCell<String>,
        clients: RefCell<String>,
        data_directory: PathBuf,
    }

    impl PostgresCommandRunner for FakeRunner {
        fn run(
            &self,
            program: &Path,
            args: &[String],
            _timeout: Duration,
        ) -> Result<CommandOutput, String> {
            let executable_identity = canonical_regular_executable(program)?.1;
            let command = args.last().map(String::as_str).unwrap_or_default();
            if command.contains("pg_database") {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: self.databases.borrow().clone(),
                    stderr: String::new(),
                    executable_identity: executable_identity.clone(),
                });
            }
            if command.contains("pg_stat_activity") {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: self.clients.borrow().clone(),
                    stderr: String::new(),
                    executable_identity: executable_identity.clone(),
                });
            }
            if args.last().map(String::as_str) == Some("status") {
                return Ok(CommandOutput {
                    status: 0,
                    stdout: "server is running (PID: 42)\n".into(),
                    stderr: String::new(),
                    executable_identity: executable_identity.clone(),
                });
            }
            if args.last().map(String::as_str) == Some("stop") {
                self.alive.set(false);
                let _ = std::fs::remove_file(self.data_directory.join("postmaster.pid"));
                return Ok(CommandOutput {
                    status: 0,
                    stdout: String::new(),
                    stderr: String::new(),
                    executable_identity,
                });
            }
            Err("unexpected-command".into())
        }

        fn pid_is_alive(&self, _pid: u32) -> bool {
            self.alive.get()
        }
    }

    fn fixture() -> (tempfile::TempDir, PostgresTestClusterRequest, FakeRunner) {
        let temp = tempfile::tempdir().unwrap();
        let data = temp.path().join("cluster");
        std::fs::create_dir(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["base", "global", "pg_wal"] {
            std::fs::create_dir(data.join(name)).unwrap();
        }
        std::fs::write(data.join("PG_VERSION"), "18\n").unwrap();
        std::fs::write(
            data.join("postmaster.pid"),
            format!(
                "42\n{}\n1\n5544\n{}\nlocalhost\n1 2\nready\n",
                data.display(),
                temp.path().display()
            ),
        )
        .unwrap();
        let psql = temp.path().join("psql");
        let pg_ctl = temp.path().join("pg_ctl");
        std::fs::write(&psql, "x").unwrap();
        std::fs::write(&pg_ctl, "x").unwrap();
        std::fs::set_permissions(&psql, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::set_permissions(&pg_ctl, std::fs::Permissions::from_mode(0o700)).unwrap();
        let request = PostgresTestClusterRequest {
            data_directory: data.clone(),
            psql_path: psql,
            pg_ctl_path: pg_ctl,
            database_user: "operator".into(),
            expected_databases: vec!["suite_test".into()],
        };
        let runner = FakeRunner {
            alive: Cell::new(true),
            databases: RefCell::new("suite_test\n".into()),
            clients: RefCell::new("0\n".into()),
            data_directory: data,
        };
        (temp, request, runner)
    }

    #[test]
    fn exact_allowlist_and_zero_clients_issue_a_plan() {
        let (_temp, request, runner) = fixture();
        let plan = plan_with_runner(&request, &runner, 7).unwrap();
        assert_eq!(plan.observed_databases, ["suite_test"]);
        assert_eq!(plan.external_client_count, 0);
        assert!(plan.exact_approval_phrase.ends_with(&plan.plan_fingerprint));
    }

    #[test]
    fn native_runner_executes_the_identity_checked_object() {
        let program = Path::new("/bin/echo");
        let expected_identity = canonical_regular_executable(program).unwrap().1;
        let output = NativePostgresCommandRunner
            .run(
                program,
                &["descriptor-bound".into()],
                Duration::from_secs(1),
            )
            .unwrap();
        assert_eq!(output.status, 0);
        assert_eq!(output.stdout, "descriptor-bound\n");
        assert_eq!(output.executable_identity, expected_identity);
    }

    #[test]
    fn mismatch_or_external_client_fails_closed() {
        let (_temp, request, runner) = fixture();
        *runner.databases.borrow_mut() = "other_test\n".into();
        assert_eq!(
            plan_with_runner(&request, &runner, 7).unwrap_err(),
            "postgres-observed-database-allowlist-mismatch"
        );
        *runner.databases.borrow_mut() = "suite_test\n".into();
        *runner.clients.borrow_mut() = "1\n".into();
        assert_eq!(
            plan_with_runner(&request, &runner, 7).unwrap_err(),
            "postgres-external-clients-active"
        );
    }

    #[test]
    fn execution_revalidates_journals_and_measures_delta() {
        let (temp, request, runner) = fixture();
        let plan = plan_with_runner(&request, &runner, 7).unwrap();
        let source = tempfile::tempdir().unwrap();
        let records = temp.path().join("records");
        std::fs::create_dir(&records).unwrap();
        std::fs::set_permissions(&records, std::fs::Permissions::from_mode(0o700)).unwrap();
        let evidence = execute_with_runner(
            &request,
            &plan,
            &plan.exact_approval_phrase,
            &records,
            source.path(),
            &runner,
            7,
        )
        .unwrap();
        assert!(evidence.outcome.completed);
        assert!(!request.data_directory.exists());
        assert!(evidence.pending.written && evidence.result.written);
        assert_eq!(std::fs::read_dir(records).unwrap().count(), 2);
    }
}
