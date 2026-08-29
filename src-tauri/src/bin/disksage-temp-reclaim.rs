//! Read-only-by-default native temp reclaim plan with explicit per-candidate Trash approval.

use std::path::PathBuf;

use disksage_lib::temp_reclaim::{
    execute_candidate, plan_native_temp_reclaim, TempReclaimApproval,
};

const USAGE: &str = "usage: disksage-temp-reclaim [--execute-fingerprint HEX --approved-by LOCAL_USER --approval-phrase EXACT_PHRASE --journal-path ABSOLUTE_PATH]";

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or_default()
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
    let observed_at_ms = now_ms();
    let plan = match plan_native_temp_reclaim(observed_at_ms) {
        Ok(plan) => plan,
        Err(error) => {
            eprintln!("임시 공간을 확인하지 못했습니다. 저장 공간 위치를 확인한 뒤 다시 시도하세요: {error}");
            std::process::exit(2);
        }
    };
    if args.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(&plan).unwrap_or_else(|_| "{}".into())
        );
        return;
    }
    let value = |flag: &str| -> Option<String> {
        args.windows(2)
            .find(|pair| pair[0] == flag)
            .map(|pair| pair[1].clone())
    };
    let (Some(fingerprint), Some(actor), Some(phrase), Some(journal)) = (
        value("--execute-fingerprint"),
        value("--approved-by"),
        value("--approval-phrase"),
        value("--journal-path"),
    ) else {
        eprintln!(
            "필요한 승인 값이 없습니다. 계획에 표시된 정확한 문구를 직접 입력하세요.\n{USAGE}"
        );
        std::process::exit(2);
    };
    let journal = PathBuf::from(journal);
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
    if let Some(parent) = journal.parent() {
        if std::fs::create_dir_all(parent).is_err() {
            eprintln!("저널 저장 위치를 준비하지 못했습니다. 쓰기 가능한 로컬 경로를 확인하세요.");
            std::process::exit(2);
        }
    }
    let result = execute_candidate(&plan, &fingerprint, &approval, &journal, now_ms());
    println!(
        "{}",
        serde_json::to_string_pretty(&result).unwrap_or_else(|_| "{}".into())
    );
    if !result.ok {
        std::process::exit(1);
    }
}
