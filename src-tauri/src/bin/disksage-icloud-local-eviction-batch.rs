//! Headless, redacted, evidence-bound iCloud local-copy batch eviction.
//!
//! Planning is read-only. Execution requires an exact batch fingerprint twice, attributed human
//! approval, a rationale, and a local immutable-record directory outside cloud storage.

use disksage_lib::cloud::{self, CloudAccountScope, CloudProvider, CloudRoot};
use disksage_lib::cloud_local_eviction_batch::{
    approve_icloud_local_eviction_batch, execute_icloud_local_eviction_batch,
    plan_icloud_local_eviction_batch, IcloudLocalEvictionBatchPlan, IcloudLocalEvictionBatchResult,
    MAX_BATCH_ITEMS,
};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};

const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;
const MAX_INVENTORY_REPORT_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    cloud_root: PathBuf,
    manifest: Option<PathBuf>,
    inventory_report: Option<PathBuf>,
    execute: bool,
    approved_batch_fingerprint: Option<String>,
    confirm_batch_fingerprint: Option<String>,
    approved_by: Option<String>,
    rationale: Option<String>,
    record_dir: Option<PathBuf>,
}

fn usage() -> &'static str {
    "usage: disksage-icloud-local-eviction-batch --cloud-root ABSOLUTE_PATH \
     (--manifest ABSOLUTE_JSON | --inventory-report ABSOLUTE_PRIVATE_JSON) \
     [--execute --approved-batch-fingerprint HEX64 \
     --confirm-batch-fingerprint HEX64 --approved-by human:IDENTITY \
     --rationale TEXT --record-dir ABSOLUTE_LOCAL_DIRECTORY]"
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut cloud_root = None;
    let mut manifest = None;
    let mut inventory_report = None;
    let mut execute = false;
    let mut approved_batch_fingerprint = None;
    let mut confirm_batch_fingerprint = None;
    let mut approved_by = None;
    let mut rationale = None;
    let mut record_dir = None;
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--cloud-root" => {
                cloud_root = Some(PathBuf::from(value(args, &mut index, "--cloud-root")?))
            }
            "--manifest" => manifest = Some(PathBuf::from(value(args, &mut index, "--manifest")?)),
            "--inventory-report" => {
                inventory_report = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--inventory-report",
                )?))
            }
            "--execute" => execute = true,
            "--approved-batch-fingerprint" => {
                approved_batch_fingerprint =
                    Some(value(args, &mut index, "--approved-batch-fingerprint")?)
            }
            "--confirm-batch-fingerprint" => {
                confirm_batch_fingerprint =
                    Some(value(args, &mut index, "--confirm-batch-fingerprint")?)
            }
            "--approved-by" => approved_by = Some(value(args, &mut index, "--approved-by")?),
            "--rationale" => rationale = Some(value(args, &mut index, "--rationale")?),
            "--record-dir" => {
                record_dir = Some(PathBuf::from(value(args, &mut index, "--record-dir")?))
            }
            "--help" | "-h" => return Err(usage().into()),
            unknown => return Err(format!("알 수 없는 인자: {unknown}")),
        }
        index += 1;
    }
    let cloud_root = cloud_root.ok_or_else(|| "--cloud-root 값이 필요함".to_string())?;
    if manifest.is_some() == inventory_report.is_some() {
        return Err("--manifest와 --inventory-report 중 정확히 하나가 필요함".into());
    }
    if !cloud_root.is_absolute()
        || manifest.as_ref().is_some_and(|path| !path.is_absolute())
        || inventory_report
            .as_ref()
            .is_some_and(|path| !path.is_absolute())
    {
        return Err("cloud root와 입력 보고서는 절대 경로여야 함".into());
    }
    let execution_fields = [
        approved_batch_fingerprint.is_some(),
        confirm_batch_fingerprint.is_some(),
        approved_by.is_some(),
        rationale.is_some(),
        record_dir.is_some(),
    ];
    if execute && execution_fields.iter().any(|present| !present) {
        return Err(
            "--execute에는 두 fingerprint, human attribution, rationale, record-dir가 모두 필요함"
                .into(),
        );
    }
    if !execute && execution_fields.iter().any(|present| *present) {
        return Err("실행 전용 인자는 --execute와 함께 사용해야 함".into());
    }
    if record_dir
        .as_ref()
        .is_some_and(|directory| !directory.is_absolute())
    {
        return Err("--record-dir은 절대 경로여야 함".into());
    }
    Ok(Args {
        cloud_root,
        manifest,
        inventory_report,
        execute,
        approved_batch_fingerprint,
        confirm_batch_fingerprint,
        approved_by,
        rationale,
        record_dir,
    })
}

