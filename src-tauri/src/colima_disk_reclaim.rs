//! Fail-closed Colima virtual-disk reclaim planning.
//!
//! Colima currently documents guest `fstrim` while the VM is running, but exposes no native
//! stopped-VM host compaction command. DiskSage therefore inventories the profile and sparse disk
//! allocation read-only and emits an unavailable execution receipt. It never stops a VM, invokes
//! `qemu-img`, or edits/truncates/deletes a backing disk.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const COLIMA_DISK_RECLAIM_SCHEMA_VERSION: u32 = 1;
const APPROVAL_MAX_AGE_MS: u64 = 5 * 60 * 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColimaDiskReclaimPlan {
    pub schema_version: u32,
    pub profile: String,
    pub runtime_state: String,
    pub runtime: String,
    pub vm_type: String,
    pub configured_disk_bytes: u64,
    pub backing_disk_logical_bytes: u64,
    pub backing_disk_allocated_bytes: u64,
    pub backing_disk_identity: String,
    pub active_workload_evidence_complete: bool,
    pub active_workloads_present: Option<bool>,
    pub native_stopped_compaction_supported: bool,
    pub execution_available: bool,
    pub blockers: Vec<String>,
    pub customer_next_action: String,
    pub observed_at_ms: u64,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub mutation_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColimaDiskReclaimApproval {
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColimaDiskReclaimReceipt {
    pub schema_version: u32,
    pub profile: String,
    pub plan_fingerprint: String,
    pub executed_at_ms: u64,
    pub executed: bool,
    pub physically_reclaimed_bytes: Option<u64>,
    pub outcome: String,
    pub customer_next_action: String,
}

#[derive(Debug, Deserialize)]
struct ColimaInstance {
    #[serde(rename = "name", alias = "Name")]
    name: String,
    #[serde(rename = "status", alias = "Status")]
    status: String,
    #[serde(default, rename = "runtime", alias = "Runtime")]
    runtime: String,
    #[serde(default, rename = "vmType", alias = "VMType")]
    vm_type: String,
    #[serde(default, rename = "disk", alias = "Disk")]
    disk: u64,
}

pub trait ColimaCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], timeout: Duration) -> Result<String, String>;
}

pub struct NativeColimaRunner;

impl ColimaCommandRunner for NativeColimaRunner {
    fn run(&self, executable: &Path, args: &[&str], timeout: Duration) -> Result<String, String> {
        crate::podman_reclaim::run_bounded_provider_text(
            executable,
            args,
            timeout,
            "colima-read-only-probe",
        )
    }
}

fn valid_profile(profile: &str) -> bool {
    !profile.is_empty()
        && profile.len() <= 128
        && profile != "."
        && profile != ".."
        && !profile.starts_with('-')
        && profile
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}

fn lima_instance_name(profile: &str) -> String {
    if profile == "default" {
        "colima".into()
    } else {
        format!("colima-{profile}")
    }
}

fn backing_disk_path(colima_home: &Path, profile: &str) -> Result<PathBuf, String> {
    if !valid_profile(profile) || !colima_home.is_absolute() {
        return Err("colima-profile-or-home-invalid".into());
    }
    let home_meta = std::fs::symlink_metadata(colima_home)
        .map_err(|_| "colima-home-unavailable".to_string())?;
    if home_meta.file_type().is_symlink() || !home_meta.is_dir() {
        return Err("colima-home-not-trusted-directory".into());
    }
    let lima_root = colima_home.join("_lima");
    let lima_root_meta = std::fs::symlink_metadata(&lima_root)
        .map_err(|_| "colima-lima-storage-unavailable".to_string())?;
    if lima_root_meta.file_type().is_symlink() || !lima_root_meta.is_dir() {
        return Err("colima-lima-storage-not-trusted-directory".into());
    }
    let instance = lima_root.join(lima_instance_name(profile));
    let instance_meta = std::fs::symlink_metadata(&instance)
        .map_err(|_| "colima-profile-storage-unavailable".to_string())?;
    if instance_meta.file_type().is_symlink() || !instance_meta.is_dir() {
        return Err("colima-profile-storage-not-trusted-directory".into());
    }
    let disk = instance.join("diffdisk");
    let disk_meta = std::fs::symlink_metadata(&disk)
        .map_err(|_| "colima-backing-disk-unavailable".to_string())?;
    if disk_meta.file_type().is_symlink() || !disk_meta.is_file() {
        return Err("colima-backing-disk-not-regular".into());
    }
    Ok(disk)
}

