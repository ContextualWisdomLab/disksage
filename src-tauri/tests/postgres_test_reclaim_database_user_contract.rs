#![cfg(unix)]

use disksage_lib::postgres_test_reclaim::{
    plan_with_runner, CommandOutput, PostgresCommandRunner, PostgresTestClusterRequest,
};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct PanicRunner;

impl PostgresCommandRunner for PanicRunner {
    fn run(
        &self,
        _program: &Path,
        _args: &[String],
        _timeout: Duration,
    ) -> Result<CommandOutput, String> {
        panic!("invalid database_user must be rejected before native command execution")
    }

    fn pid_is_alive(&self, _pid: u32) -> bool {
        panic!("invalid database_user must be rejected before process inspection")
    }
}

#[test]
fn control_characters_in_database_user_fail_closed_before_filesystem_or_native_evidence() {
    let request = PostgresTestClusterRequest {
        data_directory: PathBuf::from("/definitely-not-a-disksage-postgres-cluster"),
        psql_path: PathBuf::from("/definitely-not-psql"),
        pg_ctl_path: PathBuf::from("/definitely-not-pg-ctl"),
        database_user: "operator\nforged-audit-line".into(),
        expected_databases: vec!["suite_test".into()],
    };

    assert_eq!(
        plan_with_runner(&request, &PanicRunner, 7).unwrap_err(),
        "postgres-plan-input-invalid"
    );
}