fn home_dir() -> Result<PathBuf, String> {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .ok_or_else(|| "HOME을 확인할 수 없음".to_string())
}

fn canonical_existing(path: &Path, error_code: &str) -> Result<PathBuf, String> {
    std::fs::canonicalize(path).map_err(|_| error_code.to_string())
}

fn paths_overlap(left: &Path, right: &Path) -> bool {
    left == right || left.starts_with(right) || right.starts_with(left)
}

fn validate_control_locations(
    cloud_root: &Path,
    manifest: &Path,
    record_dir: Option<&Path>,
) -> Result<(), String> {
    let cloud_root = canonical_existing(
        cloud_root,
        "icloud-local-eviction-batch-cloud-root-unavailable",
    )?;
    let manifest =
        canonical_existing(manifest, "icloud-local-eviction-batch-manifest-unavailable")?;
    if manifest.starts_with(&cloud_root) {
        return Err("icloud-local-eviction-batch-manifest-overlaps-cloud-data".into());
    }
    if let Some(record_dir) = record_dir {
        let record_dir = canonical_existing(
            record_dir,
            "icloud-local-eviction-batch-record-dir-unavailable",
        )?;
        if record_dir.starts_with(&cloud_root) || paths_overlap(&record_dir, &manifest) {
            return Err("icloud-local-eviction-batch-record-dir-overlap".into());
        }
    }
    Ok(())
}

fn select_root<'a>(roots: &'a [CloudRoot], requested: &Path) -> Result<&'a CloudRoot, String> {
    let matches: Vec<_> = roots
        .iter()
        .filter(|root| cloud::cloud_root_path_matches(Path::new(&root.path), requested))
        .collect();
    match matches.as_slice() {
        [only] if only.provider == CloudProvider::Icloud => Ok(*only),
        [..] if matches.len() == 1 => Err("iCloud root가 필요함".into()),
        [] => Err("요청한 경로가 현재 탐지된 클라우드 루트와 일치하지 않음".into()),
        _ => Err("요청한 경로와 일치하는 클라우드 루트가 여러 개임".into()),
    }
}

#[derive(Debug, Deserialize)]
struct InputManifest {
    plans: Vec<InputManifestItem>,
}