#[cfg(unix)]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt;
    metadata.blocks().saturating_mul(512)
}

#[cfg(not(unix))]
fn allocated_bytes(metadata: &Metadata) -> u64 {
    metadata.len()
}

fn disk_identity(metadata: &Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        return format!("dev:{}:ino:{}", metadata.dev(), metadata.ino());
    }
    #[cfg(not(unix))]
    format!("len:{}", metadata.len())
}

fn frame(hasher: &mut Sha256, value: &str) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value.as_bytes());
}

fn fingerprint(plan: &ColimaDiskReclaimPlan) -> String {
    let mut hash = Sha256::new();
    frame(&mut hash, "disksage.colima-disk-reclaim-plan/v1");
    frame(&mut hash, &plan.profile);
    frame(&mut hash, &plan.runtime_state);
    frame(&mut hash, &plan.runtime);
    frame(&mut hash, &plan.vm_type);
    frame(&mut hash, &plan.configured_disk_bytes.to_string());
    frame(&mut hash, &plan.backing_disk_logical_bytes.to_string());
    frame(&mut hash, &plan.backing_disk_allocated_bytes.to_string());
    frame(&mut hash, &plan.backing_disk_identity);
    frame(
        &mut hash,
        &plan.active_workload_evidence_complete.to_string(),
    );
    frame(&mut hash, &format!("{:?}", plan.active_workloads_present));
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn plan_with_runner(
    runner: &dyn ColimaCommandRunner,
    colima_bin: &Path,
    colima_home: &Path,
    profile: &str,
    observed_at_ms: u64,
) -> Result<ColimaDiskReclaimPlan, String> {
    let colima_meta = std::fs::symlink_metadata(colima_bin)
        .map_err(|_| "colima-executable-unavailable".to_string())?;
    if !colima_bin.is_absolute() || colima_meta.file_type().is_symlink() || !colima_meta.is_file() {
        return Err("colima-executable-unavailable".into());
    }
    let output = runner.run(
        colima_bin,
        &["list", "--json", "--profile", profile],
        Duration::from_secs(10),
    )?;
    let instances = output
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            serde_json::from_str::<ColimaInstance>(line)
                .map_err(|_| "colima-list-json-invalid".to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    let expected_name = lima_instance_name(profile);
    let instance = instances
        .into_iter()
        .find(|item| item.name == expected_name || item.name == profile)
        .ok_or_else(|| "colima-profile-not-found".to_string())?;
    let disk = backing_disk_path(colima_home, profile)?;
    let metadata = std::fs::symlink_metadata(&disk)
        .map_err(|_| "colima-backing-disk-unavailable".to_string())?;
    let stopped = instance.status.eq_ignore_ascii_case("stopped");
    let mut blockers = Vec::new();
    if !stopped {
        blockers.push("colima-profile-must-already-be-stopped".into());
    }
    // The current official Colima CLI documents only guest fstrim, which requires a running VM.
    // It has no stopped-VM native compact command, so execution remains truthfully unavailable.
    blockers.push("colima-native-stopped-compaction-unavailable".into());
    let mut plan = ColimaDiskReclaimPlan {
        schema_version: COLIMA_DISK_RECLAIM_SCHEMA_VERSION,
        profile: profile.into(),
        runtime_state: instance.status,
        runtime: instance.runtime,
        vm_type: instance.vm_type,
        configured_disk_bytes: instance.disk,
        backing_disk_logical_bytes: metadata.len(),
        backing_disk_allocated_bytes: allocated_bytes(&metadata),
        backing_disk_identity: disk_identity(&metadata),
        active_workload_evidence_complete: stopped,
        active_workloads_present: stopped.then_some(false),
        native_stopped_compaction_supported: false,
        execution_available: false,
        blockers,
        customer_next_action: if stopped {
            "현재 Colima는 중지되어 있습니다. 지원되는 native 압축 기능이 추가될 때까지 디스크 파일을 직접 변경하지 마세요.".into()
        } else {
            "실행 중인 작업을 확인한 뒤 Colima를 직접 중지하고 다시 검사하세요.".into()
        },
        observed_at_ms,
        plan_fingerprint: String::new(),
        exact_approval_phrase: String::new(),
        mutation_performed: false,
    };
    plan.plan_fingerprint = fingerprint(&plan);
    plan.exact_approval_phrase =
        format!("DiskSage Colima 디스크 회수 승인 {}", plan.plan_fingerprint);
    Ok(plan)
}

pub fn execute_unavailable(
    plan: &ColimaDiskReclaimPlan,
    approval: &ColimaDiskReclaimApproval,
    executed_at_ms: u64,
) -> Result<ColimaDiskReclaimReceipt, String> {
    if approval.plan_fingerprint != plan.plan_fingerprint
        || approval.exact_approval_phrase != plan.exact_approval_phrase
        || plan.schema_version != COLIMA_DISK_RECLAIM_SCHEMA_VERSION
        || executed_at_ms < plan.observed_at_ms
        || executed_at_ms.saturating_sub(plan.observed_at_ms) > APPROVAL_MAX_AGE_MS
        || executed_at_ms < approval.approved_at_ms
        || executed_at_ms.saturating_sub(approval.approved_at_ms) > APPROVAL_MAX_AGE_MS
        || !approval.approved_by.starts_with("human:")
        || approval.rationale.trim().is_empty()
    {
        return Err("colima-disk-reclaim-approval-invalid-or-stale".into());
    }
    Ok(ColimaDiskReclaimReceipt {
        schema_version: COLIMA_DISK_RECLAIM_SCHEMA_VERSION,
        profile: plan.profile.clone(),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        executed_at_ms,
        executed: false,
        physically_reclaimed_bytes: None,
        outcome: "native-stopped-compaction-unavailable".into(),
        customer_next_action: "Colima가 공식 stopped-VM 압축 명령을 제공할 때까지 backing disk를 직접 변경하지 마세요.".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;

    struct FakeRunner(String);
    impl ColimaCommandRunner for FakeRunner {
        fn run(&self, _: &Path, args: &[&str], _: Duration) -> Result<String, String> {
            assert_eq!(args, ["list", "--json", "--profile", "work"]);
            Ok(self.0.clone())
        }
    }

    #[test]
    fn stopped_profile_reports_truthful_native_compaction_unavailable() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".colima");
        let instance = home.join("_lima/colima-work");
        std::fs::create_dir_all(&instance).unwrap();
        File::create(instance.join("diffdisk"))
            .unwrap()
            .set_len(1024 * 1024)
            .unwrap();
        let bin = temp.path().join("colima");
        File::create(&bin).unwrap();
        let runner = FakeRunner("{\"name\":\"colima-work\",\"status\":\"Stopped\",\"runtime\":\"docker\",\"vmType\":\"vz\",\"disk\":107374182400}\n".into());
        let plan = plan_with_runner(&runner, &bin, &home, "work", 100).unwrap();
        assert!(plan.active_workload_evidence_complete);
        assert_eq!(plan.active_workloads_present, Some(false));
        assert!(!plan.execution_available);
        assert_eq!(plan.backing_disk_logical_bytes, 1024 * 1024);
        assert!(plan
            .blockers
            .contains(&"colima-native-stopped-compaction-unavailable".into()));
        assert_eq!(plan.plan_fingerprint.len(), 64);
    }

    #[cfg(unix)]
    #[test]
    fn native_boundary_accepts_realistic_fake_colima_cli() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".colima");
        let instance = home.join("_lima/colima-work");
        std::fs::create_dir_all(&instance).unwrap();
        File::create(instance.join("diffdisk"))
            .unwrap()
            .set_len(4096)
            .unwrap();
        let bin = temp.path().join("colima");
        std::fs::write(
            &bin,
            "#!/bin/sh\n[ \"$1 $2 $3 $4\" = \"list --json --profile work\" ] || exit 64\nprintf '%s\\n' '{\"name\":\"colima-work\",\"status\":\"Stopped\",\"runtime\":\"docker\",\"vmType\":\"qemu\",\"disk\":107374182400}'\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&bin).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&bin, permissions).unwrap();

        let reviewed = plan_with_runner(&NativeColimaRunner, &bin, &home, "work", 100).unwrap();
        assert_eq!(reviewed.vm_type, "qemu");
        assert_eq!(reviewed.runtime_state, "Stopped");
        assert!(!reviewed.execution_available);

        // A later CLI invocation re-observes the provider. The evidence fingerprint remains
        // reviewable when the bound profile, runtime, allocation, and disk identity are unchanged.
        let live = plan_with_runner(&NativeColimaRunner, &bin, &home, "work", 200).unwrap();
        assert_eq!(live.plan_fingerprint, reviewed.plan_fingerprint);
        let approval = ColimaDiskReclaimApproval {
            plan_fingerprint: reviewed.plan_fingerprint,
            exact_approval_phrase: reviewed.exact_approval_phrase,
            approved_at_ms: 200,
            approved_by: "human:operator".into(),
            rationale: "reviewed the exact stopped profile and backing disk".into(),
        };
        let receipt = execute_unavailable(&live, &approval, 200).unwrap();
        assert!(!receipt.executed);
        assert_eq!(receipt.outcome, "native-stopped-compaction-unavailable");
    }

    #[test]
    fn macos_profile_path_contract_rejects_traversal_and_symlink_disk() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".colima");
        std::fs::create_dir_all(home.join("_lima/colima-work")).unwrap();
        assert_eq!(
            backing_disk_path(&home, "../work").unwrap_err(),
            "colima-profile-or-home-invalid"
        );
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(
                temp.path().join("outside"),
                home.join("_lima/colima-work/diffdisk"),
            )
            .unwrap();
            assert_eq!(
                backing_disk_path(&home, "work").unwrap_err(),
                "colima-backing-disk-not-regular"
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn intermediate_lima_symlink_cannot_redirect_backing_disk_authority() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join(".colima");
        let outside = temp.path().join("outside/colima-work");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&outside).unwrap();
        File::create(outside.join("diffdisk")).unwrap();
        std::os::unix::fs::symlink(temp.path().join("outside"), home.join("_lima")).unwrap();

        assert_eq!(
            backing_disk_path(&home, "work").unwrap_err(),
            "colima-lima-storage-not-trusted-directory"
        );
    }

    #[test]
    fn execution_requires_fresh_attributed_exact_approval_and_never_mutates() {
        let plan = ColimaDiskReclaimPlan {
            schema_version: 1,
            profile: "default".into(),
            runtime_state: "Stopped".into(),
            runtime: "docker".into(),
            vm_type: "vz".into(),
            configured_disk_bytes: 1,
            backing_disk_logical_bytes: 1,
            backing_disk_allocated_bytes: 1,
            backing_disk_identity: "dev:1:ino:2".into(),
            active_workload_evidence_complete: true,
            active_workloads_present: Some(false),
            native_stopped_compaction_supported: false,
            execution_available: false,
            blockers: vec!["colima-native-stopped-compaction-unavailable".into()],
            customer_next_action: "wait".into(),
            observed_at_ms: 10,
            plan_fingerprint: "a".repeat(64),
            exact_approval_phrase: format!("DiskSage Colima 디스크 회수 승인 {}", "a".repeat(64)),
            mutation_performed: false,
        };
        let approval = ColimaDiskReclaimApproval {
            plan_fingerprint: plan.plan_fingerprint.clone(),
            exact_approval_phrase: plan.exact_approval_phrase.clone(),
            approved_at_ms: 20,
            approved_by: "human:operator".into(),
            rationale: "reviewed stopped profile and exact disk identity".into(),
        };
        let receipt = execute_unavailable(&plan, &approval, 21).unwrap();
        assert!(!receipt.executed);
        assert_eq!(receipt.physically_reclaimed_bytes, None);
    }
}
