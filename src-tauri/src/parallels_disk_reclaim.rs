use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_ENTRIES: usize = 100_000;

pub trait ParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String>;
}

pub struct ProcessParallelsCommandRunner;

impl ParallelsCommandRunner for ProcessParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String> {
        crate::podman_reclaim::run_bounded_provider_text(executable, args, COMMAND_TIMEOUT, label)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParallelsDiskReclaimPlan {
    pub schema_kind: &'static str,
    pub schema_version: u32,
    pub vm_id: String,
    pub vm_name: String,
    pub vm_status: String,
    pub bundle_path: String,
    pub disk_path: String,
    pub bundle_identity: String,
    pub disk_identity: String,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub reclaimable_bytes: Option<u64>,
    pub observed_at_ms: u64,
    pub active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
    pub blockers: Vec<String>,
    pub execution_available: bool,
    pub next_action: String,
    pub plan_fingerprint: String,
}

#[derive(Debug, Deserialize)]
struct VmRecord {
    #[serde(rename = "ID", alias = "id", alias = "Uuid", alias = "uuid")]
    id: String,
    #[serde(rename = "Name", alias = "name")]
    name: String,
    #[serde(rename = "Status", alias = "status", alias = "State", alias = "state")]
    status: String,
}

fn trusted_executable(path: &Path) -> bool {
    path.is_absolute()
        && std::fs::symlink_metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && !metadata.file_type().is_symlink())
}

fn path_has_symlink(path: &Path) -> bool {
    let mut current = Some(path);
    while let Some(candidate) = current {
        if std::fs::symlink_metadata(candidate)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return true;
        }
        current = candidate.parent();
    }
    false
}

fn identity(_path: &Path, metadata: &std::fs::Metadata) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        format!("{}:{}:{}", metadata.dev(), metadata.ino(), metadata.len())
    }
    #[cfg(not(unix))]
    format!("{}:{}", _path.display(), metadata.len())
}

fn tree_allocation(root: &Path) -> Result<(u64, u64), String> {
    let mut stack = vec![root.to_path_buf()];
    let mut logical = 0_u64;
    let mut physical = 0_u64;
    let mut visited = 0_usize;
    while let Some(path) = stack.pop() {
        visited += 1;
        if visited > MAX_ENTRIES {
            return Err("parallels-disk-entry-limit".into());
        }
        let metadata = std::fs::symlink_metadata(&path)
            .map_err(|_| "parallels-disk-entry-unavailable".to_string())?;
        if metadata.file_type().is_symlink() {
            return Err("parallels-disk-symlink-rejected".into());
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            physical = physical.saturating_add(metadata.blocks().saturating_mul(512));
        }
        #[cfg(not(unix))]
        {
            physical = physical.saturating_add(metadata.len());
        }
        if metadata.is_file() {
            logical = logical.saturating_add(metadata.len());
        } else if metadata.is_dir() {
            let mut children = std::fs::read_dir(&path)
                .map_err(|_| "parallels-disk-directory-unreadable".to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "parallels-disk-entry-unavailable".to_string())?;
            children.sort_by_key(|entry| entry.file_name());
            stack.extend(children.into_iter().rev().map(|entry| entry.path()));
        } else {
            return Err("parallels-disk-non-regular-entry".into());
        }
    }
    Ok((logical, physical))
}

fn compactable_bytes(output: &str) -> Result<u64, String> {
    let mut block_size = None;
    let mut allocated = None;
    let mut used = None;
    for line in output.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let parsed = value.trim().parse::<u64>().ok();
        match key.trim() {
            "Block size" => block_size = parsed,
            "Allocated blocks" => allocated = parsed,
            "Used blocks" => used = parsed,
            _ => {}
        }
    }
    let (block_size, allocated, used) = (block_size, allocated, used);
    let (Some(block_size), Some(allocated), Some(used)) = (block_size, allocated, used) else {
        return Err("parallels-compact-info-incomplete".into());
    };
    if used > allocated || block_size == 0 {
        return Err("parallels-compact-info-invalid".into());
    }
    allocated
        .checked_sub(used)
        .and_then(|blocks| blocks.checked_mul(block_size))
        .and_then(|sectors| sectors.checked_mul(512))
        .ok_or_else(|| "parallels-compact-info-overflow".into())
}