#[derive(Debug, Deserialize)]
struct InputManifestItem {
    path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct InputInventoryBatchReport {
    version: u32,
    candidate_count: usize,
    allocated_candidate_bytes: u64,
    reports: Vec<InputInventoryRootReport>,
    contains_sensitive_local_paths: bool,
    local_only: bool,
    remote_capacity_verified: bool,
    provider_sync_attested: bool,
    source_eviction_authorized: bool,
}

#[derive(Debug, Deserialize)]
struct InputInventoryRootReport {
    provider: CloudProvider,
    account_scope: CloudAccountScope,
    cloud_root: String,
    evidence_complete: bool,
    results_truncated: bool,
    allocated_candidate_bytes: u64,
    candidates: Vec<InputInventoryCandidate>,
}

#[derive(Debug, Deserialize)]
struct InputInventoryCandidate {
    path: PathBuf,
    logical_bytes: u64,
    allocated_bytes: u64,
    provider_sync_attested: bool,
}

#[derive(Debug)]
struct InventoryPathSelection {
    paths: Vec<PathBuf>,
    evidence_complete: bool,
    results_truncated: bool,
}

fn read_manifest_paths(path: &Path) -> Result<Vec<PathBuf>, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "icloud-local-eviction-batch-manifest-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("icloud-local-eviction-batch-manifest-must-be-regular-file".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_MANIFEST_BYTES {
        return Err("icloud-local-eviction-batch-manifest-size-invalid".into());
    }
    let file = std::fs::File::open(path)
        .map_err(|_| "icloud-local-eviction-batch-manifest-open-failed".to_string())?;
    let mut encoded = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "icloud-local-eviction-batch-manifest-read-failed".to_string())?;
    if u64::try_from(encoded.len()).unwrap_or(u64::MAX) > MAX_MANIFEST_BYTES {
        return Err("icloud-local-eviction-batch-manifest-size-invalid".into());
    }
    let manifest: InputManifest = serde_json::from_slice(&encoded)
        .map_err(|_| "icloud-local-eviction-batch-manifest-json-invalid".to_string())?;
    if manifest.plans.is_empty() || manifest.plans.len() > MAX_BATCH_ITEMS {
        return Err("icloud-local-eviction-batch-manifest-item-count-invalid".into());
    }
    let paths: Vec<_> = manifest.plans.into_iter().map(|item| item.path).collect();
    if paths.iter().any(|candidate| {
        !candidate.is_absolute()
            || candidate
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
    }) {
        return Err("icloud-local-eviction-batch-manifest-path-invalid".into());
    }
    Ok(paths)
}

