//! Read-only-by-default native temp reclaim plan with explicit per-candidate Trash approval.

use std::path::PathBuf;

use disksage_lib::temp_reclaim::{
    execute_candidate, plan_native_temp_reclaim, TempReclaimApproval, MAX_APPROVAL_AGE_MS,
};

const USAGE: &str = "usage: disksage-temp-reclaim [--execute-fingerprint HEX --approved-by LOCAL_USER --approval-phrase EXACT_PHRASE --journal-path ABSOLUTE_PATH]";

#[derive(Default)]
struct ExecutionArguments {
    fingerprint: Option<String>,
    actor: Option<String>,
    phrase: Option<String>,
    journal: Option<String>,
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
}

fn parse_execution_arguments(args: &[String]) -> Result<ExecutionArguments, String> {
    let mut parsed = ExecutionArguments::default();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        let value = args
            .get(index + 1)
            .ok_or_else(|| format!("인자 값이 없습니다: {flag}"))?;
        let slot = match flag {
            "--execute-fingerprint" => &mut parsed.fingerprint,
            "--approved-by" => &mut parsed.actor,
            "--approval-phrase" => &mut parsed.phrase,
            "--journal-path" => &mut parsed.journal,
            _ => return Err(format!("지원하지 않는 인자입니다: {flag}")),
        };
        if slot.is_some() {
            return Err(format!("중복된 인자입니다: {flag}"));
        }
        *slot = Some(value.clone());
        index += 2;
    }
    if parsed.fingerprint.is_none()
        || parsed.actor.is_none()
        || parsed.phrase.is_none()
        || parsed.journal.is_none()
    {
        return Err("필요한 승인 값이 없습니다. 계획에 표시된 정확한 문구를 직접 입력하세요.".into());
    }
    Ok(parsed)
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args
        .iter()
        .any(|value| matches!(value.as_str(), "-h" | "--help"))
    {
        println!("{USAGE}");
        return;
    }
    let execution_arguments = if args.is_empty() {
        None
    } else {
        match parse_execution_arguments(&args) {
            Ok(parsed) => Some(parsed),
            Err(error) => {
                eprintln!("{error}\n{USAGE}");
                std::process::exit(2);
            }
        }
    };
    let observed_at_ms = now_ms();
    let plan = match plan_native_temp_reclaim(observed_at_ms) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("임시 공간을 확인하지 못했습니다. 저장 공간 위치를 확인한 뒤 다시 시도하세요: {error}");
            std::process::exit(2);
        }
    };
    let Some(execution_arguments) = execution_arguments else {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).unwrap_or_else(|_| "{}".into())
        );
        return;
    };
    let fingerprint = execution_arguments
        .fingerprint
        .expect("validated execution fingerprint should be present");
    let actor = execution_arguments
        .actor
        .expect("validated approval actor should be present");
    let phrase = execution_arguments
        .phrase
        .expect("validated approval phrase should be present");
    let journal = PathBuf::from(
        execution_arguments
            .journal
            .expect("validated journal path should be present"),
    );
    if !journal.is_absolute() {
        eprintln!("저널 경로는 절대 경로여야 합니다.");
        std::process::exit(2);
    }
    let approval = TempReclaimApproval {
        candidate_fingerprint: fingerprint.clone(),
        approved_at_ms: observed_at_ms,
        approved_by: actor,
        exact_phrase: phrase,
    };
    let execution_now_ms = now_ms();
    let approval_can_reach_mutation = execution_now_ms >= approval.approved_at_ms
        && execution_now_ms - approval.approved_at_ms <= MAX_APPROVAL_AGE_MS
        && approval.candidate_fingerprint == fingerprint
        && !approval.approved_by.trim().is_empty()
        && !approval.approved_by.chars().any(char::is_control)
        && plan.scan_complete
        && plan.candidates.iter().any(|candidate| {
            candidate.candidate_fingerprint == fingerprint
                && candidate.eligible_for_approval
                && candidate.exact_approval_phrase.as_deref()
                    == Some(approval.exact_phrase.as_str())
        });
    if approval_can_reach_mutation {
        if let Some(parent) = journal.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                eprintln!("저널 저장 위치를 준비하지 못했습니다. 쓰기 가능한 로컬 경로를 확인하세요.");
                std::process::exit(2);
            }
        }
    }
    let result = execute_candidate(
        &plan,
        &fingerprint,
        &approval,
        &journal,
        execution_now_ms,
    );
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
    );
    if !result.ok {
        std::process::exit(1);
    }
}