fn fingerprint(values: &[&str], numbers: &[u64]) -> String {
    let mut hash = Sha256::new();
    hash.update(b"disksage.parallels-disk-reclaim-plan/v1");
    for value in values {
        hash.update((value.len() as u64).to_be_bytes());
        hash.update(value.as_bytes());
    }
    for number in numbers {
        hash.update(number.to_be_bytes());
    }
    hash.finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn plan_with_runner(
    runner: &dyn ParallelsCommandRunner,
    prlctl: &Path,
    disk_tool: &Path,
    vm_id: &str,
    bundle: &Path,
    disk: &Path,
    observed_at_ms: u64,
    active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
) -> Result<ParallelsDiskReclaimPlan, String> {
    if !trusted_executable(prlctl) || !trusted_executable(disk_tool) {
        return Err("parallels-command-unavailable-or-untrusted".into());
    }
    if path_has_symlink(bundle) || path_has_symlink(disk) {
        return Err("parallels-symlink-path-rejected".into());
    }
    let bundle = std::fs::canonicalize(bundle).map_err(|_| "parallels-bundle-unavailable")?;
    let disk = std::fs::canonicalize(disk).map_err(|_| "parallels-disk-unavailable")?;
    if !disk.starts_with(&bundle) || bundle.extension().and_then(|v| v.to_str()) != Some("pvm") {
        return Err("parallels-disk-outside-bundle".into());
    }
    if bundle
        .components()
        .any(|component| matches!(component, std::path::Component::ParentDir))
        || bundle.to_string_lossy().contains("/Library/CloudStorage/")
        || bundle
            .to_string_lossy()
            .contains("/Library/Mobile Documents/")
    {
        return Err("parallels-provider-or-path-boundary-rejected".into());
    }
    let bundle_meta =
        std::fs::symlink_metadata(&bundle).map_err(|_| "parallels-bundle-unavailable")?;
    let disk_meta = std::fs::symlink_metadata(&disk).map_err(|_| "parallels-disk-unavailable")?;
    if !bundle_meta.is_dir() || !(disk_meta.is_dir() || disk_meta.is_file()) {
        return Err("parallels-bundle-or-disk-kind-invalid".into());
    }
    let list = runner.run(prlctl, &["list", "-a", "-j"], "parallels-list")?;
    let records: Vec<VmRecord> =
        serde_json::from_str(&list).map_err(|_| "parallels-list-json-invalid".to_string())?;
    let vm = records
        .iter()
        .find(|vm| vm.id == vm_id)
        .ok_or_else(|| "parallels-vm-not-found".to_string())?;
    let info = runner.run(
        disk_tool,
        &[
            "compact",
            "--info",
            "--details",
            "--hdd",
            disk.to_str().ok_or("parallels-disk-path-not-utf8")?,
        ],
        "parallels-compact-info",
    )?;
    let reclaimable = compactable_bytes(&info)?;
    let (logical, physical) = tree_allocation(&disk)?;
    let bundle_identity = identity(&bundle, &bundle_meta);
    let disk_identity = identity(&disk, &disk_meta);
    let mut blockers = Vec::new();
    if !vm.status.eq_ignore_ascii_case("stopped") {
        blockers.push("parallels-vm-must-be-stopped".into());
    }
    if !active_use.assessed || !active_use.evidence_complete || active_use.active {
        blockers.push("parallels-bundle-active-use-unresolved".into());
    }
    blockers.push("parallels-compact-execution-not-implemented".into());
    let fp = fingerprint(
        &[
            vm_id,
            &vm.name,
            &vm.status,
            &bundle_identity,
            &disk_identity,
        ],
        &[logical, physical, reclaimable],
    );
    let next_action = if reclaimable == 0 {
        "확보 가능한 공간이 없습니다. VM을 그대로 유지하세요."
    } else {
        "예상 확보 공간을 확인했습니다. 현재 버전은 실행하지 않으므로 VM을 그대로 유지하세요."
    };
    Ok(ParallelsDiskReclaimPlan {
        schema_kind: "disksage.parallels-disk-reclaim-plan/v1",
        schema_version: 1,
        vm_id: vm.id.clone(),
        vm_name: vm.name.clone(),
        vm_status: vm.status.clone(),
        bundle_path: bundle.to_string_lossy().into_owned(),
        disk_path: disk.to_string_lossy().into_owned(),
        bundle_identity,
        disk_identity,
        logical_bytes: logical,
        physical_bytes: physical,
        reclaimable_bytes: Some(reclaimable),
        observed_at_ms,
        active_use,
        blockers,
        execution_available: false,
        next_action: next_action.into(),
        plan_fingerprint: fp,
    })
}

pub fn plan(
    prlctl: &Path,
    disk_tool: &Path,
    vm_id: &str,
    bundle: &Path,
    disk: &Path,
    observed_at_ms: u64,
) -> Result<ParallelsDiskReclaimPlan, String> {
    let active = crate::git_worktree::active_use_evidence(bundle, 30_000, 256, true);
    plan_with_runner(
        &ProcessParallelsCommandRunner,
        prlctl,
        disk_tool,
        vm_id,
        bundle,
        disk,
        observed_at_ms,
        active,
    )
}