fn read_inventory_report_paths(
    path: &Path,
    root: &CloudRoot,
) -> Result<InventoryPathSelection, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "icloud-local-eviction-inventory-report-unavailable".to_string())?;
    if metadata.file_type().is_symlink() || !metadata.is_file() {
        return Err("icloud-local-eviction-inventory-report-must-be-regular-file".into());
    }
    if metadata.len() == 0 || metadata.len() > MAX_INVENTORY_REPORT_BYTES {
        return Err("icloud-local-eviction-inventory-report-size-invalid".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("icloud-local-eviction-inventory-report-permissions-too-open".into());
        }
    }
    #[cfg(not(unix))]
    {
        return Err("icloud-local-eviction-inventory-report-secure-mode-unsupported".into());
    }

    let file = std::fs::File::open(path)
        .map_err(|_| "icloud-local-eviction-inventory-report-open-failed".to_string())?;
    let mut encoded = Vec::with_capacity(usize::try_from(metadata.len()).unwrap_or(0));
    file.take(MAX_INVENTORY_REPORT_BYTES + 1)
        .read_to_end(&mut encoded)
        .map_err(|_| "icloud-local-eviction-inventory-report-read-failed".to_string())?;
    if u64::try_from(encoded.len()).unwrap_or(u64::MAX) > MAX_INVENTORY_REPORT_BYTES {
        return Err("icloud-local-eviction-inventory-report-size-invalid".into());
    }
    let report: InputInventoryBatchReport = serde_json::from_slice(&encoded)
        .map_err(|_| "icloud-local-eviction-inventory-report-json-invalid".to_string())?;
    if report.version != 2
        || !report.contains_sensitive_local_paths
        || !report.local_only
        || report.remote_capacity_verified
        || report.provider_sync_attested
        || report.source_eviction_authorized
        || report.reports.is_empty()
        || report.reports.len() > 64
    {
        return Err("icloud-local-eviction-inventory-report-contract-invalid".into());
    }

    let mut candidate_count = 0usize;
    let mut allocated_candidate_bytes = 0u64;
    for root_report in &report.reports {
        let root_allocated_bytes = root_report
            .candidates
            .iter()
            .try_fold(0u64, |total, candidate| {
                total.checked_add(candidate.allocated_bytes)
            })
            .ok_or_else(|| {
                "icloud-local-eviction-inventory-report-allocation-overflow".to_string()
            })?;
        if root_allocated_bytes != root_report.allocated_candidate_bytes {
            return Err("icloud-local-eviction-inventory-report-root-total-mismatch".into());
        }
        candidate_count = candidate_count
            .checked_add(root_report.candidates.len())
            .ok_or_else(|| {
                "icloud-local-eviction-inventory-report-candidate-overflow".to_string()
            })?;
        allocated_candidate_bytes = allocated_candidate_bytes
            .checked_add(root_report.allocated_candidate_bytes)
            .ok_or_else(|| {
                "icloud-local-eviction-inventory-report-allocation-overflow".to_string()
            })?;
    }
    if candidate_count != report.candidate_count
        || allocated_candidate_bytes != report.allocated_candidate_bytes
    {
        return Err("icloud-local-eviction-inventory-report-batch-total-mismatch".into());
    }

    let matches = report
        .reports
        .iter()
        .filter(|root_report| {
            root_report.provider == CloudProvider::Icloud
                && root_report.account_scope == root.account_scope
                && cloud::cloud_root_path_matches(
                    Path::new(&root_report.cloud_root),
                    Path::new(&root.path),
                )
        })
        .collect::<Vec<_>>();
    let selected = match matches.as_slice() {
        [only] => *only,
        [] => return Err("icloud-local-eviction-inventory-report-root-missing".into()),
        _ => return Err("icloud-local-eviction-inventory-report-root-ambiguous".into()),
    };
    if selected.candidates.is_empty() || selected.candidates.len() > MAX_BATCH_ITEMS {
        return Err("icloud-local-eviction-inventory-report-item-count-invalid".into());
    }
    let cloud_root = Path::new(&root.path);
    if selected.candidates.iter().any(|candidate| {
        candidate.logical_bytes == 0
            || candidate.allocated_bytes == 0
            || candidate.provider_sync_attested
            || !candidate.path.is_absolute()
            || !candidate.path.starts_with(cloud_root)
            || candidate
                .path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
    }) {
        return Err("icloud-local-eviction-inventory-report-candidate-invalid".into());
    }
    Ok(InventoryPathSelection {
        paths: selected
            .candidates
            .iter()
            .map(|candidate| candidate.path.clone())
            .collect(),
        evidence_complete: selected.evidence_complete,
        results_truncated: selected.results_truncated,
    })
}

#[derive(Debug, serde::Serialize)]
struct RedactedBatchPlan {
    version: u32,
    provider: CloudProvider,
    account_scope: CloudAccountScope,
    observed_at_ms: u64,
    input_count: u32,
    planned_count: u32,
    unavailable_count: u32,
    total_logical_bytes: u64,
    total_allocated_bytes: u64,
    batch_fingerprint: String,
    eligible_after_human_approval: bool,
    blockers: Vec<String>,
    item_blocker_counts: BTreeMap<String, u32>,
    unavailable_error_counts: BTreeMap<String, u32>,
    notices: Vec<String>,
}

fn redact_plan(plan: &IcloudLocalEvictionBatchPlan) -> RedactedBatchPlan {
    let mut item_blocker_counts = BTreeMap::new();
    for item in &plan.items {
        for blocker in &item.plan.blockers {
            let count = item_blocker_counts.entry(blocker.clone()).or_insert(0u32);
            *count = count.saturating_add(1);
        }
    }
    let mut unavailable_error_counts = BTreeMap::new();
    for unavailable in &plan.unavailable {
        let count = unavailable_error_counts
            .entry(unavailable.error_code.clone())
            .or_insert(0u32);
        *count = count.saturating_add(1);
    }
    RedactedBatchPlan {
        version: plan.version,
        provider: plan.provider,
        account_scope: plan.account_scope,
        observed_at_ms: plan.observed_at_ms,
        input_count: plan.input_count,
        planned_count: plan.planned_count,
        unavailable_count: plan.unavailable_count,
        total_logical_bytes: plan.total_logical_bytes,
        total_allocated_bytes: plan.total_allocated_bytes,
        batch_fingerprint: plan.batch_fingerprint.clone(),
        eligible_after_human_approval: plan.eligible_after_human_approval,
        blockers: plan.blockers.clone(),
        item_blocker_counts,
        unavailable_error_counts,
        notices: plan.notices.clone(),
    }
}

