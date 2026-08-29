use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const TREE_ALLOCATION_BUDGET: Duration = Duration::from_secs(5);
const MAX_ENTRIES: usize = 100_000;
const CLI_FLAGS: [&str; 3] = ["--vm-id", "--bundle", "--disk"];
const PRLCTL_PATH: &str = "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl";
const DISK_TOOL_PATH: &str = "/Applications/Parallels Desktop.app/Contents/MacOS/prl_disk_tool";

pub trait ParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String>;
}

pub struct ProcessParallelsCommandRunner;

impl ParallelsCommandRunner for ProcessParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String> {
        crate::podman_reclaim::run_bounded_provider_text(executable, args, COMMAND_TIMEOUT, label)
    }
}

/// Fails closed when the opt-in Parallels CLI is built or invoked on an unsupported host.
pub fn enforce_cli_platform() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Err("Parallels 디스크 회수 계획은 macOS에서만 지원합니다.".into())
    }
}

/// Rejects unknown positional/flag tokens before the Parallels planner CLI reads values.
///
/// Duplicate or missing recognized flags are still diagnosed by the CLI's value extractor; this
/// admission guard prevents misspelled or trailing options from being silently ignored.
pub fn validate_cli_argument_tokens(args: &[String]) -> Result<(), String> {
    let mut index = 0_usize;
    while index < args.len() {
        let flag = &args[index];
        if !CLI_FLAGS.contains(&flag.as_str()) {
            return Err(format!("지원하지 않는 인자가 있습니다: {flag}"));
        }
        let Some(value) = args.get(index + 1) else {
            return Err(format!("{flag} 값을 지정하세요."));
        };
        if value.starts_with('-') {
            return Err(format!("{flag} 값을 지정하세요."));
        }
        index += 2;
    }
    Ok(())
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
    #[serde(rename = "Home", alias = "home")]
    home: String,
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

fn canonical_bundle_for_active_use(bundle: &Path) -> Result<PathBuf, String> {
    if path_has_symlink(bundle) {
        return Err("parallels-symlink-path-rejected".into());
    }
    let bundle =
        std::fs::canonicalize(bundle).map_err(|_| "parallels-bundle-unavailable".to_string())?;
    if bundle.extension().and_then(|value| value.to_str()) != Some("pvm")
        || bundle.to_string_lossy().contains("/Library/CloudStorage/")
        || bundle
            .to_string_lossy()
            .contains("/Library/Mobile Documents/")
    {
        return Err("parallels-provider-or-path-boundary-rejected".into());
    }
    Ok(bundle)
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
    tree_allocation_with_limits(root, MAX_ENTRIES, TREE_ALLOCATION_BUDGET)
}

fn tree_allocation_with_limits(
    root: &Path,
    max_entries: usize,
    time_budget: Duration,
) -> Result<(u64, u64), String> {
    let started = Instant::now();
    let mut stack = vec![root.to_path_buf()];
    let mut logical = 0_u64;
    let mut physical = 0_u64;
    let mut visited = 0_usize;
    while let Some(path) = stack.pop() {
        if started.elapsed() >= time_budget {
            return Err("parallels-disk-scan-timeout".into());
        }
        visited += 1;
        if visited > max_entries {
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
    if path_has_symlink(disk) {
        return Err("parallels-symlink-path-rejected".into());
    }
    let bundle = canonical_bundle_for_active_use(bundle)?;
    let disk = std::fs::canonicalize(disk).map_err(|_| "parallels-disk-unavailable")?;
    if !disk.starts_with(&bundle) {
        return Err("parallels-disk-outside-bundle".into());
    }
    let bundle_meta =
        std::fs::symlink_metadata(&bundle).map_err(|_| "parallels-bundle-unavailable")?;
    let disk_meta = std::fs::symlink_metadata(&disk).map_err(|_| "parallels-disk-unavailable")?;
    if !bundle_meta.is_dir() || !(disk_meta.is_dir() || disk_meta.is_file()) {
        return Err("parallels-bundle-or-disk-kind-invalid".into());
    }
    // Detailed JSON is required because the inventory Home field is the authoritative
    // registration boundary that binds a stopped VM identity to the requested .pvm bundle.
    let list = runner.run(prlctl, &["list", "-a", "-i", "-j"], "parallels-list")?;
    let records: Vec<VmRecord> =
        serde_json::from_str(&list).map_err(|_| "parallels-list-json-invalid".to_string())?;
    let vm = records
        .iter()
        .find(|vm| vm.id == vm_id)
        .ok_or_else(|| "parallels-vm-not-found".to_string())?;
    let registered_bundle = std::fs::canonicalize(Path::new(&vm.home))
        .map_err(|_| "parallels-vm-home-unavailable".to_string())?;
    if registered_bundle
        .extension()
        .and_then(|value| value.to_str())
        != Some("pvm")
        || registered_bundle != bundle
    {
        return Err("parallels-vm-bundle-mismatch".into());
    }
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
    let active_pids = active_use
        .observed_pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let blockers_fingerprint = blockers.join("\n");
    let fp = fingerprint(
        &[
            vm_id,
            &vm.name,
            &vm.status,
            &bundle_identity,
            &disk_identity,
            &active_use.method,
            active_use.error.as_deref().unwrap_or(""),
            &active_pids,
            &blockers_fingerprint,
        ],
        &[
            logical,
            physical,
            reclaimable,
            observed_at_ms,
            active_use.assessed as u64,
            active_use.evidence_complete as u64,
            active_use.active as u64,
            active_use.results_truncated as u64,
        ],
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
    vm_id: &str,
    bundle: &Path,
    disk: &Path,
    observed_at_ms: u64,
) -> Result<ParallelsDiskReclaimPlan, String> {
    let canonical_bundle = canonical_bundle_for_active_use(bundle)?;
    let active = crate::git_worktree::active_use_evidence(&canonical_bundle, 30_000, 256, true);
    plan_with_runner(
        &ProcessParallelsCommandRunner,
        Path::new(PRLCTL_PATH),
        Path::new(DISK_TOOL_PATH),
        vm_id,
        &canonical_bundle,
        disk,
        observed_at_ms,
        active,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_use_bundle_path_is_absolute_before_probe() {
        let temp = tempfile::Builder::new()
            .prefix(".parallels-relative-test-")
            .tempdir_in(".")
            .unwrap();
        let relative = PathBuf::from(temp.path().file_name().unwrap()).join("Relative.pvm");
        std::fs::create_dir_all(&relative).unwrap();

        let canonical = canonical_bundle_for_active_use(&relative).unwrap();

        assert!(canonical.is_absolute());
        assert_eq!(canonical, std::fs::canonicalize(&relative).unwrap());
    }

    #[test]
    fn allocation_walk_fails_closed_when_time_budget_is_exhausted() {
        let temp = tempfile::tempdir().unwrap();
        let disk = temp.path().join("Disk.hdd");
        std::fs::create_dir_all(&disk).unwrap();
        std::fs::write(disk.join("descriptor.xml"), b"descriptor").unwrap();

        let error = tree_allocation_with_limits(&disk, MAX_ENTRIES, Duration::ZERO).unwrap_err();

        assert_eq!(error, "parallels-disk-scan-timeout");
    }
}
