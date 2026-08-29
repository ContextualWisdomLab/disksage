use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const COMPACT_TIMEOUT: Duration = Duration::from_secs(7 * 24 * 60 * 60);
const TREE_ALLOCATION_BUDGET: Duration = Duration::from_secs(5);
const MAX_ENTRIES: usize = 100_000;
const MAX_APPROVAL_AGE_MS: u64 = 5 * 60 * 1_000;
const CLI_FLAGS: [&str; 8] = [
    "--vm-id",
    "--bundle",
    "--disk",
    "--approved-plan",
    "--confirm",
    "--approved-by",
    "--rationale",
    "--record-dir",
];
const PRLCTL_PATH: &str = "/Applications/Parallels Desktop.app/Contents/MacOS/prlctl";
const DISK_TOOL_PATH: &str = "/Applications/Parallels Desktop.app/Contents/MacOS/prl_disk_tool";

pub trait ParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String>;

    /// Allows deterministic fake executables in tests. Production uses fixed vendor paths only.
    fn permits_injected_executables(&self) -> bool {
        false
    }

    /// Runs the mutating compact operation under a mutation-specific policy.
    fn run_compact(&self, executable: &Path, args: &[&str]) -> Result<String, String> {
        self.run(executable, args, "parallels-compact-execute")
    }
}

pub struct ProcessParallelsCommandRunner;

impl ParallelsCommandRunner for ProcessParallelsCommandRunner {
    fn run(&self, executable: &Path, args: &[&str], label: &str) -> Result<String, String> {
        crate::podman_reclaim::run_bounded_provider_text(executable, args, COMMAND_TIMEOUT, label)
    }

