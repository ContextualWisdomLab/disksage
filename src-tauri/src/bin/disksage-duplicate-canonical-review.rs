use std::path::{Path, PathBuf};

use disksage_lib::cloud;
use disksage_lib::cloud_review::validate_review_attribution;
use disksage_lib::duplicate_canonical_decision::{
    create_local_duplicate_canonical_decision, validate_local_duplicate_canonical_review_dossier,
    verify_local_duplicate_canonical_review_dossier,
    write_immutable_local_duplicate_canonical_decision, DuplicateCanonicalDecisionDisposition,
};
use disksage_lib::naruon_duplicate_canonical_review::LocalDuplicateCanonicalReviewDossier;

const MAX_DOSSIER_BYTES: u64 = 8 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
struct Args {
    dossier: PathBuf,
    verify: bool,
    cluster_ref: Option<String>,
    disposition: Option<DuplicateCanonicalDecisionDisposition>,
    selected_member_ref: Option<String>,
    reviewed_by: Option<String>,
    rationale: Option<String>,
    decision_dir: Option<PathBuf>,
}

fn value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("{flag} 값이 필요함"))
}

fn valid_lower_hex_64(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn parse_disposition(value: &str) -> Result<DuplicateCanonicalDecisionDisposition, String> {
    match value {
        "selected" => Ok(DuplicateCanonicalDecisionDisposition::Selected),
        "held" => Ok(DuplicateCanonicalDecisionDisposition::Held),
        _ => Err("--disposition은 selected 또는 held여야 함".into()),
    }
}

fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut dossier = None;
    let mut verify = false;
    let mut cluster_ref = None;
    let mut disposition = None;
    let mut selected_member_ref = None;
    let mut reviewed_by = None;
    let mut rationale = None;
    let mut decision_dir = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--dossier" => {
                if dossier.is_some() {
                    return Err("--dossier는 한 번만 지정할 수 있음".into());
                }
                dossier = Some(PathBuf::from(value(args, &mut index, "--dossier")?));
            }
            "--verify" => {
                if verify {
                    return Err("--verify는 한 번만 지정할 수 있음".into());
                }
                verify = true;
            }
            "--cluster-ref" => {
                if cluster_ref.is_some() {
                    return Err("--cluster-ref는 한 번만 지정할 수 있음".into());
                }
                cluster_ref = Some(value(args, &mut index, "--cluster-ref")?);
            }
            "--disposition" => {
                if disposition.is_some() {
                    return Err("--disposition은 한 번만 지정할 수 있음".into());
                }
                disposition = Some(parse_disposition(&value(
                    args,
                    &mut index,
                    "--disposition",
                )?)?);
            }
            "--selected-member-ref" => {
                if selected_member_ref.is_some() {
                    return Err("--selected-member-ref는 한 번만 지정할 수 있음".into());
                }
                selected_member_ref = Some(value(args, &mut index, "--selected-member-ref")?);
            }
            "--reviewed-by" => {
                if reviewed_by.is_some() {
                    return Err("--reviewed-by는 한 번만 지정할 수 있음".into());
                }
                reviewed_by = Some(value(args, &mut index, "--reviewed-by")?);
            }
            "--rationale" => {
                if rationale.is_some() {
                    return Err("--rationale는 한 번만 지정할 수 있음".into());
                }
                rationale = Some(value(args, &mut index, "--rationale")?);
            }
            "--decision-dir" => {
                if decision_dir.is_some() {
                    return Err("--decision-dir은 한 번만 지정할 수 있음".into());
                }
                decision_dir = Some(PathBuf::from(value(args, &mut index, "--decision-dir")?));
            }
            "--help" | "-h" => {
                return Err(
                    "usage: disksage-duplicate-canonical-review --dossier ABSOLUTE_0600.json [--verify | --cluster-ref HEX64 --disposition selected|held [--selected-member-ref HEX64] --reviewed-by human:ID --rationale TEXT --decision-dir ABSOLUTE_EXISTING_DIR]".into(),
                );
            }
            flag => return Err(format!("알 수 없는 인자: {flag}")),
        }
        index += 1;
    }
    let parsed = Args {
        dossier: dossier.ok_or_else(|| "--dossier가 필요함".to_string())?,
        verify,
        cluster_ref,
        disposition,
        selected_member_ref,
        reviewed_by,
        rationale,
        decision_dir,
    };
    validate_args(&parsed)?;
    Ok(parsed)
}

