//! Dry-run-first CLI for evidence-bound PostgreSQL test-cluster reclamation.

use disksage_lib::postgres_test_reclaim::{
    execute_with_runner, plan_with_runner, NativePostgresCommandRunner, PostgresTestClusterRequest,
};
use serde::Serialize;
use std::ffi::OsString;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const USAGE: &str = "Usage: disksage-postgres-test-reclaim --data-directory ABSOLUTE_PATH --psql-path ABSOLUTE_PATH --pg-ctl-path ABSOLUTE_PATH --database-user USER --expected-database NAME [--expected-database NAME...] --record-directory PRIVATE_ABSOLUTE_PATH [--execute --approved-plan-fingerprint HEX64 --exact-approval-phrase TEXT]\nDefault mode only writes a private plan. Execution requires the exact fingerprint and approval phrase printed by that plan.";

struct Args {
    request: PostgresTestClusterRequest,
    record_directory: PathBuf,
    execute: bool,
    approved_plan_fingerprint: Option<String>,
    exact_approval_phrase: Option<String>,
}

#[derive(Serialize)]
struct PublicOutput {
    mode: &'static str,
    plan_fingerprint: String,
    exact_approval_phrase: String,
    private_evidence: disksage_lib::private_evidence::PrivateEvidenceReceipt,
    completed: Option<bool>,
    physically_reclaimed_bytes: Option<u64>,
    reason_code: Option<String>,
}

fn take_value(args: &mut impl Iterator<Item = OsString>, option: &str) -> Result<OsString, String> {
    args.next()
        .ok_or_else(|| format!("{option} requires a value"))
}

fn parse_args(raw: impl IntoIterator<Item = OsString>) -> Result<Option<Args>, String> {
    let raw = raw.into_iter().collect::<Vec<_>>();
    if raw.len() == 1 && matches!(raw[0].to_str(), Some("-h" | "--help")) {
        return Ok(None);
    }

    let mut data_directory = None;
    let mut psql_path = None;
    let mut pg_ctl_path = None;
    let mut database_user = None;
    let mut expected_databases = Vec::new();
    let mut record_directory = None;
    let mut execute = false;
    let mut approved_plan_fingerprint = None;
    let mut exact_approval_phrase = None;
    let mut args = raw.into_iter();
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--data-directory") if data_directory.is_none() => {
                data_directory = Some(PathBuf::from(take_value(&mut args, "--data-directory")?))
            }
            Some("--psql-path") if psql_path.is_none() => {
                psql_path = Some(PathBuf::from(take_value(&mut args, "--psql-path")?))
            }
            Some("--pg-ctl-path") if pg_ctl_path.is_none() => {
                pg_ctl_path = Some(PathBuf::from(take_value(&mut args, "--pg-ctl-path")?))
            }
            Some("--database-user") if database_user.is_none() => {
                database_user = Some(
                    take_value(&mut args, "--database-user")?
                        .into_string()
                        .map_err(|_| "--database-user must be UTF-8")?,
                )
            }
            Some("--expected-database") => expected_databases.push(
                take_value(&mut args, "--expected-database")?
                    .into_string()
                    .map_err(|_| "--expected-database must be UTF-8")?,
            ),
            Some("--record-directory") if record_directory.is_none() => {
                record_directory = Some(PathBuf::from(take_value(&mut args, "--record-directory")?))
            }
            Some("--execute") if !execute => execute = true,
            Some("--approved-plan-fingerprint") if approved_plan_fingerprint.is_none() => {
                approved_plan_fingerprint = Some(
                    take_value(&mut args, "--approved-plan-fingerprint")?
                        .into_string()
                        .map_err(|_| "--approved-plan-fingerprint must be UTF-8")?,
                )
            }
            Some("--exact-approval-phrase") if exact_approval_phrase.is_none() => {
                exact_approval_phrase = Some(
                    take_value(&mut args, "--exact-approval-phrase")?
                        .into_string()
                        .map_err(|_| "--exact-approval-phrase must be UTF-8")?,
                )
            }
            _ => return Err(format!("unknown argument\n{USAGE}")),
        }
    }
    if execute != (approved_plan_fingerprint.is_some() && exact_approval_phrase.is_some()) {
        return Err(format!(
            "execution options must be supplied together\n{USAGE}"
        ));
    }
    let record_directory =
        record_directory.ok_or_else(|| format!("--record-directory is required\n{USAGE}"))?;
    if !record_directory.is_absolute() {
        return Err("--record-directory must be absolute".into());
    }
    Ok(Some(Args {
        request: PostgresTestClusterRequest {
            data_directory: data_directory
                .ok_or_else(|| format!("--data-directory is required\n{USAGE}"))?,
            psql_path: psql_path.ok_or_else(|| format!("--psql-path is required\n{USAGE}"))?,
            pg_ctl_path: pg_ctl_path
                .ok_or_else(|| format!("--pg-ctl-path is required\n{USAGE}"))?,
            database_user: database_user
                .ok_or_else(|| format!("--database-user is required\n{USAGE}"))?,
            expected_databases,
        },
        record_directory,
        execute,
        approved_plan_fingerprint,
        exact_approval_phrase,
    }))
}

