#!/usr/bin/env python3
"""Apply deterministic, fail-closed repairs for DiskSage pull request 89."""

from __future__ import annotations

import argparse
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CORE = ROOT / "src-tauri/src/cloud_local_eviction_batch.rs"
CLI = ROOT / "src-tauri/src/bin/disksage-icloud-local-eviction-batch.rs"


def replace_once(path: Path, old: str, new: str) -> None:
    """Replace exactly one expected source fragment or fail without writing."""
    text = path.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise SystemExit(f"expected exactly one source fragment in {path}, found {count}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


def patch_tests() -> None:
    """Add regression tests before production changes are applied."""
    replace_once(
        CORE,
        '''    #[test]\n    fn item_execution_timestamps_read_the_clock_for_each_item() {\n        let next = std::cell::Cell::new(40_u64);\n        let mut now_ms = || {\n            let current = next.get();\n            next.set(current + 7);\n            current\n        };\n        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 40);\n        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 47);\n    }\n}\n''',
        '''    #[derive(Default)]\n    struct TestBatchRecorder {\n        record_names: Vec<String>,\n        fail_result_record: bool,\n    }\n\n    impl BatchRecordWriter for TestBatchRecorder {\n        fn write<T: serde::Serialize>(\n            &mut self,\n            _record_dir: &Path,\n            name: &str,\n            _value: &T,\n        ) -> Result<(), String> {\n            self.record_names.push(name.to_string());\n            if self.fail_result_record && name.ends_with(".result.json") {\n                Err("test-result-record-failure".into())\n            } else {\n                Ok(())\n            }\n        }\n    }\n\n    fn plan_index(path: &Path) -> usize {\n        path.file_stem()\n            .unwrap()\n            .to_string_lossy()\n            .trim_start_matches("file-")\n            .parse()\n            .unwrap()\n    }\n\n    fn approved_batch(\n        item_count: usize,\n    ) -> (\n        IcloudLocalEvictionBatchPlan,\n        IcloudLocalEvictionBatchApproval,\n    ) {\n        let paths: Vec<_> = (0..item_count).map(path).collect();\n        let plan = plan_batch_with(&root(), &paths, 20, |_, path, _| {\n            Ok(safe_plan(plan_index(path)))\n        })\n        .unwrap();\n        let approval = approve_icloud_local_eviction_batch(\n            &plan,\n            &root(),\n            &plan.batch_fingerprint,\n            21,\n            "human:operator",\n            "Exact batch reviewed",\n        )\n        .unwrap();\n        (plan, approval)\n    }\n\n    fn successful_result(\n        plan: &IcloudLocalEvictionPlan,\n        approval: &IcloudLocalEvictionApproval,\n        requested_at_ms: u64,\n    ) -> IcloudLocalEvictionResult {\n        IcloudLocalEvictionResult {\n            version: crate::cloud_local_eviction::ICLOUD_LOCAL_EVICTION_VERSION,\n            result_id: format!("{requested_at_ms:064x}"),\n            plan_fingerprint: plan.plan_fingerprint.clone(),\n            approval_id: approval.approval_id.clone(),\n            path: plan.path.clone(),\n            requested_at_ms,\n            allocated_bytes_before: plan.allocated_bytes,\n            allocated_bytes_after: 0,\n            observed_allocation_reduction_bytes: plan.allocated_bytes,\n            eviction_request_succeeded: true,\n            cloud_item_path_retained: true,\n            is_ubiquitous_after: true,\n            local_allocation_reduction_verified: true,\n            verification_complete: true,\n            verification_blockers: Vec::new(),\n            notices: Vec::new(),\n        }\n    }\n\n    #[test]\n    fn execution_stops_after_first_failed_item_and_records_each_checkpoint() {\n        let root = root();\n        let (plan, approval) = approved_batch(3);\n        let calls = Cell::new(0usize);\n        let clock = Cell::new(100_u64);\n        let mut requested_times = Vec::new();\n        let mut recorder = TestBatchRecorder::default();\n\n        let result = execute_icloud_local_eviction_batch_with(\n            &root,\n            &plan,\n            &approval,\n            &plan.batch_fingerprint,\n            Path::new("/records"),\n            30,\n            |_, path, _| Ok(safe_plan(plan_index(path))),\n            |_, live_plan, individual, _, requested_at_ms| {\n                let call = calls.get();\n                calls.set(call + 1);\n                requested_times.push(requested_at_ms);\n                if call == 1 {\n                    Err("test-item-execution-failed".into())\n                } else {\n                    Ok(successful_result(live_plan, individual, requested_at_ms))\n                }\n            },\n            &mut recorder,\n            || {\n                let current = clock.get();\n                clock.set(current + 100);\n                current\n            },\n        )\n        .unwrap();\n\n        assert_eq!(calls.get(), 2);\n        assert_eq!(requested_times, vec![100, 200]);\n        assert_eq!(result.attempted_count, 2);\n        assert!(result.halted);\n        assert_eq!(\n            result.halt_reason.as_deref(),\n            Some("icloud-local-eviction-batch-item-execution-failed")\n        );\n        let checkpoints: Vec<_> = recorder\n            .record_names\n            .iter()\n            .filter(|name| name.ends_with(".batch-result.json"))\n            .collect();\n        assert_eq!(checkpoints.len(), 2);\n        assert!(recorder\n            .record_names\n            .windows(2)\n            .any(|pair| pair[0].ends_with(".result.json")\n                && pair[1].ends_with(".batch-result.json")));\n    }\n\n    #[test]\n    fn result_record_failure_halts_with_incomplete_verification_and_checkpoint() {\n        let root = root();\n        let (plan, approval) = approved_batch(1);\n        let mut recorder = TestBatchRecorder {\n            fail_result_record: true,\n            ..TestBatchRecorder::default()\n        };\n\n        let result = execute_icloud_local_eviction_batch_with(\n            &root,\n            &plan,\n            &approval,\n            &plan.batch_fingerprint,\n            Path::new("/records"),\n            30,\n            |_, path, _| Ok(safe_plan(plan_index(path))),\n            |_, live_plan, individual, _, requested_at_ms| {\n                Ok(successful_result(live_plan, individual, requested_at_ms))\n            },\n            &mut recorder,\n            || 100,\n        )\n        .unwrap();\n\n        assert!(result.halted);\n        assert!(!result.verification_complete);\n        assert_eq!(\n            result.halt_reason.as_deref(),\n            Some("icloud-local-eviction-batch-item-result-record-failed")\n        );\n        assert_eq!(\n            result.item_outcomes[0].error_code.as_deref(),\n            Some("icloud-local-eviction-batch-item-result-record-failed")\n        );\n        assert_eq!(\n            recorder\n                .record_names\n                .iter()\n                .filter(|name| name.ends_with(".batch-result.json"))\n                .count(),\n            1\n        );\n    }\n\n    #[test]\n    fn item_execution_timestamps_read_the_clock_for_each_item() {\n        let next = std::cell::Cell::new(40_u64);\n        let mut now_ms = || {\n            let current = next.get();\n            next.set(current + 7);\n            current\n        };\n        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 40);\n        assert_eq!(fresh_item_requested_at_ms(&mut now_ms), 47);\n    }\n}\n''',
    )

    replace_once(
        CLI,
        '''        let empty = temp.path().join("empty.json");\n        std::fs::write(&empty, br#"{\\"plans\\":[]}"#).unwrap();\n        assert!(read_manifest_paths(&empty).is_err());\n    }\n''',
        '''        let empty = temp.path().join("empty.json");\n        std::fs::write(&empty, br#"{\\"plans\\":[]}"#).unwrap();\n        assert!(read_manifest_paths(&empty).is_err());\n\n        let too_many = temp.path().join("too-many.json");\n        let items: Vec<_> = (0..=MAX_BATCH_ITEMS)\n            .map(|index| {\n                serde_json::json!({\n                    "path": PathBuf::from(TEST_CLOUD_ROOT).join(format!("f{index}"))\n                })\n            })\n            .collect();\n        std::fs::write(\n            &too_many,\n            serde_json::to_vec(&serde_json::json!({ "plans": items })).unwrap(),\n        )\n        .unwrap();\n        assert_eq!(\n            read_manifest_paths(&too_many).unwrap_err(),\n            "icloud-local-eviction-batch-manifest-item-count-invalid"\n        );\n\n        let oversized = temp.path().join("oversized.json");\n        let padding = "x".repeat(usize::try_from(MAX_MANIFEST_BYTES).unwrap() + 1);\n        std::fs::write(\n            &oversized,\n            serde_json::to_vec(&serde_json::json!({ "pad": padding, "plans": [] })).unwrap(),\n        )\n        .unwrap();\n        assert_eq!(\n            read_manifest_paths(&oversized).unwrap_err(),\n            "icloud-local-eviction-batch-manifest-size-invalid"\n        );\n    }\n''',
    )

    replace_once(
        CLI,
        '''        assert_eq!(\n            validate_control_locations(&cloud, &local_manifest, Some(&alias.join("records")))\n                .unwrap_err(),\n            "icloud-local-eviction-batch-record-dir-overlap"\n        );\n    }\n''',
        '''        assert_eq!(\n            validate_control_locations(&cloud, &local_manifest, Some(&alias.join("records")))\n                .unwrap_err(),\n            "icloud-local-eviction-batch-record-dir-inside-cloud-data"\n        );\n        assert_eq!(\n            validate_control_locations(&cloud, &local_manifest, Some(temp.path())).unwrap_err(),\n            "icloud-local-eviction-batch-record-dir-overlaps-manifest"\n        );\n    }\n''',
    )


def patch_core() -> None:
    """Inject deterministic execution seams and fresh per-item time reads."""
    replace_once(
        CORE,
        '''use crate::cloud_local_eviction::{\n    approve_icloud_local_eviction, execute_icloud_local_eviction, plan_icloud_local_eviction,\n    write_immutable_record, IcloudLocalEvictionPlan,\n};\n''',
        '''use crate::cloud_local_eviction::{\n    approve_icloud_local_eviction, execute_icloud_local_eviction, plan_icloud_local_eviction,\n    write_immutable_record, IcloudLocalEvictionApproval, IcloudLocalEvictionPlan,\n    IcloudLocalEvictionResult,\n};\n''',
    )

    replace_once(
        CORE,
        '''#[cfg(not(coverage))]\npub fn execute_icloud_local_eviction_batch(\n    root: &CloudRoot,\n    plan: &IcloudLocalEvictionBatchPlan,\n    approval: &IcloudLocalEvictionBatchApproval,\n    confirmation_batch_fingerprint: &str,\n    record_dir: &Path,\n    requested_at_ms: u64,\n) -> Result<IcloudLocalEvictionBatchResult, String> {\n    execute_icloud_local_eviction_batch_with_now(\n        root,\n        plan,\n        approval,\n        confirmation_batch_fingerprint,\n        record_dir,\n        requested_at_ms,\n        crate::cloud::system_now_ms,\n    )\n}\n\n#[cfg(not(coverage))]\nfn fresh_item_requested_at_ms(now_ms: &mut impl FnMut() -> u64) -> u64 {\n    now_ms()\n}\n\n#[cfg(not(coverage))]\nfn execute_icloud_local_eviction_batch_with_now(\n    root: &CloudRoot,\n    plan: &IcloudLocalEvictionBatchPlan,\n    approval: &IcloudLocalEvictionBatchApproval,\n    confirmation_batch_fingerprint: &str,\n    record_dir: &Path,\n    requested_at_ms: u64,\n    mut now_ms: impl FnMut() -> u64,\n) -> Result<IcloudLocalEvictionBatchResult, String> {\n    validate_batch_approval(root, plan, approval, confirmation_batch_fingerprint)?;\n    let _live = preflight_with(root, plan, requested_at_ms, plan_icloud_local_eviction)?;\n''',
        '''#[cfg(not(coverage))]\ntrait BatchRecordWriter {\n    fn write<T: serde::Serialize>(\n        &mut self,\n        record_dir: &Path,\n        name: &str,\n        value: &T,\n    ) -> Result<(), String>;\n}\n\n#[cfg(not(coverage))]\nstruct ImmutableBatchRecordWriter;\n\n#[cfg(not(coverage))]\nimpl BatchRecordWriter for ImmutableBatchRecordWriter {\n    fn write<T: serde::Serialize>(\n        &mut self,\n        record_dir: &Path,\n        name: &str,\n        value: &T,\n    ) -> Result<(), String> {\n        write_immutable_record(record_dir, name, value)\n    }\n}\n\n#[cfg(not(coverage))]\npub fn execute_icloud_local_eviction_batch(\n    root: &CloudRoot,\n    plan: &IcloudLocalEvictionBatchPlan,\n    approval: &IcloudLocalEvictionBatchApproval,\n    confirmation_batch_fingerprint: &str,\n    record_dir: &Path,\n    requested_at_ms: u64,\n) -> Result<IcloudLocalEvictionBatchResult, String> {\n    let mut recorder = ImmutableBatchRecordWriter;\n    execute_icloud_local_eviction_batch_with(\n        root,\n        plan,\n        approval,\n        confirmation_batch_fingerprint,\n        record_dir,\n        requested_at_ms,\n        plan_icloud_local_eviction,\n        execute_icloud_local_eviction,\n        &mut recorder,\n        crate::cloud::system_now_ms,\n    )\n}\n\n#[cfg(not(coverage))]\nfn fresh_item_requested_at_ms(now_ms: &mut impl FnMut() -> u64) -> u64 {\n    now_ms()\n}\n\n#[cfg(not(coverage))]\nfn execute_icloud_local_eviction_batch_with<P, E, R, N>(\n    root: &CloudRoot,\n    plan: &IcloudLocalEvictionBatchPlan,\n    approval: &IcloudLocalEvictionBatchApproval,\n    confirmation_batch_fingerprint: &str,\n    record_dir: &Path,\n    requested_at_ms: u64,\n    mut planner: P,\n    mut executor: E,\n    recorder: &mut R,\n    mut now_ms: N,\n) -> Result<IcloudLocalEvictionBatchResult, String>\nwhere\n    P: FnMut(&CloudRoot, &Path, u64) -> Result<IcloudLocalEvictionPlan, String>,\n    E: FnMut(\n        &CloudRoot,\n        &IcloudLocalEvictionPlan,\n        &IcloudLocalEvictionApproval,\n        &str,\n        u64,\n    ) -> Result<IcloudLocalEvictionResult, String>,\n    R: BatchRecordWriter,\n    N: FnMut() -> u64,\n{\n    validate_batch_approval(root, plan, approval, confirmation_batch_fingerprint)?;\n    let _live = preflight_with(root, plan, requested_at_ms, &mut planner)?;\n''',
    )

    replace_once(
        CORE,
        '''    write_immutable_record(\n        record_dir,\n        &format!("{}.batch-approval.json", approval.approval_id),\n        approval,\n    )\n''',
        '''    recorder\n        .write(\n            record_dir,\n            &format!("{}.batch-approval.json", approval.approval_id),\n            approval,\n        )\n''',
    )
    replace_once(
        CORE,
        '''        write_immutable_record(\n            record_dir,\n            &format!("{}.approval.json", individual.approval_id),\n            &individual,\n        )\n''',
        '''        recorder\n            .write(\n                record_dir,\n                &format!("{}.approval.json", individual.approval_id),\n                &individual,\n            )\n''',
    )
    replace_once(
        CORE,
        '''    for (offset, (item, individual)) in plan\n        .items\n        .iter()\n        .zip(individual_approvals.iter())\n        .enumerate()\n    {\n''',
        '''    for (item, individual) in plan.items.iter().zip(individual_approvals.iter()) {\n''',
    )
    replace_once(
        CORE,
        '''        let item_requested_at_ms =\n            requested_at_ms.saturating_add(u64::try_from(offset).unwrap_or(u64::MAX));\n        let execution = execute_icloud_local_eviction(\n''',
        '''        let item_requested_at_ms = fresh_item_requested_at_ms(&mut now_ms);\n        let execution = executor(\n''',
    )
    replace_once(
        CORE,
        '''                let result_record = write_immutable_record(\n                    record_dir,\n                    &format!("{}.result.json", result.result_id),\n                    &result,\n                );\n''',
        '''                let result_record = recorder.write(\n                    record_dir,\n                    &format!("{}.result.json", result.result_id),\n                    &result,\n                );\n''',
    )
    replace_once(
        CORE,
        '''        write_immutable_record(\n            record_dir,\n            &checkpoint_name(&approval.approval_id, batch_result.attempted_count),\n            &batch_result,\n        )\n''',
        '''        recorder\n            .write(\n                record_dir,\n                &checkpoint_name(&approval.approval_id, batch_result.attempted_count),\n                &batch_result,\n            )\n''',
    )


def patch_cli() -> None:
    """Clarify operator diagnostics without changing fail-closed validation."""
    replace_once(
        CLI,
        '''        if record_dir.starts_with(&cloud_root) || paths_overlap(&record_dir, &manifest) {\n            return Err("icloud-local-eviction-batch-record-dir-overlap".into());\n        }\n''',
        '''        if record_dir.starts_with(&cloud_root) {\n            return Err("icloud-local-eviction-batch-record-dir-inside-cloud-data".into());\n        }\n        if paths_overlap(&record_dir, &manifest) {\n            return Err("icloud-local-eviction-batch-record-dir-overlaps-manifest".into());\n        }\n''',
    )
    replace_once(
        CLI,
        '''    match matches.as_slice() {\n        [only] if only.provider == CloudProvider::Icloud => Ok(*only),\n        [..] if matches.len() == 1 => Err("iCloud root가 필요함".into()),\n        [] => Err("요청한 경로가 현재 탐지된 클라우드 루트와 일치하지 않음".into()),\n        _ => Err("요청한 경로와 일치하는 클라우드 루트가 여러 개임".into()),\n    }\n''',
        '''    match matches.as_slice() {\n        [] => Err("요청한 경로가 현재 탐지된 클라우드 루트와 일치하지 않음".into()),\n        [only] if only.provider == CloudProvider::Icloud => Ok(*only),\n        [_] => Err("iCloud root가 필요함".into()),\n        _ => Err("요청한 경로와 일치하는 클라우드 루트가 여러 개임".into()),\n    }\n''',
    )


def main() -> None:
    """Apply one selected deterministic patch stage."""
    parser = argparse.ArgumentParser()
    parser.add_argument("mode", choices=("tests", "core", "cli", "all"))
    args = parser.parse_args()
    if args.mode in {"tests", "all"}:
        patch_tests()
    if args.mode in {"core", "all"}:
        patch_core()
    if args.mode in {"cli", "all"}:
        patch_cli()


if __name__ == "__main__":
    main()