fn validate_args(args: &Args) -> Result<(), String> {
    if !args.dossier.is_absolute() {
        return Err("--dossier는 절대 경로여야 함".into());
    }
    let decision_fields = [
        args.cluster_ref.is_some(),
        args.disposition.is_some(),
        args.reviewed_by.is_some(),
        args.rationale.is_some(),
        args.decision_dir.is_some(),
    ];
    let decision_action = decision_fields.iter().all(|present| *present);
    if args.verify == decision_action {
        return Err("--verify 또는 완전한 decision action 중 하나만 지정해야 함".into());
    }
    if decision_fields.iter().any(|present| *present) && !decision_action {
        return Err(
            "decision action에는 cluster, disposition, reviewer, rationale, decision dir이 모두 필요함"
                .into(),
        );
    }
    if args.verify {
        if args.selected_member_ref.is_some() {
            return Err("--verify에는 --selected-member-ref를 지정할 수 없음".into());
        }
        return Ok(());
    }

    let cluster_ref = args
        .cluster_ref
        .as_deref()
        .ok_or_else(|| "--cluster-ref가 필요함".to_string())?;
    if !valid_lower_hex_64(cluster_ref) {
        return Err("--cluster-ref는 소문자 HEX64여야 함".into());
    }
    match args
        .disposition
        .ok_or_else(|| "--disposition이 필요함".to_string())?
    {
        DuplicateCanonicalDecisionDisposition::Selected => {
            if args
                .selected_member_ref
                .as_deref()
                .is_none_or(|value| !valid_lower_hex_64(value))
            {
                return Err(
                    "selected disposition에는 소문자 HEX64 --selected-member-ref가 필요함".into(),
                );
            }
        }
        DuplicateCanonicalDecisionDisposition::Held => {
            if args.selected_member_ref.is_some() {
                return Err("held disposition에는 --selected-member-ref를 지정할 수 없음".into());
            }
        }
    }
    let decision_dir = args
        .decision_dir
        .as_deref()
        .ok_or_else(|| "--decision-dir이 필요함".to_string())?;
    if !decision_dir.is_absolute() {
        return Err("--decision-dir은 절대 경로여야 함".into());
    }
    validate_review_attribution(
        args.reviewed_by
            .as_deref()
            .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
        args.rationale
            .as_deref()
            .ok_or_else(|| "--rationale가 필요함".to_string())?,
    )
}

fn read_dossier(path: &Path) -> Result<LocalDuplicateCanonicalReviewDossier, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|_| "duplicate-canonical-dossier-unavailable".to_string())?;
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || metadata.len() == 0
        || metadata.len() > MAX_DOSSIER_BYTES
    {
        return Err("duplicate-canonical-dossier-unsafe-or-unbounded".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err("duplicate-canonical-dossier-permissions-too-open".into());
        }
    }
    let encoded =
        std::fs::read(path).map_err(|_| "duplicate-canonical-dossier-read-failed".to_string())?;
    let dossier: LocalDuplicateCanonicalReviewDossier = serde_json::from_slice(&encoded)
        .map_err(|_| "duplicate-canonical-dossier-json-invalid".to_string())?;
    validate_local_duplicate_canonical_review_dossier(&dossier)?;
    Ok(dossier)
}

