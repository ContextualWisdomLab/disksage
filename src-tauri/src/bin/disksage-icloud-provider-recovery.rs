//! Plan or execute one evidence-bound graceful iCloud File Provider daemon restart.

use disksage_lib::icloud_provider_recovery::{
    execute_icloud_file_provider_recovery, observe_icloud_file_provider_daemon,
    plan_icloud_file_provider_recovery, IcloudFileProviderRecoveryPlan,
};
use disksage_lib::icloud_sync_health::{
    default_cloud_docs_db_dir, health_evidence_snapshot_from_report, probe_icloud_sync_health,
};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, PartialEq, Eq)]
struct Args {
    db_dir: PathBuf,
    output: Option<PathBuf>,
    execute_plan: Option<PathBuf>,
    confirmation: Option<String>,
    rationale: Option<String>,
}

fn parse_args(raw: &[String], home: &Path) -> Result<Args, String> {
    let mut args = Args {
        db_dir: default_cloud_docs_db_dir(home),
        output: None,
        execute_plan: None,
        confirmation: None,
        rationale: None,
    };
    let mut index = 0;
    while index < raw.len() {
        let flag = &raw[index];
        index += 1;
        let value = raw
            .get(index)
            .ok_or_else(|| format!("{flag} requires a value"))?;
        match flag.as_str() {
            "--db-dir" if args.db_dir == default_cloud_docs_db_dir(home) => {
                args.db_dir = PathBuf::from(value)
            }
            "--execute-plan" if args.execute_plan.is_none() => {
                args.execute_plan = Some(PathBuf::from(value))
            }
            "--output" if args.output.is_none() => args.output = Some(PathBuf::from(value)),
            "--confirm" if args.confirmation.is_none() => args.confirmation = Some(value.clone()),
            "--rationale" if args.rationale.is_none() => args.rationale = Some(value.clone()),
            _ => return Err("icloud-recovery-argument-invalid".into()),
        }
        index += 1;
    }
    if !args.db_dir.is_absolute()
        || args
            .execute_plan
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
        || args.output.as_ref().is_some_and(|path| !path.is_absolute())
        || (args.execute_plan.is_some() && args.output.is_some())
        || (args.execute_plan.is_some()
            != (args.confirmation.is_some() && args.rationale.is_some()))
    {
        return Err("icloud-recovery-argument-invalid".into());
    }
    Ok(args)
}

fn now_ms() -> Result<u64, String> {
    let value = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| "system-clock-before-unix-epoch".to_string())?
        .as_millis();
    u64::try_from(value).map_err(|_| "system-time-overflow".into())
}

#[cfg(target_os = "macos")]
fn current_user_uid() -> Result<u32, String> {
    Ok(unsafe { libc::getuid() })
}

#[cfg(not(target_os = "macos"))]
fn current_user_uid() -> Result<u32, String> {
    Err("icloud-recovery-platform-unsupported".into())
}

fn read_plan_with_hook<F>(
    path: &Path,
    before_read: F,
) -> Result<IcloudFileProviderRecoveryPlan, String>
where
    F: FnOnce(),
{
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "icloud-recovery-plan-unavailable".to_string())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || metadata.len() > 64 * 1024 {
        return Err("icloud-recovery-plan-unsafe".into());
    }
    before_read();
    serde_json::from_slice(
        &std::fs::read(path).map_err(|_| "icloud-recovery-plan-read-failed".to_string())?,
    )
    .map_err(|_| "icloud-recovery-plan-json-invalid".into())
}

fn read_plan(path: &Path) -> Result<IcloudFileProviderRecoveryPlan, String> {
    read_plan_with_hook(path, || {})
}

fn run() -> Result<(), String> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "HOME is unavailable".to_string())?;
    let args = parse_args(&std::env::args().skip(1).collect::<Vec<_>>(), &home)?;
    let now = now_ms()?;
    let health =
        health_evidence_snapshot_from_report(&probe_icloud_sync_health(&args.db_dir, now)?)?;
    let daemon = observe_icloud_file_provider_daemon()?;
    let output = if let Some(path) = args.execute_plan.as_deref() {
        serde_json::to_value(execute_icloud_file_provider_recovery(
            &read_plan(path)?,
            &health,
            now,
            args.confirmation.as_deref().unwrap_or_default(),
            args.rationale.as_deref().unwrap_or_default(),
        )?)
    } else {
        serde_json::to_value(plan_icloud_file_provider_recovery(
            &health,
            daemon,
            current_user_uid()?,
            now,
        ))
    }
    .map_err(|_| "icloud-recovery-output-json-invalid".to_string())?;
    let encoded = serde_json::to_vec_pretty(&output)
        .map_err(|_| "icloud-recovery-output-json-invalid".to_string())?;
    if let Some(path) = args.output.as_deref() {
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        use std::io::Write;
        let mut file = options
            .open(path)
            .map_err(|_| "icloud-recovery-output-create-failed".to_string())?;
        file.write_all(&encoded)
            .and_then(|_| file.sync_all())
            .map_err(|_| "icloud-recovery-output-write-failed".to_string())?;
    }
    println!("{}", String::from_utf8_lossy(&encoded));
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage iCloud provider recovery: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn execution_requires_absolute_plan_confirmation_and_rationale_together() {
        let home = Path::new("/home/test");
        assert!(parse_args(&[], home).is_ok());
        assert!(parse_args(&["--execute-plan".into(), "relative".into()], home).is_err());
        assert!(parse_args(
            &[
                "--execute-plan".into(),
                "/tmp/plan.json".into(),
                "--confirm".into(),
                "phrase".into(),
                "--rationale".into(),
                "reason".into(),
            ],
            home
        )
        .is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn execution_plan_read_rejects_path_replacement_after_admission() {
        use disksage_lib::icloud_provider_recovery::IcloudFileProviderDaemonIdentity;

        fn plan(observed_at_ms: u64) -> IcloudFileProviderRecoveryPlan {
            IcloudFileProviderRecoveryPlan {
                schema_version: 1,
                observed_at_ms,
                health_evidence_fingerprint_sha256: "a".repeat(64),
                stale_error_count: 1,
                oldest_stale_error_age_ms: 15 * 60 * 1_000,
                daemon: IcloudFileProviderDaemonIdentity {
                    uid: 501,
                    pid: 1234,
                    service_label: "com.apple.FileProvider".into(),
                    executable_path: "/System/Library/Frameworks/FileProvider.framework/Support/fileproviderd".into(),
                    executable_object_id: "b".repeat(64),
                    apple_signature_valid: true,
                },
                blockers: Vec::new(),
                eligible: true,
                plan_fingerprint_sha256: "c".repeat(64),
                exact_approval_phrase: "test approval".into(),
                mutation_performed: false,
            }
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let plan_path = temp.path().join("plan.json");
        let replacement_path = temp.path().join("replacement.json");
        let admitted_path = temp.path().join("admitted.json");
        std::fs::write(
            &plan_path,
            serde_json::to_vec(&plan(1)).expect("serialize admitted plan"),
        )
        .expect("write admitted plan");
        std::fs::write(
            &replacement_path,
            serde_json::to_vec(&plan(2)).expect("serialize replacement plan"),
        )
        .expect("write replacement plan");

        let error = read_plan_with_hook(&plan_path, || {
            std::fs::rename(&plan_path, &admitted_path).expect("move admitted object");
            std::fs::rename(&replacement_path, &plan_path).expect("install replacement object");
        })
        .expect_err("path replacement must not switch execution-plan authority");

        assert_eq!(error, "icloud-recovery-plan-object-changed");
    }
}