fn now_ms() -> Result<u64, String> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .map_err(|_| "system-clock-before-unix-epoch".into())
}

fn run(raw: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let Some(args) = parse_args(raw)? else {
        println!("{USAGE}");
        return Ok(());
    };
    let now = now_ms()?;
    let runner = NativePostgresCommandRunner;
    let plan = plan_with_runner(&args.request, &runner, now)?;
    let source_root = std::env::current_exe()
        .map_err(|_| "postgres-cli-executable-unavailable")?
        .parent()
        .ok_or("postgres-cli-executable-parent-missing")?
        .to_path_buf();
    let output = if args.execute {
        if args.approved_plan_fingerprint.as_deref() != Some(plan.plan_fingerprint.as_str()) {
            return Err("postgres-approved-plan-fingerprint-stale".into());
        }
        let evidence = execute_with_runner(
            &args.request,
            &plan,
            args.exact_approval_phrase.as_deref().unwrap_or_default(),
            &args.record_directory,
            &source_root,
            &runner,
            now,
        )?;
        PublicOutput {
            mode: "execute",
            plan_fingerprint: plan.plan_fingerprint,
            exact_approval_phrase: plan.exact_approval_phrase,
            private_evidence: evidence.result,
            completed: Some(evidence.outcome.completed),
            physically_reclaimed_bytes: evidence.outcome.physically_reclaimed_bytes,
            reason_code: Some(evidence.outcome.reason_code),
        }
    } else {
        let path = args.record_directory.join(format!(
            "postgres-reclaim-{}-{now}-plan.json",
            plan.plan_fingerprint
        ));
        let receipt = disksage_lib::private_evidence::write_private_json_create_new(
            &source_root,
            &path,
            &plan,
        )?;
        PublicOutput {
            mode: "plan",
            plan_fingerprint: plan.plan_fingerprint,
            exact_approval_phrase: plan.exact_approval_phrase,
            private_evidence: receipt,
            completed: None,
            physically_reclaimed_bytes: None,
            reason_code: None,
        }
    };
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|_| "postgres-public-output-invalid")?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run(std::env::args_os().skip(1)) {
        eprintln!("disksage-postgres-test-reclaim: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_to_plan_and_requires_complete_execute_authority() {
        let parsed = parse_args(
            [
                "--data-directory",
                "/tmp/pg",
                "--psql-path",
                "/bin/psql",
                "--pg-ctl-path",
                "/bin/pg_ctl",
                "--database-user",
                "operator",
                "--expected-database",
                "suite_test",
                "--record-directory",
                "/tmp/private",
            ]
            .map(OsString::from),
        )
        .unwrap()
        .unwrap();
        assert!(!parsed.execute);
        assert_eq!(parsed.request.expected_databases, ["suite_test"]);
        assert!(parse_args(["--execute"].map(OsString::from)).is_err());
        assert!(parse_args(
            [
                "--data-directory",
                "/tmp/one",
                "--data-directory",
                "/tmp/two",
            ]
            .map(OsString::from)
        )
        .is_err());
    }

    #[test]
    fn help_is_only_valid_as_the_sole_argument() {
        assert!(parse_args(["--help"].map(OsString::from)).unwrap().is_none());
        assert!(parse_args(["-h"].map(OsString::from)).unwrap().is_none());
        assert!(parse_args(["--help", "--execute"].map(OsString::from)).is_err());
        assert!(parse_args(["--execute", "--help"].map(OsString::from)).is_err());
    }

    #[test]
    fn public_output_contains_no_local_path_field() {
        let json = serde_json::to_string(&PublicOutput {
            mode: "plan",
            plan_fingerprint: "a".repeat(64),
            exact_approval_phrase: format!(
                "DiskSage PostgreSQL test cluster reclaim 승인 {}",
                "a".repeat(64)
            ),
            private_evidence: disksage_lib::private_evidence::PrivateEvidenceReceipt {
                written: true,
                sha256: "b".repeat(64),
                bytes: 1,
                unix_mode: "0600".into(),
                create_new: true,
                contains_sensitive_local_paths: true,
                is_approval: false,
            },
            completed: None,
            physically_reclaimed_bytes: None,
            reason_code: None,
        })
        .unwrap();
        assert!(!json.contains("data_directory"));
        assert!(!json.contains("record_directory"));
    }
}