fn run() -> Result<(), String> {
    let raw = std::env::args().skip(1).collect::<Vec<_>>();
    let args = parse_args(&raw)?;
    let dossier = read_dossier(&args.dossier)?;
    if args.verify {
        let summary =
            verify_local_duplicate_canonical_review_dossier(&dossier, cloud::system_now_ms())?;
        println!(
            "{}",
            serde_json::to_string_pretty(&summary).map_err(|error| error.to_string())?
        );
        return Ok(());
    }

    let decision = create_local_duplicate_canonical_decision(
        &dossier,
        args.cluster_ref
            .as_deref()
            .ok_or_else(|| "--cluster-ref가 필요함".to_string())?,
        args.disposition
            .ok_or_else(|| "--disposition이 필요함".to_string())?,
        args.selected_member_ref.as_deref(),
        cloud::system_now_ms(),
        args.reviewed_by
            .as_deref()
            .ok_or_else(|| "--reviewed-by가 필요함".to_string())?,
        args.rationale
            .as_deref()
            .ok_or_else(|| "--rationale가 필요함".to_string())?,
    )?;
    let decision_path = write_immutable_local_duplicate_canonical_decision(
        &dossier,
        args.decision_dir
            .as_deref()
            .ok_or_else(|| "--decision-dir이 필요함".to_string())?,
        &decision,
    )?;
    let output = serde_json::json!({
        "schema_version": 1,
        "action": "record-local-duplicate-canonical-selection",
        "decision_id": decision.decision_id,
        "dossier_id": decision.dossier_id,
        "cluster_ref": decision.cluster_ref,
        "disposition": decision.disposition,
        "selection_matches_recommendation": decision.selection_matches_recommendation,
        "canonical_selection_recorded": decision.canonical_selection_recorded,
        "discard_authorization": false,
        "mutation_performed": false,
        "cloud_write_performed": false,
        "decision_path": decision_path,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(value: &str) -> String {
        value.to_string()
    }

    #[test]
    fn parses_verify_and_complete_decision_actions() {
        let verify =
            parse_args(&[s("--dossier"), s("/private/dossier.json"), s("--verify")]).unwrap();
        assert!(verify.verify);

        let selected = parse_args(&[
            s("--dossier"),
            s("/private/dossier.json"),
            s("--cluster-ref"),
            "a".repeat(64),
            s("--disposition"),
            s("selected"),
            s("--selected-member-ref"),
            "b".repeat(64),
            s("--reviewed-by"),
            s("human:owner"),
            s("--rationale"),
            s("Embedded metadata and context were manually reviewed."),
            s("--decision-dir"),
            s("/private/decisions"),
        ])
        .unwrap();
        assert_eq!(
            selected.disposition,
            Some(DuplicateCanonicalDecisionDisposition::Selected)
        );
        assert_eq!(selected.selected_member_ref, Some("b".repeat(64)));

        let held = parse_args(&[
            s("--dossier"),
            s("/private/dossier.json"),
            s("--cluster-ref"),
            "a".repeat(64),
            s("--disposition"),
            s("held"),
            s("--reviewed-by"),
            s("human:owner"),
            s("--rationale"),
            s("More production context is required."),
            s("--decision-dir"),
            s("/private/decisions"),
        ])
        .unwrap();
        assert_eq!(
            held.disposition,
            Some(DuplicateCanonicalDecisionDisposition::Held)
        );
        assert!(held.selected_member_ref.is_none());
    }

    #[test]
    fn rejects_partial_ambiguous_or_unattributed_actions() {
        assert!(parse_args(&[s("--dossier"), s("relative.json"), s("--verify"),]).is_err());
        assert!(parse_args(&[
            s("--dossier"),
            s("/private/dossier.json"),
            s("--verify"),
            s("--cluster-ref"),
            "a".repeat(64),
        ])
        .is_err());
        assert!(parse_args(&[
            s("--dossier"),
            s("/private/dossier.json"),
            s("--cluster-ref"),
            "a".repeat(64),
            s("--disposition"),
            s("selected"),
            s("--reviewed-by"),
            s("agent:not-human"),
            s("--rationale"),
            s("Invalid attribution."),
            s("--decision-dir"),
            s("/private/decisions"),
        ])
        .is_err());
        assert!(parse_args(&[
            s("--dossier"),
            s("/private/dossier.json"),
            s("--cluster-ref"),
            "a".repeat(64),
            s("--disposition"),
            s("held"),
            s("--selected-member-ref"),
            "b".repeat(64),
            s("--reviewed-by"),
            s("human:owner"),
            s("--rationale"),
            s("A hold cannot select a member."),
            s("--decision-dir"),
            s("/private/decisions"),
        ])
        .is_err());
    }
}