    fn run_compact(&self, executable: &Path, args: &[&str]) -> Result<String, String> {
        crate::podman_reclaim::run_bounded_provider_text(
            executable,
            args,
            COMPACT_TIMEOUT,
            "parallels-compact-execute",
        )
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelsDiskReclaimPlan {
    pub schema_kind: String,
    pub schema_version: u32,
    pub vm_id: String,
    pub vm_name: String,
    pub vm_status: String,
    pub prlctl_identity: String,
    pub disk_tool_identity: String,
    pub bundle_path: String,
    pub disk_path: String,
    pub bundle_identity: String,
    pub disk_identity: String,
    pub logical_bytes: u64,
    pub physical_bytes: u64,
    pub reclaimable_bytes: Option<u64>,
    pub snapshots_absent: bool,
    pub observed_at_ms: u64,
    pub active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
    pub blockers: Vec<String>,
    pub execution_available: bool,
    pub next_action: String,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelsDiskReclaimApproval {
    pub schema_version: u32,
    pub approval_id: String,
    pub plan_fingerprint: String,
    pub exact_approval_phrase: String,
    pub approved_at_ms: u64,
    pub approved_by: String,
    pub rationale: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParallelsDiskReclaimResult {
    pub schema_version: u32,
    pub result_id: String,
    pub plan_fingerprint: String,
    pub approval_id: String,
    pub executed_at_ms: u64,
    pub command: Vec<String>,
    pub physical_bytes_before: u64,
    pub physical_bytes_after: Option<u64>,
    pub observed_physical_reduction_bytes: Option<u64>,
    pub volume_available_bytes_before: Option<u64>,
    pub volume_available_bytes_after: Option<u64>,
    pub observed_volume_available_gain_bytes: Option<u64>,
    pub execution_succeeded: bool,
    pub execution_error: Option<String>,
    pub verification_complete: bool,
    pub verification_blockers: Vec<String>,
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

fn trusted_vendor_executables(
    runner: &dyn ParallelsCommandRunner,
    prlctl: &Path,
    disk_tool: &Path,
) -> bool {
    trusted_executable(prlctl)
        && trusted_executable(disk_tool)
        && !path_has_symlink(prlctl)
        && !path_has_symlink(disk_tool)
        && (runner.permits_injected_executables()
            || (prlctl == Path::new(PRLCTL_PATH) && disk_tool == Path::new(DISK_TOOL_PATH)))
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
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        let report = crate::cloud::discover_cloud_roots_report(&home);
        let overlaps_root = report.roots.iter().any(|root| {
            std::fs::canonicalize(&root.path).is_ok_and(|root| bundle.starts_with(root))
        });
        let overlaps_incomplete_root = report
            .issues
            .iter()
            .any(|issue| bundle.starts_with(Path::new(&issue.path)));
        if overlaps_root || overlaps_incomplete_root {
            return Err("parallels-provider-or-path-boundary-rejected".into());
        }
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
            let mut children = Vec::new();
            for child in std::fs::read_dir(&path)
                .map_err(|_| "parallels-disk-directory-unreadable".to_string())?
            {
                if started.elapsed() >= time_budget {
                    return Err("parallels-disk-scan-timeout".into());
                }
                if visited
                    .saturating_add(stack.len())
                    .saturating_add(children.len())
                    >= max_entries
                {
                    return Err("parallels-disk-entry-limit".into());
                }
                children.push(child.map_err(|_| "parallels-disk-entry-unavailable".to_string())?);
            }
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
            "Block size"
                if block_size
                    .replace(parsed.ok_or("parallels-compact-info-invalid")?)
                    .is_some() =>
            {
                return Err("parallels-compact-info-duplicate-field".into());
            }
            "Allocated blocks"
                if allocated
                    .replace(parsed.ok_or("parallels-compact-info-invalid")?)
                    .is_some() =>
            {
                return Err("parallels-compact-info-duplicate-field".into());
            }
            "Used blocks"
                if used
                    .replace(parsed.ok_or("parallels-compact-info-invalid")?)
                    .is_some() =>
            {
                return Err("parallels-compact-info-duplicate-field".into());
            }
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

fn snapshots_absent(output: &str) -> Result<bool, String> {
    let value: serde_json::Value =
        serde_json::from_str(output).map_err(|_| "parallels-snapshot-json-invalid".to_string())?;
    match value {
        serde_json::Value::Array(items) => Ok(items.is_empty()),
        serde_json::Value::Object(mut object) => {
            let snapshots = object
                .remove("Snapshots")
                .or_else(|| object.remove("snapshots"))
                .ok_or_else(|| "parallels-snapshot-json-shape-unknown".to_string())?;
            snapshots
                .as_array()
                .map(|items| items.is_empty())
                .ok_or_else(|| "parallels-snapshot-json-shape-unknown".into())
        }
        _ => Err("parallels-snapshot-json-shape-unknown".into()),
    }
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

fn blocked_plan(
    vm: &VmRecord,
    bundle: &Path,
    disk: &Path,
    bundle_meta: &std::fs::Metadata,
    disk_meta: &std::fs::Metadata,
    prlctl_identity: String,
    disk_tool_identity: String,
    observed_at_ms: u64,
    active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
    blockers: Vec<String>,
) -> Result<ParallelsDiskReclaimPlan, String> {
    let (logical_bytes, physical_bytes) = tree_allocation(disk)?;
    let bundle_identity = identity(bundle, bundle_meta);
    let disk_identity = identity(disk, disk_meta);
    let active_pids = active_use
        .observed_pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let blocker_text = blockers.join("\n");
    let plan_fingerprint = fingerprint(
        &[
            &vm.id,
            &vm.name,
            &vm.status,
            &prlctl_identity,
            &disk_tool_identity,
            &bundle.to_string_lossy(),
            &disk.to_string_lossy(),
            &bundle_identity,
            &disk_identity,
            &active_use.method,
            active_use.error.as_deref().unwrap_or(""),
            &active_pids,
            &blocker_text,
        ],
        &[
            logical_bytes,
            physical_bytes,
            observed_at_ms,
            active_use.assessed as u64,
            active_use.evidence_complete as u64,
            active_use.active as u64,
            active_use.results_truncated as u64,
        ],
    );
    Ok(ParallelsDiskReclaimPlan {
        schema_kind: "disksage.parallels-disk-reclaim-plan/v1".into(),
        schema_version: 1,
        vm_id: vm.id.clone(),
        vm_name: vm.name.clone(),
        vm_status: vm.status.clone(),
        prlctl_identity,
        disk_tool_identity,
        bundle_path: bundle.to_string_lossy().into_owned(),
        disk_path: disk.to_string_lossy().into_owned(),
        bundle_identity,
        disk_identity,
        logical_bytes,
        physical_bytes,
        reclaimable_bytes: None,
        snapshots_absent: false,
        observed_at_ms,
        active_use,
        blockers,
        execution_available: false,
        next_action: "VM을 완전히 종료하고 사용 중인 앱을 닫은 뒤 다시 검사하세요.".into(),
        plan_fingerprint,
        exact_approval_phrase: None,
    })
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
    if !trusted_vendor_executables(runner, prlctl, disk_tool) {
        return Err("parallels-command-unavailable-or-untrusted".into());
    }
    let prlctl_identity = identity(
        prlctl,
        &std::fs::symlink_metadata(prlctl)
            .map_err(|_| "parallels-command-unavailable-or-untrusted")?,
    );
    let disk_tool_identity = identity(
        disk_tool,
        &std::fs::symlink_metadata(disk_tool)
            .map_err(|_| "parallels-command-unavailable-or-untrusted")?,
    );
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
    let mut blockers = Vec::new();
    if !vm.status.eq_ignore_ascii_case("stopped") {
        blockers.push("parallels-vm-must-be-stopped".into());
    }
    if !active_use.assessed || !active_use.evidence_complete || active_use.active {
        blockers.push("parallels-bundle-active-use-unresolved".into());
    }
    if !blockers.is_empty() {
        return Ok(blocked_plan(
            vm,
            &bundle,
            &disk,
            &bundle_meta,
            &disk_meta,
            prlctl_identity,
            disk_tool_identity,
            observed_at_ms,
            active_use,
            blockers,
        )?);
    }
    let snapshot_output = runner.run(
        prlctl,
        &["snapshot-list", vm_id, "--json"],
        "parallels-snapshot-list",
    )?;
    let no_snapshots = snapshots_absent(&snapshot_output)?;
    if !no_snapshots {
        blockers.push("parallels-snapshots-must-be-removed".into());
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
            &prlctl_identity,
            &disk_tool_identity,
            &bundle.to_string_lossy(),
            &disk.to_string_lossy(),
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
            no_snapshots as u64,
        ],
    );
    let execution_available = blockers.is_empty() && reclaimable > 0;
    let exact_approval_phrase =
        execution_available.then(|| format!("DiskSage Parallels compact 승인 {fp}"));
    let next_action = if !no_snapshots {
        "Parallels에서 필요한 상태를 백업하고 보존할 스냅샷을 확인하세요. 압축하려면 사용자가 직접 스냅샷을 정리한 뒤 다시 검사하세요. DiskSage는 스냅샷을 삭제하지 않습니다."
    } else if reclaimable == 0 {
        "확보 가능한 공간이 없습니다. VM을 그대로 유지하세요."
    } else {
        "예상 확보 공간과 안전 조건을 확인했습니다. 승인 문구를 검토한 뒤 압축을 실행하세요."
    };
    Ok(ParallelsDiskReclaimPlan {
        schema_kind: "disksage.parallels-disk-reclaim-plan/v1".into(),
        schema_version: 1,
        vm_id: vm.id.clone(),
        vm_name: vm.name.clone(),
        vm_status: vm.status.clone(),
        prlctl_identity,
        disk_tool_identity,
        bundle_path: bundle.to_string_lossy().into_owned(),
        disk_path: disk.to_string_lossy().into_owned(),
        bundle_identity,
        disk_identity,
        logical_bytes: logical,
        physical_bytes: physical,
        reclaimable_bytes: Some(reclaimable),
        snapshots_absent: no_snapshots,
        observed_at_ms,
        active_use,
        blockers,
        execution_available,
        next_action: next_action.into(),
        plan_fingerprint: fp,
        exact_approval_phrase,
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

fn approval_id(
    plan_fingerprint: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> String {
    fingerprint(
        &[plan_fingerprint, approved_by, rationale],
        &[approved_at_ms],
    )
}

fn executable_plan_fingerprint(plan: &ParallelsDiskReclaimPlan) -> Option<String> {
    let reclaimable = plan.reclaimable_bytes?;
    let active_pids = plan
        .active_use
        .observed_pids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let blockers = plan.blockers.join("\n");
    Some(fingerprint(
        &[
            &plan.vm_id,
            &plan.vm_name,
            &plan.vm_status,
            &plan.prlctl_identity,
            &plan.disk_tool_identity,
            &plan.bundle_path,
            &plan.disk_path,
            &plan.bundle_identity,
            &plan.disk_identity,
            &plan.active_use.method,
            plan.active_use.error.as_deref().unwrap_or(""),
            &active_pids,
            &blockers,
        ],
        &[
            plan.logical_bytes,
            plan.physical_bytes,
            reclaimable,
            plan.observed_at_ms,
            plan.active_use.assessed as u64,
            plan.active_use.evidence_complete as u64,
            plan.active_use.active as u64,
            plan.active_use.results_truncated as u64,
            plan.snapshots_absent as u64,
        ],
    ))
}

pub fn approve(
    plan: &ParallelsDiskReclaimPlan,
    confirmation_phrase: &str,
    approved_at_ms: u64,
    approved_by: &str,
    rationale: &str,
) -> Result<ParallelsDiskReclaimApproval, String> {
    if plan.schema_kind != "disksage.parallels-disk-reclaim-plan/v1"
        || plan.schema_version != 1
        || executable_plan_fingerprint(plan).as_deref() != Some(&plan.plan_fingerprint)
    {
        return Err("parallels-plan-integrity-mismatch".into());
    }
    let expected = plan
        .exact_approval_phrase
        .as_deref()
        .ok_or_else(|| "parallels-plan-not-executable".to_string())?;
    if !plan.execution_available
        || !plan.blockers.is_empty()
        || confirmation_phrase != expected
        || expected != format!("DiskSage Parallels compact 승인 {}", plan.plan_fingerprint)
    {
        return Err("parallels-approval-phrase-mismatch".into());
    }
    if approved_at_ms < plan.observed_at_ms
        || approved_at_ms.saturating_sub(plan.observed_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("parallels-plan-stale".into());
    }
    crate::cloud_review::validate_review_attribution(approved_by, rationale)
        .map_err(|_| "parallels-review-attribution-invalid".to_string())?;
    let approved_by = approved_by.trim();
    let rationale = rationale.trim();
    Ok(ParallelsDiskReclaimApproval {
        schema_version: 1,
        approval_id: approval_id(
            &plan.plan_fingerprint,
            approved_at_ms,
            approved_by,
            rationale,
        ),
        plan_fingerprint: plan.plan_fingerprint.clone(),
        exact_approval_phrase: expected.into(),
        approved_at_ms,
        approved_by: approved_by.into(),
        rationale: rationale.into(),
    })
}

fn validate_approval(
    plan: &ParallelsDiskReclaimPlan,
    approval: &ParallelsDiskReclaimApproval,
    confirmation_phrase: &str,
    executed_at_ms: u64,
) -> Result<(), String> {
    if approval.schema_version != 1
        || approval.plan_fingerprint != plan.plan_fingerprint
        || approval.exact_approval_phrase != confirmation_phrase
        || plan.exact_approval_phrase.as_deref() != Some(confirmation_phrase)
        || approval.approval_id
            != approval_id(
                &approval.plan_fingerprint,
                approval.approved_at_ms,
                &approval.approved_by,
                &approval.rationale,
            )
    {
        return Err("parallels-approval-integrity-mismatch".into());
    }
    if executed_at_ms < approval.approved_at_ms
        || executed_at_ms.saturating_sub(approval.approved_at_ms) > MAX_APPROVAL_AGE_MS
    {
        return Err("parallels-approval-expired".into());
    }
    crate::cloud_review::validate_review_attribution(&approval.approved_by, &approval.rationale)
        .map_err(|_| "parallels-review-attribution-invalid".to_string())
}

fn same_execution_object(
    approved: &ParallelsDiskReclaimPlan,
    live: &ParallelsDiskReclaimPlan,
) -> bool {
    live.execution_available
        && live.blockers.is_empty()
        && approved.vm_id == live.vm_id
        && approved.vm_status == live.vm_status
        && approved.prlctl_identity == live.prlctl_identity
        && approved.disk_tool_identity == live.disk_tool_identity
        && approved.bundle_path == live.bundle_path
        && approved.disk_path == live.disk_path
        && approved.bundle_identity == live.bundle_identity
        && approved.disk_identity == live.disk_identity
        && approved.logical_bytes == live.logical_bytes
        && approved.physical_bytes == live.physical_bytes
        && approved.reclaimable_bytes == live.reclaimable_bytes
        && approved.snapshots_absent
        && live.snapshots_absent
        && live.active_use.assessed
        && live.active_use.evidence_complete
        && !live.active_use.active
        && !live.active_use.results_truncated
}

pub fn execute_with_runner(
    runner: &dyn ParallelsCommandRunner,
    prlctl: &Path,
    disk_tool: &Path,
    approved_plan: &ParallelsDiskReclaimPlan,
    approval: &ParallelsDiskReclaimApproval,
    confirmation_phrase: &str,
    executed_at_ms: u64,
    active_use: crate::git_worktree::GitWorktreeActiveUseEvidence,
) -> Result<ParallelsDiskReclaimResult, String> {
    validate_approval(approved_plan, approval, confirmation_phrase, executed_at_ms)?;
    let live = plan_with_runner(
        runner,
        prlctl,
        disk_tool,
        &approved_plan.vm_id,
        Path::new(&approved_plan.bundle_path),
        Path::new(&approved_plan.disk_path),
        executed_at_ms,
        active_use,
    )?;
    if !same_execution_object(approved_plan, &live) {
        return Err("parallels-live-evidence-changed".into());
    }
    let disk = Path::new(&approved_plan.disk_path);
    let volume_before = crate::volume_pressure::snapshot_volume(disk, executed_at_ms)
        .ok()
        .map(|snapshot| snapshot.available_bytes);
    let live_disk_tool_identity = identity(
        disk_tool,
        &std::fs::symlink_metadata(disk_tool)
            .map_err(|_| "parallels-command-unavailable-or-untrusted")?,
    );
    if live_disk_tool_identity != approved_plan.disk_tool_identity
        || !trusted_vendor_executables(runner, prlctl, disk_tool)
    {
        return Err("parallels-command-identity-changed".into());
    }
    let execution_error = runner
        .run_compact(
            disk_tool,
            &[
                "compact",
                "-hdd",
                disk.to_str().ok_or("parallels-disk-path-not-utf8")?,
            ],
        )
        .err();
    let physical_bytes_after = tree_allocation(disk).ok().map(|(_, physical)| physical);
    let observed_at_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(executed_at_ms);
    let volume_after = crate::volume_pressure::snapshot_volume(disk, observed_at_ms)
        .ok()
        .map(|snapshot| snapshot.available_bytes);
    let physical_reduction =
        physical_bytes_after.map(|after| approved_plan.physical_bytes.saturating_sub(after));
    let volume_gain = volume_before
        .zip(volume_after)
        .and_then(|(before, after)| after.checked_sub(before));
    let mut verification_blockers = Vec::new();
    if let Some(error) = &execution_error {
        verification_blockers.push(format!("parallels-compact-command-failed: {error}"));
    }
    if physical_bytes_after.is_none() {
        verification_blockers.push("parallels-post-compact-allocation-unavailable".into());
    }
    let blocker_text = verification_blockers.join("\n");
    let result_id = fingerprint(
        &[
            &approved_plan.plan_fingerprint,
            &approval.approval_id,
            &approved_plan.disk_identity,
            &blocker_text,
        ],
        &[
            executed_at_ms,
            approved_plan.physical_bytes,
            physical_bytes_after.unwrap_or(u64::MAX),
            physical_reduction.unwrap_or(u64::MAX),
            volume_before.unwrap_or(u64::MAX),
            volume_after.unwrap_or(u64::MAX),
            volume_gain.unwrap_or(u64::MAX),
        ],
    );
    Ok(ParallelsDiskReclaimResult {
        schema_version: 1,
        result_id,
        plan_fingerprint: approved_plan.plan_fingerprint.clone(),
        approval_id: approval.approval_id.clone(),
        executed_at_ms,
        command: vec![
            "prl_disk_tool".into(),
            "compact".into(),
            "-hdd".into(),
            "<approved-disk>".into(),
        ],
        physical_bytes_before: approved_plan.physical_bytes,
        physical_bytes_after,
        observed_physical_reduction_bytes: physical_reduction,
        volume_available_bytes_before: volume_before,
        volume_available_bytes_after: volume_after,
        observed_volume_available_gain_bytes: volume_gain,
        execution_succeeded: execution_error.is_none(),
        execution_error,
        verification_complete: verification_blockers.is_empty(),
        verification_blockers,
    })
}

pub fn execute(
    approved_plan: &ParallelsDiskReclaimPlan,
    approval: &ParallelsDiskReclaimApproval,
    confirmation_phrase: &str,
    executed_at_ms: u64,
) -> Result<ParallelsDiskReclaimResult, String> {
    let bundle = canonical_bundle_for_active_use(Path::new(&approved_plan.bundle_path))?;
    let active_use = crate::git_worktree::active_use_evidence(&bundle, 30_000, 256, true);
    execute_with_runner(
        &ProcessParallelsCommandRunner,
        Path::new(PRLCTL_PATH),
        Path::new(DISK_TOOL_PATH),
        approved_plan,
        approval,
        confirmation_phrase,
        executed_at_ms,
        active_use,
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

    #[test]
    fn allocation_walk_limits_one_large_directory_while_enumerating() {
        let temp = tempfile::tempdir().unwrap();
        for index in 0..4 {
            std::fs::write(temp.path().join(format!("{index}.bin")), b"x").unwrap();
        }
        let error =
            tree_allocation_with_limits(temp.path(), 3, Duration::from_secs(1)).unwrap_err();
        assert_eq!(error, "parallels-disk-entry-limit");
    }

    #[test]
    fn compact_info_rejects_duplicate_native_fields() {
        let error = compactable_bytes(
            "Block size: 8\nBlock size: 16\nAllocated blocks: 10\nUsed blocks: 1\n",
        )
        .unwrap_err();
        assert_eq!(error, "parallels-compact-info-duplicate-field");
    }
}