#[derive(Debug, serde::Serialize)]
struct PlanOutput {
    action: &'static str,
    input_source: &'static str,
    input_inventory_evidence_complete: Option<bool>,
    input_inventory_results_truncated: Option<bool>,
    remote_capacity_verified: bool,
    source_eviction_authorized: bool,
    mutation_executed: bool,
    individual_paths_redacted: bool,
    plan: RedactedBatchPlan,
}

#[derive(Debug, serde::Serialize)]
struct RedactedBatchResult {
    version: u32,
    result_id: String,
    batch_fingerprint: String,
    approval_id: String,
    started_at_ms: u64,
    completed_at_ms: u64,
    input_count: u32,
    planned_count: u32,
    unavailable_count: u32,
    attempted_count: u32,
    succeeded_count: u32,
    verified_count: u32,
    total_allocated_bytes_before: u64,
    observed_allocation_reduction_bytes: u64,
    execution_complete: bool,
    verification_complete: bool,
    halted: bool,
    halt_reason: Option<String>,
    notices: Vec<String>,
}

fn redact_result(result: &IcloudLocalEvictionBatchResult) -> RedactedBatchResult {
    RedactedBatchResult {
        version: result.version,
        result_id: result.result_id.clone(),
        batch_fingerprint: result.batch_fingerprint.clone(),
        approval_id: result.approval_id.clone(),
        started_at_ms: result.started_at_ms,
        completed_at_ms: result.completed_at_ms,
        input_count: result.input_count,
        planned_count: result.planned_count,
        unavailable_count: result.unavailable_count,
        attempted_count: result.attempted_count,
        succeeded_count: result.succeeded_count,
        verified_count: result.verified_count,
        total_allocated_bytes_before: result.total_allocated_bytes_before,
        observed_allocation_reduction_bytes: result.observed_allocation_reduction_bytes,
        execution_complete: result.execution_complete,
        verification_complete: result.verification_complete,
        halted: result.halted,
        halt_reason: result.halt_reason.clone(),
        notices: result.notices.clone(),
    }
}

#[derive(Debug, serde::Serialize)]
struct ExecuteOutput {
    action: &'static str,
    input_source: &'static str,
    mutation_executed: bool,
    individual_paths_redacted: bool,
    batch_approval_id: String,
    result: RedactedBatchResult,
}

