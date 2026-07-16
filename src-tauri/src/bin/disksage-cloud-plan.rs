//! Headless entrypoint for planning, reviewing, copying, and attesting cloud archive candidates.

#[cfg(not(coverage))]
use std::path::{Path, PathBuf};

#[cfg(not(coverage))]
use disksage_lib::cloud::{self, CloudPlanOptions, CloudProvider, CloudRoot};
#[cfg(not(coverage))]
use disksage_lib::cloud_eviction::{self, CloudEvictionResult};
#[cfg(not(coverage))]
use disksage_lib::cloud_review::{self, CloudReviewDecision, CloudReviewDisposition};
#[cfg(not(coverage))]
use disksage_lib::cloud_transfer::{self, CloudCopyReceipt, LocalEvictionPermit};
#[cfg(not(coverage))]
use disksage_lib::provider_sync;

#[cfg(not(coverage))]
#[derive(Debug, PartialEq, Eq)]
struct Args {
    root: PathBuf,
    cloud_root: Option<PathBuf>,
    provider: Option<CloudProvider>,
    min_size_mib: u64,
    min_age_days: u64,
    limit: usize,
    list_roots: bool,
    inspect_roots: bool,
    copy_fingerprint: Option<String>,
    receipt_dir: Option<PathBuf>,
    attest_receipt: Option<PathBuf>,
    evict_receipt: Option<PathBuf>,
    confirm_receipt_id: Option<String>,
    eviction_dir: Option<PathBuf>,
    journal_path: Option<PathBuf>,
    review_candidate_fingerprint: Option<String>,
    review_fingerprint: Option<String>,
    review_disposition: Option<CloudReviewDisposition>,
    review_dir: Option<PathBuf>,
}

#[cfg(not(coverage))]
fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

#[cfg(not(coverage))]
fn parse_provider(value: &str) -> Result<CloudProvider, String> {
    match value {
        "icloud" => Ok(CloudProvider::Icloud),
        "onedrive" => Ok(CloudProvider::Onedrive),
        "google-drive" => Ok(CloudProvider::GoogleDrive),
        _ => Err(format!("지원하지 않는 provider: {value}")),
    }
}