fn print_json<T: serde::Serialize>(value: &T) -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn run() -> Result<(), String> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw)?;
    let roots = cloud::discover_cloud_roots(&home_dir()?);
    let root = select_root(&roots, &args.cloud_root)?.clone();
    let (input_path, input_source) = match (&args.manifest, &args.inventory_report) {
        (Some(path), None) => (path.as_path(), "manifest"),
        (None, Some(path)) => (path.as_path(), "private-local-inventory-report"),
        _ => return Err("icloud-local-eviction-batch-input-shape-invalid".into()),
    };
    validate_control_locations(
        Path::new(&root.path),
        input_path,
        args.record_dir.as_deref(),
    )?;
    let (paths, inventory_evidence_complete, inventory_results_truncated) =
        if args.inventory_report.is_some() {
            let selection = read_inventory_report_paths(input_path, &root)?;
            (
                selection.paths,
                Some(selection.evidence_complete),
                Some(selection.results_truncated),
            )
        } else {
            (read_manifest_paths(input_path)?, None, None)
        };
    let plan = plan_icloud_local_eviction_batch(&root, &paths, cloud::system_now_ms())?;
    if !args.execute {
        return print_json(&PlanOutput {
            action: "plan-icloud-local-eviction-batch",
            input_source,
            input_inventory_evidence_complete: inventory_evidence_complete,
            input_inventory_results_truncated: inventory_results_truncated,
            remote_capacity_verified: false,
            source_eviction_authorized: false,
            mutation_executed: false,
            individual_paths_redacted: true,
            plan: redact_plan(&plan),
        });
    }

    let approved_fingerprint = args
        .approved_batch_fingerprint
        .as_deref()
        .ok_or_else(|| "approved-batch-fingerprint-missing".to_string())?;
    let confirmation = args
        .confirm_batch_fingerprint
        .as_deref()
        .ok_or_else(|| "confirm-batch-fingerprint-missing".to_string())?;
    if approved_fingerprint != confirmation {
        return Err("icloud-local-eviction-batch-double-confirmation-mismatch".into());
    }
    let approved_by = args
        .approved_by
        .as_deref()
        .ok_or_else(|| "approved-by-missing".to_string())?;
    let rationale = args
        .rationale
        .as_deref()
        .ok_or_else(|| "rationale-missing".to_string())?;
    let record_dir = args
        .record_dir
        .as_deref()
        .ok_or_else(|| "record-dir-missing".to_string())?;
    let approved_at_ms = cloud::system_now_ms();
    let approval = approve_icloud_local_eviction_batch(
        &plan,
        &root,
        approved_fingerprint,
        approved_at_ms,
        approved_by,
        rationale,
    )?;
    let result = execute_icloud_local_eviction_batch(
        &root,
        &plan,
        &approval,
        confirmation,
        record_dir,
        cloud::system_now_ms(),
    )?;
    print_json(&ExecuteOutput {
        action: "evict-icloud-local-copy-batch",
        input_source,
        mutation_executed: result.attempted_count > 0,
        individual_paths_redacted: true,
        batch_approval_id: approval.approval_id,
        result: redact_result(&result),
    })
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use disksage_lib::cloud_local_eviction::{
        ActiveUseEvidence, IcloudLocalEvictionPlan, IcloudLocalState,
    };
    use disksage_lib::cloud_local_eviction_batch::{
        IcloudLocalEvictionBatchItem, IcloudLocalEvictionBatchUnavailable,
        ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
    };
    use std::io::Write;

    #[cfg(not(windows))]
    const TEST_CLOUD_ROOT: &str = "/cloud";
    #[cfg(windows)]
    const TEST_CLOUD_ROOT: &str = r"C:\cloud";
    #[cfg(not(windows))]
    const TEST_MANIFEST: &str = "/tmp/manifest.json";
    #[cfg(windows)]
    const TEST_MANIFEST: &str = r"C:\tmp\manifest.json";
    #[cfg(not(windows))]
    const TEST_RECORD_DIR: &str = "/tmp/records";
    #[cfg(windows)]
    const TEST_RECORD_DIR: &str = r"C:\tmp\records";

    #[test]
    fn parser_requires_complete_explicit_execution_fields() {
        let base = vec![
            "--cloud-root".into(),
            TEST_CLOUD_ROOT.into(),
            "--manifest".into(),
            TEST_MANIFEST.into(),
        ];
        assert!(!parse_args(&base).unwrap().execute);
        let mut partial = base.clone();
        partial.push("--execute".into());
        assert!(parse_args(&partial).is_err());

        let mut complete = base;
        complete.extend([
            "--execute".into(),
            "--approved-batch-fingerprint".into(),
            "a".repeat(64),
            "--confirm-batch-fingerprint".into(),
            "a".repeat(64),
            "--approved-by".into(),
            "human:operator".into(),
            "--rationale".into(),
            "Exact batch reviewed".into(),
            "--record-dir".into(),
            TEST_RECORD_DIR.into(),
        ]);
        assert!(parse_args(&complete).unwrap().execute);

        let inventory = vec![
            "--cloud-root".into(),
            TEST_CLOUD_ROOT.into(),
            "--inventory-report".into(),
            TEST_MANIFEST.into(),
        ];
        let parsed = parse_args(&inventory).unwrap();
        assert!(parsed.manifest.is_none());
        assert_eq!(parsed.inventory_report, Some(PathBuf::from(TEST_MANIFEST)));
        let mut both = inventory;
        both.extend(["--manifest".into(), TEST_MANIFEST.into()]);
        assert!(parse_args(&both).is_err());
    }

    #[test]
    fn manifest_reader_is_bounded_and_accepts_extra_evidence_fields() {
        let temp = tempfile::tempdir().unwrap();
        let manifest = temp.path().join("manifest.json");
        let candidate = PathBuf::from(TEST_CLOUD_ROOT).join("a");
        let mut file = std::fs::File::create(&manifest).unwrap();
        file.write_all(
            &serde_json::to_vec(&serde_json::json!({
                "version": 99,
                "plans": [{
                    "path": candidate,
                    "plan_fingerprint": "ignored"
                }]
            }))
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            read_manifest_paths(&manifest).unwrap(),
            [PathBuf::from(TEST_CLOUD_ROOT).join("a")]
        );

        let empty = temp.path().join("empty.json");
        std::fs::write(&empty, br#"{"plans":[]}"#).unwrap();
        assert!(read_manifest_paths(&empty).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn manifest_reader_rejects_symlink() {
        use std::os::unix::fs::symlink;
        let temp = tempfile::tempdir().unwrap();
        let real = temp.path().join("real.json");
        std::fs::write(&real, br#"{"plans":[{"path":"/cloud/a"}]}"#).unwrap();
        let link = temp.path().join("link.json");
        symlink(&real, &link).unwrap();
        assert_eq!(
            read_manifest_paths(&link).unwrap_err(),
            "icloud-local-eviction-batch-manifest-must-be-regular-file"
        );
    }

    #[cfg(unix)]
    #[test]
    fn private_inventory_report_is_mode_bound_and_selects_icloud_paths() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().unwrap();
        let cloud_root = temp.path().join("icloud");
        std::fs::create_dir(&cloud_root).unwrap();
        let candidate = cloud_root.join("candidate.bin");
        let report_path = temp.path().join("private.json");
        let report = serde_json::json!({
            "version": 2,
            "candidate_count": 1,
            "allocated_candidate_bytes": 4096,
            "reports": [{
                "provider": "icloud",
                "account_scope": "personal",
                "cloud_root": cloud_root,
                "evidence_complete": false,
                "results_truncated": false,
                "allocated_candidate_bytes": 4096,
                "candidates": [{
                    "path": candidate,
                    "logical_bytes": 8192,
                    "allocated_bytes": 4096,
                    "provider_sync_attested": false
                }]
            }],
            "contains_sensitive_local_paths": true,
            "local_only": true,
            "remote_capacity_verified": false,
            "provider_sync_attested": false,
            "source_eviction_authorized": false
        });
        std::fs::write(&report_path, serde_json::to_vec(&report).unwrap()).unwrap();
        std::fs::set_permissions(&report_path, std::fs::Permissions::from_mode(0o600)).unwrap();
        let root = CloudRoot {
            id: "icloud:test".into(),
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            label: "iCloud".into(),
            path: cloud_root.to_string_lossy().into_owned(),
            readable: true,
            access_issue: None,
        };
        let selection = read_inventory_report_paths(&report_path, &root).unwrap();
        assert_eq!(selection.paths, [candidate]);
        assert!(!selection.evidence_complete);
        assert!(!selection.results_truncated);

        std::fs::set_permissions(&report_path, std::fs::Permissions::from_mode(0o644)).unwrap();
        assert_eq!(
            read_inventory_report_paths(&report_path, &root).unwrap_err(),
            "icloud-local-eviction-inventory-report-permissions-too-open"
        );
    }

    #[cfg(unix)]
    #[test]
    fn control_locations_reject_symlinked_cloud_ancestors() {
        use std::os::unix::fs::symlink;

        let temp = tempfile::tempdir().unwrap();
        let cloud = temp.path().join("cloud");
        let records = cloud.join("records");
        std::fs::create_dir_all(&records).unwrap();
        let cloud_manifest = cloud.join("manifest.json");
        std::fs::write(&cloud_manifest, br#"{"plans":[{"path":"/cloud/a"}]}"#).unwrap();
        let alias = temp.path().join("cloud-alias");
        symlink(&cloud, &alias).unwrap();

        assert_eq!(
            validate_control_locations(&cloud, &alias.join("manifest.json"), None).unwrap_err(),
            "icloud-local-eviction-batch-manifest-overlaps-cloud-data"
        );

        let local_manifest = temp.path().join("local-manifest.json");
        std::fs::write(&local_manifest, br#"{"plans":[{"path":"/cloud/a"}]}"#).unwrap();
        assert_eq!(
            validate_control_locations(&cloud, &local_manifest, Some(&alias.join("records")))
                .unwrap_err(),
            "icloud-local-eviction-batch-record-dir-overlap"
        );
    }

    #[test]
    fn redacted_plan_json_contains_no_path_or_root_field() {
        let plan = IcloudLocalEvictionBatchPlan {
            version: ICLOUD_LOCAL_EVICTION_BATCH_VERSION,
            provider: CloudProvider::Icloud,
            account_scope: CloudAccountScope::Personal,
            cloud_root: "/secret/cloud".into(),
            observed_at_ms: 1,
            input_count: 2,
            planned_count: 1,
            unavailable_count: 1,
            total_logical_bytes: 10,
            total_allocated_bytes: 20,
            items: vec![IcloudLocalEvictionBatchItem {
                input_index: 0,
                plan: IcloudLocalEvictionPlan {
                    version: 1,
                    provider: CloudProvider::Icloud,
                    account_scope: CloudAccountScope::Personal,
                    cloud_root: "/secret/cloud".into(),
                    path: "/secret/cloud/private.pdf".into(),
                    logical_bytes: 10,
                    allocated_bytes: 20,
                    filesystem_modified_ms: 1,
                    observed_at_ms: 1,
                    icloud_state: IcloudLocalState {
                        is_ubiquitous: true,
                        is_uploaded: true,
                        is_uploading: false,
                        is_downloading: false,
                        downloading_status_current: true,
                        has_unresolved_conflicts: false,
                        is_excluded_from_sync: false,
                    },
                    active_use: ActiveUseEvidence {
                        method: "test".into(),
                        evidence_complete: true,
                        active: false,
                        observed_pids: Vec::new(),
                        results_truncated: false,
                        error: None,
                    },
                    plan_fingerprint: "a".repeat(64),
                    eligible_after_human_approval: true,
                    blockers: vec!["human-local-eviction-approval-required".into()],
                    notices: Vec::new(),
                },
            }],
            unavailable: vec![IcloudLocalEvictionBatchUnavailable {
                input_index: 1,
                error_code: "item-unavailable".into(),
            }],
            batch_fingerprint: "b".repeat(64),
            eligible_after_human_approval: true,
            blockers: vec!["human-local-eviction-batch-approval-required".into()],
            notices: Vec::new(),
        };
        let encoded = serde_json::to_string(&redact_plan(&plan)).unwrap();
        assert!(!encoded.contains("private.pdf"));
        assert!(!encoded.contains("/secret"));
        assert!(!encoded.contains("\"path\""));
        assert!(!encoded.contains("cloud_root"));
    }
}