#[cfg(not(coverage))]
fn parse_args(args: &[String], home: &Path) -> Result<Args, String> {
    let mut parsed = Args {
        root: home.to_path_buf(),
        cloud_root: None,
        provider: None,
        min_size_mib: 256,
        min_age_days: 90,
        limit: 200,
        list_roots: false,
        inspect_roots: false,
        copy_fingerprint: None,
        receipt_dir: None,
        attest_receipt: None,
        evict_receipt: None,
        confirm_receipt_id: None,
        eviction_dir: None,
        journal_path: None,
        review_candidate_fingerprint: None,
        review_fingerprint: None,
        review_disposition: None,
        review_dir: None,
    };
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--root" => parsed.root = PathBuf::from(value(args, &mut index, "--root")?),
            "--cloud-root" => {
                parsed.cloud_root = Some(PathBuf::from(value(args, &mut index, "--cloud-root")?))
            }
            "--provider" => {
                parsed.provider = Some(parse_provider(&value(args, &mut index, "--provider")?)?)
            }
            "--min-size-mib" => {
                parsed.min_size_mib = value(args, &mut index, "--min-size-mib")?
                    .parse()
                    .map_err(|_| "--min-size-mib는 정수여야 함".to_string())?
            }
            "--min-age-days" => {
                parsed.min_age_days = value(args, &mut index, "--min-age-days")?
                    .parse()
                    .map_err(|_| "--min-age-days는 정수여야 함".to_string())?
            }
            "--limit" => {
                parsed.limit = value(args, &mut index, "--limit")?
                    .parse()
                    .map_err(|_| "--limit는 정수여야 함".to_string())?
            }
            "--list-roots" => parsed.list_roots = true,
            "--inspect-roots" => parsed.inspect_roots = true,
            "--copy-fingerprint" => {
                parsed.copy_fingerprint = Some(value(args, &mut index, "--copy-fingerprint")?)
            }
            "--receipt-dir" => {
                parsed.receipt_dir = Some(PathBuf::from(value(args, &mut index, "--receipt-dir")?))
            }
            "--attest-receipt" => {
                parsed.attest_receipt = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--attest-receipt",
                )?))
            }
            "--evict-receipt" => {
                parsed.evict_receipt = Some(PathBuf::from(value(
                    args,
                    &mut index,
                    "--evict-receipt",
                )?))
            }
            "--confirm-receipt-id" => {
                parsed.confirm_receipt_id =
                    Some(value(args, &mut index, "--confirm-receipt-id")?)
            }
            "--eviction-dir" => {
                parsed.eviction_dir =
                    Some(PathBuf::from(value(args, &mut index, "--eviction-dir")?))
            }
            "--journal-path" => {
                parsed.journal_path =
                    Some(PathBuf::from(value(args, &mut index, "--journal-path")?))
            }
            "--review-candidate-fingerprint" => {
                parsed.review_candidate_fingerprint = Some(value(
                    args,
                    &mut index,
                    "--review-candidate-fingerprint",
                )?)
            }
            "--review-fingerprint" => {
                parsed.review_fingerprint =
                    Some(value(args, &mut index, "--review-fingerprint")?)
            }
            "--review-disposition" => {
                parsed.review_disposition = Some(match value(
                    args,
                    &mut index,
                    "--review-disposition",
                )?
                .as_str()
                {
                    "approved" => CloudReviewDisposition::Approved,
                    "held" => CloudReviewDisposition::Held,
                    value => return Err(format!("지원하지 않는 review disposition: {value}")),
                })
            }
            "--review-dir" => {
                parsed.review_dir =
                    Some(PathBuf::from(value(args, &mut index, "--review-dir")?))
            }
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-cloud-plan [--list-roots | --inspect-roots] [--root PATH] [--cloud-root PATH | --provider icloud|onedrive|google-drive] [--min-size-mib N] [--min-age-days N] [--limit N] [--copy-fingerprint HEX64 --receipt-dir PATH [--review-dir PATH] | --attest-receipt RECEIPT.json | --evict-receipt RECEIPT.json --confirm-receipt-id HEX64 --eviction-dir ABSOLUTE_PATH --journal-path ABSOLUTE_PATH | --review-candidate-fingerprint HEX64 --review-fingerprint HEX64 --review-disposition approved|held --review-dir PATH]".into(),
                )
            }
            flag => return Err(format!("알 수 없는 인자: {flag}")),
        }
        index += 1;
    }
    Ok(parsed)
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct CopyOutput {
    action: &'static str,
    receipt: CloudCopyReceipt,
    receipt_path: String,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct AttestationOutput {
    action: &'static str,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    permit: Option<LocalEvictionPermit>,
    blockers: Vec<String>,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct EvictionOutput {
    action: &'static str,
    receipt_id: String,
    evidence: disksage_lib::cloud_transfer::ProviderSyncEvidence,
    permit: LocalEvictionPermit,
    eviction: CloudEvictionResult,
}

#[cfg(not(coverage))]
#[derive(Debug, serde::Serialize)]
struct ReviewOutput {
    action: &'static str,
    decision: CloudReviewDecision,
    decision_path: String,
}

#[cfg(not(coverage))]
fn validate_action_args(args: &Args) -> Result<(), String> {
    if args.copy_fingerprint.is_some() != args.receipt_dir.is_some() {
        return Err("--copy-fingerprint와 --receipt-dir은 함께 지정해야 함".into());
    }
    let review_fields = [
        args.review_candidate_fingerprint.is_some(),
        args.review_fingerprint.is_some(),
        args.review_disposition.is_some(),
    ];
    if review_fields.iter().any(|value| *value) && !review_fields.iter().all(|value| *value) {
        return Err("review fingerprint와 disposition은 모두 함께 지정해야 함".into());
    }
    let review_action = review_fields.iter().all(|value| *value);
    let eviction_fields = [
        args.evict_receipt.is_some(),
        args.confirm_receipt_id.is_some(),
        args.eviction_dir.is_some(),
        args.journal_path.is_some(),
    ];
    if eviction_fields.iter().any(|value| *value) && !eviction_fields.iter().all(|value| *value) {
        return Err(
            "eviction action에는 receipt, 확인 id, eviction dir, journal path가 모두 필요함".into(),
        );
    }
    let eviction_action = eviction_fields.iter().all(|value| *value);
    if review_action && args.review_dir.is_none() {
        return Err("review action에는 --review-dir이 필요함".into());
    }
    if args.review_dir.is_some() && !review_action && args.copy_fingerprint.is_none() {
        return Err("--review-dir은 review action 또는 copy action에만 지정할 수 있음".into());
    }
    let actions = usize::from(args.list_roots)
        + usize::from(args.inspect_roots)
        + usize::from(args.copy_fingerprint.is_some())
        + usize::from(args.attest_receipt.is_some())
        + usize::from(eviction_action)
        + usize::from(review_action);
    if actions > 1 {
        return Err(
            "--list-roots, --inspect-roots, --copy-fingerprint, --attest-receipt, eviction action, review action은 동시에 사용할 수 없음".into(),
        );
    }
    for (flag, fingerprint) in [
        ("--copy-fingerprint", args.copy_fingerprint.as_ref()),
        (
            "--review-candidate-fingerprint",
            args.review_candidate_fingerprint.as_ref(),
        ),
        ("--review-fingerprint", args.review_fingerprint.as_ref()),
        ("--confirm-receipt-id", args.confirm_receipt_id.as_ref()),
    ] {
        let Some(fingerprint) = fingerprint else {
            continue;
        };
        if fingerprint.len() != 64 || !fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(format!("{flag}는 64자리 16진수여야 함"));
        }
    }
    if let Some(receipt_dir) = &args.receipt_dir {
        if !receipt_dir.is_absolute() {
            return Err("--receipt-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(receipt_path) = &args.attest_receipt {
        if !receipt_path.is_absolute() {
            return Err("--attest-receipt는 절대 경로여야 함".into());
        }
    }
    if let Some(receipt_path) = &args.evict_receipt {
        if !receipt_path.is_absolute() {
            return Err("--evict-receipt는 절대 경로여야 함".into());
        }
    }
    if let Some(eviction_dir) = &args.eviction_dir {
        if !eviction_dir.is_absolute() {
            return Err("--eviction-dir은 절대 경로여야 함".into());
        }
    }
    if let Some(journal_path) = &args.journal_path {
        if !journal_path.is_absolute() {
            return Err("--journal-path는 절대 경로여야 함".into());
        }
    }
    if let Some(review_dir) = &args.review_dir {
        if !review_dir.is_absolute() {
            return Err("--review-dir은 절대 경로여야 함".into());
        }
    }
    Ok(())
}

#[cfg(not(coverage))]
fn attest_native_receipt(path: &Path) -> Result<AttestationOutput, String> {
    let receipt = cloud_transfer::read_immutable_receipt(path)?;
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = match receipt.provider {
        CloudProvider::Icloud => {
            provider_sync::collect_icloud_sync_evidence(&receipt, confirmed_at_ms)?
        }
        CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
            provider_sync::collect_file_provider_sync_evidence(&receipt, confirmed_at_ms)?
        }
    };
    let (permit, blockers) = match cloud_transfer::approve_local_eviction(&receipt, &evidence) {
        Ok(permit) => (Some(permit), Vec::new()),
        Err(blockers) => (None, blockers),
    };
    Ok(AttestationOutput {
        action: "attest-provider-native",
        receipt_id: receipt.receipt_id,
        evidence,
        permit,
        blockers,
    })
}

#[cfg(not(coverage))]
fn evict_native_receipt(
    path: &Path,
    confirmation_receipt_id: &str,
    eviction_dir: &Path,
    journal_path: &Path,
) -> Result<EvictionOutput, String> {
    let receipt = cloud_transfer::read_immutable_receipt(path)?;
    if confirmation_receipt_id != receipt.receipt_id {
        return Err("eviction-confirmation-receipt-id-mismatch".into());
    }
    let confirmed_at_ms = cloud::system_now_ms();
    let evidence = match receipt.provider {
        CloudProvider::Icloud => {
            provider_sync::collect_icloud_sync_evidence(&receipt, confirmed_at_ms)?
        }
        CloudProvider::Onedrive | CloudProvider::GoogleDrive => {
            provider_sync::collect_file_provider_sync_evidence(&receipt, confirmed_at_ms)?
        }
    };
    let permit = cloud_transfer::approve_local_eviction(&receipt, &evidence)
        .map_err(|blockers| blockers.join(","))?;
    let eviction = cloud_eviction::evict_source(
        &receipt,
        &permit,
        confirmation_receipt_id,
        eviction_dir,
        journal_path,
        cloud::system_now_ms(),
    )?;
    Ok(EvictionOutput {
        action: "attest-and-trash-verified-cloud-source",
        receipt_id: receipt.receipt_id,
        evidence,
        permit,
        eviction,
    })
}

#[cfg(not(coverage))]
fn home_dir() -> Result<PathBuf, String> {
    std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(PathBuf::from)
        .map_err(|_| "HOME/USERPROFILE을 찾을 수 없음".into())
}

#[cfg(not(coverage))]
fn select_root(roots: &[CloudRoot], args: &Args) -> Result<CloudRoot, String> {
    let matches: Vec<&CloudRoot> = roots
        .iter()
        .filter(|root| {
            args.cloud_root
                .as_ref()
                .map(|path| Path::new(&root.path) == path)
                .unwrap_or(true)
                && args.provider.map(|p| p == root.provider).unwrap_or(true)
        })
        .collect();
    match matches.as_slice() {
        [only] => Ok((*only).clone()),
        [] => Err("조건과 일치하는 탐지된 클라우드 루트가 없음 (--list-roots로 확인)".into()),
        _ => Err("클라우드 루트가 여러 개임; --cloud-root로 하나를 선택해야 함".into()),
    }
}

#[cfg(not(coverage))]
fn run() -> Result<(), String> {
    let home = home_dir()?;
    let raw: Vec<String> = std::env::args().skip(1).collect();
    let args = parse_args(&raw, &home)?;
    validate_action_args(&args)?;
    if let Some(receipt_path) = &args.evict_receipt {
        let output = evict_native_receipt(
            receipt_path,
            args.confirm_receipt_id
                .as_deref()
                .ok_or_else(|| "--confirm-receipt-id가 필요함".to_string())?,
            args.eviction_dir
                .as_deref()
                .ok_or_else(|| "--eviction-dir이 필요함".to_string())?,
            args.journal_path
                .as_deref()
                .ok_or_else(|| "--journal-path가 필요함".to_string())?,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(receipt_path) = &args.attest_receipt {
        println!(
            "{}",
            serde_json::to_string_pretty(&attest_native_receipt(receipt_path)?)
                .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    let discovery = cloud::discover_cloud_roots_report(&home);
    if args.inspect_roots {
        println!(
            "{}",
            serde_json::to_string_pretty(&discovery).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    let roots = discovery.roots;
    if args.list_roots {
        println!(
            "{}",
            serde_json::to_string_pretty(&roots).map_err(|e| e.to_string())?
        );
        return Ok(());
    }
    cloud::validate_source_root_readable(&args.root)?;
    let selected = select_root(&roots, &args)?;
    cloud::validate_cloud_root_readable(&selected)?;
    let excluded: Vec<PathBuf> = roots.iter().map(|r| PathBuf::from(&r.path)).collect();
    if excluded
        .iter()
        .any(|cloud_root| args.root.starts_with(cloud_root))
    {
        return Err("이미 클라우드 안에 있는 경로는 오프로드 원본으로 사용할 수 없음".into());
    }
    let files = cloud::collect_archive_files(&args.root, &excluded);
    let report = cloud::plan_cloud_archive(
        &files,
        &args.root,
        &selected,
        cloud::system_now_ms(),
        CloudPlanOptions {
            min_size_bytes: args.min_size_mib.saturating_mul(1024 * 1024),
            min_age_days: args.min_age_days,
            limit: args.limit.clamp(1, 1_000),
        },
    );
    if let Some(candidate_fingerprint) = &args.review_candidate_fingerprint {
        let review_fingerprint = args
            .review_fingerprint
            .as_deref()
            .ok_or_else(|| "--review-fingerprint가 필요함".to_string())?;
        let matches: Vec<_> = report
            .candidates
            .iter()
            .filter(|candidate| {
                candidate.metadata_fingerprint == *candidate_fingerprint
                    && candidate.review_fingerprint == review_fingerprint
            })
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("현재 fresh plan에 review fingerprint가 일치하는 후보가 없음".into()),
            _ => return Err("현재 fresh plan에서 review fingerprint가 중복됨".into()),
        };
        let disposition = args
            .review_disposition
            .ok_or_else(|| "--review-disposition이 필요함".to_string())?;
        let decision =
            cloud_review::create_decision(candidate, disposition, cloud::system_now_ms())?;
        let decision_path = cloud_review::write_immutable_decision(
            args.review_dir
                .as_deref()
                .ok_or_else(|| "--review-dir이 필요함".to_string())?,
            &decision,
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&ReviewOutput {
                action: "review",
                decision,
                decision_path: decision_path.to_string_lossy().into_owned(),
            })
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    if let Some(fingerprint) = &args.copy_fingerprint {
        let matches: Vec<_> = report
            .candidates
            .iter()
            .filter(|candidate| candidate.metadata_fingerprint == *fingerprint)
            .collect();
        let candidate = match matches.as_slice() {
            [only] => *only,
            [] => return Err("현재 fresh plan에 fingerprint가 일치하는 후보가 없음".into()),
            _ => return Err("현재 fresh plan에서 fingerprint가 중복됨".into()),
        };
        let receipt_dir = args
            .receipt_dir
            .as_deref()
            .ok_or_else(|| "--receipt-dir이 필요함".to_string())?;
        let review_decision = if candidate.requires_review {
            args.review_dir
                .as_deref()
                .map(cloud_review::load_latest_decisions)
                .transpose()?
                .unwrap_or_default()
                .into_iter()
                .find(|decision| decision.candidate_fingerprint == candidate.metadata_fingerprint)
        } else {
            None
        };
        let (receipt, receipt_path) = cloud_transfer::prepare_cloud_copy_with_review(
            candidate,
            &selected,
            receipt_dir,
            cloud::system_now_ms(),
            review_decision.as_ref(),
        )?;
        println!(
            "{}",
            serde_json::to_string_pretty(&CopyOutput {
                action: "copy-only",
                receipt,
                receipt_path: receipt_path.to_string_lossy().into_owned(),
            })
            .map_err(|error| error.to_string())?
        );
        return Ok(());
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
    );
    Ok(())
}

#[cfg(not(coverage))]
fn main() {
    if let Err(error) = run() {
        eprintln!("DiskSage cloud planner: {error}");
        std::process::exit(2);
    }
}

#[cfg(coverage)]
fn main() {}

#[cfg(all(test, coverage))]
mod coverage_tests {
    #[test]
    fn noop_main_runs() {
        super::main();
    }
}

#[cfg(all(test, not(coverage)))]
mod tests {
    use super::*;

    #[test]
    fn parses_defaults_and_explicit_values() {
        let defaults = parse_args(&[], Path::new("/home/test")).unwrap();
        assert_eq!(defaults.root, PathBuf::from("/home/test"));
        assert_eq!(defaults.min_size_mib, 256);
        assert!(defaults.copy_fingerprint.is_none());
        assert!(defaults.evict_receipt.is_none());
        assert!(defaults.review_candidate_fingerprint.is_none());
        let args = vec![
            "--root".into(),
            "/scan".into(),
            "--provider".into(),
            "icloud".into(),
            "--min-size-mib".into(),
            "1".into(),
            "--min-age-days".into(),
            "2".into(),
            "--limit".into(),
            "3".into(),
        ];
        let parsed = parse_args(&args, Path::new("/home/test")).unwrap();
        assert_eq!(parsed.root, PathBuf::from("/scan"));
        assert_eq!(parsed.provider, Some(CloudProvider::Icloud));
        assert_eq!(
            (parsed.min_size_mib, parsed.min_age_days, parsed.limit),
            (1, 2, 3)
        );
    }

    #[test]
    fn parser_and_selector_reject_ambiguous_or_invalid_input() {
        assert!(parse_args(&["--wat".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--provider".into(), "box".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--limit".into(), "x".into()], Path::new("/h")).is_err());
        assert!(parse_args(&["--root".into()], Path::new("/h")).is_err());
        let inspect = parse_args(&["--inspect-roots".into()], Path::new("/h")).unwrap();
        assert!(inspect.inspect_roots);
        let both = parse_args(
            &["--list-roots".into(), "--inspect-roots".into()],
            Path::new("/h"),
        )
        .unwrap();
        assert!(validate_action_args(&both).is_err());
        let roots = vec![
            CloudRoot {
                id: "/a".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Unknown,
                label: "a".into(),
                path: "/a".into(),
                readable: true,
                access_issue: None,
            },
            CloudRoot {
                id: "/b".into(),
                provider: CloudProvider::Icloud,
                account_scope: disksage_lib::cloud::CloudAccountScope::Unknown,
                label: "b".into(),
                path: "/b".into(),
                readable: true,
                access_issue: None,
            },
        ];
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        assert!(select_root(&roots, &args).is_err());
        args.cloud_root = Some(PathBuf::from("/b"));
        assert_eq!(select_root(&roots, &args).unwrap().path, "/b");
        args.cloud_root = Some(PathBuf::from("/missing"));
        assert!(select_root(&roots, &args).is_err());
    }

    #[test]
    fn action_validation_requires_explicit_consistent_copy_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.copy_fingerprint = Some("a".repeat(64));
        assert!(validate_action_args(&args).is_err());
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        assert!(validate_action_args(&args).is_ok());
        args.receipt_dir = Some(PathBuf::from("relative-receipts"));
        assert!(validate_action_args(&args).is_err());
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        args.copy_fingerprint = Some("not-a-fingerprint".into());
        assert!(validate_action_args(&args).is_err());

        args.list_roots = false;
        args.attest_receipt = Some(PathBuf::from("relative-receipt.json"));
        assert!(validate_action_args(&args).is_err());
        args.copy_fingerprint = None;
        args.receipt_dir = None;
        args.list_roots = true;
        args.attest_receipt = Some(PathBuf::from("/receipt.json"));
        assert!(validate_action_args(&args).is_err());

        let parsed = parse_args(
            &[
                "--copy-fingerprint".into(),
                "b".repeat(64),
                "--receipt-dir".into(),
                "/receipts".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(parsed.copy_fingerprint, Some("b".repeat(64)));
        assert_eq!(parsed.receipt_dir, Some(PathBuf::from("/receipts")));

        let review = parse_args(
            &[
                "--review-candidate-fingerprint".into(),
                "c".repeat(64),
                "--review-fingerprint".into(),
                "d".repeat(64),
                "--review-disposition".into(),
                "approved".into(),
                "--review-dir".into(),
                "/reviews".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(review.review_candidate_fingerprint, Some("c".repeat(64)));
        assert_eq!(review.review_fingerprint, Some("d".repeat(64)));
        assert_eq!(
            review.review_disposition,
            Some(CloudReviewDisposition::Approved)
        );
        assert_eq!(review.review_dir, Some(PathBuf::from("/reviews")));
        assert!(validate_action_args(&review).is_ok());

        assert!(parse_args(
            &["--review-disposition".into(), "maybe".into(),],
            Path::new("/h"),
        )
        .is_err());
    }

    #[test]
    fn action_validation_requires_complete_review_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.review_candidate_fingerprint = Some("c".repeat(64));
        assert!(validate_action_args(&args).is_err());
        args.review_fingerprint = Some("d".repeat(64));
        args.review_disposition = Some(CloudReviewDisposition::Held);
        assert!(validate_action_args(&args).is_err());
        args.review_dir = Some(PathBuf::from("relative-reviews"));
        assert!(validate_action_args(&args).is_err());
        args.review_dir = Some(PathBuf::from("/reviews"));
        assert!(validate_action_args(&args).is_ok());

        args.copy_fingerprint = Some("a".repeat(64));
        args.receipt_dir = Some(PathBuf::from("/receipts"));
        assert!(validate_action_args(&args).is_err());

        args.review_candidate_fingerprint = None;
        args.review_fingerprint = None;
        args.review_disposition = None;
        assert!(validate_action_args(&args).is_ok());
    }

    #[test]
    fn action_validation_requires_explicit_complete_eviction_arguments() {
        let mut args = parse_args(&[], Path::new("/h")).unwrap();
        args.evict_receipt = Some(PathBuf::from("/receipts/a.json"));
        assert!(validate_action_args(&args).is_err());
        args.confirm_receipt_id = Some("a".repeat(64));
        args.eviction_dir = Some(PathBuf::from("/evictions"));
        args.journal_path = Some(PathBuf::from("relative-journal"));
        assert!(validate_action_args(&args).is_err());
        args.journal_path = Some(PathBuf::from("/journal/operations.jsonl"));
        assert!(validate_action_args(&args).is_ok());

        args.attest_receipt = Some(PathBuf::from("/receipt.json"));
        assert!(validate_action_args(&args).is_err());

        let parsed = parse_args(
            &[
                "--evict-receipt".into(),
                "/receipts/a.json".into(),
                "--confirm-receipt-id".into(),
                "b".repeat(64),
                "--eviction-dir".into(),
                "/evictions".into(),
                "--journal-path".into(),
                "/journal/operations.jsonl".into(),
            ],
            Path::new("/h"),
        )
        .unwrap();
        assert_eq!(parsed.confirm_receipt_id, Some("b".repeat(64)));
        assert!(validate_action_args(&parsed).is_ok());
    }

    #[test]
    fn attestation_rejects_forged_receipt_before_destination_probe() {
        let temp = tempfile::tempdir().unwrap();
        let receipt = CloudCopyReceipt {
            version: cloud_transfer::RECEIPT_VERSION,
            receipt_id: "0".repeat(64),
            candidate_fingerprint: "1".repeat(64),
            provider: CloudProvider::Icloud,
            source: temp
                .path()
                .join("source.pdf")
                .to_string_lossy()
                .into_owned(),
            destination: temp
                .path()
                .join("destination-does-not-exist.pdf")
                .to_string_lossy()
                .into_owned(),
            bytes: 1,
            blake3: "2".repeat(64),
            sha256: "3".repeat(64),
            quick_xor_base64: "AAAAAAAAAAAAAAAAAAAAAAAAAAA=".into(),
            source_modified_ms: 1,
            copied_at_ms: 2,
            copy_verified: true,
            provider_sync_confirmed: false,
            lineage_fingerprint: None,
            lineage: None,
        };
        let path = temp.path().join(format!("{}.json", receipt.receipt_id));
        std::fs::write(&path, serde_json::to_vec(&receipt).unwrap()).unwrap();
        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_readonly(true);
        std::fs::set_permissions(&path, permissions).unwrap();

        let error = attest_native_receipt(&path).unwrap_err();
        assert!(error.contains("receipt-integrity-mismatch"));
        assert!(!error.contains("No such file"));
    }
}
